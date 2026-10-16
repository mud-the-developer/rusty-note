use crate::{
    document::{Document, Kind},
    layout::{Font, FontMeasure, Scene},
    motion::Motion,
    store::{self, Note, State},
    theme::Palette,
};
use pulldown_cmark::Alignment;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fs,
    path::PathBuf,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Dwm::*,
            Dxgi::Common::*,
            Gdi::*,
        },
        System::{Com::*, LibraryLoader::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, Result, w},
};
use windows_numerics::Vector2;

#[path = "native_inline.rs"]
mod inline_host;

#[derive(Default)]
pub struct Options {
    pub notes_dir: PathBuf,
    pub state_file: PathBuf,
    pub smoke_output: Option<PathBuf>,
    pub metrics_file: Option<PathBuf>,
    pub light: bool,
    pub reduced_motion: bool,
    pub explicit_notes_dir: bool,
    pub editor_mode: bool,
    pub inline_edit: bool,
    pub file: Option<PathBuf>,
    pub smoke_drop_file: Option<PathBuf>,
}

struct Fonts {
    factory: IDWriteFactory,
    formats: RefCell<HashMap<Font, IDWriteTextFormat>>,
    measurements: Cell<u64>,
}
impl Fonts {
    fn format(&self, font: Font) -> Result<IDWriteTextFormat> {
        if let Some(format) = self.formats.borrow().get(&font) {
            return Ok(format.clone());
        }
        let format = unsafe {
            self.factory.CreateTextFormat(
                if font.mono {
                    w!("Cascadia Mono")
                } else {
                    w!("Segoe UI")
                },
                None,
                if font.bold {
                    DWRITE_FONT_WEIGHT_BOLD
                } else {
                    DWRITE_FONT_WEIGHT_NORMAL
                },
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font.size as f32,
                w!("en-us"),
            )?
        };
        unsafe {
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        }
        self.formats.borrow_mut().insert(font, format.clone());
        Ok(format)
    }
}
impl FontMeasure for Fonts {
    fn width(&self, text: &str, font: Font) -> f64 {
        self.measurements.set(self.measurements.get() + 1);
        let measured = (|| -> Result<f64> {
            let format = self.format(font)?;
            let wide: Vec<u16> = text.encode_utf16().collect();
            let layout = unsafe {
                self.factory
                    .CreateTextLayout(&wide, &format, 100000.0, 1000.0)?
            };
            let mut metrics = DWRITE_TEXT_METRICS::default();
            unsafe {
                layout.GetMetrics(&mut metrics)?;
            }
            Ok(f64::from(metrics.widthIncludingTrailingWhitespace))
        })();
        measured.expect("DirectWrite font measurement failed")
    }
}

fn color(rgb: u32, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 255) as f32 / 255.0,
        g: ((rgb >> 8) & 255) as f32 / 255.0,
        b: (rgb & 255) as f32 / 255.0,
        a: alpha,
    }
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

