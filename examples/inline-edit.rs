//! Isolated M2 DEVELOPMENT PROTOTYPE. Never opens a user-supplied note.
//! cargo run --locked --example inline-edit [-- --light --narrow]
//! All sample originals, drafts, diagnostics and recovery live in target/v1-m2/.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!(
        "inline-edit is a Windows-native Msftedit development prototype; Windows is required"
    );
}
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run()
}

#[cfg(windows)]
mod native {
    use rusty_note::{
        draft::Session,
        inline::Paragraph,
        inline_projection::{MAX_PARAGRAPH_UNITS, Projection},
    };
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::{
        cell::RefCell,
        fs::{self, OpenOptions},
        io::{self, Write},
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };
    use windows::{
        Win32::{
            Foundation::*,
            Graphics::Gdi::*,
            System::{Com::*, LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
            UI::{
                Controls::RichEdit::EM_STOPGROUPTYPING, HiDpi::*,
                Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*,
            },
        },
        core::w,
    };

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
    const FIXTURE: &str = include_str!("../tests/fixtures/inline-edit.md");
    const SYNC: u32 = WM_APP + 1;
    const REPORT: u32 = WM_APP + 20;
    const PROBE: u32 = WM_APP + 21;
    const CHECKPOINT: usize = 1;

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct InputCheckpoint {
        version: u32,
        source: String,
        utf16: Vec<u16>,
        error: Option<String>,
    }
    struct App {
        hwnd: HWND,
        paragraph: Option<Paragraph>,
        session: Session,
        projected: Projection,
        paragraph_start: usize,
        latest: Vec<u16>,
        processed: Option<Vec<u16>>,
        projection_error: Option<String>,
        status: String,
        directory: PathBuf,
        light: bool,
        title: String,
        intro: String,
        outro: String,
        unsupported: String,
        font: HFONT,
        heading_font: HFONT,
        background: HBRUSH,
        pending: bool,
        checkpoints: u64,
        paints: u64,
        changes: u64,
        style_passes: u64,
        selection_preserved: bool,
        last_native_result: Option<bool>,
    }
    fn error(message: &str) -> Box<dyn std::error::Error> {
        io::Error::other(message).into()
    }
    fn rgb(value: u32) -> COLORREF {
        COLORREF((value & 255) << 16 | (value & 0xff00) | (value >> 16 & 255))
    }
    fn new_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }
    fn name_is_safe(name: &str, prefix: &str) -> bool {
        name.starts_with(prefix)
            && name.len() <= 128
            && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    }
    fn contained(root: &Path, path: &Path) -> Result<PathBuf> {
        let canonical = fs::canonicalize(path)?;
        if !canonical.starts_with(fs::canonicalize(root)?) {
            return Err(error("Recovery path leaves isolated target directory"));
        }
        Ok(canonical)
    }
    impl App {
        fn open(directory: PathBuf, light: bool, reopen: bool) -> Result<Self> {
            let original = directory.join("original.md");
            let session = if reopen {
                contained(&directory, &original)?;
                let index = directory.join("session-name.txt");
                if fs::metadata(&index)?.len() > 128 {
                    return Err(error("Invalid session index"));
                }
                let name = fs::read_to_string(index)?;
                if !name_is_safe(&name, "draft-") {
                    return Err(error("Invalid session name"));
                }
                Session::reopen(
                    &original,
                    &contained(&directory, &directory.join("drafts").join(name))?,
                )?
            } else {
                new_file(&original, FIXTURE.as_bytes())?;
                let session = Session::create(&original, &directory.join("drafts"))?;
                new_file(
                    &directory.join("session-name.txt"),
                    session
                        .directory()
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .as_bytes(),
                )?;
                session
            };
            // Fixture schema is intentionally fixed; not a general note/block picker.
            let blocks: Vec<&str> = session.snapshot().split("\n\n").collect();
            if blocks.len() != 5 || session.snapshot() != FIXTURE {
                return Err(error("Not this prototype's embedded fixture"));
            }
            let title = blocks[0]
                .strip_prefix("# ")
                .ok_or_else(|| error("Fixture heading"))?
                .to_owned();
            let intro = blocks[1].to_owned();
            let paragraph_start = blocks[0].len() + blocks[1].len() + 4;
            let outro = blocks[3].to_owned();
            let unsupported = blocks[4..].join("\n\n");
            let current = session.text()[paragraph_start..]
                .split("\n\n")
                .next()
                .unwrap();
            let projected = Projection::parse(current).map_err(error)?;
            let mut latest: Vec<u16> = projected.text.encode_utf16().collect();
            let mut projection_error = None;
            let checkpoint = directory.join("input.json");
            if reopen && checkpoint.exists() {
                contained(&directory, &checkpoint)?;
                if fs::metadata(&checkpoint)?.len() > 128 * 1024 {
                    return Err(error("Oversized input checkpoint"));
                }
                let saved: InputCheckpoint = serde_json::from_slice(&fs::read(checkpoint)?)?;
                if saved.version != 1
                    || saved.source != projected.source
                    || saved.utf16.len() > MAX_PARAGRAPH_UNITS
                    || saved.utf16.contains(&0)
                {
                    return Err(error(
                        "Input checkpoint does not match completed source checkpoint; files retained for manual review",
                    ));
                }
                latest = saved.utf16;
                projection_error = saved.error;
                if latest != projected.text.encode_utf16().collect::<Vec<_>>()
                    && projection_error.is_none()
                {
                    return Err(error(
                        "Unlabelled unprojected checkpoint; retained for manual review",
                    ));
                }
            }
            unsafe {
                let font = CreateFontW(
                    -18,
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
                    w!("Segoe UI"),
                );
                let heading_font = CreateFontW(
                    -28,
                    0,
                    0,
                    0,
                    700,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET,
                    OUT_DEFAULT_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    CLEARTYPE_QUALITY,
                    0,
                    w!("Segoe UI"),
                );
                let background = CreateSolidBrush(rgb(if light { 0xfbf6ef } else { 0x24201d }));
                if font.0.is_null() || heading_font.0.is_null() || background.0.is_null() {
                    return Err(error("Native font/brush allocation failed"));
                }
                Ok(Self {
                    hwnd: HWND::default(),
                    paragraph: None,
                    session,
                    projected,
                    paragraph_start,
                    processed: Some(latest.clone()),
                    latest,
                    status: if projection_error.is_some() {
                        "REOPENED UNPROJECTED INPUT. Last valid Markdown retained; edit to repair."
                    } else if reopen {
                        "Reopened completed checkpoint; original untouched."
                    } else {
                        "Click the rendered paragraph and edit an existing style run."
                    }
                    .into(),
                    projection_error,
                    directory,
                    light,
                    title,
                    intro,
                    outro,
                    unsupported,
                    font,
                    heading_font,
                    background,
                    pending: false,
                    checkpoints: 0,
                    paints: 0,
                    changes: 0,
                    style_passes: 0,
                    selection_preserved: true,
                    last_native_result: None,
                })
            }
        }
        fn paragraph(&self) -> &Paragraph {
            self.paragraph.as_ref().expect("initialized paragraph")
        }
        fn invalidate(&self) {
            unsafe {
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        }
        fn schedule(&mut self) -> Result<()> {
            if !self.pending {
                if unsafe { SetTimer(Some(self.hwnd), CHECKPOINT, 500, None) } == 0 {
                    return Err(error(
                        "Checkpoint timer unavailable; close to retry checkpoint",
                    ));
                }
                self.pending = true;
            }
            Ok(())
        }
        fn sync(&mut self) -> Result<()> {
            let input = self.paragraph().text_utf16()?;
            if self.processed.as_ref() == Some(&input) {
                return Ok(());
            }
            self.latest.clone_from(&input);
            self.changes += 1;
            self.schedule()?;
            if self.paragraph().composing() {
                self.processed = None;
                self.projection_error = Some(
                    "Native composition pending; latest UTF-16 retained, decoration deferred."
                        .into(),
                );
                self.invalidate();
                return Ok(());
            }
            self.processed = Some(input.clone());
            let before = self.paragraph().selection();
            let result = String::from_utf16(&input)
                .map_err(|_| "Incomplete UTF-16 input retained".to_owned())
                .and_then(|text| self.projected.edit(&text).map_err(str::to_owned))
                .and_then(|edit| {
                    if let Some(edit) = edit {
                        self.session
                            .replace_range(
                                self.paragraph_start + edit.range.start
                                    ..self.paragraph_start + edit.range.end,
                                &edit.replacement,
                            )
                            .map_err(|error| error.to_string())?;
                        self.projected = edit.projected;
                    }
                    Ok(())
                });
            match result {
                Ok(()) => {
                    self.projection_error = None;
                    self.paragraph().apply_styles(&self.projected)?;
                    self.style_passes += 1;
                    self.status =
                        "Live working Markdown updated. Original untouched; checkpoint queued."
                            .into();
                }
                Err(reason) => {
                    self.projection_error = Some(reason);
                    self.paragraph().show_unprojected()?;
                    self.status = "UNPROJECTED INPUT RETAINED. Last valid Markdown unchanged; undo to return.".into();
                }
            }
            self.selection_preserved &= before == self.paragraph().selection();
            self.invalidate();
            Ok(())
        }
        fn checkpoint(&mut self) -> Result<()> {
            // Kill first, including on I/O failure. No retry/animation timer at idle.
            unsafe {
                let _ = KillTimer(Some(self.hwnd), CHECKPOINT);
            }
            self.pending = false;
            self.session.checkpoint()?;
            let saved = InputCheckpoint {
                version: 1,
                source: self.projected.source.clone(),
                utf16: self.latest.clone(),
                error: self.projection_error.clone(),
            };
            let temporary = self.directory.join("input.next");
            new_file(&temporary, &serde_json::to_vec(&saved)?)?;
            fs::rename(&temporary, self.directory.join("input.json"))?;
            self.checkpoints += 1;
            self.status = if self.projection_error.is_some() { "Checkpoint: last valid Markdown + separate exact unprojected UTF-16. Original untouched." } else { "Checkpoint complete. Working draft only; original untouched." }.into();
            self.invalidate();
            Ok(())
        }
        fn layout(&self) -> Result<()> {
            let mut rect = RECT::default();
            unsafe {
                GetClientRect(self.hwnd, &mut rect)?;
            }
            self.paragraph().resize(RECT {
                left: 24,
                top: 158,
                right: (rect.right - 24).max(160),
                bottom: 322,
            })?;
            self.invalidate();
            Ok(())
        }
        fn draw(&mut self) {
            unsafe {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(self.hwnd, &mut paint);
                self.paints += 1;
                let mut rect = RECT::default();
                let _ = GetClientRect(self.hwnd, &mut rect);
                FillRect(dc, &rect, self.background);
                let old = SelectObject(dc, HGDIOBJ(self.font.0));
                SetBkMode(dc, TRANSPARENT);
                let ink = if self.light { 0x302722 } else { 0xf5eadc };
                let muted = if self.light { 0x604a3d } else { 0xcfb7a2 };
                let accent = if self.light { 0x87391d } else { 0xf0a574 };
                let draw = |text: &str, top: i32, height: i32, color: u32| {
                    SetTextColor(dc, rgb(color));
                    let mut text: Vec<u16> = text.encode_utf16().collect();
                    let mut area = RECT {
                        left: 24,
                        top,
                        right: rect.right - 24,
                        bottom: top + height,
                    };
                    DrawTextW(
                        dc,
                        &mut text,
                        &mut area,
                        DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
                    );
                };
                draw("DEVELOPMENT PROTOTYPE · M2 / RichEdit", 16, 50, accent);
                SelectObject(dc, HGDIOBJ(self.heading_font.0));
                draw(&self.title, 57, 40, ink);
                SelectObject(dc, HGDIOBJ(self.font.0));
                draw(&self.intro, 100, 48, ink);
                draw(&self.outro, 338, 50, ink);
                draw(
                    "Unsupported paragraph — read-only exact source:",
                    395,
                    50,
                    accent,
                );
                draw(self.unsupported.trim_end_matches('\n'), 448, 80, muted);
                draw(
                    "Edit inside existing plain / bold / italic runs. No style toolbar, cross-style edits or new Markdown syntax yet.",
                    540,
                    80,
                    muted,
                );
                draw(&self.status, 625, 80, accent);
                draw(
                    "Close keeps the draft. No original Save in this prototype.",
                    714,
                    55,
                    muted,
                );
                SelectObject(dc, old);
                let _ = EndPaint(self.hwnd, &paint);
            }
        }
        fn report(&mut self) -> Result<()> {
            self.sync()?;
            let native_runs: Vec<_> = if self.projection_error.is_none() {
                self.projected.runs.iter().map(|run| {
                    let style = self.paragraph().style_at(run.utf16.clone());
                    json!({"start":run.utf16.start,"end":run.utf16.end,"bold":run.style.bold,"italic":run.style.italic,"native":style.ok()})
                }).collect()
            } else {
                Vec::new()
            };
            let reopened = Session::reopen(self.session.original_path(), self.session.directory())?;
            let selection = self.paragraph().selection();
            let mut gui = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            unsafe {
                GetGUIThreadInfo(GetCurrentThreadId(), &mut gui)?;
            }
            let data = json!({
                "prototype":true,"pid":std::process::id(),"light":self.light,"directory":self.directory,"session":self.session.directory(),
                "native_class":"RICHEDIT50W","text":String::from_utf16(&self.latest).ok(),"utf16":self.latest,
                "source":self.session.text(),"projected_source":self.projected.source,"projected_text":self.projected.text,
                "dirty":self.session.dirty(),"original_unchanged":fs::read(self.session.original_path())? == self.session.snapshot().as_bytes(),
                "reopened_source":reopened.text(),"projection_error":self.projection_error,"status":self.status,
                "selection":[selection.start,selection.end],"selection_preserved":self.selection_preserved,
                "can_undo":self.paragraph().can_undo(),"can_redo":self.paragraph().can_redo(),"last_native_result":self.last_native_result,
                "runs":native_runs,"input_format":self.paragraph().style_at(0..self.latest.len())?,
                "change_events":self.changes,"style_passes":self.style_passes,
                "parent_paints":self.paints,"paragraph_paints":self.paragraph().paints(),"checkpoint_timer":self.pending,"checkpoints":self.checkpoints,
                "composition_active":self.paragraph().composing(),"physical_ime_verified":false,
                "foreground":unsafe { GetForegroundWindow() == self.hwnd },
                "native_focus":gui.hwndFocus == self.paragraph().hwnd,
                "native_caret":gui.hwndCaret == self.paragraph().hwnd,
                "caret_blinking":gui.flags.contains(GUI_CARETBLINKING),
                "caret_rect":[gui.rcCaret.left,gui.rcCaret.top,gui.rcCaret.right,gui.rcCaret.bottom],
            });
            fs::write(
                self.directory.join("diagnostics.json"),
                serde_json::to_vec_pretty(&data)?,
            )?;
            Ok(())
        }
        fn styled_range(&self, bold: bool) -> Result<std::ops::Range<usize>> {
            self.projected
                .runs
                .iter()
                .find(|run| {
                    if bold {
                        run.style.bold
                    } else {
                        run.style.italic
                    }
                })
                .map(|run| run.utf16.clone())
                .ok_or_else(|| error("Missing probe style run"))
        }
        fn probe(&mut self, action: usize) -> Result<()> {
            self.sync()?;
            unsafe {
                let _ = SetFocus(Some(self.paragraph().hwnd));
            }
            match action {
                1 | 4 => {
                    self.paragraph().select(self.styled_range(action == 1)?)?;
                    self.paragraph().replace_selection(if action == 1 {
                        "굵은 😀 글자"
                    } else {
                        "바로 italic 🚀"
                    })?;
                }
                2 => self.last_native_result = Some(self.paragraph().undo()),
                3 => self.last_native_result = Some(self.paragraph().redo()),
                5 => {
                    self.paragraph().select(0..0)?;
                    unsafe {
                        SendMessageW(self.paragraph().hwnd, EM_STOPGROUPTYPING, None, None);
                        // Receiver-local Unicode injection; emphatically NOT an IME test.
                        for unit in "한😀 ".encode_utf16() {
                            SendMessageW(
                                self.paragraph().hwnd,
                                WM_CHAR,
                                Some(WPARAM(unit as usize)),
                                Some(LPARAM(1)),
                            );
                        }
                        SendMessageW(self.paragraph().hwnd, EM_STOPGROUPTYPING, None, None);
                    }
                }
                6 => {
                    let range = self.styled_range(true)?;
                    self.paragraph().select(range.start + 1..range.start + 1)?;
                    self.paragraph().replace_selection("&amp;")?;
                }
                7 => self.paragraph().select(self.styled_range(true)?)?,
                8 => self.checkpoint()?,
                9 => {
                    let range = self.styled_range(false)?;
                    self.paragraph().select(range.start + 1..range.start + 1)?;
                }
                10 => {
                    let before = self.paragraph().selection();
                    self.paragraph().apply_styles(&self.projected)?;
                    self.style_passes += 1;
                    self.selection_preserved &= before == self.paragraph().selection();
                }
                11 => {
                    let text = String::from_utf16(&self.latest)?;
                    let byte = text
                        .find("&amp;")
                        .ok_or_else(|| error("Missing retained probe text"))?;
                    let start = rusty_note::inline_projection::byte_to_utf16(&text, byte).unwrap();
                    self.paragraph().select(start..start + 5)?;
                    self.paragraph().replace_selection("")?;
                }
                12 => unsafe {
                    // One native input event followed by the ordinary live sync.
                    // Repeating this action tests typing-group undo across styling.
                    SendMessageW(
                        self.paragraph().hwnd,
                        WM_CHAR,
                        Some(WPARAM('x' as usize)),
                        Some(LPARAM(1)),
                    );
                },
                13 => {
                    let range = self.styled_range(true)?;
                    self.paragraph().select(range.start + 1..range.start + 1)?;
                    unsafe {
                        SendMessageW(self.paragraph().hwnd, EM_STOPGROUPTYPING, None, None);
                    }
                }
                14 => unsafe {
                    SendMessageW(self.paragraph().hwnd, EM_STOPGROUPTYPING, None, None);
                },
                _ => return Err(error("Unknown receiver-local diagnostic action")),
            }
            self.sync()?;
            self.report()
        }
        fn handle(&mut self, message: u32, w: WPARAM) -> Result<bool> {
            match message {
                SYNC => self.sync()?,
                REPORT => self.report()?,
                PROBE => self.probe(w.0)?,
                WM_TIMER if w.0 == CHECKPOINT && self.pending => {
                    self.sync()?;
                    self.checkpoint()?;
                }
                WM_SIZE if self.paragraph.is_some() => self.layout()?,
                WM_PAINT => self.draw(),
                WM_ERASEBKGND => return Ok(true),
                WM_CLOSE => {
                    self.sync()?;
                    if self.paragraph().composing() {
                        return Err(error("Close deferred while native composition is active"));
                    }
                    self.checkpoint()?;
                    self.report()?;
                    // Drop child/TOM/library before destroying the parent HWND.
                    self.paragraph.take();
                    unsafe {
                        DestroyWindow(self.hwnd)?;
                    }
                }
                _ => return Ok(false),
            }
            Ok(true)
        }
    }
    impl Drop for App {
        fn drop(&mut self) {
            unsafe {
                self.paragraph.take();
                let _ = DeleteObject(HGDIOBJ(self.font.0));
                let _ = DeleteObject(HGDIOBJ(self.heading_font.0));
                let _ = DeleteObject(HGDIOBJ(self.background.0));
            }
        }
    }
    unsafe extern "system" fn wndproc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        unsafe {
            if message == WM_GETMINMAXINFO {
                // This fixed-fixture development window has no outer scroller.
                // Bound resizing so its labelled unsupported source stays visible.
                let limits = &mut *(l.0 as *mut MINMAXINFO);
                limits.ptMinTrackSize = POINT { x: 420, y: 840 };
                return LRESULT(0);
            }
            if message == WM_NCCREATE {
                let create = &*(l.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            // EN_CHANGE can arrive synchronously inside TOM/Win32 calls. Post a
            // UI-thread sync, rather than reentrantly borrowing the App. This is
            // event-driven, not a timer or render loop; duplicates become no-ops.
            if message == WM_COMMAND && (w.0 >> 16) as u32 == EN_CHANGE && (w.0 & 0xffff) == 100 {
                let _ = PostMessageW(Some(hwnd), SYNC, WPARAM(0), LPARAM(0));
                return LRESULT(0);
            }
            if message == WM_DESTROY {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            if message == WM_NCDESTROY {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App>;
            if !pointer.is_null()
                && let Ok(mut app) = (&*pointer).try_borrow_mut()
            {
                match app.handle(message, w) {
                    Ok(true) => return LRESULT(if message == WM_ERASEBKGND { 1 } else { 0 }),
                    Err(error) => {
                        app.status = format!("ERROR — input/files retained: {error}");
                        let _ = fs::write(app.directory.join("last-error.txt"), &app.status);
                        app.invalidate();
                        return LRESULT(0);
                    }
                    _ => {}
                }
            }
            DefWindowProcW(hwnd, message, w, l)
        }
    }
    pub fn run() -> Result<()> {
        let mut light = false;
        let mut narrow = false;
        let mut reopen = false;
        let mut name = format!(
            "run-{}-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
            std::process::id()
        );
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--light" => light = true,
                "--narrow" => narrow = true,
                "--run" | "--reopen" => {
                    reopen = arg == "--reopen";
                    name = args.next().ok_or_else(|| error("Missing run name"))?;
                }
                _ => {
                    return Err(error(
                        "Use --light, --narrow, --run run-NAME or --reopen run-NAME only; no note paths",
                    ));
                }
            }
        }
        if !name_is_safe(&name, "run-") {
            return Err(error(
                "Run name must be run- followed by ASCII letters/digits/hyphens",
            ));
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/v1-m2");
        fs::create_dir_all(&root)?;
        let directory = root.join(&name);
        if !reopen {
            fs::create_dir(&directory)?;
        }
        let directory = contained(&root, &directory)?;
        println!("DEVELOPMENT PROTOTYPE artifacts: {}", directory.display());
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            // No per-monitor renderer integration in this prototype. Coordinates
            // are physical pixels; metrics/DPI equivalence remains unclaimed.
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let app = Box::new(RefCell::new(App::open(directory, light, reopen)?));
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                hInstance: HINSTANCE(instance.0),
                lpszClassName: w!("RustyNote.InlinePrototype"),
                lpfnWndProc: Some(wndproc),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(windows::core::Error::from_thread().into());
            }
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class.lpszClassName,
                w!("Rusty Note — DEVELOPMENT inline paragraph prototype"),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
                100,
                80,
                if narrow { 420 } else { 720 },
                840,
                None,
                None,
                Some(HINSTANCE(instance.0)),
                Some((&*app as *const RefCell<App>).cast()),
            )?;
            {
                let mut state = app.borrow_mut();
                state.hwnd = hwnd;
                let paragraph = Paragraph::create(hwnd, 100, SYNC, &state.projected, light)?;
                if state.projection_error.is_some() {
                    paragraph.restore_initial_input(&state.latest)?;
                }
                state.paragraph = Some(paragraph);
                state.layout()?;
                state.report()?;
                let _ = SetFocus(Some(state.paragraph().hwnd));
            }
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = UpdateWindow(hwnd);
            let mut message = MSG::default();
            loop {
                let status = GetMessageW(&mut message, None, 0, 0).0;
                if status == -1 {
                    return Err(windows::core::Error::from_thread().into());
                }
                if status == 0 {
                    break;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            drop(app);
            CoUninitialize();
        }
        Ok(())
    }
}
