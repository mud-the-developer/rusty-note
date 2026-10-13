use crate::{
    document::{Document, Kind},
    layout::{Font, FontMeasure, Scene},
    motion::Motion,
    store::{self, Note, State},
};
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
    pub file: Option<PathBuf>,
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

#[derive(Clone, Copy)]
struct Palette {
    background: u32,
    card: u32,
    panel: u32,
    ink: u32,
    muted: u32,
    accent: u32,
    line: u32,
}
impl Palette {
    fn new(light: bool) -> Self {
        if light {
            Self {
                background: 0xe8e6df,
                card: 0xfffdf6,
                panel: 0xf4f0e6,
                ink: 0x263a39,
                muted: 0x74837a,
                accent: 0x167d68,
                line: 0xd5ddd4,
            }
        } else {
            Self {
                background: 0x090f1b,
                card: 0x111c2d,
                panel: 0x172538,
                ink: 0xe7f0fb,
                muted: 0x8197ad,
                accent: 0x85e6cb,
                line: 0x26394d,
            }
        }
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
    paints: u64,
    error: Option<String>,
    editor: Option<crate::editor::Editor>,
    images: crate::images::Images,
    library_scroll: usize,
    tray_available: bool,
}
impl App {
    unsafe fn new(mut options: Options) -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let fonts = Fonts {
                factory: dwrite,
                formats: RefCell::new(HashMap::new()),
                measurements: Cell::new(0),
            };
            let mut state = State::load(&options.state_file);
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
                let initial = options.file.clone().or_else(|| state.selected.clone());
                if let Some(path) = initial {
                    match crate::viewer::open(&path) {
                        Ok((note, _)) => {
                            options.notes_dir = note.path.parent().unwrap().to_owned();
                            options.file = Some(note.path.clone());
                            notes = vec![note];
                        }
                        Err(e) => error = Some(e.to_string()),
                    }
                }
            }
            let selected = state
                .selected
                .as_ref()
                .and_then(|path| notes.iter().position(|n| &n.path == path))
                .unwrap_or(0);
            let source = notes.get(selected).and_then(|note| crate::edit::read_note(&note.path).ok())
            .unwrap_or_else(|| "# Markdown sticker\n\nClick Open or press Ctrl+O to choose a .md file.\n\nDrag the header to place this small window. P keeps it above other windows.\n\nThe original Markdown is read-only.".into());
            let mut scene = Scene::new(&Document::parse(&source), &fonts);
            scene.reflow(state.width as f32 - 72.0);
            let mut motion = Motion::default();
            motion.reduced = options.reduced_motion;
            Ok(Self {
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
                scroll: 0.0,
                paints: 0,
                editor: None,
                images: crate::images::Images::new()?,
                library_scroll: 0,
                tray_available: false,
                error,
            })
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
                if x >= *left && x < *left + 30.0 {
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
        self.scene
            .reflow((self.width - self.content_x() - 28.0).max(48.0));
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
                    let title = format!(
                        "{} — {}",
                        if self.options.editor_mode {
                            "Rusty Note"
                        } else {
                            "MD Sticker"
                        },
                        self.notes[index].title
                    );
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
    unsafe fn line_layout(
        &self,
        line: &crate::layout::Line,
        width: f32,
    ) -> Result<IDWriteTextLayout> {
        unsafe {
            let wide: Vec<u16> = line.text.encode_utf16().collect();
            let format = self.fonts.format(line.font)?;
            let layout = self.fonts.factory.CreateTextLayout(
                &wide,
                &format,
                width.max(16.0),
                line.height + 4.0,
            )?;
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
    unsafe fn rich_text(
        &self,
        line: &crate::layout::Line,
        x: f32,
        y: f32,
        width: f32,
        rgb: u32,
        alpha: f32,
    ) -> Result<()> {
        unsafe {
            let layout = self.line_layout(line, width)?;
            let brush = self.brush.as_ref().unwrap();
            brush.SetColor(&color(rgb, alpha));
            self.target.as_ref().unwrap().DrawTextLayout(
                Vector2 { X: x, Y: y },
                &layout,
                brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
            );
            Ok(())
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
                if self.options.editor_mode {
                    "RUSTY NOTE"
                } else {
                    "MD STICKER"
                },
                tiny,
                rect(
                    if self.options.editor_mode {
                        126.0
                    } else {
                        64.0
                    },
                    17.0,
                    (self.width
                        - if self.options.editor_mode {
                            246.0
                        } else {
                            188.0
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
                        "↑"
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
                if !self.options.editor_mode && (48.0..116.0).contains(&x) {
                    continue;
                }
                let hovered = self.hover.0 >= x && self.hover.0 < x + 30.0 && self.hover.1 < 43.0;
                self.fill(
                    rect(x, 9.0, 30.0, 30.0),
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
                        size: 17,
                        bold: false,
                        mono: false,
                    },
                    rect(x + 8.0, 12.0, 23.0, 25.0),
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
            for line in &self.scene.lines {
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
                let rgb = match line.kind {
                    Kind::Heading(_) => palette.ink,
                    Kind::Code => palette.accent,
                    Kind::Quote => palette.muted,
                    _ => palette.ink,
                };
                self.rich_text(
                    line,
                    content_x + inset,
                    y,
                    (line.width + 3.0).min(self.width - content_x - 28.0 - inset),
                    rgb,
                    progress,
                )?;
            }
            target.PopAxisAlignedClip();
            self.text(
                self.error
                    .as_deref()
                    .unwrap_or(if self.options.editor_mode {
                        "CACHED LAYOUT  ·  EVENT-DRIVEN  ·  RUST"
                    } else {
                        "READ ONLY  ·  Ctrl+O OPEN  ·  F5 RELOAD"
                    }),
                tiny,
                rect(27.0, self.height - 34.0, self.width - 60.0, 20.0),
                palette.muted,
                1.0,
            )?;
            let visible = (self.height - 160.0).max(100.0);
            if self.scene.height > visible {
                let track = self.height - 158.0;
                let thumb = (track * visible / self.scene.height).max(24.0);
                let position =
                    self.scroll / (self.scene.height - visible).max(1.0) * (track - thumb);
                self.fill(
                    rect(self.width - 15.0, 111.0 + position, 3.0, thumb),
                    1.5,
                    palette.accent,
                    0.45,
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
    fn open_markdown(&mut self, path: PathBuf) {
        match crate::viewer::open(&path) {
            Ok((note, text)) => {
                self.options.notes_dir = note.path.parent().unwrap().to_owned();
                self.options.file = Some(note.path.clone());
                self.notes = vec![note];
                self.selected = 0;
                self.scroll = 0.0;
                self.images.clear();
                self.scene = Scene::new(&Document::parse(&text), &self.fonts);
                self.reflow();
                self.error = None;
                unsafe {
                    let title: Vec<u16> = format!("MD Sticker — {}", self.notes[0].title)
                        .encode_utf16()
                        .chain([0])
                        .collect();
                    let _ = SetWindowTextW(self.hwnd, PCWSTR(title.as_ptr()));
                    self.animate();
                }
            }
            Err(error) => {
                self.error = Some(error.to_string());
                unsafe {
                    let _ = InvalidateRect(Some(self.hwnd), None, false);
                }
            }
        }
    }
    fn choose_markdown(&mut self) {
        if let Some(path) = crate::desktop::choose_markdown(self.hwnd) {
            self.open_markdown(path);
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
                    (207, "Quit"),
                ] {
                    if !self.options.editor_mode && ![204, 205, 206, 207].contains(&id) {
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
                if x < 44.0 {
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
            } else if self.library && x < 160.0 && y >= 112.0 {
                self.select(self.library_scroll + ((y - 112.0) / 48.0) as usize);
            } else if self.editor.is_none() {
                let local_y = y - 116.0 + self.scroll;
                if let Some(line) = self
                    .scene
                    .lines
                    .iter()
                    .find(|line| local_y >= line.y && local_y < line.y + line.height)
                    && let Ok(layout) = self.line_layout(line, (line.width + 3.0).max(16.0))
                {
                    let mut trailing = windows::core::BOOL::default();
                    let mut inside = windows::core::BOOL::default();
                    let mut metrics = DWRITE_HIT_TEST_METRICS::default();
                    if layout
                        .HitTestPoint(
                            x - self.content_x() - line.x,
                            local_y - line.y,
                            &mut trailing,
                            &mut inside,
                            &mut metrics,
                        )
                        .is_ok()
                        && inside.as_bool()
                    {
                        for run in &line.runs {
                            let start = line.text[..run.start].encode_utf16().count() as u32;
                            let end = line.text[..run.end].encode_utf16().count() as u32;
                            if metrics.textPosition >= start
                                && metrics.textPosition < end
                                && let Some(url) = &run.style.link
                            {
                                crate::desktop::open_link(self.hwnd, url);
                            }
                        }
                    }
                }
            }
        }
    }
    fn refresh_notes(&mut self) {
        if !self.options.editor_mode {
            if let Some(path) = self.options.file.clone() {
                self.open_markdown(path);
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
    fn metrics(&self) -> serde_json::Value {
        serde_json::json!({"backend":"Direct2D + DirectWrite", "layout":"gpui-pretext 0.6.2", "notes":self.notes.len(),
            "font_measurements":self.fonts.measurements.get(),"prepare_blocks":self.scene.prepare_passes,
            "layout_passes":self.scene.layout_passes,"paint_count":self.paints,"animation_active":self.motion.active(),
            "lines":self.scene.lines.len(),"width_dip":self.width,"height_dip":self.height,
            "selected":self.notes.get(self.selected).map(|n| &n.title),"read_only":!self.options.editor_mode,"table_cells":self.scene.cells.len(),"images":self.scene.images.len(),"styled_runs":self.scene.lines.iter().map(|l|l.runs.iter().filter(|r|r.style.bold||r.style.italic||r.style.code||r.style.link.is_some()||r.style.strike).count()).sum::<usize>()})
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
            WM_DPICHANGED => {
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
                    && x > if app.options.editor_mode { 120.0 } else { 48.0 }
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
            WM_LBUTTONUP => {
                let x = (l.0 as u32 & 0xffff) as i16 as f32 / app.scale;
                let y = ((l.0 as u32 >> 16) & 0xffff) as i16 as f32 / app.scale;
                app.click(x, y);
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let delta = ((w.0 >> 16) & 0xffff) as i16 as f32;
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
                        app.library = false;
                        app.reflow();
                        app.animate();
                    }
                    0x09 if app.options.editor_mode => {
                        app.library = !app.library;
                        app.reflow();
                        app.animate();
                    }
                    0x71 => {
                        let _ = PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(208), LPARAM(0));
                    }
                    0x74 => {
                        let index = app.selected;
                        app.select(index);
                    }
                    0x27 => {
                        let index = (app.selected + 1) % app.notes.len().max(1);
                        app.select(index);
                    }
                    0x25 => {
                        let index = app.selected.saturating_sub(1);
                        app.select(index);
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
            WM_TIMER => {
                if w.0 == 1 {
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
                if !app.options.editor_mode
                    && [200, 201, 202, 203, 208, 209, 210].contains(&command)
                {
                    return LRESULT(0);
                }
                if command == 205 && !app.options.editor_mode {
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
                if !app.finish_edit() {
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
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hIcon: LoadIconW(None, IDI_APPLICATION)?,
            hInstance: HINSTANCE(instance.0),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: w!("RustyNote.Native"),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
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
            position.left = state
                .x
                .clamp(info.rcWork.left, info.rcWork.right - state.width);
            position.top = state
                .y
                .clamp(info.rcWork.top, info.rcWork.bottom - state.height);
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
            state.width,
            state.height,
            None,
            None,
            Some(HINSTANCE(instance.0)),
            None,
        )?;
        cell.borrow_mut().hwnd = hwnd;
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
                "{} — {}",
                if app.options.editor_mode {
                    "Rusty Note"
                } else {
                    "MD Sticker"
                },
                app.notes
                    .get(app.selected)
                    .map(|note| note.title.as_str())
                    .unwrap_or("Quiet corner")
            );
            let wide: Vec<u16> = title.encode_utf16().chain([0]).collect();
            let _ = SetWindowTextW(hwnd, PCWSTR(wide.as_ptr()));
            app.reflow();
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
            let result = GetMessageW(&mut message, None, 0, 0);
            if result.0 <= 0 {
                break;
            }
            if message.message == WM_KEYDOWN && GetKeyState(0x11) < 0 {
                let command = match message.wParam.0 {
                    0x45 => Some(200),
                    0x53 => Some(201),
                    0x4e => Some(202),
                    0x0d => Some(203),
                    0x4f => Some(205),
                    0x48 => Some(206),
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