struct App {
    hwnd: HWND,
    factory: ID2D1Factory,
    target: Option<ID2D1HwndRenderTarget>,
    brush: Option<ID2D1SolidColorBrush>,
    fonts: Fonts,
    state: State,
    options: Options,
    notes: Vec<Note>,
    selected: usize,
    scene: Scene,
    motion: Motion,
    library: bool,
    hover: (f32, f32),
    width: f32,
    height: f32,
    scale: f32,
    scroll: f32,
    scroll_drag: Option<f32>,
    paints: u64,
    error: Option<String>,
    editor: Option<crate::editor::Editor>,
    inline: inline_host::InlineState,
    image_popup: Option<HWND>,
    images: crate::images::Images,
    library_scroll: usize,
    tray_available: bool,
    viewer_source: Option<String>,
    search_query: String,
    search_hits: Vec<crate::layout::SearchHit>,
    search_index: usize,
}
impl App {
    unsafe fn new(mut options: Options) -> Result<Self> {
        unsafe {
            if options.editor_mode && options.inline_edit {
                return Err(windows::core::Error::new(
                    E_INVALIDARG,
                    "--inline-edit cannot be combined with --editor",
                ));
            }
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let fonts = Fonts {
                factory: dwrite,
                formats: RefCell::new(HashMap::new()),
                measurements: Cell::new(0),
            };
            let mut state_problem = None;
            let mut state = match State::load_checked(&options.state_file) {
                Ok(state) => state,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => State::default(),
                Err(error) => {
                    if options.inline_edit {
                        state_problem = Some(format!(
                            "State/recovery association needs review; editing blocked and file retained: {error}"
                        ));
                    }
                    State::default()
                }
            };
            if !options.editor_mode && !options.state_file.exists() {
                state.width = 360;
                state.height = 500;
            }
            if options.light {
                state.light = true;
            }
            state.width = state.width.clamp(340, 1600);
            state.height = state.height.clamp(420, 1600);
            if !options.explicit_notes_dir
                && let Some(directory) = &state.notes_dir
            {
                options.notes_dir = directory.clone();
            }
            let (mut notes, mut error) = if options.editor_mode {
                let _ = fs::create_dir_all(&options.notes_dir);
                match store::list_notes(&options.notes_dir) {
                    Ok(notes) => (notes, None),
                    Err(error) => (Vec::new(), Some(error.to_string())),
                }
            } else {
                (Vec::new(), None)
            };
            if !options.editor_mode {
                let initial = options
                    .file
                    .clone()
                    .or_else(|| state.selected.clone())
                    .or_else(|| {
                        options
                            .inline_edit
                            .then(|| {
                                state
                                    .inline_draft
                                    .as_ref()
                                    .map(|draft| draft.original.clone())
                            })
                            .flatten()
                    });
                if let Some(path) = initial {
                    if options.inline_edit {
                        options.file = Some(path.clone());
                    }
                    match crate::viewer::open(&path) {
                        Ok((note, _)) => {
                            options.notes_dir = note.path.parent().unwrap().to_owned();
                            options.file = Some(note.path.clone());
                            notes = vec![note];
                        }
                        Err(e) => {
                            error = Some(format!("Could not open {}\n\n{}", path.display(), e))
                        }
                    }
                }
            }
            let selected = state
                .selected
                .as_ref()
                .and_then(|path| notes.iter().position(|n| &n.path == path))
                .unwrap_or(0);
            let source = notes.get(selected).and_then(|note| crate::edit::read_note(&note.path).ok())
            .unwrap_or_else(|| "# Markdown sticker\n\nDrop one .md file here, or press Ctrl+O to open it.\n\nDrag the header to place this small window. P keeps it above other windows.\n\nThe original Markdown is read-only.".into());
            let viewer_source = (!options.editor_mode).then(|| source.clone());
            let mut scene = Scene::with_zoom(
                &Document::parse(&source),
                &fonts,
                if options.editor_mode { 1.0 } else { state.zoom },
            );
            scene.reflow(state.width as f32 - 72.0);
            let scroll = if !options.editor_mode
                && notes
                    .get(selected)
                    .is_some_and(|note| state.selected.as_ref() == Some(&note.path))
            {
                crate::viewer::reload_scroll(
                    state.scroll,
                    scene.height,
                    state.height as f32 - 160.0,
                )
            } else {
                0.0
            };
            let mut motion = Motion::default();
            motion.reduced = options.reduced_motion;
            let mut app = Self {
                hwnd: HWND::default(),
                factory,
                target: None,
                brush: None,
                fonts,
                state,
                options,
                notes,
                selected,
                scene,
                motion,
                library: false,
                hover: (-1.0, -1.0),
                width: 460.0,
                height: 700.0,
                scale: 1.0,
                scroll,
                scroll_drag: None,
                paints: 0,
                editor: None,
                inline: inline_host::InlineState::default(),
                image_popup: None,
                images: crate::images::Images::new()?,
                library_scroll: 0,
                tray_available: false,
                viewer_source,
                search_query: String::new(),
                search_hits: Vec::new(),
                search_index: 0,
                error,
            };
            if let Some(error) = state_problem {
                app.inline.blocked = true;
                app.error = Some(error);
            } else {
                app.restore_inline_note();
            }
            Ok(app)
        }
    }
    unsafe fn resources(&mut self) -> Result<()> {
        unsafe {
            if self.target.is_some() {
                return Ok(());
            }
            let mut bounds = RECT::default();
            GetClientRect(self.hwnd, &mut bounds)?;
            self.scale = GetDpiForWindow(self.hwnd) as f32 / 96.0;
            self.width = (bounds.right - bounds.left) as f32 / self.scale;
            self.height = (bounds.bottom - bounds.top) as f32 / self.scale;
            let target = self.factory.CreateHwndRenderTarget(
                &D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_UNKNOWN,
                        alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
                    },
                    dpiX: self.scale * 96.0,
                    dpiY: self.scale * 96.0,
                    usage: D2D1_RENDER_TARGET_USAGE_NONE,
                    minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
                },
                &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                    hwnd: self.hwnd,
                    pixelSize: D2D_SIZE_U {
                        width: (bounds.right - bounds.left).max(1) as u32,
                        height: (bounds.bottom - bounds.top).max(1) as u32,
                    },
                    presentOptions: D2D1_PRESENT_OPTIONS_NONE,
                },
            )?;
            self.brush = Some(target.CreateSolidColorBrush(&color(0xffffff, 1.0), None)?);
            self.target = Some(target);
            Ok(())
        }
    }
    fn hover_region(&self, (x, y): (f32, f32)) -> i32 {
        if y < 43.0 {
            for (i, left) in [
                12.0,
                48.0,
                86.0,
                self.width - 114.0,
                self.width - 78.0,
                self.width - 42.0,
            ]
            .iter()
            .enumerate()
            {
                if !self.options.editor_mode
                    && [1, 2].contains(&i)
                    && !(self.options.inline_edit && i == 2)
                {
                    continue;
                }
                let width = if !self.options.editor_mode && i == 0 {
                    56.0
                } else {
                    30.0
                };
                if x >= *left && x < *left + width {
                    return i as i32;
                }
            }
        }
        if self.library && x < 158.0 && y >= 112.0 {
            return 100 + ((y - 112.0) / 48.0) as i32;
        }
        -1
    }
    fn content_x(&self) -> f32 {
        if self.library { 178.0 } else { 28.0 }
    }
    fn reflow(&mut self) {
        let passes = self.scene.layout_passes;
        self.scene
            .reflow((self.width - self.content_x() - 28.0).max(48.0));
        if self.scene.layout_passes != passes {
            self.search_hits = self.scene.find(&self.search_query);
            self.search_index = self
                .search_index
                .min(self.search_hits.len().saturating_sub(1));
        }
        self.scroll =
            crate::viewer::reload_scroll(self.scroll, self.scene.height, self.height - 160.0);
        self.position_inline();
    }
    fn reading_hint(&self) -> &str {
        if self.options.editor_mode {
            return "CACHED LAYOUT  ·  EVENT-DRIVEN  ·  RUST";
        }
        match self.hover_region(self.hover) {
            0 => "Open Markdown  ·  Ctrl+O or drop a file",
            3 => "Always on top  ·  P",
            4 => "Light / dark theme  ·  T",
            5 => "Close this sticker",
            _ if self.options.inline_edit => "CLICK TO EDIT · Ctrl+N new · Ctrl+S saves original",
            _ => "READ ONLY  ·  Ctrl+O OPEN  ·  F5 RELOAD",
        }
    }
    unsafe fn animate(&mut self) {
        unsafe {
            self.motion.start();
            if self.motion.active() {
                SetTimer(Some(self.hwnd), 1, 16, None);
            }
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }
    unsafe fn persist(&mut self) {
        unsafe {
            let mut bounds = RECT::default();
            if GetWindowRect(self.hwnd, &mut bounds).is_ok() && !IsIconic(self.hwnd).as_bool() {
                self.state.x = bounds.left;
                self.state.y = bounds.top;
                self.state.width = ((bounds.right - bounds.left) as f32 / self.scale) as i32;
                self.state.height = ((bounds.bottom - bounds.top) as f32 / self.scale) as i32;
            }
            self.state.selected = self.notes.get(self.selected).map(|n| n.path.clone());
            self.state.notes_dir = Some(self.options.notes_dir.clone());
            self.state.scroll = if self.options.editor_mode {
                0.0
            } else {
                self.scroll
            };
            if let Err(error) = self.state.save(&self.options.state_file) {
                eprintln!("state save: {error}");
            }
        }
    }
    unsafe fn select(&mut self, index: usize) {
        unsafe {
            if index >= self.notes.len() || !self.finish_edit() {
                return;
            }
            match crate::edit::read_note(&self.notes[index].path) {
                Ok(source) => {
                    self.scene = Scene::new(&Document::parse(&source), &self.fonts);
                    self.selected = index;
                    self.scroll = 0.0;
                    self.reflow();
                    self.error = None;
                    let title = format!("Rusty Note — {}", self.notes[index].title);
                    let wide: Vec<u16> = title.encode_utf16().chain([0]).collect();
                    let _ = SetWindowTextW(self.hwnd, PCWSTR(wide.as_ptr()));
                    self.animate();
                }
                Err(error) => {
                    self.error = Some(error.to_string());
                    let _ = InvalidateRect(Some(self.hwnd), None, false);
                }
            }
        }
    }
    unsafe fn fill(&self, box_: D2D_RECT_F, radius: f32, rgb: u32, alpha: f32) {
        unsafe {
            let target = self.target.as_ref().unwrap();
            let brush = self.brush.as_ref().unwrap();
            brush.SetColor(&color(rgb, alpha));
            target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: box_,
                    radiusX: radius,
                    radiusY: radius,
                },
                brush,
            );
        }
    }
    unsafe fn line_layout(&self, line: &crate::layout::Line) -> Result<IDWriteTextLayout> {
        unsafe {
            let wide: Vec<u16> = line.text.encode_utf16().collect();
            let format = self.fonts.format(line.font)?;
            let width = line
                .layout_width
                .min(self.width - self.content_x() - 28.0 - line.x)
                .max(1.0);
            let layout =
                self.fonts
                    .factory
                    .CreateTextLayout(&wide, &format, width, line.height + 4.0)?;
            layout.SetTextAlignment(match line.alignment {
                Alignment::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                Alignment::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
                _ => DWRITE_TEXT_ALIGNMENT_LEADING,
            })?;
            for run in &line.runs {
                let start = line.text[..run.start].encode_utf16().count() as u32;
                let range = DWRITE_TEXT_RANGE {
                    startPosition: start,
                    length: line.text[run.start..run.end].encode_utf16().count() as u32,
                };
                if run.style.bold {
                    layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range)?;
                }
                if run.style.italic {
                    layout.SetFontStyle(DWRITE_FONT_STYLE_ITALIC, range)?;
                }
                if run.style.code {
                    layout.SetFontFamilyName(w!("Cascadia Mono"), range)?;
                }
                if run.style.strike {
                    layout.SetStrikethrough(true, range)?;
                }
                if run.style.link.is_some() {
                    layout.SetUnderline(true, range)?;
                }
            }
            Ok(layout)
        }
    }
    unsafe fn text(
        &self,
        text: &str,
        font: Font,
        box_: D2D_RECT_F,
        rgb: u32,
        alpha: f32,
    ) -> Result<()> {
        unsafe {
            let target = self.target.as_ref().unwrap();
            let brush = self.brush.as_ref().unwrap();
            brush.SetColor(&color(rgb, alpha));
            let wide: Vec<u16> = text.encode_utf16().collect();
            let format = self.fonts.format(font)?;
            target.DrawText(
                &wide,
                &format,
                &box_,
                brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            Ok(())
        }
    }
    unsafe fn draw_diagram(
        &self,
        diagram: &crate::layout::DiagramBox,
        top: f32,
        palette: &Palette,
        alpha: f32,
    ) -> Result<()> {
        unsafe {
            let target = self.target.as_ref().unwrap();
            let brush = self.brush.as_ref().unwrap();
            let point = |(x, y): (f32, f32)| Vector2 {
                X: self.content_x() + x,
                Y: top + y - self.scroll,
            };
            brush.SetColor(&color(palette.accent, alpha));
            for arrow in &diagram.arrows {
                for pair in arrow.points.windows(2) {
                    target.DrawLine(point(pair[0]), point(pair[1]), brush, 1.5, None);
                }
                let a = arrow.points[2];
                let b = arrow.points[3];
                let length = (b.0 - a.0).hypot(b.1 - a.1).max(1.0);
                let (dx, dy) = ((b.0 - a.0) / length, (b.1 - a.1) / length);
                for side in [-1.0, 1.0] {
                    target.DrawLine(
                        point(b),
                        point((
                            b.0 - 7.0 * dx + side * 4.0 * dy,
                            b.1 - 7.0 * dy - side * 4.0 * dx,
                        )),
                        brush,
                        1.5,
                        None,
                    );
                }
            }
            for node in &diagram.nodes {
                let b = node.bounds;
                let y = top + b.y - self.scroll;
                if y + b.height < 106.0 || y > self.height - 46.0 {
                    continue;
                }
                let bounds = rect(self.content_x() + b.x, y, b.width, b.height);
                if node.shape == crate::diagram::Shape::Diamond {
                    let geometry = self.factory.CreatePathGeometry()?;
                    let sink = geometry.Open()?;
                    sink.BeginFigure(
                        Vector2 {
                            X: bounds.left + b.width / 2.0,
                            Y: bounds.top,
                        },
                        D2D1_FIGURE_BEGIN_FILLED,
                    );
                    sink.AddLines(&[
                        Vector2 {
                            X: bounds.right,
                            Y: bounds.top + b.height / 2.0,
                        },
                        Vector2 {
                            X: bounds.left + b.width / 2.0,
                            Y: bounds.bottom,
                        },
                        Vector2 {
                            X: bounds.left,
                            Y: bounds.top + b.height / 2.0,
                        },
                    ]);
                    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    sink.Close()?;
                    brush.SetColor(&color(palette.panel, alpha));
                    target.FillGeometry(&geometry, brush, None);
                    brush.SetColor(&color(palette.accent, alpha));
                    target.DrawGeometry(&geometry, brush, 1.5, None);
                } else {
                    let radius = if node.shape == crate::diagram::Shape::Rounded {
                        9.0
                    } else {
                        0.0
                    };
                    self.fill(bounds, radius, palette.panel, alpha);
                    brush.SetColor(&color(palette.accent, alpha));
                    target.DrawRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: bounds,
                            radiusX: radius,
                            radiusY: radius,
                        },
                        brush,
                        1.5,
                        None,
                    );
                }
            }
            for b in &diagram.labels {
                self.fill(
                    rect(
                        self.content_x() + b.x,
                        top + b.y - self.scroll,
                        b.width,
                        b.height,
                    ),
                    3.0,
                    palette.card,
                    alpha,
                );
            }
            Ok(())
        }
    }
    unsafe fn render(&mut self) -> Result<()> {
        unsafe {
            self.resources()?;
            self.reflow();
            let (progress, active) = self.motion.sample();
            if !active {
                let _ = KillTimer(Some(self.hwnd), 1);
            }
            let palette = Palette::new(self.state.light);
            let target = self.target.as_ref().unwrap();
            target.BeginDraw();
            target.Clear(Some(&color(palette.background, 1.0)));
            self.paints += 1;
            let tiny = Font {
                size: 11,
                bold: true,
                mono: false,
            };
            let small = Font {
                size: 13,
                bold: false,
                mono: false,
            };
            let label = Font {
                size: 14,
                bold: true,
                mono: false,
            };
            // Offset back plates give the document an actual card-deck silhouette.
            self.fill(
                rect(16.0, 62.0, self.width - 32.0, self.height - 84.0),
                16.0,
                palette.line,
                0.32,
            );
            self.fill(
                rect(11.0, 56.0, self.width - 22.0, self.height - 74.0),
                17.0,
                palette.panel,
                0.8,
            );
            self.fill(
                rect(8.0, 50.0, self.width - 16.0, self.height - 62.0),
                18.0,
                palette.card,
                1.0,
            );
            self.fill(rect(30.0, 48.0, 38.0, 3.0), 1.5, palette.accent, 0.85);
            self.text(
                "RUSTY NOTE",
                tiny,
                rect(
                    if self.options.editor_mode || self.options.inline_edit {
                        126.0
                    } else {
                        82.0
                    },
                    17.0,
                    (self.width
                        - if self.options.editor_mode {
                            246.0
                        } else {
                            206.0
                        })
                    .max(0.0),
                    20.0,
                ),
                palette.muted,
                1.0,
            )?;
            let title = self
                .notes
                .get(self.selected)
                .map(|n| n.title.as_str())
                .unwrap_or("Quiet corner");
            let show_badge = self.options.editor_mode && self.width - self.content_x() >= 330.0;
            self.text(
                title,
                label,
                rect(
                    self.content_x(),
                    74.0,
                    (self.width - self.content_x() - if show_badge { 138.0 } else { 28.0 })
                        .max(32.0),
                    25.0,
                ),
                palette.ink,
                1.0,
            )?;
            if show_badge {
                self.text(
                    "NATIVE CANVAS",
                    tiny,
                    rect(self.width - 124.0, 80.0, 106.0, 18.0),
                    palette.accent,
                    0.85,
                )?;
            }
            for (x, text, on) in [
                (
                    12.0,
                    if self.options.editor_mode {
                        "≡"
                    } else {
                        "Open"
                    },
                    self.library,
                ),
                (
                    48.0,
                    if self.editor.is_some() { "✓" } else { "✎" },
                    self.editor.is_some(),
                ),
                (86.0, "+", false),
                (self.width - 114.0, "P", self.state.pinned),
                (self.width - 78.0, "◐", self.state.light),
                (self.width - 42.0, "×", false),
            ] {
                if !self.options.editor_mode
                    && (48.0..116.0).contains(&x)
                    && !(self.options.inline_edit && x == 86.0)
                {
                    continue;
                }
                let open_button = !self.options.editor_mode && x == 12.0;
                let button_width = if open_button { 56.0 } else { 30.0 };
                let hovered =
                    self.hover.0 >= x && self.hover.0 < x + button_width && self.hover.1 < 43.0;
                self.fill(
                    rect(x, 9.0, button_width, 30.0),
                    8.0,
                    if on { palette.accent } else { palette.line },
                    if on {
                        0.18
                    } else if hovered {
                        0.7
                    } else {
                        0.3
                    },
                );
                self.text(
                    text,
                    Font {
                        size: if open_button { 13 } else { 17 },
                        bold: open_button,
                        mono: false,
                    },
                    rect(
                        x + 8.0,
                        if open_button { 15.0 } else { 12.0 },
                        button_width - 9.0,
                        25.0,
                    ),
                    if on { palette.accent } else { palette.ink },
                    1.0,
                )?;
            }
            if self.library {
                self.fill(
                    rect(15.0, 65.0, 145.0, self.height - 92.0),
                    13.0,
                    palette.background,
                    0.85,
                );
                self.text(
                    "YOUR NOTES",
                    tiny,
                    rect(28.0, 80.0, 125.0, 20.0),
                    palette.muted,
                    1.0,
                )?;
                for (i, note) in self
                    .notes
                    .iter()
                    .enumerate()
                    .skip(self.library_scroll)
                    .take(((self.height - 170.0) / 48.0).max(0.0) as usize)
                {
                    let y = 112.0 + (i - self.library_scroll) as f32 * 48.0;
                    let hovered =
                        self.hover.0 < 158.0 && self.hover.1 >= y && self.hover.1 < y + 40.0;
                    if i == self.selected || hovered {
                        self.fill(
                            rect(23.0, y, 129.0, 40.0),
                            8.0,
                            palette.accent,
                            if i == self.selected { 0.15 } else { 0.07 },
                        );
                    }
                    self.text(
                        &note.title,
                        small,
                        rect(34.0, y + 10.0, 109.0, 25.0),
                        if i == self.selected {
                            palette.accent
                        } else {
                            palette.ink
                        },
                        1.0,
                    )?;
                }
            }
            let content_x = self.content_x();
            let top = 116.0 + (1.0 - progress) * 20.0;
            target.PushAxisAlignedClip(
                &rect(
                    content_x - 4.0,
                    106.0,
                    self.width - content_x - 20.0,
                    self.height - 156.0,
                ),
                D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            );
            for cell in &self.scene.cells {
                let y = top + cell.y - self.scroll;
                self.fill(
                    rect(content_x + cell.x, y, cell.width, cell.height),
                    0.0,
                    if cell.header {
                        palette.panel
                    } else {
                        palette.card
                    },
                    progress,
                );
                self.fill(
                    rect(content_x + cell.x, y, cell.width, 1.0),
                    0.0,
                    palette.line,
                    progress,
                );
                self.fill(
                    rect(content_x + cell.x, y, 1.0, cell.height),
                    0.0,
                    palette.line,
                    progress,
                );
                self.fill(
                    rect(content_x + cell.x + cell.width - 1.0, y, 1.0, cell.height),
                    0.0,
                    palette.line,
                    progress,
                );
                self.fill(
                    rect(content_x + cell.x, y + cell.height - 1.0, cell.width, 1.0),
                    0.0,
                    palette.line,
                    progress,
                );
            }
            for y in &self.scene.rules {
                self.fill(
                    rect(
                        content_x,
                        top + y - self.scroll,
                        self.width - content_x - 28.0,
                        1.0,
                    ),
                    0.0,
                    palette.line,
                    progress,
                );
            }
            for image in &self.scene.images {
                let y = top + image.y - self.scroll;
                if y + image.height < 106.0 || y > self.height - 46.0 {
                    continue;
                }
                if let Some(bitmap) =
                    self.images
                        .load(target, &self.options.notes_dir, &image.source)
                {
                    let size = bitmap.GetSize();
                    let scale = (image.width / size.width).min(image.height / size.height);
                    let destination = rect(
                        content_x + (image.width - size.width * scale) * 0.5,
                        y,
                        size.width * scale,
                        size.height * scale,
                    );
                    target.DrawBitmap(
                        &bitmap,
                        Some(&destination),
                        progress,
                        D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                        None,
                    );
                    continue;
                }
                self.fill(
                    rect(content_x, y, image.width, image.height),
                    8.0,
                    palette.panel,
                    progress,
                );
                self.text(
                    &format!("Image: {}", image.alt),
                    small,
                    rect(content_x + 12.0, y + 12.0, image.width - 24.0, 24.0),
                    palette.muted,
                    progress,
                )?;
            }
            for diagram in &self.scene.diagrams {
                self.draw_diagram(diagram, top, &palette, progress)?;
            }
            let inline_origins = self.inline_origins();
            for (line_index, line) in self.scene.lines.iter().enumerate() {
                if inline_origins.contains(&line.origin) {
                    if line.source_start == 0
                        && let Some((_, prefix, width)) = self
                            .scene
                            .edit_prefixes
                            .iter()
                            .find(|(origin, _, _)| *origin == line.origin)
                    {
                        self.text(
                            prefix,
                            line.font,
                            rect(
                                content_x + line.x,
                                top + line.y - self.scroll,
                                *width + 2.0,
                                line.height + 4.0,
                            ),
                            palette.ink,
                            progress,
                        )?;
                    }
                    continue;
                }
                let y = top + line.y - self.scroll;
                if y + line.height < 106.0 || y > self.height - 46.0 {
                    continue;
                }
                let inset = line.x;
                if line.kind == Kind::Code {
                    self.fill(
                        rect(
                            content_x - 3.0,
                            y - 2.0,
                            self.width - content_x - 26.0,
                            line.height + 3.0,
                        ),
                        4.0,
                        palette.panel,
                        progress,
                    );
                }
                if line.kind == Kind::Quote {
                    self.fill(
                        rect(content_x, y + 2.0, 3.0, line.height - 4.0),
                        1.5,
                        palette.accent,
                        0.65 * progress,
                    );
                }
                let layout = self.line_layout(line)?;
                if self
                    .search_hits
                    .get(self.search_index)
                    .is_some_and(|hit| hit.lines.contains(&line_index))
                {
                    let mut metrics = DWRITE_TEXT_METRICS::default();
                    layout.GetMetrics(&mut metrics)?;
                    let left = metrics.left.max(0.0);
                    let right = (metrics.left + metrics.widthIncludingTrailingWhitespace)
                        .min(layout.GetMaxWidth());
                    self.fill(
                        rect(
                            content_x + inset + left,
                            y,
                            (right - left).max(0.0),
                            line.height,
                        ),
                        3.0,
                        palette.accent,
                        0.2,
                    );
                }
                let rgb = match line.kind {
                    Kind::Heading(_) => palette.ink,
                    Kind::Code => palette.accent,
                    Kind::Quote => palette.muted,
                    _ => palette.ink,
                };
                let brush = self.brush.as_ref().unwrap();
                brush.SetColor(&color(rgb, progress));
                target.DrawTextLayout(
                    Vector2 {
                        X: content_x + inset,
                        Y: y,
                    },
                    &layout,
                    brush,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                );
            }
            target.PopAxisAlignedClip();
            let size_hint = if self.options.inline_edit {
                let state = if self
                    .inline
                    .control
                    .as_ref()
                    .is_some_and(crate::inline::Paragraph::composing)
                {
                    "COMPOSING · draft checkpoint only"
                } else if self.inline.note.as_ref().is_some_and(|note| note.pending()) {
                    "INPUT RETAINED · repair before Save"
                } else if self.inline.note.as_ref().is_some_and(|note| note.dirty()) {
                    "WORKING DRAFT · Ctrl+S saves original"
                } else {
                    "CLICK TO EDIT · Ctrl+N new · Ctrl+S save"
                };
                format!("{state} · {}%", (self.state.zoom * 100.0).round() as u32)
            } else {
                format!(
                    "READ ONLY  ·  {}%  ·  Ctrl+wheel zoom",
                    (self.state.zoom * 100.0).round() as u32
                )
            };
            let find_status = if self.search_hits.is_empty() {
                "Find: no results  ·  Ctrl+F change  ·  Esc clear".to_owned()
            } else {
                format!(
                    "Find {}/{}{}  ·  F3 next  ·  Shift+F3 back",
                    self.search_index + 1,
                    self.search_hits.len(),
                    if self.search_hits.len() == 4096 {
                        " (limit)"
                    } else {
                        ""
                    }
                )
            };
            self.text(
                if self.error.is_some() {
                    if self.options.inline_edit {
                        "Draft / Save details · click status"
                    } else {
                        "Read error  ·  click for details  ·  F5 retry"
                    }
                } else if !self.search_query.is_empty() {
                    &find_status
                } else if !self.options.editor_mode && self.hover_region(self.hover) == -1 {
                    &size_hint
                } else {
                    self.reading_hint()
                },
                tiny,
                rect(27.0, self.height - 34.0, self.width - 60.0, 20.0),
                if self.error.is_some() {
                    palette.error
                } else {
                    palette.muted
                },
                1.0,
            )?;
            if let Some(bar) = crate::viewer::Scrollbar::new(self.scene.height, self.height) {
                self.fill(
                    rect(self.width - 20.0, bar.top, 8.0, bar.track),
                    4.0,
                    palette.line,
                    0.35,
                );
                self.fill(
                    rect(
                        self.width - 20.0,
                        bar.thumb_top(self.scroll),
                        8.0,
                        bar.thumb,
                    ),
                    4.0,
                    palette.accent,
                    if self.scroll_drag.is_some() {
                        0.85
                    } else {
                        0.55
                    },
                );
            }
            let result = target.EndDraw(None, None);
            if result.is_err() {
                self.target = None;
                self.brush = None;
            }
            result
        }
    }
    fn open_markdown(&mut self, path: PathBuf, explicit_open: bool) {
        if self.options.inline_edit && !self.prepare_inline_open(&path, explicit_open) {
            return;
        }
        match crate::viewer::open(&path) {
            Ok((note, text)) => {
                let same_file = self.options.file.as_ref() == Some(&note.path);
                let previous_scroll = if same_file { self.scroll } else { 0.0 };
                if same_file && self.viewer_source.as_deref() == Some(text.as_str()) {
                    let changed = self.images.refresh_changed();
                    let cleared_error = explicit_open && self.error.take().is_some();
                    if changed || cleared_error {
                        unsafe {
                            let _ = InvalidateRect(Some(self.hwnd), None, false);
                        }
                    }
                    return;
                }
                if !same_file {
                    self.search_index = 0;
                }
                self.viewer_source = Some(text.clone());
                self.options.notes_dir = note.path.parent().unwrap().to_owned();
                self.options.file = Some(note.path.clone());
                self.notes = vec![note];
                self.selected = 0;
                self.images.clear();
                self.scene =
                    Scene::with_zoom(&Document::parse(&text), &self.fonts, self.state.zoom);
                self.reflow();
                self.scroll = crate::viewer::reload_scroll(
                    previous_scroll,
                    self.scene.height,
                    self.height - 160.0,
                );
                self.error = None;
                self.restore_inline_note();
                unsafe {
                    let title: Vec<u16> = format!("Rusty Note — {}", self.notes[0].title)
                        .encode_utf16()
                        .chain([0])
                        .collect();
                    let _ = SetWindowTextW(self.hwnd, PCWSTR(title.as_ptr()));
                    self.animate();
                }
            }
            Err(error) => {
                self.error = Some(format!("Could not open {}\n\n{}", path.display(), error));
                unsafe {
                    let _ = InvalidateRect(Some(self.hwnd), None, false);
                }
            }
        }
    }
    fn change_zoom(&mut self, zoom: f32) {
        if self.options.editor_mode {
            return;
        }
        let zoom = crate::layout::reading_zoom(zoom);
        let Some(source) = self.viewer_source.as_deref() else {
            return;
        };
        if zoom == self.state.zoom {
            return;
        }
        let old = self.state.zoom;
        let previous_scroll = self.scroll;
        self.state.zoom = zoom;
        self.scene = Scene::with_zoom(&Document::parse(source), &self.fonts, zoom);
        self.scroll = previous_scroll * zoom / old;
        self.reflow();
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }
    unsafe fn show_error(&self) {
        if let Some(error) = &self.error {
            let text: Vec<u16> = error.encode_utf16().chain([0]).collect();
            unsafe {
                MessageBoxW(
                    Some(self.hwnd),
                    PCWSTR(text.as_ptr()),
                    w!("Rusty Note — read error"),
                    MB_OK | MB_ICONWARNING,
                );
            }
        }
    }
    fn focus_search_hit(&mut self) {
        if let Some(hit) = self.search_hits.get(self.search_index)
            && let Some(line) = hit
                .lines
                .first()
                .and_then(|index| self.scene.lines.get(*index))
        {
            self.scroll =
                crate::viewer::reload_scroll(line.y - 16.0, self.scene.height, self.height - 160.0);
        }
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }
    fn begin_find(&mut self) {
        if self.options.editor_mode || (self.options.inline_edit && !self.end_inline_paragraph()) {
            return;
        }
        if let Some(query) = crate::prompt::ask(self.hwnd, "Find in Markdown", &self.search_query) {
            let query = query.trim();
            if query.len() > 512 {
                unsafe {
                    MessageBoxW(
                        Some(self.hwnd),
                        w!("Find text must be at most 512 UTF-8 bytes."),
                        w!("Find in Markdown"),
                        MB_OK | MB_ICONINFORMATION,
                    );
                }
                return;
            }
            self.search_query = query.into();
            self.search_index = 0;
            self.search_hits = self.scene.find(&self.search_query);
            self.focus_search_hit();
        }
    }
    fn step_find(&mut self, back: bool) {
        if self.search_hits.is_empty() {
            return;
        }
        let count = self.search_hits.len();
        self.search_index = if back {
            (self.search_index + count - 1) % count
        } else {
            (self.search_index + 1) % count
        };
        self.focus_search_hit();
    }
    fn clear_find(&mut self) {
        self.search_query.clear();
        self.search_hits.clear();
        self.search_index = 0;
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }
    fn choose_markdown(&mut self) {
        if let Some(path) = crate::desktop::choose_markdown(self.hwnd) {
            self.open_markdown(path, true);
        }
    }
    unsafe fn actions_menu(&self) {
        unsafe {
            if let Ok(menu) = CreatePopupMenu() {
                for (id, label) in [
                    (202, "New note\tCtrl+N"),
                    (200, "Edit\tCtrl+E"),
                    (201, "Save\tCtrl+S"),
                    (203, "Read\tCtrl+Enter"),
                    (204, "Open notes folder"),
                    (205, "Choose folder\tCtrl+O"),
                    (206, "Hide to tray\tCtrl+H"),
                    (208, "Rename note\tF2"),
                    (209, "Move note to trash"),
                    (210, "Open note trash"),
                    (211, "Larger text\tCtrl++"),
                    (212, "Smaller text\tCtrl+-"),
                    (213, "Reset text size\tCtrl+0"),
                    (214, "Error details"),
                    (215, "Find in Markdown\tCtrl+F"),
                    (216, "Next result\tF3"),
                    (217, "Previous result\tShift+F3"),
                    (218, "Clear find\tEsc"),
                    (207, "Quit"),
                ] {
                    if self.options.editor_mode && id >= 211 {
                        continue;
                    }
                    if id == 214 && self.error.is_none() {
                        continue;
                    }
                    if !self.options.editor_mode
                        && !(self.options.inline_edit && [201, 202].contains(&id))
                        && ![204, 205, 206, 207, 211, 212, 213, 214, 215, 216, 217, 218]
                            .contains(&id)
                    {
                        continue;
                    }
                    let label = if !self.options.editor_mode && id == 205 {
                        "Open Markdown…\tCtrl+O"
                    } else {
                        label
                    };
                    let wide: Vec<u16> = label.encode_utf16().chain([0]).collect();
                    let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR(wide.as_ptr()));
                }
                if self.options.inline_edit && self.state.inline_draft.is_some() {
                    let _ = AppendMenuW(menu, MF_STRING, 219, w!("Discard active draft…"));
                }
                let mut point = POINT::default();
                let _ = GetCursorPos(&mut point);
                let _ = SetForegroundWindow(self.hwnd);
                let command = TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    point.x,
                    point.y,
                    None,
                    self.hwnd,
                    None,
                );
                let _ = DestroyMenu(menu);
                if command.0 != 0 {
                    let _ = PostMessageW(
                        Some(self.hwnd),
                        WM_COMMAND,
                        WPARAM(command.0 as usize),
                        LPARAM(0),
                    );
                }
            }
        }
    }
    fn start_edit(&mut self) {
        if !self.options.editor_mode {
            return;
        }
        if self.editor.is_some() {
            return;
        }
        let Some(note) = self.notes.get(self.selected) else {
            return;
        };
        match crate::editor::Editor::open(
            self.hwnd,
            &note.path,
            self.width,
            self.height,
            self.scale,
        ) {
            Ok(editor) => {
                self.library = false;
                self.editor = Some(editor);
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }
    fn save_edit(&mut self) -> bool {
        let Some(editor) = self.editor.as_mut() else {
            return true;
        };
        match editor.save() {
            Ok(()) => {
                self.scene = Scene::new(&Document::parse(&editor.buffer.text), &self.fonts);
                self.reflow();
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                unsafe {
                    MessageBoxW(
                        Some(self.hwnd),
                        PCWSTR(
                            error
                                .to_string()
                                .encode_utf16()
                                .chain([0])
                                .collect::<Vec<_>>()
                                .as_ptr(),
                        ),
                        w!("Save blocked"),
                        MB_OK | MB_ICONWARNING,
                    );
                };
                false
            }
        }
    }
    fn finish_edit(&mut self) -> bool {
        if let Some(editor) = self.editor.as_mut() {
            editor.sync();
            if editor.buffer.dirty() {
                let answer = unsafe {
                    MessageBoxW(
                        Some(self.hwnd),
                        w!("Save changes before leaving this note?"),
                        w!("Rusty Note"),
                        MB_YESNOCANCEL | MB_ICONQUESTION,
                    )
                };
                if answer == IDCANCEL {
                    return false;
                }
                if answer == IDYES && !self.save_edit() {
                    return false;
                }
            }
        }
        self.editor = None;
        true
    }
    fn new_note(&mut self) {
        if !self.options.editor_mode {
            return;
        }
        if !self.finish_edit() {
            return;
        }
        match crate::edit::create_note(&self.options.notes_dir) {
            Ok(path) => {
                self.notes = store::list_notes(&self.options.notes_dir).unwrap_or_default();
                if let Some(index) = self.notes.iter().position(|n| n.path == path) {
                    unsafe { self.select(index) };
                    self.start_edit();
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    unsafe fn click(&mut self, x: f32, y: f32) {
        unsafe {
            if y < 43.0 {
                if x < if self.options.editor_mode { 44.0 } else { 72.0 } {
                    if !self.options.editor_mode {
                        self.choose_markdown();
                        return;
                    }
                    if !self.finish_edit() {
                        return;
                    }
                    self.library = !self.library;
                    self.reflow();
                    self.animate();
                } else if self.options.editor_mode && (48.0..78.0).contains(&x) {
                    if self.editor.is_some() {
                        if self.save_edit() {
                            self.editor = None;
                            self.animate();
                        }
                    } else {
                        self.start_edit();
                    }
                } else if self.options.inline_edit && (86.0..116.0).contains(&x) {
                    self.new_inline_note();
                } else if self.options.editor_mode && (86.0..116.0).contains(&x) {
                    self.new_note();
                } else if x > self.width - 42.0 {
                    let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                } else if x > self.width - 78.0 {
                    self.state.light = !self.state.light;
                    self.animate();
                } else if x > self.width - 114.0 {
                    self.state.pinned = !self.state.pinned;
                    let _ = SetWindowPos(
                        self.hwnd,
                        Some(if self.state.pinned {
                            HWND_TOPMOST
                        } else {
                            HWND_NOTOPMOST
                        }),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                    let _ = InvalidateRect(Some(self.hwnd), None, false);
                }
            } else if self.error.is_some() && y > self.height - 42.0 {
                self.show_error();
            } else if self.library && x < 160.0 && y >= 112.0 {
                self.select(self.library_scroll + ((y - 112.0) / 48.0) as usize);
            } else if self.open_image_at(x, y) {
                // The owned image window handled this click.
            } else if self.options.inline_edit
                && y >= 106.0
                && y <= self.height - 46.0
                && x >= self.content_x()
                && x < self.width - 28.0
            {
                if !self.toggle_inline_task(x, y) {
                    self.start_inline_at(x, y);
                }
            } else if let Some(url) = self.link_at(x, y) {
                crate::desktop::open_link(self.hwnd, url);
            }
        }
    }
    fn refresh_notes(&mut self) {
        if !self.options.editor_mode {
            if let Some(path) = self.options.file.clone() {
                self.open_markdown(path, false);
            }
            return;
        }
        let path = self
            .editor
            .as_ref()
            .map(|e| e.buffer.path.clone())
            .or_else(|| self.notes.get(self.selected).map(|n| n.path.clone()));
        match store::list_notes(&self.options.notes_dir) {
            Ok(notes) => {
                self.notes = notes;
                self.selected = path
                    .as_ref()
                    .and_then(|p| self.notes.iter().position(|n| &n.path == p))
                    .unwrap_or(0);
                if self.editor.is_none() {
                    if self.notes.is_empty() {
                        self.scene = Scene::new(
                            &Document::parse(
                                "# Your quiet corner\n\nUse + to create a note or choose a folder with Ctrl+O.",
                            ),
                            &self.fonts,
                        );
                        self.images.clear();
                        self.reflow();
                    }
                    if let Some(note) = self.notes.get(self.selected)
                        && let Ok(buffer) = crate::edit::EditBuffer::open(&note.path)
                    {
                        self.scene = Scene::new(&Document::parse(&buffer.text), &self.fonts);
                        self.images.clear();
                        self.reflow();
                    }
                } else {
                    self.error = Some("Source changed — Save checks for conflicts".into());
                }
                unsafe {
                    let _ = InvalidateRect(Some(self.hwnd), None, false);
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    unsafe fn link_at(&self, x: f32, y: f32) -> Option<&str> {
        unsafe {
            if self.editor.is_some()
                || y < 106.0
                || y > self.height - 46.0
                || x < self.content_x()
                || x > self.width - 28.0
            {
                return None;
            }
            let local_y = y - 116.0 + self.scroll;
            for line in &self.scene.lines {
                let local_x = x - self.content_x() - line.x;
                if local_y < line.y
                    || local_y >= line.y + line.height
                    || local_x < 0.0
                    || local_x > line.layout_width
                {
                    continue;
                }
                let Ok(layout) = self.line_layout(line) else {
                    continue;
                };
                let mut trailing = windows::core::BOOL::default();
                let mut inside = windows::core::BOOL::default();
                let mut hit = DWRITE_HIT_TEST_METRICS::default();
                if layout
                    .HitTestPoint(
                        local_x,
                        local_y - line.y,
                        &mut trailing,
                        &mut inside,
                        &mut hit,
                    )
                    .is_err()
                    || !inside.as_bool()
                {
                    continue;
                }
                for run in &line.runs {
                    let start = line.text[..run.start].encode_utf16().count() as u32;
                    let end = line.text[..run.end].encode_utf16().count() as u32;
                    if hit.textPosition >= start
                        && hit.textPosition < end
                        && let Some(url) = run.style.link.as_deref()
                    {
                        return Some(url);
                    }
                }
            }
            None
        }
    }
    fn metrics(&self) -> serde_json::Value {
        let table_geometry:Vec<_>=self.scene.lines.iter().enumerate().filter(|(_,line)|line.kind==Kind::Table).filter_map(|(index,line)|unsafe {
            let layout=self.line_layout(line).ok()?;let mut metrics=DWRITE_TEXT_METRICS::default();layout.GetMetrics(&mut metrics).ok()?;
            Some(serde_json::json!({"line":index,"origin":line.origin,"x":self.content_x()+line.x,"y":116.0+line.y-self.scroll,"height":line.height,"box_width":layout.GetMaxWidth(),"ink_left":metrics.left,"ink_width":metrics.width,"alignment":format!("{:?}",line.alignment),"linked":line.runs.iter().any(|r|r.style.link.is_some())}))
        }).collect();
        let diagram_geometry: Vec<_> = self.scene.lines.iter().filter(|l| l.kind == Kind::Diagram).filter_map(|line| unsafe {
            let layout = self.line_layout(line).ok()?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics).ok()?;
            Some(serde_json::json!({"x":line.x,"y":line.y,"box_width":layout.GetMaxWidth(),"ink_width":metrics.widthIncludingTrailingWhitespace,"ink_height":metrics.height,"height":line.height,"font_size":line.font.size}))
        }).collect();
        let diagram_nodes: Vec<_> = self.scene.diagrams.iter().flat_map(|d| &d.nodes).map(|n| {
            let b = n.bounds;
            serde_json::json!({"x":b.x,"y":b.y,"width":b.width,"height":b.height,"shape":format!("{:?}",n.shape)})
        }).collect();
        serde_json::json!({"image_open":self.image_popup.is_some_and(crate::image_viewer::is_view),"inline":self.inline_metrics(),"diagram_prepared":self.scene.diagram_prepared,"diagram_rendered":self.scene.diagrams.len(),"diagram_nodes":diagram_nodes,"diagram_arrows":self.scene.diagrams.iter().map(|d|d.arrows.len()).sum::<usize>(),"diagram_geometry":diagram_geometry,"table_geometry":table_geometry,"backend":"Direct2D + DirectWrite", "layout":"gpui-pretext 0.6.2", "notes":self.notes.len(),
            "font_measurements":self.fonts.measurements.get(),"prepare_blocks":self.scene.prepare_passes,
            "layout_passes":self.scene.layout_passes,"paint_count":self.paints,"animation_active":self.motion.active(),
            "lines":self.scene.lines.len(),"width_dip":self.width,"height_dip":self.height,
            "selected":self.notes.get(self.selected).map(|n| &n.title),"read_only":!self.options.editor_mode && !self.options.inline_edit,"scroll":self.scroll,"pinned":self.state.pinned,"light":self.state.light,"hint":self.reading_hint(),"scroll_thumb_top":crate::viewer::Scrollbar::new(self.scene.height,self.height).map(|bar|bar.thumb_top(self.scroll)),"scroll_thumb_size":crate::viewer::Scrollbar::new(self.scene.height,self.height).map(|bar|bar.thumb),"scale":self.scale,"zoom":self.state.zoom,"find_count":self.search_hits.len(),"find_index":self.search_index,"find_active":!self.search_query.is_empty(),"error":self.error,"table_cells":self.scene.cells.len(),"images":self.scene.images.len(),"styled_runs":self.scene.lines.iter().map(|l|l.runs.iter().filter(|r|r.style.bold||r.style.italic||r.style.code||r.style.link.is_some()||r.style.strike).count()).sum::<usize>()})
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<App>;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, w, l);
        }
        if msg == WM_NCDESTROY {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(ptr));
            return DefWindowProcW(hwnd, msg, w, l);
        }
        let Ok(mut app) = (*ptr).try_borrow_mut() else {
            // A modal owner dialog holds App; a reentrant close must not destroy it.
            if msg == WM_CLOSE {
                return LRESULT(0);
            }
            return DefWindowProcW(hwnd, msg, w, l);
        };
        match msg {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut paint);
                if let Err(error) = app.render() {
                    eprintln!("render: {error}");
                }
                let _ = EndPaint(hwnd, &paint);
                app.queue_inline_paint();
                LRESULT(0)
            }
            WM_SIZE => {
                app.width = (l.0 as u32 & 0xffff) as f32 / app.scale;
                app.height = ((l.0 as u32 >> 16) & 0xffff) as f32 / app.scale;
                if app.width > 0.0 && app.height > 0.0 {
                    if let Some(target) = &app.target
                        && target
                            .Resize(&D2D_SIZE_U {
                                width: (app.width * app.scale) as u32,
                                height: (app.height * app.scale) as u32,
                            })
                            .is_err()
                    {
                        app.target = None;
                        app.brush = None;
                    }
                    if let Some(editor) = &app.editor {
                        editor.resize(app.width, app.height, app.scale);
                    }
                    app.reflow();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            0x8007
                if app.options.metrics_file.is_some() && app.options.smoke_drop_file.is_some() =>
            {
                let path = app.options.smoke_drop_file.clone().unwrap();
                let paths = if w.0 == 2 {
                    vec![path.clone(), path]
                } else {
                    vec![path]
                };
                match crate::desktop::diagnostic_drop(&paths) {
                    Ok(payload) => {
                        drop(app);
                        SendMessageW(hwnd, WM_DROPFILES, Some(WPARAM(payload.0 as usize)), None);
                    }
                    Err(error) => {
                        app.error = Some(error.to_string());
                    }
                }
                LRESULT(0)
            }
            WM_DROPFILES => {
                let dropped = crate::desktop::dropped_markdown(windows::Win32::UI::Shell::HDROP(
                    w.0 as *mut _,
                ));
                if !app.options.editor_mode {
                    match dropped {
                        Ok(path) => app.open_markdown(path, true),
                        Err(error) => {
                            app.error = Some(error.to_string());
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                    }
                }
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                drop(app);
                crate::desktop::fit_window_to_work_area(hwnd);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                app.images.clear();
                app.scale = (w.0 >> 16) as f32 / 96.0;
                app.target = None;
                app.brush = None;
                let r = *(l.0 as *const RECT);
                drop(app);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let mut bounds = RECT::default();
                let _ = GetWindowRect(hwnd, &mut bounds);
                let x = ((l.0 as u32 & 0xffff) as i16 as i32 - bounds.left) as f32 / app.scale;
                let y =
                    (((l.0 as u32 >> 16) & 0xffff) as i16 as i32 - bounds.top) as f32 / app.scale;
                let right = app.width;
                let bottom = app.height;
                let hit = if y < 6.0 {
                    if x < 8.0 {
                        HTTOPLEFT
                    } else if x > right - 8.0 {
                        HTTOPRIGHT
                    } else {
                        HTTOP
                    }
                } else if y > bottom - 6.0 {
                    if x < 8.0 {
                        HTBOTTOMLEFT
                    } else if x > right - 8.0 {
                        HTBOTTOMRIGHT
                    } else {
                        HTBOTTOM
                    }
                } else if x < 6.0 {
                    HTLEFT
                } else if x > right - 6.0 {
                    HTRIGHT
                } else if y < 45.0
                    && x > if app.options.editor_mode || app.options.inline_edit {
                        120.0
                    } else {
                        76.0
                    }
                    && x < right - 118.0
                {
                    HTCAPTION
                } else {
                    HTCLIENT
                };
                LRESULT(hit as isize)
            }
            WM_GETMINMAXINFO => {
                let limits = &mut *(l.0 as *mut MINMAXINFO);
                limits.ptMinTrackSize = POINT {
                    x: (340.0 * app.scale) as i32,
                    y: (420.0 * app.scale) as i32,
                };
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let old_region = app.hover_region(app.hover);
                app.hover = (
                    (l.0 as u32 & 0xffff) as i16 as f32 / app.scale,
                    ((l.0 as u32 >> 16) & 0xffff) as i16 as f32 / app.scale,
                );
                let track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&track as *const _ as *mut _);
                if let Some(grab) = app.scroll_drag
                    && let Some(bar) = crate::viewer::Scrollbar::new(app.scene.height, app.height)
                {
                    let scroll = bar.scroll_at(app.hover.1, grab);
                    if scroll != app.scroll {
                        app.scroll = scroll;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    return LRESULT(0);
                }
                if old_region != app.hover_region(app.hover) {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            0x02a3 => {
                // WM_MOUSELEAVE (not a wildcard pattern)
                let old_region = app.hover_region(app.hover);
                app.hover = (-1.0, -1.0);
                if old_region != -1 {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let x = (l.0 as u32 & 0xffff) as i16 as f32 / app.scale;
                let y = ((l.0 as u32 >> 16) & 0xffff) as i16 as f32 / app.scale;
                if app.editor.is_none()
                    && let Some(bar) = crate::viewer::Scrollbar::new(app.scene.height, app.height)
                    && x >= app.width - 25.0
                    && x <= app.width - 5.0
                    && y >= bar.top
                    && y <= bar.top + bar.track
                {
                    let top = bar.thumb_top(app.scroll);
                    let grab = if y >= top && y <= top + bar.thumb {
                        y - top
                    } else {
                        bar.thumb / 2.0
                    };
                    app.scroll_drag = Some(grab);
                    app.scroll = bar.scroll_at(y, grab);
                    let _ = SetCapture(hwnd);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_CAPTURECHANGED | WM_CANCELMODE => {
                if app.scroll_drag.take().is_some() {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if app.scroll_drag.take().is_some() {
                    drop(app);
                    let _ = ReleaseCapture();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                let x = (l.0 as u32 & 0xffff) as i16 as f32 / app.scale;
                let y = ((l.0 as u32 >> 16) & 0xffff) as i16 as f32 / app.scale;
                app.click(x, y);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let delta = ((w.0 >> 16) & 0xffff) as i16 as f32;
                if !app.options.editor_mode && GetKeyState(0x11) < 0 {
                    let zoom = app.state.zoom + if delta > 0.0 { 0.1 } else { -0.1 };
                    app.change_zoom(zoom);
                    return LRESULT(0);
                }
                if app.library && app.hover.0 < 160.0 {
                    app.library_scroll = if delta < 0.0 {
                        (app.library_scroll + 1).min(app.notes.len().saturating_sub(1))
                    } else {
                        app.library_scroll.saturating_sub(1)
                    };
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return LRESULT(0);
                }
                app.scroll = (app.scroll - delta / 120.0 * 72.0)
                    .clamp(0.0, (app.scene.height - (app.height - 160.0)).max(0.0));
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                match w.0 as u16 {
                    0x1b => {
                        if app.options.inline_edit && app.inline.control.is_some() {
                            app.end_inline_paragraph();
                            return LRESULT(0);
                        }
                        if !app.options.editor_mode {
                            app.clear_find();
                            return LRESULT(0);
                        }
                        app.library = false;
                        app.reflow();
                        app.animate();
                    }
                    0x09 if app.options.editor_mode => {
                        app.library = !app.library;
                        app.reflow();
                        app.animate();
                    }
                    0x72 if !app.options.editor_mode => {
                        app.step_find(GetKeyState(0x10) < 0);
                    }
                    0x71 => {
                        let _ = PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(208), LPARAM(0));
                    }
                    0x74 => {
                        if app.options.editor_mode {
                            let index = app.selected;
                            app.select(index);
                        } else if let Some(path) = app.options.file.clone() {
                            app.open_markdown(path, true);
                        }
                    }
                    0x27 if app.options.editor_mode => {
                        let index = (app.selected + 1) % app.notes.len().max(1);
                        app.select(index);
                    }
                    0x25 if app.options.editor_mode => {
                        let index = app.selected.saturating_sub(1);
                        app.select(index);
                    }
                    0x50 if !app.options.editor_mode => {
                        let width = app.width;
                        app.click(width - 100.0, 20.0);
                    }
                    0x54 if !app.options.editor_mode => {
                        let width = app.width;
                        app.click(width - 64.0, 20.0);
                    }
                    0x26 | 0x28 if !app.options.editor_mode => {
                        let delta = if w.0 == 0x28 { 32.0 } else { -32.0 };
                        app.scroll = crate::viewer::reload_scroll(
                            app.scroll + delta,
                            app.scene.height,
                            app.height - 160.0,
                        );
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    0x21 | 0x22 => {
                        let delta =
                            (app.height - 190.0).max(100.0) * if w.0 == 0x22 { 1.0 } else { -1.0 };
                        app.scroll = (app.scroll + delta)
                            .clamp(0.0, (app.scene.height - (app.height - 160.0)).max(0.0));
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    0x23 => {
                        app.scroll = (app.scene.height - (app.height - 160.0)).max(0.0);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    0x24 => {
                        app.scroll = 0.0;
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            inline_host::SYNC => {
                if w.0 == 2 || w.0 == 3 {
                    app.toggle_inline_style(w.0 == 2);
                } else {
                    app.sync_inline();
                }
                LRESULT(0)
            }
            inline_host::PAINT => {
                app.repaint_inline_child();
                LRESULT(0)
            }
            inline_host::PROBE if app.options.inline_edit && app.options.metrics_file.is_some() => {
                app.inline_probe(w.0, l.0);
                LRESULT(0)
            }
            WM_TIMER => {
                if w.0 == inline_host::CHECKPOINT {
                    app.checkpoint_inline();
                } else if w.0 == 1 {
                    let (_, active) = app.motion.sample();
                    if !active {
                        let _ = KillTimer(Some(hwnd), 1);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else if w.0 == 2 {
                    let _ = KillTimer(Some(hwnd), 2);
                    if let Some(path) = &app.options.smoke_output {
                        if let Some(parent) = path.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        let _ = fs::write(path, serde_json::to_vec_pretty(&app.metrics()).unwrap());
                    }
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                LRESULT(0)
            }
            0x8001 => {
                // explicit diagnostics request, no repaint/timer
                if let Some(path) = &app.options.metrics_file {
                    if let Some(parent) = path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    if let Err(error) =
                        fs::write(path, serde_json::to_vec_pretty(&app.metrics()).unwrap())
                    {
                        eprintln!("metrics: {error}");
                    }
                }
                LRESULT(0)
            }
            0x8008 if app.options.metrics_file.is_some() => LRESULT(
                app.link_at((l.0 as i16) as f32, ((l.0 >> 16) as i16) as f32)
                    .is_some() as isize,
            ),
            0x8006 => {
                app.refresh_notes();
                LRESULT(0)
            }
            crate::desktop::TRAY_MESSAGE => {
                if l.0 as u32 == WM_LBUTTONDBLCLK {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                } else if l.0 as u32 == WM_RBUTTONUP {
                    app.actions_menu();
                }
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                app.actions_menu();
                LRESULT(0)
            }
            WM_COMMAND => {
                let command = w.0 & 0xffff;
                if command == inline_host::CONTROL_ID {
                    // EN_UPDATE during paint must not invalidate the parent again.
                    if w.0 >> 16 == 0x0300 {
                        let _ = PostMessageW(Some(hwnd), inline_host::SYNC, WPARAM(0), LPARAM(0));
                    }
                    return LRESULT(0);
                }
                if app.options.inline_edit && command == 202 {
                    app.new_inline_note();
                    return LRESULT(0);
                }
                if app.options.inline_edit && command == 230 {
                    app.undo_inline(false);
                    return LRESULT(0);
                }
                if app.options.inline_edit && command == 231 {
                    app.undo_inline(true);
                    return LRESULT(0);
                }
                if app.options.inline_edit && command == 201 {
                    app.save_inline();
                    return LRESULT(0);
                }
                if app.options.inline_edit && command == 219 {
                    app.confirm_discard_inline();
                    return LRESULT(0);
                }
                if app.options.inline_edit && command == 220 {
                    app.end_inline_paragraph();
                    return LRESULT(0);
                }
                if !app.options.editor_mode
                    && [200, 201, 202, 203, 208, 209, 210].contains(&command)
                {
                    return LRESULT(0);
                }
                if command == 211 {
                    let zoom = app.state.zoom + 0.1;
                    app.change_zoom(zoom);
                } else if command == 212 {
                    let zoom = app.state.zoom - 0.1;
                    app.change_zoom(zoom);
                } else if command == 213 {
                    app.change_zoom(1.0);
                } else if command == 214 {
                    app.show_error();
                } else if command == 215 {
                    app.begin_find();
                } else if command == 216 {
                    app.step_find(false);
                } else if command == 217 {
                    app.step_find(true);
                } else if command == 218 {
                    app.clear_find();
                } else if command == 205 && !app.options.editor_mode {
                    app.choose_markdown();
                } else if command == 204 {
                    crate::desktop::open_folder(hwnd, &app.options.notes_dir);
                } else if command == 205 && app.finish_edit() {
                    if let Some(path) = crate::desktop::choose_folder(hwnd) {
                        app.options.notes_dir = path;
                        app.refresh_notes();
                        app.persist();
                    }
                } else if command == 206 {
                    if app.tray_available {
                        app.persist();
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    } else {
                        app.error =
                            Some("Tray is unavailable; the note window stays visible".into());
                    }
                } else if command == 208 && app.finish_edit() {
                    let path = app.notes.get(app.selected).map(|n| n.path.clone());
                    if let Some(path) = path
                        && let Some(name) = crate::prompt::ask(
                            hwnd,
                            "Rename note",
                            path.file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .as_ref(),
                        )
                    {
                        match crate::notes::rename(&path, &name) {
                            Ok(destination) => {
                                app.refresh_notes();
                                if let Some(i) =
                                    app.notes.iter().position(|n| n.path == destination)
                                {
                                    app.select(i);
                                }
                            }
                            Err(error) => app.error = Some(error.to_string()),
                        }
                    }
                } else if command == 209 && app.finish_edit() {
                    if MessageBoxW(
                        Some(hwnd),
                        w!("Move this note to the local note trash? It can be restored manually."),
                        w!("Rusty Note"),
                        MB_YESNO | MB_ICONQUESTION,
                    ) == IDYES
                        && let Some(note) = app.notes.get(app.selected)
                    {
                        if let Err(error) = crate::notes::trash(&note.path) {
                            app.error = Some(error.to_string());
                        } else {
                            app.refresh_notes();
                        }
                    }
                } else if command == 210 {
                    let path = app.options.notes_dir.join(".rusty-note-trash");
                    let _ = fs::create_dir_all(&path);
                    crate::desktop::open_folder(hwnd, &path);
                } else if command == 207 {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                } else if w.0 & 0xffff == 200 {
                    app.start_edit();
                } else if w.0 & 0xffff == 201 {
                    app.save_edit();
                } else if w.0 & 0xffff == 202 {
                    app.new_note();
                } else if w.0 & 0xffff == 203 && app.finish_edit() {
                    app.animate();
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_CTLCOLOREDIT => {
                let light = app.state.light;
                if let Some(editor) = app.editor.as_mut() {
                    let brush = editor.paint_background(HDC(w.0 as *mut _), light);
                    return LRESULT(brush.0 as isize);
                }
                drop(app);
                DefWindowProcW(hwnd, msg, w, l)
            }
            WM_EXITSIZEMOVE => {
                app.state.notes_dir = Some(app.options.notes_dir.clone());
                app.persist();
                LRESULT(0)
            }
            WM_CLOSE => {
                if !app.leave_inline(true) || !app.finish_edit() {
                    return LRESULT(0);
                }
                app.persist();
                drop(app);
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => {
                drop(app);
                DefWindowProcW(hwnd, msg, w, l)
            }
        }
    }
}

pub fn run(options: Options) -> Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_proc),
            hIcon: crate::desktop::app_icon(false)?,
            hIconSm: crate::desktop::app_icon(true)?,
            hInstance: HINSTANCE(instance.0),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: w!("RustyNote.Native"),
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err(windows::core::Error::from_hresult(
                windows::core::HRESULT::from_win32(GetLastError().0),
            ));
        }
        let cell = Box::new(RefCell::new(App::new(options)?));
        let state = cell.borrow().state.clone();
        let mut position = RECT {
            left: state.x,
            top: state.y,
            right: state.x + state.width,
            bottom: state.y + state.height,
        };
        let monitor = MonitorFromRect(&position, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let mut dpi_x = 96;
            let mut dpi_y = 96;
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
            let scale = dpi_x as f32 / 96.0;
            let fitted = crate::placement::Placement {
                x: state.x,
                y: state.y,
                width: (state.width as f32 * scale).round() as i32,
                height: (state.height as f32 * scale).round() as i32,
            }
            .fit(
                info.rcWork.left,
                info.rcWork.top,
                info.rcWork.right,
                info.rcWork.bottom,
            );
            position = RECT {
                left: fitted.x,
                top: fitted.y,
                right: fitted.x + fitted.width,
                bottom: fitted.y + fitted.height,
            };
        }
        let hwnd = CreateWindowExW(
            if state.pinned {
                WS_EX_TOPMOST
            } else {
                WINDOW_EX_STYLE(0)
            },
            w!("RustyNote.Native"),
            w!("Rusty Note"),
            WS_POPUP | WS_THICKFRAME | WS_MINIMIZEBOX,
            position.left,
            position.top,
            position.right - position.left,
            position.bottom - position.top,
            None,
            None,
            Some(HINSTANCE(instance.0)),
            None,
        )?;
        cell.borrow_mut().hwnd = hwnd;
        windows::Win32::UI::Shell::DragAcceptFiles(hwnd, !cell.borrow().options.editor_mode);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(cell) as isize);
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const _,
            std::mem::size_of_val(&corner) as u32,
        );
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<App>;
        {
            let mut app = (*ptr).borrow_mut();
            app.resources()?;
            let title = format!(
                "Rusty Note — {}",
                app.notes
                    .get(app.selected)
                    .map(|note| note.title.as_str())
                    .unwrap_or("Quiet corner")
            );
            let wide: Vec<u16> = title.encode_utf16().chain([0]).collect();
            let _ = SetWindowTextW(hwnd, PCWSTR(wide.as_ptr()));
            app.reflow();
            app.restore_inline_control();
            app.animate();
            if app.options.smoke_output.is_some() {
                SetTimer(Some(hwnd), 2, 1400, None);
            }
        }
        let tray = crate::desktop::Tray::new(hwnd).ok();
        (*ptr).borrow_mut().tray_available = tray.is_some();
        let _tray = tray;
        let mut watched = (*ptr).borrow().options.notes_dir.clone();
        let mut watch = crate::desktop::Watch::new(&watched).ok();
        let mut message = MSG::default();
        loop {
            if !IsWindow(Some(hwnd)).as_bool() {
                let result = GetMessageW(&mut message, None, 0, 0);
                if result.0 <= 0 {
                    break;
                }
                continue;
            }
            let directory = (*ptr).borrow().options.notes_dir.clone();
            if directory != watched {
                watched = directory;
                watch = crate::desktop::Watch::new(&watched).ok();
            }
            if let Some(watch) = &watch {
                let wait = MsgWaitForMultipleObjectsEx(
                    Some(&[watch.handle()]),
                    u32::MAX,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                );
                if wait.0 == 0 {
                    watch.rearm();
                    SendMessageW(hwnd, 0x8006, None, None);
                    continue;
                }
            }
            if watch.is_some() {
                // A sent message can wake the wait without leaving a queued message.
                // GetMessage would then block past directory notifications; Peek returns
                // us to the native handle wait without an idle polling/render loop.
                if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    continue;
                }
                if message.message == WM_QUIT {
                    break;
                }
            } else if GetMessageW(&mut message, None, 0, 0).0 <= 0 {
                break;
            }
            if message.message == WM_KEYDOWN && GetKeyState(0x11) < 0 {
                let command = match message.wParam.0 {
                    0x5a if (*ptr).borrow().options.inline_edit => Some(230),
                    0x59 if (*ptr).borrow().options.inline_edit => Some(231),
                    0x46 => Some(215),
                    0x45 => Some(200),
                    0x53 => Some(201),
                    0x4e => Some(202),
                    0x0d => Some(203),
                    0x4f => Some(205),
                    0x48 => Some(206),
                    0xBB | 0x6B => Some(211),
                    0xBD | 0x6D => Some(212),
                    0x30 | 0x60 => Some(213),
                    _ => None,
                };
                if let Some(command) = command {
                    SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(command)), None);
                    continue;
                }
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        CoUninitialize();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mermaid_uses_directwrite_and_keeps_literal_fallback_at_narrow_width() {
        let fonts = Fonts {
            factory: unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).unwrap() },
            formats: RefCell::new(HashMap::new()),
            measurements: Cell::new(0),
        };
        let source = "graph TD\n    A-->B\n    click A \"https://example.com/never-opened\"\n";
        let mut scene = Scene::new(
            &Document::parse(&format!("```mermaid\n{source}```\n")),
            &fonts,
        );
        assert!(fonts.measurements.get() > 0);
        for width in [530.0, 290.0, 48.0] {
            let calls = fonts.measurements.get();
            scene.reflow(width);
            assert_eq!(fonts.measurements.get(), calls);
            let visible: String = scene
                .lines
                .iter()
                .flat_map(|l| l.text.chars())
                .filter(|c| !c.is_whitespace())
                .collect();
            let expected: String = source.chars().filter(|c| !c.is_whitespace()).collect();
            assert_eq!(visible, expected);
            for line in &scene.lines {
                assert!(
                    fonts.width(&line.text, line.font) <= f64::from(line.layout_width) + 0.75,
                    "{}: cached {}, actual {}",
                    line.text,
                    line.layout_width,
                    fonts.width(&line.text, line.font)
                );
            }
        }
        let doc = Document::parse(include_str!("../tests/fixtures/mermaid.md"));
        for zoom in [1.0, 1.4] {
            let mut scene = Scene::with_zoom(&doc, &fonts, zoom);
            for width in [530.0, 290.0] {
                let calls = fonts.measurements.get();
                scene.reflow(width);
                assert_eq!(fonts.measurements.get(), calls);
                assert!(scene.diagrams.len() >= 3);
                for line in scene.lines.iter().filter(|l| l.kind == Kind::Diagram) {
                    assert!(
                        fonts.width(&line.text, line.font) <= f64::from(line.layout_width) + 0.75
                    );
                }
            }
        }
    }
}
