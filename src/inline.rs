//! DEVELOPMENT PROTOTYPE: a reusable Msftedit paragraph, not main-window UI.
//!
//! RichEdit owns text input, caret, accessibility, selection and native undo.
//! TOM ranges apply projection styles without selecting text or resetting it;
//! tomSuspend/tomResume keep decoration out of native undo/redo. The caller owns
//! exact Markdown/draft state and handles EN_CHANGE on its UI thread. This does
//! not replace, or claim metric equivalence with, the production DirectWrite UI.
use crate::{
    inline_projection::{MAX_PARAGRAPH_UNITS, Projection},
    theme::Palette,
};
use std::{cell::Cell, ops::Range};
use windows::{
    Win32::{
        Foundation::*,
        System::LibraryLoader::{LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
        UI::{
            Controls::{EM_CANUNDO, EM_EMPTYUNDOBUFFER, EM_REPLACESEL, EM_UNDO, RichEdit::*},
            Input::KeyboardAndMouse::{GetFocus, GetKeyState, VK_CONTROL},
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::{IUnknown, Interface, Result, w},
};

fn failure(message: &str) -> windows::core::Error {
    windows::core::Error::new(E_FAIL, message)
}
fn color(rgb: u32) -> i32 {
    ((rgb & 255) << 16 | (rgb & 0xff00) | (rgb >> 16 & 255)) as i32
}
struct Library(HMODULE);
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            let _ = FreeLibrary(self.0);
        }
    }
}
struct ControlState {
    paints: Cell<u64>,
    composing: Cell<bool>,
    parent: HWND,
    notify: u32,
}
unsafe extern "system" fn subclass(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    // Box storage belongs to Paragraph and outlives the HWND/subclass. Cell keeps
    // nested Win32 notifications from creating aliased mutable Rust references.
    unsafe {
        let state = &*(data as *const ControlState);
        match message {
            WM_PAINT => state.paints.set(state.paints.get() + 1),
            WM_IME_STARTCOMPOSITION => state.composing.set(true),
            WM_IME_ENDCOMPOSITION => {
                let result = DefSubclassProc(hwnd, message, w, l);
                state.composing.set(false);
                let _ = PostMessageW(Some(state.parent), state.notify, WPARAM(0), LPARAM(0));
                return result;
            }
            WM_MOUSEWHEEL => {
                let _ = PostMessageW(Some(state.parent), WM_MOUSEWHEEL, w, l);
                return LRESULT(0);
            }
            WM_KEYDOWN if w.0 == 0x1b => {
                let _ = PostMessageW(Some(state.parent), WM_COMMAND, WPARAM(220), LPARAM(0));
                return LRESULT(0);
            }
            WM_KEYDOWN if GetKeyState(VK_CONTROL.0 as i32) < 0 && matches!(w.0, 0x42 | 0x49) => {
                let action = if w.0 == 0x42 { 2 } else { 3 };
                let _ = PostMessageW(Some(state.parent), state.notify, WPARAM(action), LPARAM(0));
                return LRESULT(0);
            }
            WM_KEYDOWN if GetKeyState(VK_CONTROL.0 as i32) < 0 && w.0 == 0x55 => return LRESULT(0),
            WM_CHAR if GetKeyState(VK_CONTROL.0 as i32) < 0 && matches!(w.0, 2 | 9 | 21) => {
                return LRESULT(0);
            }
            // Plain Unicode paste only; never import RTF/HTML or embedded OLE.
            WM_PASTE => {
                return SendMessageW(hwnd, EM_PASTESPECIAL, Some(WPARAM(13)), Some(LPARAM(0)));
            }
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(subclass), id);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, message, w, l)
    }
}

