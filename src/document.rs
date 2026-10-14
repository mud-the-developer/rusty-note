use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Heading(u8),
    Paragraph,
    List,
    Quote,
    Code,
    Table,
    Rule,
    Image,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Run {
    pub start: usize,
    pub end: usize,
    pub style: Style,
}
#[derive(Clone, Debug, Default)]
pub struct Cell {
    pub text: String,
    pub runs: Vec<Run>,
}
#[derive(Clone, Debug)]
pub struct Table {
    pub rows: Vec<Vec<Cell>>,
    pub alignments: Vec<Alignment>,
}
#[derive(Clone, Debug)]
pub struct Image {
    pub source: String,
    pub alt: String,
}
#[derive(Clone, Debug)]
pub struct Block {
    pub kind: Kind,
    pub text: String,
    pub runs: Vec<Run>,
    pub indent: usize,
    pub table: Option<Table>,
    pub image: Option<Image>,
}
impl Block {
    fn new(kind: Kind, indent: usize) -> Self {
        Self {
            kind,
            text: String::new(),
            runs: Vec::new(),
            indent,
            table: None,
            image: None,
        }
    }
    fn append(&mut self, text: &str, style: &Style) {
        append(&mut self.text, &mut self.runs, text, style);
    }
}
fn append(text: &mut String, runs: &mut Vec<Run>, value: &str, style: &Style) {
    let start = text.len();
    text.push_str(value);
    if let Some(last) = runs.last_mut()
        && last.end == start
        && last.style == *style
    {
        last.end = text.len();
        return;
    }
    runs.push(Run {
        start,
        end: text.len(),
        style: style.clone(),
    });
}
#[derive(Clone, Debug)]
pub struct Document {
    pub blocks: Vec<Block>,
}
impl Document {
    pub fn parse(source: &str) -> Self {
        let mut blocks = Vec::new();
        let mut current: Option<Block> = None;
        let mut style = Style::default();
        let mut stack: Vec<Style> = Vec::new();
        let mut lists: Vec<Option<u64>> = Vec::new();
        let mut quote = 0usize;
        let mut table: Option<Table> = None;
        let mut row: Vec<Cell> = Vec::new();
        let mut cell: Option<Cell> = None;
        let mut image: Option<Image> = None;
        fn flush(current: &mut Option<Block>, blocks: &mut Vec<Block>) {
            if let Some(block) = current.take()
                && !block.text.is_empty()
            {
                blocks.push(block);
            }
        }
        for event in Parser::new_ext(
            source.trim_start_matches('\u{feff}'),
            Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH,
        ) {
            match event {
                Event::Start(
                    Tag::Strong | Tag::Emphasis | Tag::Strikethrough | Tag::Link { .. },
                ) => {
                    stack.push(style.clone());
                    match event {
                        Event::Start(Tag::Strong) => style.bold = true,
                        Event::Start(Tag::Emphasis) => style.italic = true,
                        Event::Start(Tag::Strikethrough) => style.strike = true,
                        Event::Start(Tag::Link { dest_url, .. }) => {
                            style.link = Some(dest_url.to_string())
                        }
                        _ => {}
                    }
                }
                Event::End(
                    TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough | TagEnd::Link,
                ) => {
                    style = stack.pop().unwrap_or_default();
                }
                Event::Start(Tag::Image { dest_url, .. }) => {
                    image = Some(Image {
                        source: dest_url.to_string(),
                        alt: String::new(),
                    })
                }
                Event::End(TagEnd::Image) => {
                    if let Some(img) = image.take() {
                        flush(&mut current, &mut blocks);
                        let mut block = Block::new(Kind::Image, 0);
                        block.text = img.alt.clone();
                        block.image = Some(img);
                        blocks.push(block);
                    }
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    flush(&mut current, &mut blocks);
                    current = Some(Block::new(
                        Kind::Heading(level as u8),
                        lists.len().saturating_sub(1),
                    ));
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    flush(&mut current, &mut blocks);
                    quote += 1;
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    flush(&mut current, &mut blocks);
                    quote = quote.saturating_sub(1);
                }
                Event::Start(Tag::List(start)) => {
                    flush(&mut current, &mut blocks);
                    lists.push(start);
                }
                Event::End(TagEnd::List(_)) => {
                    flush(&mut current, &mut blocks);
                    lists.pop();
                }
                Event::Start(Tag::Item) => {
                    flush(&mut current, &mut blocks);
                    let mut b = Block::new(Kind::List, lists.len().saturating_sub(1));
                    let marker = match lists.last_mut() {
                        Some(Some(n)) => {
                            let label = format!("{n}. ");
                            *n += 1;
                            label
                        }
                        _ => "• ".into(),
                    };
                    b.append(&marker, &Style::default());
                    current = Some(b);
                }
                Event::End(TagEnd::Item) => flush(&mut current, &mut blocks),
                Event::Start(Tag::CodeBlock(_)) => {
                    flush(&mut current, &mut blocks);
                    current = Some(Block::new(Kind::Code, 0));
                }
                Event::Start(Tag::Table(alignments)) => {
                    flush(&mut current, &mut blocks);
                    table = Some(Table {
                        rows: Vec::new(),
                        alignments,
                    });
                }
                Event::Start(Tag::TableHead | Tag::TableRow) => row.clear(),
                Event::Start(Tag::TableCell) => cell = Some(Cell::default()),
                Event::End(TagEnd::TableCell) => {
                    if let Some(c) = cell.take() {
                        row.push(c);
                    }
                }
                Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                    if let Some(t) = table.as_mut() {
                        t.rows.push(std::mem::take(&mut row));
                    }
                }
                Event::End(TagEnd::Table) => {
                    if let Some(t) = table.take() {
                        let mut block = Block::new(Kind::Table, 0);
                        block.text = t
                            .rows
                            .iter()
                            .flat_map(|r| r.iter().map(|c| c.text.as_str()))
                            .collect::<Vec<_>>()
                            .join(" | ");
                        block.table = Some(t);
                        blocks.push(block);
                    }
                }
                Event::Start(Tag::Paragraph) => {
                    if current.is_none() {
                        current = Some(Block::new(
                            if quote > 0 {
                                Kind::Quote
                            } else if !lists.is_empty() {
                                Kind::List
                            } else {
                                Kind::Paragraph
                            },
                            lists.len().saturating_sub(1),
                        ));
                    }
                }
                Event::End(TagEnd::Heading(_) | TagEnd::Paragraph | TagEnd::CodeBlock) => {
                    flush(&mut current, &mut blocks)
                }
                Event::Text(ref text) | Event::Code(ref text) => {
                    if let Some(img) = image.as_mut() {
                        img.alt.push_str(text);
                        continue;
                    }
                    let mut applied = style.clone();
                    if matches!(event, Event::Code(_)) {
                        applied.code = true;
                    }
                    if let Some(c) = cell.as_mut() {
                        append(&mut c.text, &mut c.runs, text, &applied);
                    } else {
                        current
                            .get_or_insert_with(|| Block::new(Kind::Paragraph, 0))
                            .append(text, &applied);
                    }
                }
                Event::SoftBreak | Event::HardBreak => {
                    let value = if matches!(event, Event::HardBreak) {
                        "\n"
                    } else {
                        " "
                    };
                    if let Some(c) = cell.as_mut() {
                        append(&mut c.text, &mut c.runs, value, &style);
                    } else if let Some(b) = current.as_mut() {
                        b.append(value, &style);
                    }
                }
                Event::TaskListMarker(checked) => {
                    if let Some(b) = current.as_mut() {
                        b.text.clear();
                        b.runs.clear();
                        b.append(if checked { "☑  " } else { "☐  " }, &Style::default());
                    }
                }
                Event::Rule => {
                    flush(&mut current, &mut blocks);
                    blocks.push(Block::new(Kind::Rule, 0));
                }
                Event::Html(text) | Event::InlineHtml(text) => {
                    current
                        .get_or_insert_with(|| Block::new(Kind::Paragraph, 0))
                        .append(&text, &Style::default());
                }
                _ => {}
            }
        }
        flush(&mut current, &mut blocks);
        Self { blocks }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rusty_theme_fixture_keeps_contrast_samples_structured() {
        let doc = Document::parse(include_str!("../tests/fixtures/rusty-theme.md"));
        for kind in [
            Kind::Heading(1),
            Kind::List,
            Kind::Quote,
            Kind::Code,
            Kind::Table,
        ] {
            assert!(doc.blocks.iter().any(|block| block.kind == kind));
        }
        let runs: Vec<_> = doc.blocks.iter().flat_map(|block| &block.runs).collect();
        assert!(runs.iter().any(|run| run.style.bold));
        assert!(runs.iter().any(|run| run.style.italic));
        assert!(runs.iter().any(|run| run.style.code));
        assert!(runs.iter().any(|run| run.style.link.is_some()));
        assert!(doc.blocks.iter().any(|block| block.text.contains("구리빛")));
        assert!(doc.blocks.last().unwrap().text.contains("final paragraph"));
    }
    #[test]
    fn utf8_bom_does_not_turn_heading_into_plain_text() {
        let doc = Document::parse("\u{feff}# Heading\n\n한국어");
        assert_eq!(doc.blocks[0].kind, Kind::Heading(1));
        assert_eq!(doc.blocks[0].text, "Heading");
    }
    #[test]
    fn mixed_document_keeps_content_and_block_structure() {
        let doc = Document::parse(include_str!("../tests/fixtures/studio.md"));
        for kind in [
            Kind::Heading(1),
            Kind::List,
            Kind::Quote,
            Kind::Code,
            Kind::Table,
        ] {
            assert!(doc.blocks.iter().any(|b| b.kind == kind));
        }
        assert!(doc.blocks.last().unwrap().text.contains("한국어"));
        assert!(doc.blocks.iter().any(|b| b.text.contains('☑')));
    }
    #[test]
    fn nested_styles_links_tables_and_code_remain_structured() {
        let doc = Document::parse(
            "**bold *nested*** and [safe](https://example.com)\n\n| A | B |\n|---|---|\n| long | **cell** |\n\n```rust\n  indented\n\n  last\n```\n\n3. third\n4. fourth\n\n---\n\n![alt](local.png)\n\nAfter",
        );
        assert!(
            doc.blocks[0]
                .runs
                .iter()
                .any(|r| r.style.bold && r.style.italic)
        );
        assert!(doc.blocks[0].runs.iter().any(|r| r.style.link.is_some()));
        let t = doc.blocks.iter().find_map(|b| b.table.as_ref()).unwrap();
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[1][1].text, "cell");
        assert!(t.rows[1][1].runs[0].style.bold);
        assert!(
            doc.blocks
                .iter()
                .any(|b| b.kind == Kind::Code && b.text.contains("\n\n"))
        );
        assert!(doc.blocks.iter().any(|b| b.text == "3. third"));
        assert!(doc.blocks.iter().any(|b| b.kind == Kind::Rule));
        assert_eq!(doc.blocks.last().unwrap().text, "After");
    }
}
