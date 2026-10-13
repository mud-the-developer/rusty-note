//! Native Unicode editor: Windows supplies selection, undo, clipboard and IME.
use crate::edit::EditBuffer;
use std::path::Path;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, Result, w},
};

pub struct Editor {
    pub hwnd: HWND,
    font: HFONT,
    pub buffer: EditBuffer,
}
impl Editor {
    pub fn open(parent: HWND, path: &Path, width: f32, height: f32, scale: f32) -> Result<Self> {
        let buffer = EditBuffer::open(path)
            .map_err(|error| windows::core::Error::new(E_FAIL, error.to_string()))?;
        let text: Vec<u16> = buffer
            .text
            .replace("\r\n", "\n")
            .replace('\n', "\r\n")
            .encode_utf16()
            .chain([0])
            .collect();
        unsafe {
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                PCWSTR(text.as_ptr()),
                WS_CHILD
                    | WS_VISIBLE
                    | WS_VSCROLL
                    | WS_TABSTOP
                    | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32),
                (24.0 * scale) as i32,
                (112.0 * scale) as i32,
                ((width - 48.0) * scale) as i32,
                ((height - 160.0) * scale) as i32,
                Some(parent),
                Some(HMENU(100usize as *mut _)),
                None,
                None,
            )?;
            let font = CreateFontW(
                -(16.0 * scale) as i32,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                w!("Cascadia Mono"),
            );
            if !font.0.is_null() {
                SendMessageW(
                    hwnd,
                    WM_SETFONT,
                    Some(WPARAM(font.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
            SendMessageW(
                hwnd,
                0x00c5,
                Some(WPARAM(crate::edit::MAX_NOTE_BYTES as usize)),
                None,
            ); // EM_SETLIMITTEXT
            let _ = SetFocus(Some(hwnd));
            Ok(Self { hwnd, font, buffer })
        }
    }
    pub fn resize(&self, width: f32, height: f32, scale: f32) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                None,
                (24.0 * scale) as i32,
                (112.0 * scale) as i32,
                ((width - 48.0).max(100.0) * scale) as i32,
                ((height - 160.0).max(100.0) * scale) as i32,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
    pub fn sync(&mut self) {
        unsafe {
            let length = GetWindowTextLengthW(self.hwnd).max(0) as usize;
            let mut text = vec![0u16; length + 1];
            let count = GetWindowTextW(self.hwnd, &mut text).max(0) as usize;
            self.buffer.text = String::from_utf16_lossy(&text[..count]).replace("\r\n", "\n");
        }
    }
    pub fn save(&mut self) -> std::io::Result<()> {
        self.sync();
        self.buffer.save()
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            if !self.font.0.is_null() {
                let _ = DeleteObject(HGDIOBJ(self.font.0));
            }
        }
    }
}
