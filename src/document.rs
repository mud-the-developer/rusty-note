use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Heading(u8),
    Paragraph,
    List,
    Quote,
    Code,
    Table,
}
#[derive(Clone, Debug)]
pub struct Block {
    pub kind: Kind,
    pub text: String,
}
#[derive(Clone, Debug)]
pub struct Document {
    pub blocks: Vec<Block>,
}

impl Document {
    pub fn parse(source: &str) -> Self {
        let source = source.trim_start_matches('\u{feff}');
        let mut blocks = Vec::new();
        let mut current: Option<Block> = None;
        let mut quote = 0usize;
        let mut list = 0usize;
        let mut table = false;
        let flush = |current: &mut Option<Block>, blocks: &mut Vec<Block>| {
            if let Some(mut block) = current.take() {
                if block.kind != Kind::Code {
                    block.text = block.text.trim().to_owned();
                }
                if !block.text.is_empty() {
                    blocks.push(block);
                }
            }
        };
        for event in Parser::new_ext(
            source,
            Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH,
        ) {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    flush(&mut current, &mut blocks);
                    current = Some(Block {
                        kind: Kind::Heading(level as u8),
                        text: String::new(),
                    });
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    quote += 1;
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    flush(&mut current, &mut blocks);
                    quote = quote.saturating_sub(1);
                }
                Event::Start(Tag::List(_)) => {
                    list += 1;
                }
                Event::End(TagEnd::List(_)) => {
                    flush(&mut current, &mut blocks);
                    list = list.saturating_sub(1);
                }
                Event::Start(Tag::Item) => {
                    flush(&mut current, &mut blocks);
                    current = Some(Block {
                        kind: Kind::List,
                        text: "• ".into(),
                    });
                }
                Event::End(TagEnd::Item) => {
                    flush(&mut current, &mut blocks);
                }
                Event::Start(Tag::CodeBlock(_)) => {
                    flush(&mut current, &mut blocks);
                    current = Some(Block {
                        kind: Kind::Code,
                        text: String::new(),
                    });
                }
                Event::Start(Tag::Table(_)) => {
                    flush(&mut current, &mut blocks);
                    table = true;
                }
                Event::End(TagEnd::Table) => {
                    flush(&mut current, &mut blocks);
                    table = false;
                }
                Event::Start(Tag::TableHead | Tag::TableRow) => {
                    flush(&mut current, &mut blocks);
                    current = Some(Block {
                        kind: Kind::Table,
                        text: String::new(),
                    });
                }
                Event::End(TagEnd::TableCell) => {
                    if let Some(block) = current.as_mut() {
                        block.text.push_str("   ·   ");
                    }
                }
                Event::Start(Tag::Paragraph) => {
                    if current.is_none() {
                        current = Some(Block {
                            kind: if quote > 0 {
                                Kind::Quote
                            } else if list > 0 {
                                Kind::List
                            } else {
                                Kind::Paragraph
                            },
                            text: String::new(),
                        });
                    }
                }
                Event::End(
                    TagEnd::Heading(_)
                    | TagEnd::Paragraph
                    | TagEnd::CodeBlock
                    | TagEnd::TableHead
                    | TagEnd::TableRow,
                ) => {
                    flush(&mut current, &mut blocks);
                }
                Event::Text(text) | Event::Code(text) => {
                    let kind = if table {
                        Kind::Table
                    } else if quote > 0 {
                        Kind::Quote
                    } else if list > 0 {
                        Kind::List
                    } else {
                        Kind::Paragraph
                    };
                    current
                        .get_or_insert(Block {
                            kind,
                            text: String::new(),
                        })
                        .text
                        .push_str(&text);
                }
                Event::SoftBreak | Event::HardBreak => {
                    if let Some(block) = current.as_mut() {
                        block.text.push('\n');
                    }
                }
                Event::TaskListMarker(checked) => {
                    if let Some(block) = current.as_mut() {
                        if block.text == "• " {
                            block.text.clear();
                        }
                        block.text.push_str(if checked { "☑  " } else { "☐  " });
                    }
                }
                Event::Rule => {
                    flush(&mut current, &mut blocks);
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
    fn utf8_bom_does_not_turn_heading_into_plain_text() {
        let doc = Document::parse("\u{feff}# Heading\n\n한국어");
        assert_eq!(doc.blocks[0].kind, Kind::Heading(1));
        assert_eq!(doc.blocks[0].text, "Heading");
    }
    #[test]
    fn mixed_document_keeps_content_and_block_structure() {
        let doc = Document::parse(include_str!("../tests/fixtures/studio.md"));
        for expected in [
            Kind::Heading(1),
            Kind::List,
            Kind::Quote,
            Kind::Code,
            Kind::Table,
        ] {
            assert!(
                doc.blocks.iter().any(|b| b.kind == expected),
                "missing {expected:?}"
            );
        }
        assert!(doc.blocks.last().unwrap().text.contains("한국어"));
        assert!(doc.blocks.iter().any(|b| b.text.contains("☑")));
    }
}
