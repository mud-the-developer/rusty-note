use crate::{
    diagram::{Budget, Diagram, Shape},
    document::{Document, Kind, Run},
};
use gpui_pretext::{
    EngineProfile, LayoutCursor, PrepareOptions, PreparedTextWithSegments, TextMeasure,
    WhiteSpaceMode, layout_next_line, layout_with_lines, prepare_with_segments,
};
use pulldown_cmark::Alignment;
use unicode_segmentation::UnicodeSegmentation;

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
    text: Vec<PreparedTextWithSegments>,
    literal: bool,
    source: String,
    runs: Vec<Run>,
    font: Font,
    origin: usize,
}
impl Prepared {
    fn new(
        text: &str,
        runs: &[Run],
        font: Font,
        meter: &dyn FontMeasure,
        origin: usize,
        literal: bool,
    ) -> Self {
        let mut fonts = vec![font];
        for run in runs {
            let mut f = font;
            f.bold |= run.style.bold;
            f.mono |= run.style.code;
            if !fonts.contains(&f) {
                fonts.push(f);
            }
        }
        let measure = Measure {
            backend: meter,
            fonts,
        };
        let prepare = |text: &str| {
            prepare_with_segments(
                text,
                &measure,
                &EngineProfile::default(),
                &PrepareOptions {
                    white_space: WhiteSpaceMode::PreWrap,
                },
            )
        };
        let prepared = if literal {
            // Mermaid-only workaround: Pretext 0.6.2's greedy walker can skip
            // pending runs (e.g. a URL) on overflow. Its cursor API retains them.
            // That API clones its input per line: cap chunks at ~512 UTF-8 bytes
            // on grapheme boundaries to bound reflow work, even for code fallback.
            let mut chunks = Vec::new();
            let mut start = 0;
            for (index, _) in text.grapheme_indices(true) {
                if index - start >= 512 {
                    chunks.push(prepare(&text[start..index]));
                    start = index;
                }
            }
            chunks.push(prepare(&text[start..]));
            chunks
        } else {
            vec![prepare(text)]
        };
        Self {
            text: prepared,
            literal,
            source: text.into(),
            runs: runs.to_vec(),
            font,
            origin,
        }
    }
    fn wrap(&self, width: f32, x: f32, y: &mut f32, kind: Kind, lines: &mut Vec<Line>) {
        let height = self.font.size as f32 * 1.55;
        if self.source.is_empty() && matches!(kind, Kind::Paragraph | Kind::Heading(_)) {
            lines.push(Line {
                kind,
                font: self.font,
                text: String::new(),
                y: *y,
                x,
                width: 0.0,
                layout_width: width,
                alignment: Alignment::None,
                height,
                runs: Vec::new(),
                origin: self.origin,
                source_start: 0,
                source_end: 0,
            });
            *y += height;
            return;
        }
        let wrapped = if self.literal {
            let mut wrapped = Vec::new();
            for text in &self.text {
                let mut cursor = LayoutCursor {
                    segment_index: 0,
                    grapheme_index: 0,
                };
                while let Some(line) = layout_next_line(
                    text,
                    cursor,
                    f64::from(width.max(24.0)),
                    &EngineProfile::default(),
                ) {
                    if line.end == cursor {
                        break;
                    }
                    cursor = line.end;
                    wrapped.push(line);
                }
            }
            wrapped
        } else {
            layout_with_lines(
                &self.text[0],
                f64::from(width.max(24.0)),
                f64::from(height),
                &EngineProfile::default(),
            )
            .lines
        };
        let mut search = 0usize;
        for line in wrapped {
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
    Diagram(Box<PreparedDiagram>),
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
#[derive(Clone, Copy, Debug, Default)]
pub struct DiagramRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl DiagramRect {
    fn overlaps(self, other: Self) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}
#[derive(Debug)]
pub struct DiagramNode {
    pub bounds: DiagramRect,
    pub shape: Shape,
}
#[derive(Debug)]
pub struct DiagramArrow {
    pub points: [(f32, f32); 4],
}
#[derive(Debug, Default)]
pub struct DiagramBox {
    pub nodes: Vec<DiagramNode>,
    pub arrows: Vec<DiagramArrow>,
    pub labels: Vec<DiagramRect>,
}
struct PreparedDiagram {
    model: Diagram,
    nodes: Vec<Prepared>,
    labels: Vec<Option<Prepared>>,
    fallback: Box<Prepared>,
    zoom: f32,
}
impl PreparedDiagram {
    fn label(text: &Prepared, width: f32, limit: usize) -> Option<(Vec<Line>, f32, f32)> {
        let mut lines = Vec::new();
        let mut height = 0.0;
        text.wrap(width, 0.0, &mut height, Kind::Diagram, &mut lines);
        if lines.len() > limit
            || lines
                .iter()
                .any(|l| !l.width.is_finite() || l.width + 3.0 > width)
        {
            return None;
        }
        let measured = lines.iter().map(|l| l.width).fold(0.0, f32::max);
        for line in &mut lines {
            line.alignment = Alignment::Center;
            line.layout_width = width;
        }
        Some((lines, measured, height))
    }
    // Only cached real font advances are used here. An unfit/colliding layout
    // falls back to prepared code, never bitmap shrinking or omitted labels.
    fn reflow(&self, width: f32, top: f32) -> Option<(DiagramBox, Vec<Line>, f32)> {
        if !width.is_finite() {
            return None;
        }
        let z = self.zoom;
        let horizontal = self.model.horizontal;
        let mut gap = 44.0 * z;
        let mut labels = Vec::new();
        for text in &self.labels {
            let prepared = if let Some(text) = text {
                let (mut lines, w, h) = Self::label(text, 96.0 * z, 4)?;
                let w = w + 8.0;
                for line in &mut lines {
                    line.layout_width = w - 4.0;
                }
                gap = gap.max(if horizontal { w } else { h } + 24.0 * z);
                Some((lines, w, h + 4.0))
            } else {
                None
            };
            labels.push(prepared);
        }
        let columns = self.model.ranks.len();
        let column_width = ((width - gap * (columns - 1) as f32) / columns as f32).min(220.0 * z);
        let mut boxes = vec![DiagramRect::default(); self.nodes.len()];
        let mut node_lines = vec![Vec::new(); self.nodes.len()];
        let mut rank_bounds = Vec::new();
        let mut along = 0.0;
        let mut height: f32 = 0.0;
        for rank in &self.model.ranks {
            let node_width = if horizontal {
                column_width
            } else {
                ((width - 24.0 * z * (rank.len() - 1) as f32) / rank.len() as f32).min(220.0 * z)
            };
            if node_width < 80.0 * z {
                return None;
            }
            let mut rank_height: f32 = 0.0;
            for &index in rank {
                let diamond = self.model.nodes[index].shape == Shape::Diamond;
                let inner = if diamond {
                    node_width / 2.0
                } else {
                    node_width
                } - 16.0 * z;
                let (lines, _, h) = Self::label(&self.nodes[index], inner, 8)?;
                let h = (h + 20.0 * z) * if diamond { 2.0 } else { 1.0 };
                boxes[index] = DiagramRect {
                    width: node_width,
                    height: h,
                    ..Default::default()
                };
                node_lines[index] = lines;
                if horizontal {
                    rank_height += h + 24.0 * z;
                } else {
                    rank_height = rank_height.max(h);
                }
            }
            if horizontal {
                rank_height -= 24.0 * z;
            }
            let mut cross = 0.0;
            for (slot, &index) in rank.iter().enumerate() {
                let b = &mut boxes[index];
                if horizontal {
                    b.x = along;
                    b.y = cross;
                    cross += b.height + 24.0 * z;
                } else {
                    let total = rank.len() as f32 * node_width + (rank.len() - 1) as f32 * 24.0 * z;
                    b.x = (width - total) / 2.0 + slot as f32 * (node_width + 24.0 * z);
                    b.y = along + (rank_height - b.height) / 2.0;
                }
            }
            let extent = if horizontal { node_width } else { rank_height };
            rank_bounds.push((along, along + extent));
            along += extent + gap;
            height = if horizontal {
                height.max(rank_height)
            } else {
                along - gap
            };
        }
        if height > 8192.0 || !height.is_finite() {
            return None;
        }
        if horizontal {
            for rank in &self.model.ranks {
                let bottom = rank
                    .iter()
                    .map(|&i| boxes[i].y + boxes[i].height)
                    .fold(0.0, f32::max);
                for &i in rank {
                    boxes[i].y += (height - bottom) / 2.0;
                }
            }
        }
        let mut geometry = DiagramBox::default();
        let mut lines = Vec::new();
        for (index, b) in boxes.iter().enumerate() {
            let text_height: f32 = node_lines[index].iter().map(|l| l.height).sum();
            for mut line in node_lines[index].drain(..) {
                line.x += b.x + (b.width - line.layout_width) / 2.0;
                line.y += b.y + (b.height - text_height) / 2.0 + top;
                lines.push(line);
            }
            geometry.nodes.push(DiagramNode {
                bounds: DiagramRect { y: b.y + top, ..*b },
                shape: self.model.nodes[index].shape,
            });
        }
        let mut label_owners = Vec::new();
        for (index, edge) in self.model.edges.iter().enumerate() {
            let a = boxes[edge.from];
            let b = boxes[edge.to];
            let rank = self
                .model
                .ranks
                .iter()
                .position(|r| r.contains(&edge.from))?;
            let (_, end) = rank_bounds[rank];
            let (next, _) = rank_bounds[rank + 1];
            let points = if horizontal {
                let ay = a.y + a.height / 2.0;
                let by = b.y + b.height / 2.0;
                [
                    (a.x + a.width, ay),
                    (end + 8.0 * z, ay),
                    (next - 8.0 * z, by),
                    (b.x, by),
                ]
            } else {
                let ax = a.x + a.width / 2.0;
                let bx = b.x + b.width / 2.0;
                [
                    (ax, a.y + a.height),
                    (ax, end + 8.0 * z),
                    (bx, next - 8.0 * z),
                    (bx, b.y),
                ]
            };
            geometry.arrows.push(DiagramArrow {
                points: points.map(|(x, y)| (x, y + top)),
            });
            if let Some((label_lines, w, h)) = labels[index].take() {
                let x = (points[1].0 + points[2].0) / 2.0 - w / 2.0;
                let y = (points[1].1 + points[2].1) / 2.0 - h / 2.0 + top;
                let bounds = DiagramRect {
                    x,
                    y,
                    width: w,
                    height: h,
                };
                if x < 0.0 || x + w > width || geometry.labels.iter().any(|b| bounds.overlaps(*b)) {
                    return None;
                }
                geometry.labels.push(bounds);
                label_owners.push(index);
                for mut line in label_lines {
                    line.x += x + 2.0;
                    line.y += y + 2.0;
                    lines.push(line);
                }
            }
        }
        // Conservative collision rejection: don't cover a different connector
        // with an edge-label plate. False positives simply retain readable code.
        for (label, owner) in geometry.labels.iter().zip(label_owners) {
            for (i, arrow) in geometry.arrows.iter().enumerate() {
                if i == owner {
                    continue;
                }
                for pair in arrow.points.windows(2) {
                    let segment = DiagramRect {
                        x: pair[0].0.min(pair[1].0) - 1.0,
                        y: pair[0].1.min(pair[1].1) - 1.0,
                        width: (pair[0].0 - pair[1].0).abs() + 2.0,
                        height: (pair[0].1 - pair[1].1).abs() + 2.0,
                    };
                    if label.overlaps(segment) {
                        return None;
                    }
                }
            }
        }
        Some((geometry, lines, height))
    }
}
pub struct Scene {
    blocks: Vec<PreparedBlock>,
    pub lines: Vec<Line>,
    pub cells: Vec<CellBox>,
    pub rules: Vec<f32>,
    pub images: Vec<ImageBox>,
    pub diagrams: Vec<DiagramBox>,
    pub diagram_prepared: usize,
    pub paragraph_sources: Vec<(usize, std::ops::Range<usize>)>,
    pub edit_prefixes: Vec<(usize, String, f32)>,
    pub tasks: Vec<(usize, crate::document::Task)>,
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
        let mut prepare = |text: &str, runs: &[Run], font: Font, literal: bool| {
            let id = origin;
            origin += 1;
            Prepared::new(text, runs, font, meter, id, literal)
        };
        let mut budget = Budget::default();
        let mut diagram_prepared = 0;
        let mut paragraph_sources = Vec::new();
        let mut edit_prefixes = Vec::new();
        let mut tasks = Vec::new();
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
                                        false,
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
                _ => {
                    let mermaid = b.kind == Kind::Code
                        && b.fence
                            .as_ref()
                            .is_some_and(|f| f.language().eq_ignore_ascii_case("mermaid"));
                    let text = Box::new(prepare(
                        &b.text,
                        &b.runs,
                        Font::for_kind(b.kind).zoomed(zoom),
                        mermaid,
                    ));
                    if let Some(source) = &b.paragraph_source {
                        paragraph_sources.push((text.origin, source.clone()));
                    }
                    if !b.edit_prefix.is_empty() {
                        edit_prefixes.push((
                            text.origin,
                            b.edit_prefix.clone(),
                            meter.width(&b.edit_prefix, text.font) as f32,
                        ));
                    }
                    if let Some(task) = &b.task {
                        tasks.push((text.origin, task.clone()));
                    }
                    if mermaid && let Some(model) = budget.prepare(&b.text) {
                        let font = Font {
                            size: 14,
                            bold: false,
                            mono: false,
                        }
                        .zoomed(zoom);
                        let nodes = model
                            .nodes
                            .iter()
                            .map(|n| prepare(&n.label, &[], font, true))
                            .collect();
                        let labels = model
                            .edges
                            .iter()
                            .map(|e| e.label.as_ref().map(|s| prepare(s, &[], font, true)))
                            .collect();
                        diagram_prepared += 1;
                        PreparedBlock::Diagram(Box::new(PreparedDiagram {
                            model,
                            nodes,
                            labels,
                            fallback: text,
                            zoom: reading_zoom(zoom),
                        }))
                    } else {
                        PreparedBlock::Text {
                            kind: b.kind,
                            indent: b.indent,
                            text,
                        }
                    }
                }
            })
            .collect();
        Self {
            blocks,
            lines: Vec::new(),
            cells: Vec::new(),
            rules: Vec::new(),
            images: Vec::new(),
            diagrams: Vec::new(),
            diagram_prepared,
            paragraph_sources,
            edit_prefixes,
            tasks,
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
        self.diagrams.clear();
        let mut y = 0.0;
        for block in &self.blocks {
            match block {
                PreparedBlock::Diagram(diagram) => {
                    if let Some((geometry, lines, height)) = diagram.reflow(width, y) {
                        self.diagrams.push(geometry);
                        self.lines.extend(lines);
                        y += height + 18.0;
                    } else {
                        diagram.fallback.wrap(
                            (width - 16.0).max(24.0),
                            16.0,
                            &mut y,
                            Kind::Code,
                            &mut self.lines,
                        );
                        y += 18.0;
                    }
                }
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
                PreparedBlock::Diagram(d) => {
                    texts.push(d.fallback.as_ref());
                    texts.extend(&d.nodes);
                    texts.extend(d.labels.iter().flatten());
                }
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
    fn mermaid_cached_native_layout_keeps_find_markdown_zoom_and_fallback() {
        let meter = Meter(Cell::new(0));
        let source = include_str!("../tests/fixtures/mermaid.md");
        let doc = Document::parse(source);
        for zoom in [0.8, 1.0, 1.4, 1.6] {
            let before = meter.0.get();
            let mut scene = Scene::with_zoom(&doc, &meter, zoom);
            let calls = meter.0.get();
            assert!(calls > before);
            assert_eq!(scene.diagram_prepared, 4);
            for width in [544.0, 304.0, 48.0, 1200.0, 544.0] {
                scene.reflow(width);
                assert_eq!(calls, meter.0.get());
                assert!(scene.height.is_finite());
                assert_eq!(scene.cells.len(), 6);
                assert_eq!(scene.images.len(), 1);
                assert_eq!(scene.find("Final paragraph").len(), 1);
                assert_eq!(scene.find("sequenceDiagram").len(), 1);
                assert_eq!(scene.find("never-opened").len(), 1);
                assert_eq!(scene.find("unclosed").len(), 1);
                assert_eq!(scene.find("println!").len(), 1);
                assert!(scene.lines.iter().filter(|l| l.kind == Kind::Diagram).all(
                    |l| !l.font.mono
                        && l.font.size == (14.0 * zoom).round() as u16
                        && l.x >= 0.0
                        && l.x + l.layout_width <= width + 0.1
                ));
                for d in &scene.diagrams {
                    for n in &d.nodes {
                        let b = n.bounds;
                        assert!(b.x >= 0.0 && b.x + b.width <= width + 0.1 && b.height > 0.0);
                    }
                    for a in &d.arrows {
                        assert!(a.points.iter().all(|(x, y)| x.is_finite()
                            && y.is_finite()
                            && *x >= 0.0
                            && *x <= width));
                    }
                }
                if width >= 544.0 {
                    assert_eq!(scene.diagrams.len(), 4);
                    assert_eq!(scene.find("시작 Start").len(), 1);
                    assert_eq!(scene.find("선택?").len(), 1);
                    assert!(
                        scene
                            .diagrams
                            .iter()
                            .any(|d| d.nodes.iter().any(|n| n.shape == Shape::Diamond))
                    );
                }
                if width == 48.0 {
                    assert!(scene.diagrams.is_empty());
                }
                let passes = scene.layout_passes;
                scene.reflow(width);
                assert_eq!(scene.layout_passes, passes);
            }
        }
    }
    #[test]
    fn mermaid_resource_and_language_fallback_is_original_searchable_code() {
        let oversized = format!("graph TD; A[{}]", "x".repeat(crate::diagram::MAX_SOURCE));
        let source = format!(
            "```mermaid\n{oversized}\n```\n\n```rust\ngraph TD; A-->B\n```\n\n```\ngraph TD; A-->B\n```\n\n    graph TD; A-->B\n"
        );
        let doc = Document::parse(&source);
        assert_eq!(doc.blocks[0].text, format!("{oversized}\n"));
        let meter = Meter(Cell::new(0));
        let mut scene = Scene::new(&doc, &meter);
        scene.reflow(544.0);
        assert_eq!(scene.diagram_prepared, 0);
        assert!(scene.diagrams.is_empty());
        assert!(scene.lines.iter().all(|l| l.kind == Kind::Code));
        assert_eq!(scene.find("A-->B").len(), 3);
        let calls = meter.0.get();
        for width in [48.0, 290.0, 1200.0] {
            scene.reflow(width);
            let retained: String = scene
                .lines
                .iter()
                .filter(|l| l.origin == 0)
                .map(|l| l.text.as_str())
                .collect();
            assert_eq!(retained, oversized);
            assert_eq!(meter.0.get(), calls);
        }
        let repeated = "```mermaid\ngraph TD; A-->B\n```\n\n".repeat(40);
        let mut scene = Scene::new(&Document::parse(&repeated), &meter);
        assert_eq!(scene.diagram_prepared, 32);
        let calls = meter.0.get();
        scene.reflow(544.0);
        assert_eq!(scene.diagrams.len(), 32);
        assert_eq!(scene.find("graph TD").len(), 8);
        scene.reflow(304.0);
        assert_eq!(meter.0.get(), calls);
    }
    #[test]
    fn mermaid_unfit_labels_and_collisions_retain_code_without_remeasurement() {
        let meter = Meter(Cell::new(0));
        let doc = Document::parse(
            "```mermaid\ngraph TD; A-->|A very long label|B; A-->|Another long label|C\n```\n",
        );
        let mut scene = Scene::new(&doc, &meter);
        assert_eq!(scene.diagram_prepared, 1);
        let calls = meter.0.get();
        scene.reflow(180.0);
        assert!(scene.diagrams.is_empty());
        assert_eq!(scene.find("A very long label").len(), 1);
        assert_eq!(meter.0.get(), calls);
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
    fn rusty_theme_fixture_wraps_all_text_surfaces_without_remeasurement() {
        let meter = Meter(Cell::new(0));
        let doc = Document::parse(include_str!("../tests/fixtures/rusty-theme.md"));
        let mut scene = Scene::new(&doc, &meter);
        let calls = meter.0.get();
        for width in [550.0, 270.0, 550.0] {
            scene.reflow(width);
            assert_eq!(scene.cells.len(), 8);
            assert_eq!(scene.images.len(), 1);
            assert!(
                scene
                    .cells
                    .iter()
                    .all(|cell| cell.x + cell.width <= width + 0.1)
            );
            for kind in [Kind::Quote, Kind::Code, Kind::Table] {
                assert!(scene.lines.iter().any(|line| line.kind == kind));
            }
            assert!(scene.find("copper").len() >= 3);
            assert_eq!(scene.find("final paragraph").len(), 1);
            assert!(scene.height.is_finite());
            assert_eq!(meter.0.get(), calls);
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
