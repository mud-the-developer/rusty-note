use crate::document::{Document, Kind};
use gpui_pretext::{
    EngineProfile, PrepareOptions, PreparedTextWithSegments, TextMeasure, layout_with_lines,
    prepare_with_segments,
};

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
pub trait FontMeasure {
    fn width(&self, text: &str, font: Font) -> f64;
}
struct Measure<'a> {
    backend: &'a dyn FontMeasure,
    font: Font,
}
impl TextMeasure for Measure<'_> {
    fn measure_width(&self, text: &str) -> f64 {
        self.backend.width(text, self.font)
    }
}
struct PreparedBlock {
    kind: Kind,
    font: Font,
    text: PreparedTextWithSegments,
}
#[derive(Clone)]
pub struct Line {
    pub kind: Kind,
    pub font: Font,
    pub text: String,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
pub struct Scene {
    blocks: Vec<PreparedBlock>,
    pub lines: Vec<Line>,
    pub height: f32,
    width: f32,
    pub prepare_passes: usize,
    pub layout_passes: usize,
}
impl Scene {
    pub fn new(document: &Document, measure: &dyn FontMeasure) -> Self {
        let profile = EngineProfile::default();
        let blocks = document
            .blocks
            .iter()
            .map(|block| {
                let font = Font::for_kind(block.kind);
                let text = prepare_with_segments(
                    &block.text,
                    &Measure {
                        backend: measure,
                        font,
                    },
                    &profile,
                    &PrepareOptions::default(),
                );
                PreparedBlock {
                    kind: block.kind,
                    font,
                    text,
                }
            })
            .collect();
        Self {
            blocks,
            lines: Vec::new(),
            height: 0.0,
            width: -1.0,
            prepare_passes: document.blocks.len(),
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
        let mut y = 0.0;
        for block in &self.blocks {
            let inset = if matches!(block.kind, Kind::Quote | Kind::Code) {
                20.0
            } else {
                0.0
            };
            let line_height = block.font.size as f32 * 1.55;
            let wrapped = layout_with_lines(
                &block.text,
                f64::from((width - inset).max(32.0)),
                f64::from(line_height),
                &EngineProfile::default(),
            );
            for line in wrapped.lines {
                self.lines.push(Line {
                    kind: block.kind,
                    font: block.font,
                    text: line.text,
                    y,
                    width: line.width as f32,
                    height: line_height,
                });
                y += line_height;
            }
            y += if matches!(block.kind, Kind::Heading(_)) {
                13.0
            } else {
                18.0
            };
        }
        self.height = y;
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
            text.chars().count() as f64 * f64::from(font.size) * 0.55
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
}