pub struct Paragraph {
    pub hwnd: HWND,
    document: Option<ITextDocument>,
    state: Box<ControlState>,
    presentation: Cell<Option<(u16, bool)>>,
    base_bold: Cell<bool>,
    _library: Library,
}
impl Paragraph {
    /// UI-thread only; caller initializes an STA. `notify` is a posted parent
    /// message used after composition ends. Normal edits use native EN_CHANGE.
    pub fn create(
        parent: HWND,
        id: usize,
        notify: u32,
        projection: &Projection,
        light: bool,
    ) -> Result<Self> {
        unsafe {
            let library = Library(LoadLibraryExW(
                w!("Msftedit.dll"),
                None,
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )?);
            let hwnd = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                w!("RICHEDIT50W"),
                w!(""),
                WS_CHILD
                    | WS_VISIBLE
                    | WS_TABSTOP
                    | WS_VSCROLL
                    | WINDOW_STYLE(
                        (ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN | ES_NOHIDESEL) as u32,
                    ),
                0,
                0,
                100,
                100,
                Some(parent),
                Some(HMENU(id as *mut _)),
                None,
                None,
            )?;
            let mut result = Self {
                hwnd,
                document: None,
                state: Box::new(ControlState {
                    paints: Cell::new(0),
                    composing: Cell::new(false),
                    parent,
                    notify,
                }),
                presentation: Cell::new(None),
                base_bold: Cell::new(false),
                _library: library,
            };
            let mut unknown: Option<IUnknown> = None;
            if SendMessageW(
                hwnd,
                EM_GETOLEINTERFACE,
                None,
                Some(LPARAM((&mut unknown as *mut Option<IUnknown>) as isize)),
            )
            .0 == 0
            {
                return Err(failure("Msftedit EM_GETOLEINTERFACE unavailable"));
            }
            result.document = Some(
                unknown
                    .ok_or_else(|| failure("Msftedit returned no TOM interface"))?
                    .cast()?,
            );
            SendMessageW(
                hwnd,
                EM_EXLIMITTEXT,
                None,
                Some(LPARAM(MAX_PARAGRAPH_UNITS as isize)),
            );
            SendMessageW(hwnd, EM_SETUNDOLIMIT, Some(WPARAM(100)), None);
            let palette = Palette::new(light);
            SendMessageW(
                hwnd,
                EM_SETBKGNDCOLOR,
                None,
                Some(LPARAM(color(palette.card) as isize)),
            );
            // Set the *default* format, not only existing text. Startup recovery
            // via WM_SETTEXT and typing into an emptied control must stay legible.
            let mut base = CHARFORMATW {
                cbSize: std::mem::size_of::<CHARFORMATW>() as u32,
                dwMask: CFM_FACE | CFM_SIZE | CFM_COLOR | CFM_BOLD | CFM_ITALIC,
                yHeight: 270, // 13.5 points in twentieths of a point
                crTextColor: COLORREF(color(palette.ink) as u32),
                ..Default::default()
            };
            for (slot, unit) in base.szFaceName.iter_mut().zip("Segoe UI".encode_utf16()) {
                *slot = unit;
            }
            if SendMessageW(
                hwnd,
                EM_SETCHARFORMAT,
                Some(WPARAM(SCF_DEFAULT as usize)),
                Some(LPARAM((&base as *const CHARFORMATW) as isize)),
            )
            .0 == 0
            {
                return Err(failure("Msftedit default font/color failed"));
            }
            // Exactly one initial load. Never use WM_SETTEXT during input/styling.
            let text: Vec<u16> = projection.text.encode_utf16().chain([0]).collect();
            if SendMessageW(hwnd, WM_SETTEXT, None, Some(LPARAM(text.as_ptr() as isize))).0 == 0 {
                return Err(failure("Msftedit initial text failed"));
            }
            result.apply_styles(projection)?;
            SendMessageW(hwnd, EM_EMPTYUNDOBUFFER, None, None);
            if !SetWindowSubclass(
                hwnd,
                Some(subclass),
                1,
                (&*result.state as *const ControlState) as usize,
            )
            .as_bool()
            {
                return Err(failure("Msftedit subclass failed"));
            }
            SendMessageW(
                hwnd,
                EM_SETEVENTMASK,
                None,
                Some(LPARAM(ENM_CHANGE as isize)),
            );
            Ok(result)
        }
    }
    fn doc(&self) -> &ITextDocument {
        self.document.as_ref().expect("live paragraph")
    }
    pub fn resize(&self, rect: RECT) -> Result<()> {
        unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
    }
    pub fn paints(&self) -> u64 {
        self.state.paints.get()
    }
    pub fn composing(&self) -> bool {
        self.state.composing.get()
    }
    /// Native input uses point sizes; the document renderer supplies logical pixels.
    /// Decoration never rewrites text or resets caret/native undo.
    pub fn set_presentation(&self, size: u16, light: bool) -> Result<()> {
        if self.presentation.get() == Some((size, light)) || self.composing() {
            return Ok(());
        }
        let palette = Palette::new(light);
        let base = CHARFORMATW {
            cbSize: std::mem::size_of::<CHARFORMATW>() as u32,
            dwMask: CFM_SIZE | CFM_COLOR,
            yHeight: i32::from(size) * 15,
            crTextColor: COLORREF(color(palette.ink) as u32),
            ..Default::default()
        };
        unsafe {
            SendMessageW(
                self.hwnd,
                EM_SETBKGNDCOLOR,
                None,
                Some(LPARAM(color(palette.card) as isize)),
            );
            if SendMessageW(
                self.hwnd,
                EM_SETCHARFORMAT,
                Some(WPARAM(SCF_DEFAULT as usize)),
                Some(LPARAM((&base as *const CHARFORMATW) as isize)),
            )
            .0 == 0
            {
                return Err(failure("Native paragraph presentation failed"));
            }
        }
        let length = self.text_utf16()?.len();
        self.decorate(|document| unsafe {
            let font = document.Range(0, length as i32)?.GetFont()?;
            font.SetSize(f32::from(size) * 0.75)?;
            font.SetForeColor(color(palette.ink))
        })?;
        self.presentation.set(Some((size, light)));
        Ok(())
    }
    pub fn make_borderless(&self) -> Result<()> {
        unsafe {
            let style = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                self.hwnd,
                GWL_EXSTYLE,
                style & !(WS_EX_CLIENTEDGE.0 as isize),
            );
            SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )
        }
    }

    /// Retain exact UTF-16, including an interim unpaired surrogate. No lossy
    /// decoding or implicit CRLF normalization on a failed projection.
    pub fn text_utf16(&self) -> Result<Vec<u16>> {
        unsafe {
            let length_options = GETTEXTLENGTHEX {
                flags: GTL_PRECISE | GTL_NUMCHARS,
                codepage: 1200,
            };
            let length = SendMessageW(
                self.hwnd,
                EM_GETTEXTLENGTHEX,
                Some(WPARAM((&length_options as *const GETTEXTLENGTHEX) as usize)),
                None,
            )
            .0;
            if !(0..=MAX_PARAGRAPH_UNITS as isize).contains(&length) {
                return Err(failure("Native paragraph exceeds bound"));
            }
            let mut text = vec![0u16; length as usize + 1];
            let options = GETTEXTEX {
                cb: (text.len() * 2) as u32,
                flags: GT_RAWTEXT,
                codepage: 1200,
                ..Default::default()
            };
            let count = SendMessageW(
                self.hwnd,
                EM_GETTEXTEX,
                Some(WPARAM((&options as *const GETTEXTEX) as usize)),
                Some(LPARAM(text.as_mut_ptr() as isize)),
            )
            .0;
            if count != length {
                return Err(failure("Incomplete native text read"));
            }
            text.truncate(count as usize);
            Ok(text)
        }
    }
    pub fn selection(&self) -> Range<usize> {
        unsafe {
            let mut range = CHARRANGE::default();
            SendMessageW(
                self.hwnd,
                EM_EXGETSEL,
                None,
                Some(LPARAM((&mut range as *mut CHARRANGE) as isize)),
            );
            range.cpMin.max(0) as usize..range.cpMax.max(0) as usize
        }
    }
    /// Used for initial recovery and receiver-local diagnostics, not decoration.
    pub fn select(&self, range: Range<usize>) -> Result<()> {
        let text = self.text_utf16()?;
        let decoded =
            String::from_utf16(&text).map_err(|_| failure("Selection requires valid UTF-16"))?;
        if range.start > range.end
            || crate::inline_projection::utf16_to_byte(&decoded, range.start).is_none()
            || crate::inline_projection::utf16_to_byte(&decoded, range.end).is_none()
        {
            return Err(failure("Selection cuts a surrogate pair"));
        }
        let native = CHARRANGE {
            cpMin: range.start as i32,
            cpMax: range.end as i32,
        };
        unsafe {
            SendMessageW(
                self.hwnd,
                EM_EXSETSEL,
                None,
                Some(LPARAM((&native as *const CHARRANGE) as isize)),
            );
        }
        Ok(())
    }
    /// Native undoable replacement; no application-level substitute undo stack.
    pub fn replace_selection(&self, text: &str) -> Result<()> {
        let current = self.text_utf16()?;
        let selected = self.selection();
        let resulting = current.len().saturating_sub(selected.len()) + text.encode_utf16().count();
        if text.contains('\0') || resulting > MAX_PARAGRAPH_UNITS {
            return Err(failure("Invalid native replacement"));
        }
        let text: Vec<u16> = text.encode_utf16().chain([0]).collect();
        unsafe {
            SendMessageW(self.hwnd, EM_STOPGROUPTYPING, None, None);
            SendMessageW(
                self.hwnd,
                EM_REPLACESEL,
                Some(WPARAM(1)),
                Some(LPARAM(text.as_ptr() as isize)),
            );
            SendMessageW(self.hwnd, EM_STOPGROUPTYPING, None, None);
        }
        Ok(())
    }
    /// Startup recovery only, never an input/formatting operation. Keeps even
    /// undecodable UTF-16 in the native control and starts a fresh native history.
    pub fn restore_initial_input(&self, text: &[u16]) -> Result<()> {
        if text.len() > MAX_PARAGRAPH_UNITS || text.contains(&0) {
            return Err(failure("Invalid recovery input"));
        }
        let terminated: Vec<u16> = text.iter().copied().chain([0]).collect();
        unsafe {
            SendMessageW(
                self.hwnd,
                WM_SETTEXT,
                None,
                Some(LPARAM(terminated.as_ptr() as isize)),
            );
            SendMessageW(self.hwnd, EM_EMPTYUNDOBUFFER, None, None);
        }
        if self.text_utf16()? != text {
            return Err(failure(
                "Native recovery text mismatch; recovery file retained",
            ));
        }
        self.show_unprojected()
    }
    pub fn can_undo(&self) -> bool {
        unsafe { SendMessageW(self.hwnd, EM_CANUNDO, None, None).0 != 0 }
    }
    pub fn can_redo(&self) -> bool {
        unsafe { SendMessageW(self.hwnd, EM_CANREDO, None, None).0 != 0 }
    }
    pub fn undo(&self) -> bool {
        unsafe { SendMessageW(self.hwnd, EM_UNDO, None, None).0 != 0 }
    }
    pub fn redo(&self) -> bool {
        unsafe { SendMessageW(self.hwnd, EM_REDO, None, None).0 != 0 }
    }

    pub fn set_base_bold(&self, bold: bool) -> Result<()> {
        if self.base_bold.replace(bold) != bold && bold {
            let length = self.text_utf16()?.len();
            self.decorate(|doc| unsafe {
                doc.Range(0, length as i32)?.GetFont()?.SetBold(tomTrue.0)
            })?;
        }
        Ok(())
    }
    pub fn toggle_style(&self, bold: bool) -> Result<()> {
        if self.composing() || (bold && self.base_bold.get()) {
            return Ok(());
        }
        unsafe {
            SendMessageW(self.hwnd, EM_STOPGROUPTYPING, None, None);
            let font = self.doc().GetSelection()?.GetFont()?;
            if bold {
                font.SetBold(if font.GetBold()? == tomTrue.0 {
                    tomFalse.0
                } else {
                    tomTrue.0
                })?;
            } else {
                font.SetItalic(if font.GetItalic()? == tomTrue.0 {
                    tomFalse.0
                } else {
                    tomTrue.0
                })?;
            }
            SendMessageW(self.hwnd, EM_STOPGROUPTYPING, None, None);
        }
        Ok(())
    }
    pub fn styled_runs(&self) -> Result<Vec<crate::inline_projection::Styled>> {
        use crate::inline_projection::{Style, Styled, utf16_to_byte};
        let input = self.text_utf16()?;
        let text =
            String::from_utf16(&input).map_err(|_| failure("Incomplete UTF-16 input retained"))?;
        let mut result: Vec<Styled> = Vec::new();
        let mut cursor = 0;
        while cursor < input.len() {
            let byte =
                utf16_to_byte(&text, cursor).ok_or_else(|| failure("Invalid native scalar"))?;
            let next = cursor + text[byte..].chars().next().unwrap().len_utf16();
            unsafe {
                let range = self.doc().Range(cursor as i32, next as i32)?;
                let font = range.GetFont()?;
                let style = Style {
                    bold: font.GetBold()? == tomTrue.0 && !self.base_bold.get(),
                    italic: font.GetItalic()? == tomTrue.0,
                };
                let mut end = next;
                if range.Expand(tomCharFormat.0).is_ok() {
                    let candidate = (range.GetEnd()?.max(next as i32) as usize).min(input.len());
                    if utf16_to_byte(&text, candidate).is_some() {
                        end = candidate;
                    }
                }
                if let Some(previous) = result.last_mut().filter(|run| run.style == style) {
                    previous.utf16.end = end;
                } else {
                    result.push(Styled {
                        utf16: cursor..end,
                        style,
                    });
                }
                cursor = end;
            }
        }
        Ok(result)
    }
    pub fn apply_styles(&self, projection: &Projection) -> Result<()> {
        if self.composing() {
            return Err(failure("Decoration deferred during native composition"));
        }
        if self.text_utf16()? != projection.text.encode_utf16().collect::<Vec<_>>() {
            return Err(failure("Style projection does not match native text"));
        }
        self.decorate(|document| unsafe {
            for run in &projection.runs {
                let font = document
                    .Range(run.utf16.start as i32, run.utf16.end as i32)?
                    .GetFont()?;
                font.SetBold(if run.style.bold || self.base_bold.get() {
                    tomTrue.0
                } else {
                    tomFalse.0
                })?;
                font.SetItalic(if run.style.italic {
                    tomTrue.0
                } else {
                    tomFalse.0
                })?;
            }
            Ok(())
        })
    }
    /// On projection failure, all latest input remains visible, without implying
    /// that its inherited native formatting is safely represented in Markdown.
    pub fn show_unprojected(&self) -> Result<()> {
        if self.composing() {
            return Ok(());
        }
        let length = self.text_utf16()?.len();
        self.decorate(|document| unsafe {
            let font = document.Range(0, length as i32)?.GetFont()?;
            font.SetBold(if self.base_bold.get() {
                tomTrue.0
            } else {
                tomFalse.0
            })?;
            font.SetItalic(tomFalse.0)
        })
    }
    fn decorate(&self, operation: impl FnOnce(&ITextDocument) -> Result<()>) -> Result<()> {
        let document = self.doc();
        // Restore undo recording even if a formatting call fails. No selection
        // writes, WM_SETTEXT, EM_EMPTYUNDOBUFFER or undo limit changes here.
        unsafe {
            let selection = self.selection();
            let insertion = if selection.is_empty() {
                let font = document.GetSelection()?.GetFont()?;
                Some((font.GetBold()?, font.GetItalic()?))
            } else {
                None
            };
            document.Undo(tomSuspend.0)?;
            let result = operation(document);
            let restored = (|| -> Result<()> {
                if let Some((bold, italic)) = insertion {
                    let font = document.GetSelection()?.GetFont()?;
                    font.SetBold(if self.base_bold.get() {
                        tomTrue.0
                    } else {
                        bold
                    })?;
                    font.SetItalic(italic)?;
                }
                Ok(())
            })();
            let resumed = document.Undo(tomResume.0);
            restored?;
            result?;
            resumed?;
            if GetFocus() == self.hwnd {
                SendMessageW(self.hwnd, 0x00b7, None, None);
            } // EM_SCROLLCARET
        }
        Ok(())
    }
    /// Receiver-local TOM inspection: no foreign-process structures/pointers.
    pub fn style_at(&self, range: Range<usize>) -> Result<(i32, i32, f32, i32)> {
        unsafe {
            let font = self
                .doc()
                .Range(range.start as i32, range.end as i32)?
                .GetFont()?;
            Ok((
                font.GetBold()?,
                font.GetItalic()?,
                font.GetSize()?,
                font.GetForeColor()?,
            ))
        }
    }
}
impl Drop for Paragraph {
    fn drop(&mut self) {
        unsafe {
            let _ = RemoveWindowSubclass(self.hwnd, Some(subclass), 1);
            self.document.take();
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
