//! Experimental main-window paragraph integration; the default reader stays unchanged.
use super::*;
use crate::{inline::Paragraph, inline_projection::byte_to_utf16, inline_session};
use std::ops::Range;

pub(super) const CONTROL_ID: usize = 101;
pub(super) const SYNC: u32 = WM_APP + 0x20;
pub(super) const PROBE: u32 = WM_APP + 0x21;
pub(super) const PAINT: u32 = WM_APP + 0x22;
pub(super) const CHECKPOINT: usize = 3;

#[derive(Default)]
pub(super) struct InlineState {
    pub note: Option<inline_session::Note>,
    pub control: Option<Paragraph>,
    pub timer: bool,
    pub(super) blocked: bool,
    composing: bool,
    external_change: bool,
    layout: Option<[i32; 6]>,
    viewport: Option<(u32, u32, u32)>,
    paint_queued: bool,
    force_style: bool,
}
fn failure(message: impl ToString) -> windows::core::Error {
    windows::core::Error::new(E_FAIL, message.to_string())
}
impl App {
    pub(super) fn queue_inline_paint(&mut self) {
        if self.inline.control.is_some() && !self.inline.paint_queued {
            // Restore the GDI child after a D2D parent update, outside nested paint.
            self.inline.paint_queued =
                unsafe { PostMessageW(Some(self.hwnd), PAINT, WPARAM(0), LPARAM(0)).is_ok() };
        }
    }
    pub(super) fn repaint_inline_child(&mut self) {
        self.inline.paint_queued = false;
        if let Some(control) = &self.inline.control {
            unsafe {
                let _ = RedrawWindow(
                    Some(control.hwnd),
                    None,
                    None,
                    RDW_INVALIDATE | RDW_UPDATENOW,
                );
            }
        }
    }
    pub(super) fn open_image_at(&mut self, x: f32, y: f32) -> bool {
        if y < 106.0 || y > self.height - 46.0 {
            return false;
        }
        let local_y = y - 116.0 + self.scroll;
        let source = self
            .scene
            .images
            .iter()
            .find(|image| {
                x >= self.content_x()
                    && x <= self.content_x() + image.width
                    && local_y >= image.y
                    && local_y < image.y + image.height
            })
            .map(|image| image.source.clone());
        let Some(source) = source else { return false };
        if !self.end_inline_paragraph() {
            return true;
        }
        if let Some(previous) = self
            .image_popup
            .filter(|hwnd| crate::image_viewer::is_view(*hwnd))
        {
            unsafe {
                let _ = DestroyWindow(previous);
            }
        }
        match crate::image_viewer::open(
            self.hwnd,
            self.factory.clone(),
            self.options.notes_dir.clone(),
            source,
            self.state.light,
        ) {
            Ok(hwnd) => self.image_popup = Some(hwnd),
            Err(error) => self.inline_error(error),
        }
        true
    }
    pub(super) fn new_inline_note(&mut self) {
        if !self.leave_inline(false) {
            return;
        }
        let result = (|| -> Result<()> {
            fs::create_dir_all(&self.options.notes_dir).map_err(failure)?;
            let path = crate::edit::create_note(&self.options.notes_dir).map_err(failure)?;
            self.open_markdown(path, true);
            self.start_empty_tail();
            Ok(())
        })();
        if let Err(error) = result {
            self.inline_error(error);
        }
    }
    fn start_empty_tail(&mut self) {
        if !self.end_inline_paragraph() {
            return;
        }
        let result = (|| -> Result<()> {
            self.ensure_inline_note()?;
            let note = self.inline.note.as_mut().unwrap();
            let before = note.text().replace("\r\n", "\n");
            if !before
                .trim_matches(['\u{feff}', '\r', '\n', ' ', '\t'])
                .is_empty()
                && !before.ends_with("\n\n")
            {
                let newline = note.newline();
                let separator = if before.ends_with('\n') {
                    newline.to_owned()
                } else {
                    newline.repeat(2)
                };
                let end = note.text().len();
                note.apply_edit(end..end, &separator).map_err(failure)?;
            }
            let end = note.text().len();
            note.activate(end..end).map_err(failure)?;
            self.rebuild_inline_scene();
            self.show_inline_control(Some(0..0))
        })();
        if let Err(error) = result {
            self.inline_error(error);
        }
    }
    fn retarget_after_break(&mut self) -> Result<()> {
        let Some(note) = self.inline.note.as_ref() else {
            return Ok(());
        };
        let Some(active) = note.active.as_ref() else {
            return Ok(());
        };
        if active.error.is_some() || !active.selection.is_empty() || !active.input.contains(&13) {
            return Ok(());
        }
        let row = active.input[..active.selection.start]
            .iter()
            .filter(|unit| **unit == 13)
            .count();
        let column = active.input[..active.selection.start]
            .iter()
            .rposition(|unit| *unit == 13)
            .map_or(active.selection.start, |at| active.selection.start - at - 1);
        let prefix = note.continuation_prefix();
        let separator = if prefix.is_empty() {
            note.newline().repeat(2)
        } else {
            format!("{}{prefix}", note.newline())
        };
        let parts: Vec<_> = active.projection.source.split(&separator).collect();
        if row >= parts.len() {
            return Err(failure(
                "New line coordinates need review; working text retained",
            ));
        }
        let start = active.source.start
            + parts[..row]
                .iter()
                .map(|part| part.len() + separator.len())
                .sum::<usize>();
        let range = start..start + parts[row].len();
        let note = self.inline.note.as_mut().unwrap();
        note.deactivate().map_err(failure)?;
        note.activate(range).map_err(failure)?;
        self.inline.control.take();
        self.inline.layout = None;
        self.rebuild_inline_scene();
        self.show_inline_control(Some(column..column))
    }
    pub(super) fn toggle_inline_style(&mut self, bold: bool) {
        let result = (|| -> Result<()> {
            let Some(control) = &self.inline.control else {
                return Ok(());
            };
            let selection = control.selection();
            control.toggle_style(bold)?;
            if !selection.is_empty() {
                self.inline.force_style = true;
                self.sync_inline();
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.inline_error(error);
        }
    }
    pub(super) fn undo_inline(&mut self, redo: bool) {
        if let Some(control) = &self.inline.control
            && if redo {
                control.can_redo()
            } else {
                control.can_undo()
            }
        {
            if redo {
                control.redo();
            } else {
                control.undo();
            }
            self.inline.force_style = true;
            self.sync_inline();
            return;
        }
        if !self.end_inline_paragraph() {
            return;
        }
        if let Some(note) = &mut self.inline.note {
            match note.undo(redo) {
                Ok(true) => {
                    self.rebuild_inline_scene();
                    self.checkpoint_inline();
                }
                Ok(false) => {}
                Err(error) => self.inline_error(error),
            }
        }
    }
    pub(super) fn toggle_inline_task(&mut self, x: f32, y: f32) -> bool {
        let local_y = y - 116.0 + self.scroll;
        let task = self.scene.tasks.iter().find_map(|(origin, task)| {
            let line = self
                .scene
                .lines
                .iter()
                .find(|line| line.origin == *origin)?;
            let width = self
                .scene
                .edit_prefixes
                .iter()
                .find(|(id, _, _)| id == origin)?
                .2;
            (local_y >= line.y
                && local_y < line.y + line.height
                && x >= self.content_x() + line.x
                && x < self.content_x() + line.x + width)
                .then_some(task.clone())
        });
        let Some(task) = task else { return false };
        if !self.end_inline_paragraph() {
            return true;
        }
        let result = (|| -> Result<()> {
            self.ensure_inline_note()?;
            let note = self.inline.note.as_mut().unwrap();
            let value = note
                .text()
                .get(task.marker.clone())
                .ok_or_else(|| failure("Stale checkbox coordinates"))?;
            if !matches!(value, " " | "x" | "X") {
                return Err(failure("Invalid checkbox source"));
            }
            let replacement = if value == " " { "x" } else { " " };
            note.apply_edit(task.marker, replacement).map_err(failure)?;
            self.rebuild_inline_scene();
            self.checkpoint_inline();
            Ok(())
        })();
        if let Err(error) = result {
            self.inline_error(error);
        }
        true
    }
    fn inline_error(&mut self, error: impl ToString) {
        self.error = Some(error.to_string());
        self.inline_invalidate();
    }
    fn focus_inline_control(&self) {
        if let Some(control) = &self.inline.control {
            unsafe {
                if IsWindowVisible(control.hwnd).as_bool() {
                    let _ = SetFocus(Some(control.hwnd));
                    SendMessageW(control.hwnd, 0x00b7, None, None); // EM_SCROLLCARET
                }
            }
        }
    }
    fn inline_invalidate(&self) {
        if !self.hwnd.0.is_null() {
            unsafe {
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        }
    }
    fn rebuild_inline_scene(&mut self) {
        if let Some(note) = &self.inline.note {
            let text = note.text().to_owned();
            let mut document = Document::parse(&text);
            if let Some(active) = note
                .active
                .as_ref()
                .filter(|active| active.source.is_empty())
            {
                document.insert_edit_slot(active.source.start);
            }
            self.scene = Scene::with_zoom(&document, &self.fonts, self.state.zoom);
            self.viewer_source = Some(text);
            self.reflow();
        }
        self.inline_invalidate();
    }
    pub(super) fn restore_inline_note(&mut self) {
        if !self.options.inline_edit || self.inline.note.is_some() {
            return;
        }
        let Some(reference) = self.state.inline_draft.clone() else {
            return;
        };
        let Some(path) = self.options.file.as_ref() else {
            return;
        };
        let identity = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        if identity != reference.original {
            self.inline.blocked = true;
            self.inline_error(format!("A previous draft is retained. Reopen {} before editing another note, or explicitly discard its association.", reference.original.display()));
            return;
        }
        match inline_session::Note::reopen(path, &self.options.state_file, &reference) {
            Ok(note) => {
                self.options.file = Some(reference.original.clone());
                if let Some(parent) = reference.original.parent() {
                    self.options.notes_dir = parent.to_owned();
                }
                self.inline.blocked = false;
                self.inline.external_change = note.original_changed().unwrap_or(true);
                let message = note.recovery_error.clone().unwrap_or_else(|| {
                    if self.inline.external_change { "Recovered working draft; original changed externally. Conflicting Save is blocked.".into() }
                    else { "Recovered working draft. Ctrl+S explicitly saves the original; pending input must be repaired first.".into() }
                });
                self.inline.note = Some(note);
                self.rebuild_inline_scene();
                self.inline_error(message);
                if !self.hwnd.0.is_null() {
                    self.restore_inline_control();
                }
            }
            Err(error) => {
                self.inline.blocked = true;
                self.inline_error(format!("Draft recovery failed; files retained: {error}"));
            }
        }
    }
    fn ensure_inline_note(&mut self) -> Result<()> {
        if self.inline.note.is_some() {
            return Ok(());
        }
        if self.state.inline_draft.is_some() {
            self.restore_inline_note();
            if self.inline.note.is_some() {
                return Ok(());
            }
            return Err(failure(
                "Previous recovery data must be reviewed or explicitly discarded first",
            ));
        }
        if self.inline.blocked {
            return Err(failure("Recovery data needs review"));
        }
        let path = self
            .options
            .file
            .as_ref()
            .ok_or_else(|| failure("Open a Markdown note first"))?;
        let source = self
            .viewer_source
            .as_deref()
            .ok_or_else(|| failure("No loaded source"))?;
        let note = inline_session::Note::create(path, &self.options.state_file, source)
            .map_err(failure)?;
        let previous = self.state.clone();
        self.state.inline_draft = Some(note.reference());
        self.state.selected = Some(path.clone());
        self.state.notes_dir = Some(self.options.notes_dir.clone());
        if let Err(error) = self.state.save(&self.options.state_file) {
            self.state = previous;
            return Err(failure(format!(
                "Could not persist draft association; original unchanged: {error}"
            )));
        }
        self.inline.note = Some(note);
        Ok(())
    }
    fn active_origin(&self) -> Option<usize> {
        let source = &self.inline.note.as_ref()?.active.as_ref()?.source;
        self.scene
            .paragraph_sources
            .iter()
            .find_map(|(origin, range)| {
                (range.start == source.start && range.end <= source.end).then_some(*origin)
            })
    }
    pub(super) fn inline_origins(&self) -> Vec<usize> {
        if self.inline.control.is_none() {
            return Vec::new();
        }
        let Some(active) = self
            .inline
            .note
            .as_ref()
            .and_then(|note| note.active.as_ref())
        else {
            return Vec::new();
        };
        self.scene
            .paragraph_sources
            .iter()
            .filter_map(|(origin, range)| {
                (range.start >= active.source.start && range.end <= active.source.end)
                    .then_some(*origin)
            })
            .collect()
    }
    fn show_inline_control(&mut self, selection: Option<Range<usize>>) -> Result<()> {
        let active = self
            .inline
            .note
            .as_ref()
            .and_then(|note| note.active.as_ref())
            .cloned()
            .ok_or_else(|| failure("No active paragraph"))?;
        let control = Paragraph::create(
            self.hwnd,
            CONTROL_ID,
            SYNC,
            &active.projection,
            self.state.light,
        )?;
        control.make_borderless()?;
        if active.input != active.projection.text.encode_utf16().collect::<Vec<_>>() {
            control.restore_initial_input(&active.input)?;
        }
        if String::from_utf16(&active.input).is_ok() {
            control.select(selection.unwrap_or(active.selection))?;
        }
        self.inline.control = Some(control);
        self.inline.layout = None;
        self.inline.composing = false;
        self.position_inline();
        self.focus_inline_control();
        self.inline_invalidate();
        Ok(())
    }
    pub(super) fn restore_inline_control(&mut self) {
        if !self.options.inline_edit || self.inline.control.is_some() {
            return;
        }
        if self
            .inline
            .note
            .as_ref()
            .is_some_and(|note| note.active.is_some() && note.recovery_error.is_none())
            && let Err(error) = self.show_inline_control(None)
        {
            self.inline_error(error);
        }
    }
    fn hit_inline(&self, x: f32, y: f32) -> Result<(Range<usize>, usize)> {
        let local_y = y - 116.0 + self.scroll;
        let line = self.scene.lines.iter().find(|line| matches!(line.kind, Kind::Paragraph | Kind::Heading(_) | Kind::List)
            && local_y >= line.y && local_y < line.y + line.height)
            .ok_or_else(|| failure("Click supported paragraph text; this first integration does not edit other block types"))?;
        let source = self
            .scene
            .paragraph_sources
            .iter()
            .find_map(|(origin, source)| (*origin == line.origin).then_some(source.clone()))
            .ok_or_else(|| failure("This paragraph has no lossless source coordinates"))?;
        let raw = self
            .viewer_source
            .as_deref()
            .and_then(|text| text.get(source.clone()))
            .ok_or_else(|| failure("Stale source coordinates"))?;
        let projection = crate::inline_projection::Projection::parse(raw).map_err(failure)?;
        let mut trailing = windows::core::BOOL::default();
        let mut inside = windows::core::BOOL::default();
        let mut hit = DWRITE_HIT_TEST_METRICS::default();
        unsafe {
            self.line_layout(line)?.HitTestPoint(
                x - self.content_x() - line.x,
                local_y - line.y,
                &mut trailing,
                &mut inside,
                &mut hit,
            )?;
        }
        let marker = self
            .scene
            .edit_prefixes
            .iter()
            .find(|(origin, _, _)| *origin == line.origin)
            .map(|(_, text, _)| text.as_str())
            .unwrap_or("");
        let visible = format!("{marker}{}", projection.text);
        let prefix = byte_to_utf16(&visible, line.source_start)
            .ok_or_else(|| failure("Invalid rendered UTF-8 boundary"))?;
        let caret = prefix
            + hit.textPosition as usize
            + if trailing.as_bool() {
                hit.length as usize
            } else {
                0
            };
        Ok((
            source,
            caret
                .saturating_sub(marker.encode_utf16().count())
                .min(projection.text.encode_utf16().count()),
        ))
    }
    pub(super) fn start_inline_at(&mut self, x: f32, y: f32) {
        if self.options.file.is_none() {
            self.new_inline_note();
            return;
        }
        let local_y = y - 116.0 + self.scroll;
        if local_y >= self.scene.height || self.scene.lines.is_empty() {
            self.start_empty_tail();
            return;
        }
        if !self.sync_inline() {
            return;
        }
        let result = (|| -> Result<()> {
            let (source, caret) = self.hit_inline(x, y)?;
            if self
                .inline
                .note
                .as_ref()
                .and_then(|note| note.active.as_ref())
                .is_some_and(|active| active.source == source)
                && let Some(control) = self.inline.control.as_ref()
            {
                if !self.inline.note.as_ref().unwrap().pending() {
                    control.select(caret..caret)?;
                }
                unsafe {
                    let _ = SetFocus(Some(control.hwnd));
                }
                return Ok(());
            }
            if !self.end_inline_paragraph() {
                return Err(failure(
                    "Pending paragraph input must be repaired before moving",
                ));
            }
            self.ensure_inline_note()?;
            self.inline
                .note
                .as_mut()
                .unwrap()
                .activate(source)
                .map_err(failure)?;
            self.rebuild_inline_scene();
            self.show_inline_control(Some(caret..caret))
        })();
        if let Err(error) = result {
            self.inline_error(error);
        }
    }
    pub(super) fn position_inline(&mut self) {
        let Some(control) = &self.inline.control else {
            return;
        };
        let Some(origin) = self.active_origin() else {
            return;
        };
        let Some(first) = self.scene.lines.iter().find(|line| line.origin == origin) else {
            return;
        };
        let owned = self.inline_origins();
        let marker_width = self
            .scene
            .edit_prefixes
            .iter()
            .find(|(id, _, _)| *id == origin)
            .map(|(_, _, width)| *width)
            .unwrap_or(0.0);
        let bottom = self
            .scene
            .lines
            .iter()
            .filter(|line| owned.contains(&line.origin))
            .map(|line| line.y + line.height)
            .fold(first.y, f32::max);
        let viewport = (
            self.width.to_bits(),
            self.height.to_bits(),
            self.state.zoom.to_bits(),
        );
        let focused = unsafe { GetFocus() == control.hwnd };
        if self.inline.viewport != Some(viewport) && focused {
            let top = 116.0 + first.y - self.scroll;
            if top < 106.0 || top + (bottom - first.y + 4.0) > self.height - 46.0 {
                self.scroll = crate::viewer::reload_scroll(
                    first.y - 8.0,
                    self.scene.height,
                    self.height - 160.0,
                );
            }
        }
        self.inline.viewport = Some(viewport);
        let x = ((self.content_x() + first.x + marker_width) * self.scale).round() as i32;
        let top_dip = 116.0 + first.y - self.scroll;
        let y = (top_dip * self.scale).round() as i32;
        let width = ((self.width - self.content_x() - first.x - marker_width - 28.0) * self.scale)
            .round()
            .max(1.0) as i32;
        // The native control must know its visible height so it can scroll its caret.
        let available = if top_dip >= 106.0 {
            self.height - 46.0 - top_dip
        } else {
            self.height - 152.0
        };
        let height = ((bottom - first.y + 4.0).min(available.max(1.0)) * self.scale)
            .round()
            .max(1.0) as i32;
        let top = ((106.0 * self.scale).round() as i32 - y).max(0);
        let end = (((self.height - 46.0) * self.scale).round() as i32 - y).min(height);
        let layout = [x, y, width, height, top, end];
        let result = (|| -> Result<()> {
            control.set_base_bold(first.font.bold)?;
            control.set_presentation(first.font.size, self.state.light)?;
            if self.inline.layout == Some(layout) {
                return Ok(());
            }
            control.resize(RECT {
                left: x,
                top: y,
                right: x + width,
                bottom: y + height,
            })?;
            unsafe {
                if end <= top {
                    let _ = ShowWindow(control.hwnd, SW_HIDE);
                    if GetFocus() == control.hwnd {
                        let _ = SetFocus(Some(self.hwnd));
                    }
                } else {
                    let region = CreateRectRgn(0, top, width, end);
                    if region.0.is_null() {
                        return Err(failure(format!(
                            "Inline clip allocation failed: {}",
                            GetLastError().0
                        )));
                    }
                    if SetWindowRgn(control.hwnd, Some(region), true) == 0 {
                        let code = GetLastError().0;
                        let _ = DeleteObject(HGDIOBJ(region.0));
                        return Err(failure(format!("Inline clip update failed: {code}")));
                    }
                    let _ = ShowWindow(control.hwnd, SW_SHOWNOACTIVATE);
                    if focused {
                        SendMessageW(control.hwnd, 0x00b7, None, None);
                    } // EM_SCROLLCARET
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.inline.layout = Some(layout),
            Err(error) => self.inline_error(error),
        }
    }
    fn schedule_inline(&mut self) -> Result<()> {
        if !self.inline.timer {
            if unsafe { SetTimer(Some(self.hwnd), CHECKPOINT, 500, None) } == 0 {
                return Err(failure(
                    "Draft timer unavailable; explicitly checkpoint/Save before closing",
                ));
            }
            self.inline.timer = true;
        }
        Ok(())
    }
    fn cancel_inline_timer(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), CHECKPOINT);
        }
        self.inline.timer = false;
    }
    pub(super) fn sync_inline(&mut self) -> bool {
        let result = (|| -> Result<()> {
            let Some(control) = &self.inline.control else {
                return Ok(());
            };
            let input = control.text_utf16()?;
            let selection = control.selection();
            let composing = control.composing();
            let Some(active) = self
                .inline
                .note
                .as_mut()
                .and_then(|note| note.active.as_mut())
            else {
                return Err(failure("Native input has no working-copy binding"));
            };
            let force_style = std::mem::take(&mut self.inline.force_style);
            if !force_style && active.input == input && self.inline.composing == composing {
                active.selection = selection;
                return Ok(());
            }
            self.inline.composing = composing;
            let styles = if composing {
                Ok(Vec::new())
            } else {
                self.inline.control.as_ref().unwrap().styled_runs()
            };
            let changed = match styles {
                Ok(styles) => self
                    .inline
                    .note
                    .as_mut()
                    .unwrap()
                    .sync_styled(input, selection, composing, &styles),
                Err(error) => self.inline.note.as_mut().unwrap().retain_input(
                    input,
                    selection,
                    error.to_string(),
                ),
            }
            .map_err(failure)?;
            let active = self.inline.note.as_ref().unwrap().active.as_ref().unwrap();
            let control = self.inline.control.as_ref().unwrap();
            if !composing {
                if active.error.is_some() {
                    control.show_unprojected()?;
                } else {
                    control.apply_styles(&active.projection)?;
                }
            }
            self.schedule_inline()?;
            if changed {
                self.rebuild_inline_scene();
                self.retarget_after_break()?;
            }
            if !composing {
                if let Some(error) = self
                    .inline
                    .note
                    .as_ref()
                    .and_then(|note| note.active.as_ref())
                    .and_then(|active| active.error.clone())
                {
                    self.error = Some(format!("Input retained; original Save blocked: {error}"));
                } else if !self.inline.external_change {
                    self.error = None;
                }
            } else if !self.inline.external_change {
                self.error = None;
            }
            self.position_inline();
            self.inline_invalidate();
            Ok(())
        })();
        match result {
            Ok(()) => true,
            Err(error) => {
                self.inline_error(error);
                false
            }
        }
    }
    pub(super) fn checkpoint_inline(&mut self) -> bool {
        let synced = self.sync_inline();
        self.cancel_inline_timer();
        if !synced {
            return false;
        }
        if let Some(note) = &mut self.inline.note
            && let Err(error) = note.checkpoint()
        {
            self.inline_error(format!(
                "Draft checkpoint failed; input/files retained: {error}"
            ));
            return false;
        }
        true
    }
    pub(super) fn end_inline_paragraph(&mut self) -> bool {
        if !self.sync_inline() {
            return false;
        }
        self.cancel_inline_timer();
        if let Some(note) = &mut self.inline.note
            && note.active.is_some()
            && let Err(error) = note.deactivate()
        {
            self.inline_error(error);
            return false;
        }
        let focused = self
            .inline
            .control
            .as_ref()
            .is_some_and(|control| unsafe { GetFocus() == control.hwnd });
        self.inline.control.take();
        if focused {
            unsafe {
                let _ = SetFocus(Some(self.hwnd));
            }
        }
        self.inline.layout = None;
        self.inline.composing = false;
        self.inline_invalidate();
        true
    }
    pub(super) fn save_inline(&mut self) -> bool {
        if !self.sync_inline() {
            return false;
        }
        self.cancel_inline_timer();
        if self
            .inline
            .control
            .as_ref()
            .is_some_and(Paragraph::composing)
        {
            self.inline_error("Finish IME composition before saving");
            return false;
        }
        let Some(note) = &mut self.inline.note else {
            return !self.inline.blocked;
        };
        match note.save_original() {
            Ok(outcome) => {
                self.inline.external_change = false;
                self.error = outcome.recovery_error.map(|error| {
                    format!("Original saved, but recovery update failed. Files retained: {error}")
                });
                self.inline_invalidate();
                true
            }
            Err(error) => {
                self.inline_error(format!(
                    "Original Save refused; working input retained: {error}"
                ));
                false
            }
        }
    }
    fn clear_inline_association(&mut self) -> bool {
        let previous = self.state.inline_draft.take();
        if let Err(error) = self.state.save(&self.options.state_file) {
            self.state.inline_draft = previous;
            self.inline_error(format!(
                "Cannot update recovery association; draft retained: {error}"
            ));
            return false;
        }
        self.cancel_inline_timer();
        self.inline.control.take();
        self.inline.note.take();
        self.inline.blocked = false;
        self.inline.external_change = false;
        self.inline.layout = None;
        true
    }
    fn discard_inline(&mut self, closing: bool) -> bool {
        let source = if closing {
            None
        } else {
            match self
                .options
                .file
                .as_ref()
                .map(|path| crate::edit::read_note(path))
            {
                Some(Ok(source)) => Some(source),
                Some(Err(error)) => {
                    self.inline_error(error);
                    return false;
                }
                None => None,
            }
        };
        if !self.clear_inline_association() {
            return false;
        }
        if let Some(source) = source {
            self.viewer_source = Some(source.clone());
            self.scene = Scene::with_zoom(&Document::parse(&source), &self.fonts, self.state.zoom);
            self.error = None;
            self.reflow();
            self.inline_invalidate();
        }
        true
    }
    pub(super) fn confirm_discard_inline(&mut self) {
        let answer = unsafe {
            MessageBoxW(
                Some(self.hwnd),
                w!(
                    "Discard the active draft association? The original is unchanged. Recovery files are retained for manual review."
                ),
                w!("Discard working draft"),
                MB_YESNO | MB_ICONQUESTION,
            )
        };
        if answer == IDYES {
            self.discard_inline(false);
        }
    }
    pub(super) fn leave_inline(&mut self, closing: bool) -> bool {
        if !self.options.inline_edit {
            return true;
        }
        if !self.sync_inline() {
            return false;
        }
        if self
            .inline
            .control
            .as_ref()
            .is_some_and(Paragraph::composing)
        {
            self.inline_error("Finish IME composition before leaving this note");
            return false;
        }
        let Some(note) = &self.inline.note else {
            return true;
        };
        if note.pending() {
            if !closing {
                self.inline_error(
                    "Repair or explicitly discard pending input before opening another note",
                );
                return false;
            }
            let answer = unsafe {
                MessageBoxW(
                    Some(self.hwnd),
                    w!(
                        "Input cannot safely be saved as Markdown yet.\nYes: checkpoint and close keeping the draft.\nNo: discard its active association (recovery files remain).\nCancel: keep editing."
                    ),
                    w!("Pending input"),
                    MB_YESNOCANCEL | MB_ICONQUESTION,
                )
            };
            return if answer == IDYES {
                self.checkpoint_inline()
            } else if answer == IDNO {
                self.discard_inline(true)
            } else {
                self.focus_inline_control();
                false
            };
        }
        if note.dirty() {
            let answer = unsafe {
                MessageBoxW(
                    Some(self.hwnd),
                    w!(
                        "Save working changes to the original?\nYes: Save with backup and conflict checks.\nNo: discard active edits; recovery files are retained.\nCancel: keep editing."
                    ),
                    w!("Working draft"),
                    MB_YESNOCANCEL | MB_ICONQUESTION,
                )
            };
            if answer == IDCANCEL {
                self.focus_inline_control();
                return false;
            }
            if answer == IDNO {
                return self.discard_inline(closing);
            }
            if !self.save_inline() {
                self.focus_inline_control();
                return false;
            }
            if let Some(error) = &self.error {
                let text: Vec<u16> = error.encode_utf16().chain([0]).collect();
                unsafe {
                    MessageBoxW(
                        Some(self.hwnd),
                        PCWSTR(text.as_ptr()),
                        w!("Original saved — recovery warning"),
                        MB_OK | MB_ICONWARNING,
                    );
                }
            }
        }
        self.clear_inline_association()
    }
    pub(super) fn prepare_inline_open(&mut self, path: &std::path::Path, explicit: bool) -> bool {
        if !self.sync_inline() {
            return false;
        }
        let same =
            self.options.file.as_ref().is_some_and(|old| {
                path == old || fs::canonicalize(path).ok().as_ref() == Some(old)
            });
        if same && let Some(note) = &self.inline.note {
            let changed = note.original_changed().unwrap_or(true);
            if note.dirty() || !changed {
                if changed {
                    self.inline.external_change = true;
                    self.inline_error("Original changed externally; working draft kept. Conflicting Save is blocked.");
                } else if explicit && note.dirty() {
                    self.inline_error(
                        "Working draft kept. Save or explicitly discard it before reloading.",
                    );
                }
                return false;
            }
            return self.clear_inline_association();
        }
        if !same && self.inline.note.is_some() {
            return self.leave_inline(false);
        }
        true
    }
    /// Receiver-local WM_PRINTCLIENT: foreign-process HDCs are unreliable for RichEdit.
    /// Only the explicit diagnostics route can write this bounded, owned client BMP.
    fn capture_inline_client(&self) -> Result<()> {
        let control = self
            .inline
            .control
            .as_ref()
            .ok_or_else(|| failure("No inline control"))?;
        let path = self
            .options
            .metrics_file
            .as_ref()
            .ok_or_else(|| failure("Diagnostics are disabled"))?
            .with_extension("inline.bmp");
        unsafe {
            let mut bounds = RECT::default();
            GetClientRect(control.hwnd, &mut bounds)?;
            let (width, height) = (bounds.right, bounds.bottom);
            if width <= 0 || height <= 0 || i64::from(width) * i64::from(height) > 4_000_000 {
                return Err(failure("Native client capture exceeds pixel bound"));
            }
            let length = width as usize * height as usize * 4;
            let screen = GetDC(Some(control.hwnd));
            if screen.0.is_null() {
                return Err(failure("Native client DC unavailable"));
            }
            let memory = CreateCompatibleDC(Some(screen));
            if memory.0.is_null() {
                ReleaseDC(Some(control.hwnd), screen);
                return Err(failure("Capture DC unavailable"));
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: 0,
                    biSizeImage: length as u32,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0);
            ReleaseDC(Some(control.hwnd), screen);
            let bitmap = match bitmap {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    let _ = DeleteDC(memory);
                    return Err(error);
                }
            };
            if bits.is_null() {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                let _ = DeleteDC(memory);
                return Err(failure("Capture pixels unavailable"));
            }
            let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
            if previous.0.is_null() || previous.0 as isize == -1 {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                let _ = DeleteDC(memory);
                return Err(failure("Capture bitmap selection failed"));
            }
            let result = {
                // End the Rust pixel borrow before GDI writes through its own handle.
                {
                    let pixels = std::slice::from_raw_parts_mut(bits.cast::<u8>(), length);
                    let background = Palette::new(self.state.light).card;
                    for pixel in pixels.as_chunks_mut::<4>().0 {
                        *pixel = [
                            background as u8,
                            (background >> 8) as u8,
                            (background >> 16) as u8,
                            255,
                        ];
                    }
                }
                SendMessageW(
                    control.hwnd,
                    WM_PRINTCLIENT,
                    Some(WPARAM(memory.0 as usize)),
                    Some(LPARAM((PRF_CLIENT | PRF_ERASEBKGND) as isize)),
                );
                let _ = GdiFlush();
                // This bitmap still owns the bounded DIB; GDI has completed its writes.
                let pixels = std::slice::from_raw_parts_mut(bits.cast::<u8>(), length);
                for pixel in pixels.as_chunks_mut::<4>().0 {
                    pixel[3] = 255;
                }
                let mut bmp = vec![0u8; 54];
                bmp[..2].copy_from_slice(b"BM");
                bmp[2..6].copy_from_slice(&((54 + length) as u32).to_le_bytes());
                bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
                bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
                bmp[18..22].copy_from_slice(&width.to_le_bytes());
                bmp[22..26].copy_from_slice(&(-height).to_le_bytes());
                bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
                bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
                bmp[34..38].copy_from_slice(&(length as u32).to_le_bytes());
                bmp.extend_from_slice(pixels);
                fs::write(path, bmp).map_err(failure)
            };
            SelectObject(memory, previous);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(memory);
            result
        }
    }
    pub(super) fn inline_probe(&mut self, action: usize, packed: isize) {
        let result = (|| -> Result<()> {
            if action == 5 {
                return self.capture_inline_client();
            }
            if action == 3 {
                self.checkpoint_inline();
                return Ok(());
            }
            if action == 4 {
                self.end_inline_paragraph();
                return Ok(());
            }
            let control = self
                .inline
                .control
                .as_ref()
                .ok_or_else(|| failure("No inline control"))?;
            match action {
                0 => control
                    .select((packed as usize & 0xffff)..((packed as usize >> 16) & 0xffff))?,
                1 => {
                    control.undo();
                }
                2 => {
                    control.redo();
                }
                _ => return Err(failure("Unknown inline diagnostic action")),
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.inline_error(error);
        }
    }
    pub(super) fn inline_metrics(&self) -> serde_json::Value {
        let paragraphs: Vec<_> = self
            .scene
            .paragraph_sources
            .iter()
            .filter_map(|(origin, source)| {
                let first = self
                    .scene
                    .lines
                    .iter()
                    .find(|line| line.origin == *origin)?;
                let supported = self
                    .viewer_source
                    .as_deref()
                    .and_then(|text| text.get(source.clone()))
                    .is_some_and(|text| crate::inline_projection::Projection::parse(text).is_ok());
                Some(
                    serde_json::json!({"origin":origin,"start":source.start,"end":source.end,
                "x":self.content_x()+first.x,"y":116.0+first.y-self.scroll,"height":first.height,
                "supported":supported,"kind":format!("{:?}",first.kind)}),
                )
            })
            .collect();
        let styles: Vec<_> = self.inline.note.as_ref().and_then(|note| note.active.as_ref()).map(|active| {
            active.projection.runs.iter().filter_map(|run| {
                let (bold,italic,size,color) = self.inline.control.as_ref()?.style_at(run.utf16.clone()).ok()?;
                Some(serde_json::json!({"start":run.utf16.start,"end":run.utf16.end,"bold":bold,"italic":italic,"size":size,"color":color}))
            }).collect()
        }).unwrap_or_default();
        let tasks:Vec<_>=self.scene.tasks.iter().filter_map(|(origin,task)|{
            let line=self.scene.lines.iter().find(|line|line.origin==*origin)?;
            Some(serde_json::json!({"x":self.content_x()+line.x+8.0,"y":116.0+line.y-self.scroll+line.height/2.0,"checked":task.checked,"marker":task.marker.start}))
        }).collect();
        let image_boxes:Vec<_>=self.scene.images.iter().map(|image|serde_json::json!({"x":self.content_x(),"y":116.0+image.y-self.scroll,"width":image.width,"height":image.height})).collect();
        serde_json::json!({"enabled":self.options.inline_edit,"tasks":tasks,"image_boxes":image_boxes,
            "input_units":self.inline.control.as_ref().and_then(|control|control.text_utf16().ok()).map(|text|text.len()),"active":self.inline.control.is_some(),
            "dirty":self.inline.note.as_ref().is_some_and(|note|note.dirty()),
            "pending":self.inline.note.as_ref().is_some_and(|note|note.pending()),
            "blocked":self.inline.blocked,"checkpoint_timer":self.inline.timer,"child_paint_queued":self.inline.paint_queued,
            "directory":self.inline.note.as_ref().map(|note|note.directory()),
            "active_source":self.inline.note.as_ref().and_then(|note|note.active.as_ref()).map(|active|active.source.clone()),
            "focused":self.inline.control.as_ref().is_some_and(|control|unsafe {GetFocus()==control.hwnd}),
            "foreground":unsafe {GetForegroundWindow()==self.hwnd},
            "composing":self.inline.control.as_ref().is_some_and(Paragraph::composing),
            "selection":self.inline.control.as_ref().map(Paragraph::selection),
            "native_paints":self.inline.control.as_ref().map(Paragraph::paints),
            "first_visible_line":self.inline.control.as_ref().map(|control|unsafe {SendMessageW(control.hwnd, 0x00ce, None, None).0}),
            "native_undo":self.inline.control.as_ref().is_some_and(Paragraph::can_undo),
            "native_redo":self.inline.control.as_ref().is_some_and(Paragraph::can_redo),
            "paragraphs":paragraphs,"styles":styles})
    }
}
