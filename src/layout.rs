use crate::document::{Document, Kind, Run};
use gpui_pretext::{
    EngineProfile, PrepareOptions, PreparedTextWithSegments, TextMeasure, WhiteSpaceMode,
    layout_with_lines, prepare_with_segments,
};
use pulldown_cmark::Alignment;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct Font {
    pub size: u16,
    pub bold: bool,
    pub mono: bool,
}
impl Font {
    pub fn for_kind(kind: Kind) -> Self {
        match kind {
            Kind::Heading(level) => Self {
                size: match level {
                    1 => 30,
                    2 => 23,
                    _ => 19,
                },
                bold: true,
                mono: false,
            },
            Kind::Code => Self {
                size: 13,
                bold: false,
                mono: true,
            },
            _ => Self {
                size: 16,
                bold: false,
                mono: false,
            },
        }
    }
}
pub fn reading_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() {
        (zoom.clamp(0.8, 1.6) * 10.0).round() / 10.0
    } else {
        1.0
    }
}
impl Font {
    fn zoomed(mut self, zoom: f32) -> Self {
        self.size = (f32::from(self.size) * reading_zoom(zoom)).round() as u16;
        self
    }
}
pub trait FontMeasure {
    fn width(&self, text: &str, font: Font) -> f64;
}
// Conservative styled wrapping: each prepared segment reserves the largest real
// advance among fonts used in this block. Rendering applies exact DirectWrite
// ranges; resize never measures fonts or guesses from character counts.
struct Measure<'a> {
    backend: &'a dyn FontMeasure,
    fonts: Vec<Font>,
}
impl TextMeasure for Measure<'_> {
    fn measure_width(&self, text: &str) -> f64 {
        self.fonts
            .iter()
            .map(|f| self.backend.width(text, *f))
            .fold(0.0, f64::max)
    }
}
struct Prepared {
    text: PreparedTextWithSegments,
    source: String,
    runs: Vec<Run>,
    font: Font,
    origin: usize,
}
impl Prepared {
    fn new(text: &str, runs: &[Run], font: Font, meter: &dyn FontMeasure, origin: usize) -> Self {
        let mut fonts = vec![font];
        for run in runs {
            let mut f = font;
            f.bold |= run.style.bold;
            f.mono |= run.style.code;
            if !fonts.contains(&f) {
                fonts.push(f);
            }
        }
        Self {
            text: prepare_with_segments(
                text,
                &Measure {
                    backend: meter,
                    fonts,
                },
                &EngineProfile::default(),
                &PrepareOptions {
                    white_space: WhiteSpaceMode::PreWrap,
                },
            ),
            source: text.into(),
            runs: runs.to_vec(),
            font,
            origin,
        }
    }
    fn wrap(&self, width: f32, x: f32, y: &mut f32, kind: Kind, lines: &mut Vec<Line>) {
        let height = self.font.size as f32 * 1.55;
        let wrapped = layout_with_lines(
            &self.text,
            f64::from(width.max(24.0)),
            f64::from(height),
            &EngineProfile::default(),
        );
        let mut search = 0usize;
        for line in wrapped.lines {
            let start = self
                .source
                .get(search..)
                .and_then(|tail| tail.find(&line.text))
                .map(|i| search + i);
            let matched = start.is_some();
            let start = start.unwrap_or(search);
            let end = if matched {
                start + line.text.len()
            } else {
                start
            };
            let runs = self
                .runs
                .iter()
                .filter_map(|r| {
                    let a = r.start.max(start);
                    let b = r.end.min(end);
                    (matched && a < b).then(|| Run {
                        start: a - start,
                        end: b - start,
                        style: r.style.clone(),
                    })
                })
                .collect();
            search = end;
            lines.push(Line {
                kind,
                font: self.font,
                text: line.text,
                y: *y,
                x,
                width: line.width as f32,
                layout_width: line.width as f32 + 3.0,
                alignment: Alignment::None,
                height,
                runs,
                origin: self.origin,
                source_start: start,
                source_end: end,
            });
            *y += height;
        }
    }
}
enum PreparedBlock {
    Text {
        kind: Kind,
        indent: usize,
        text: Box<Prepared>,
    },
    Table {
        rows: Vec<Vec<Prepared>>,
        alignments: Vec<Alignment>,
    },
    Rule,
    Image {
        source: String,
        alt: String,
    },
}
#[derive(Clone)]
pub struct Line {
    pub kind: Kind,
    pub font: Font,
    pub text: String,
    pub y: f32,
    pub x: f32,
    pub width: f32,
    pub layout_width: f32,
    pub alignment: Alignment,
    pub height: f32,
    pub runs: Vec<Run>,
    pub origin: usize,
    pub source_start: usize,
    pub source_end: usize,
}
#[derive(Clone, Debug)]
pub struct SearchHit {
    pub lines: Vec<usize>,
}
#[derive(Clone, Debug)]
pub struct CellBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub header: bool,
}
#[derive(Clone, Debug)]
pub struct ImageBox {
    pub source: String,
    pub alt: String,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
pub struct Scene {
    blocks: Vec<PreparedBlock>,
    pub lines: Vec<Line>,
    pub cells: Vec<CellBox>,
    pub rules: Vec<f32>,
    pub images: Vec<ImageBox>,
    pub height: f32,
    width: f32,
    pub prepare_passes: usize,
    pub layout_passes: usize,
}
impl Scene {
    pub fn new(doc: &Document, meter: &dyn FontMeasure) -> Self {
        Self::with_zoom(doc, meter, 1.0)
    }
    pub fn with_zoom(doc: &Document, meter: &dyn FontMeasure, zoom: f32) -> Self {
        let mut origin = 0;
        let mut prepare = |text: &str, runs: &[Run], font: Font| {
            let id = origin;
            origin += 1;
            Prepared::new(text, runs, font, meter, id)
        };
        let blocks = doc
            .blocks
            .iter()
            .map(|b| match b.kind {
                Kind::Rule => PreparedBlock::Rule,
                Kind::Image => {
                    let img = b.image.as_ref().unwrap();
                    PreparedBlock::Image {
                        source: img.source.clone(),
                        alt: img.alt.clone(),
                    }
                }
                Kind::Table => {
                    let rows = b
                        .table
                        .as_ref()
                        .unwrap()
                        .rows
                        .iter()
                        .enumerate()
                        .map(|(index, row)| {
                            row.iter()
                                .map(|cell| {
                                    prepare(
                                        &cell.text,
                                        &cell.runs,
                                        Font {
                                            bold: index == 0,
                                            ..Font::for_kind(Kind::Table).zoomed(zoom)
                                        },
                                    )
                                })
                                .collect()
                        })
                        .collect();
                    PreparedBlock::Table {
                        rows,
                        alignments: b.table.as_ref().unwrap().alignments.clone(),
                    }
                }
                _ => PreparedBlock::Text {
                    kind: b.kind,
                    indent: b.indent,
                    text: Box::new(prepare(
                        &b.text,
                        &b.runs,
                        Font::for_kind(b.kind).zoomed(zoom),
                    )),
                },
            })
            .collect();
        Self {
            blocks,
            lines: Vec::new(),
            cells: Vec::new(),
            rules: Vec::new(),
            images: Vec::new(),
            height: 0.0,
            width: -1.0,
            prepare_passes: doc.blocks.len(),
            layout_passes: 0,
        }
    }
    pub fn reflow(&mut self, width: f32) {
        let width = width.max(48.0).round();
        if self.width == width {
            return;
        }
        self.width = width;
        self.layout_passes += 1;
        self.lines.clear();
        self.cells.clear();
        self.rules.clear();
        self.images.clear();
        let mut y = 0.0;
        for block in &self.blocks {
            match block {
                PreparedBlock::Text { kind, indent, text } => {
                    let inset = if matches!(kind, Kind::Quote | Kind::Code) {
                        16.0
                    } else {
                        0.0
                    } + (*indent as f32 * 14.0).min(width * 0.25);
                    text.wrap(
                        (width - inset).max(24.0),
                        inset,
                        &mut y,
                        *kind,
                        &mut self.lines,
                    );
                    y += if matches!(kind, Kind::Heading(_)) {
                        13.0
                    } else {
                        18.0
                    };
                }
                PreparedBlock::Rule => {
                    self.rules.push(y + 8.0);
                    y += 28.0;
                }
                PreparedBlock::Image { source, alt } => {
                    let height = (width * 0.6).clamp(90.0, 240.0);
                    self.images.push(ImageBox {
                        source: source.clone(),
                        alt: alt.clone(),
                        y,
                        width,
                        height,
                    });
                    y += height + 18.0;
                }
                PreparedBlock::Table { rows, alignments } => {
                    let columns = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
                    let cell_width = width / columns as f32;
                    for (index, row) in rows.iter().enumerate() {
                        let top = y;
                        let mut bottom = top + 32.0;
                        for (col, cell) in row.iter().enumerate() {
                            let mut cell_y = top + 8.0;
                            let first_line = self.lines.len();
                            cell.wrap(
                                (cell_width - 16.0).max(8.0),
                                col as f32 * cell_width + 8.0,
                                &mut cell_y,
                                Kind::Table,
                                &mut self.lines,
                            );
                            for line in &mut self.lines[first_line..] {
                                line.alignment =
                                    alignments.get(col).copied().unwrap_or(Alignment::None);
                                line.layout_width = (cell_width - 16.0).max(1.0);
                            }
                            bottom = bottom.max(cell_y + 8.0);
                        }
                        for col in 0..columns {
                            self.cells.push(CellBox {
                                x: col as f32 * cell_width,
                                y: top,
                                width: cell_width,
                                height: bottom - top,
                                header: index == 0,
                            });
                        }
                        y = bottom;
                    }
                    y += 18.0;
                }
            }
        }
        self.height = y;
    }
    pub fn find(&self, query: &str) -> Vec<SearchHit> {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() || query.len() > 512 {
            return Vec::new();
        }
        let mut texts = Vec::new();
        for block in &self.blocks {
            match block {
                PreparedBlock::Text { text, .. } => texts.push(text.as_ref()),
                PreparedBlock::Table { rows, .. } => texts.extend(rows.iter().flatten()),
                _ => {}
            }
        }
        let mut by_source = vec![Vec::new(); texts.len()];
        for (index, line) in self.lines.iter().enumerate() {
            by_source[line.origin].push(index);
        }
        let mut hits = Vec::new();
        for text in texts {
            // shortcut: ASCII-insensitive case; use Unicode folding if non-ASCII case matching is required.
            let source = text.source.to_ascii_lowercase();
            for (start, _) in source.match_indices(&query) {
                let end = start + query.len();
                let candidates = &by_source[text.origin];
                let first =
                    candidates.partition_point(|index| self.lines[*index].source_end <= start);
                let lines = candidates[first..]
                    .iter()
                    .take_while(|index| self.lines[**index].source_start < end)
                    .copied()
                    .collect::<Vec<_>>();
                if !lines.is_empty() {
                    hits.push(SearchHit { lines });
                    if hits.len() == 4096 {
                        return hits;
                    }
                }
            }
        }
        hits
    }
    // shortcut: prepared link bounds are coarse; use native glyph hit tests for exact styled clicks.
    pub fn link_at(&self, x: f32, y: f32) -> Option<&str> {
        self.lines
            .iter()
            .find(|l| {
                let gap = (l.layout_width - l.width).max(0.0);
                let left = l.x
                    + match l.alignment {
                        Alignment::Center => gap / 2.0,
                        Alignment::Right => gap,
                        _ => 0.0,
                    };
                y >= l.y && y < l.y + l.height && x >= left && x <= left + l.width
            })
            .and_then(|l| l.runs.iter().find_map(|r| r.style.link.as_deref()))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct Meter(Cell<usize>);
    impl FontMeasure for Meter {
        fn width(&self, text: &str, font: Font) -> f64 {
            self.0.set(self.0.get() + 1);
            text.chars().count() as f64 * f64::from(font.size) * if font.bold { 0.61 } else { 0.55 }
        }
    }
    #[test]
    fn pretext_resize_and_idle_do_not_remeasure_fonts() {
        let meter = Meter(Cell::new(0));
        let doc = Document::parse(include_str!("../tests/fixtures/studio.md"));
        let mut scene = Scene::new(&doc, &meter);
        let calls = meter.0.get();
        assert!(calls > 0);
        for width in [260.0, 360.0, 600.0, 360.0] {
            scene.reflow(width);
            assert_eq!(meter.0.get(), calls);
            assert!(
                scene
                    .lines
                    .iter()
                    .all(|l| l.y.is_finite() && l.width.is_finite())
            );
            assert!(scene.lines.iter().any(|l| l.text.contains("한국어")));
        }
        let passes = scene.layout_passes;
        scene.reflow(360.0);
        assert_eq!(passes, scene.layout_passes);
    }
    #[test]
    fn find_uses_semantic_text_and_matches_phrases_across_wrapped_lines() {
        let meter = Meter(Cell::new(0));
        let doc = Document::parse(include_str!("../tests/fixtures/reader-find.md"));
        let mut scene = Scene::new(&doc, &meter);
        scene.reflow(150.0);
        let calls = meter.0.get();
        let hits = scene.find("needle phrase");
        assert_eq!(hits.len(), 3);
        assert!(hits[0].lines.len() > 1);
        assert_eq!(scene.find("NEEDLE PHRASE").len(), 3);
        assert_eq!(scene.find("한국어 찾기").len(), 2);
        assert!(scene.find("not-found-123").is_empty());
        assert!(scene.find("").is_empty());
        assert_eq!(meter.0.get(), calls);
        scene.reflow(550.0);
        assert_eq!(scene.find("needle phrase").len(), 3);
        assert_eq!(meter.0.get(), calls);
    }
    #[test]
    fn find_caps_results_and_rejects_overlong_input_without_measuring_fonts() {
        let meter = Meter(Cell::new(0));
        let doc = Document::parse(&"needle ".repeat(4200));
        let mut scene = Scene::new(&doc, &meter);
        scene.reflow(300.0);
        let calls = meter.0.get();
        assert_eq!(scene.find("needle").len(), 4096);
        assert!(scene.find(&"n".repeat(513)).is_empty());
        assert_eq!(meter.0.get(), calls);
    }
    #[test]
    fn reading_zoom_uses_scaled_fonts_and_keeps_resize_cached() {
        let meter = Meter(Cell::new(0));
        let doc = Document::parse(include_str!("../tests/fixtures/reader-comfort.md"));
        let mut scene = Scene::with_zoom(&doc, &meter, 1.5);
        let calls = meter.0.get();
        for width in [260.0, 600.0, 260.0] {
            scene.reflow(width);
            assert_eq!(meter.0.get(), calls);
            assert!(
                scene
                    .lines
                    .iter()
                    .any(|line| line.font.size == 24 && line.kind == Kind::Paragraph)
            );
            assert!(
                scene
                    .lines
                    .iter()
                    .any(|line| line.font.size == 45 && matches!(line.kind, Kind::Heading(1)))
            );
            assert!(scene.cells.len() >= 4);
            assert!(
                scene
                    .lines
                    .iter()
                    .all(|line| line.y.is_finite() && line.height > 0.0)
            );
        }
        assert_eq!(reading_zoom(f32::NAN), 1.0);
        assert_eq!(reading_zoom(9.0), 1.6);
        assert_eq!(reading_zoom(0.0), 0.8);
    }
    #[test]
    fn table_alignment_survives_wrapping_zoom_and_search_without_remeasurement() {
        let meter = Meter(Cell::new(0));
        let doc = Document::parse(include_str!("../tests/fixtures/table-alignment.md"));
        let tables: Vec<_> = doc.blocks.iter().filter_map(|b| b.table.as_ref()).collect();
        assert_eq!(
            tables[0].alignments,
            [Alignment::Left, Alignment::Center, Alignment::Right]
        );
        assert_eq!(
            tables[1].alignments,
            [Alignment::None, Alignment::Center, Alignment::Right]
        );
        for zoom in [1.0, 1.4] {
            let mut scene = Scene::with_zoom(&doc, &meter, zoom);
            let calls = meter.0.get();
            for width in [270.0, 550.0, 270.0] {
                scene.reflow(width);
                let inner = width / 3.0 - 16.0;
                let second_top = scene.cells.iter().filter(|c| c.header).nth(3).unwrap().y;
                for line in &scene.lines {
                    if line.kind == Kind::Table {
                        let col = (line.x / (width / 3.0)) as usize;
                        let expected = if col == 0 {
                            if line.y >= second_top {
                                Alignment::None
                            } else {
                                Alignment::Left
                            }
                        } else if col == 1 {
                            Alignment::Center
                        } else {
                            Alignment::Right
                        };
                        assert_eq!(line.alignment, expected);
                        assert!((line.layout_width - inner).abs() < 0.01);
                        assert!(line.x + line.layout_width <= width + 0.01);
                    } else {
                        assert_eq!(line.alignment, Alignment::None);
                    }
                }
                assert_eq!(scene.find("After everything").len(), 1);
                assert_eq!(scene.find("123.45").len(), 2);
                let link = scene
                    .lines
                    .iter()
                    .find(|l| {
                        l.runs
                            .iter()
                            .any(|r| r.style.link.as_deref() == Some("https://example.com/center"))
                    })
                    .unwrap();
                let center = link.x + link.layout_width / 2.0;
                assert_eq!(
                    scene.link_at(center, link.y + link.height / 2.0),
                    Some("https://example.com/center")
                );
                assert_eq!(meter.0.get(), calls);
            }
        }
    }
    #[test]
    fn tables_are_real_cells_and_surrounding_content_survives() {
        let meter = Meter(Cell::new(0));
        let doc = Document::parse(
            "Before **bold**\n\n| A | B |\n|---|---|\n| multiple long words wrapped | other |\n\nAfter\n\n```\n  indent\n\n  last\n```",
        );
        let mut scene = Scene::new(&doc, &meter);
        scene.reflow(180.0);
        assert_eq!(scene.cells.len(), 4);
        assert!(scene.cells.iter().all(|c| c.x + c.width <= 180.1));
        assert!(
            scene
                .lines
                .iter()
                .any(|l| l.runs.iter().any(|r| r.style.bold))
        );
        assert!(scene.lines.iter().any(|l| l.text == "After"));
        assert!(
            scene
                .lines
                .iter()
                .any(|l| l.kind == Kind::Code && l.text.starts_with("  "))
        );
    }
}
