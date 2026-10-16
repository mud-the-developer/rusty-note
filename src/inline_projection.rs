//! Visible native text to Markdown for one edited text block. Unedited document
//! bytes are never round-tripped. Edited blocks use canonical CommonMark spelling.
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;
pub const MAX_PARAGRAPH_UNITS: usize = 8192;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub source: Range<usize>,
    pub text: Range<usize>,
    pub utf16: Range<usize>,
    pub style: Style,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Styled {
    pub utf16: Range<usize>,
    pub style: Style,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Projection {
    pub source: String,
    pub text: String,
    pub runs: Vec<Run>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEdit {
    pub range: Range<usize>,
    pub replacement: String,
    pub projected: Projection,
}
pub fn byte_to_utf16(text: &str, byte: usize) -> Option<usize> {
    Some(text.get(..byte)?.encode_utf16().count())
}
pub fn utf16_to_byte(text: &str, unit: usize) -> Option<usize> {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units == unit {
            return Some(byte);
        }
        units += ch.len_utf16();
    }
    (units == unit).then_some(text.len())
}
fn append(p: &mut Projection, text: &str, source: Range<usize>, style: Style) {
    if text.is_empty() {
        return;
    }
    let start = p.text.len();
    let units = p.text.encode_utf16().count();
    p.text.push_str(text);
    p.runs.push(Run {
        source,
        text: start..p.text.len(),
        utf16: units..p.text.encode_utf16().count(),
        style,
    });
}
impl Projection {
    pub fn parse(source: &str) -> Result<Self, &'static str> {
        if source.len() > MAX_PARAGRAPH_UNITS * 12
            || source.contains(['\0', '\u{fffc}', '\u{feff}'])
        {
            return Err("Unsupported or oversized text block");
        }
        let normalized = source.replace("\r\n", "\n");
        let leading = normalized.bytes().take_while(|b| *b == b'\n').count() / 2;
        let trailing = normalized.bytes().rev().take_while(|b| *b == b'\n').count() / 2;
        let mut p = Self {
            source: source.into(),
            text: String::new(),
            runs: Vec::new(),
        };
        let mut style = Style::default();
        let mut stack = Vec::new();
        let mut paragraphs = 0;
        if source.trim_matches(['\r', '\n']).is_empty() {
            append(&mut p, &"\r".repeat(leading), 0..source.len(), style);
            return Ok(p);
        }
        append(&mut p, &"\r".repeat(leading), 0..0, style);
        for (event, range) in Parser::new_ext(source, Options::empty()).into_offset_iter() {
            match event {
                Event::Start(Tag::Paragraph) => {
                    if paragraphs > 0 {
                        append(&mut p, "\r", range.start..range.start, Style::default());
                    }
                    paragraphs += 1;
                }
                Event::End(TagEnd::Paragraph) => {}
                Event::Start(Tag::Strong) => {
                    stack.push(style);
                    style.bold = true;
                }
                Event::Start(Tag::Emphasis) => {
                    stack.push(style);
                    style.italic = true;
                }
                Event::End(TagEnd::Strong | TagEnd::Emphasis) => {
                    style = stack.pop().ok_or("Unbalanced styles")?
                }
                Event::Text(text) => append(&mut p, &text, range, style),
                Event::SoftBreak => append(&mut p, " ", range, style),
                Event::HardBreak => append(&mut p, "\r", range, style),
                _ => return Err("This Markdown block needs the source editor"),
            }
        }
        append(
            &mut p,
            &"\r".repeat(trailing),
            source.len()..source.len(),
            Style::default(),
        );
        if p.text.encode_utf16().count() > MAX_PARAGRAPH_UNITS {
            return Err("Text block exceeds limit");
        }
        Ok(p)
    }
    pub fn parse_with_prefix(source: &str, prefix: &str) -> Result<Self, &'static str> {
        if prefix.is_empty() {
            return Self::parse(source);
        }
        let newline = if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let separator = format!("{newline}{prefix}");
        let mut result = Self {
            source: source.into(),
            text: String::new(),
            runs: Vec::new(),
        };
        let mut offset = 0;
        for (index, part) in source.split(&separator).enumerate() {
            if index > 0 {
                append(&mut result, "\r", offset..offset, Style::default());
            }
            let projected = Self::parse(part)?;
            for run in projected.runs {
                append(
                    &mut result,
                    &projected.text[run.text],
                    offset + run.source.start..offset + run.source.end,
                    run.style,
                );
            }
            offset += part.len() + separator.len();
        }
        Ok(result)
    }
    /// Serialize the actual native bold/italic runs, including selection replacements.
    pub fn from_native(text: &str, styles: &[Styled], newline: &str) -> Result<Self, &'static str> {
        Self::from_native_in(text, styles, newline, "")
    }
    pub fn from_native_in(
        text: &str,
        styles: &[Styled],
        newline: &str,
        prefix: &str,
    ) -> Result<Self, &'static str> {
        if text.encode_utf16().count() > MAX_PARAGRAPH_UNITS
            || text.contains(['\0', '\n', '\u{fffc}', '\u{feff}'])
        {
            return Err("Unsupported or oversized native input");
        }
        let length = text.encode_utf16().count();
        let mut cursor = 0;
        for span in styles {
            if span.utf16.start != cursor
                || span.utf16.end <= cursor
                || span.utf16.end > length
                || utf16_to_byte(text, cursor).is_none()
                || utf16_to_byte(text, span.utf16.end).is_none()
            {
                return Err("Invalid native style coordinates");
            }
            cursor = span.utf16.end;
        }
        if cursor != length {
            return Err("Incomplete native style coverage");
        }
        let mut source_lines = Vec::new();
        let mut start = 0;
        for line in text.split('\r') {
            let end = start + line.encode_utf16().count();
            let mut events = vec![Event::Start(Tag::Paragraph)];
            for span in styles {
                let a = span.utf16.start.max(start);
                let b = span.utf16.end.min(end);
                if a >= b {
                    continue;
                }
                let a_byte = utf16_to_byte(text, a).ok_or("Invalid scalar")?;
                let b_byte = utf16_to_byte(text, b).ok_or("Invalid scalar")?;
                if span.style.bold {
                    events.push(Event::Start(Tag::Strong));
                }
                if span.style.italic {
                    events.push(Event::Start(Tag::Emphasis));
                }
                let value = &text[a_byte..b_byte];
                events.push(Event::Text(value.into()));
                if span.style.italic {
                    events.push(Event::End(TagEnd::Emphasis));
                }
                if span.style.bold {
                    events.push(Event::End(TagEnd::Strong));
                }
            }
            events.push(Event::End(TagEnd::Paragraph));
            let mut output = String::new();
            pulldown_cmark_to_cmark::cmark(events, &mut output, Options::empty())
                .map_err(|_| "Markdown serialization failed")?;
            source_lines.push(output.trim_end_matches('\n').to_owned());
            start = end + 1;
        }
        let separator = if prefix.is_empty() {
            "\n\n".to_owned()
        } else {
            format!("\n{prefix}")
        };
        let source = source_lines.join(&separator).replace('\n', newline);
        let projected = Self::parse_with_prefix(&source, prefix)?;
        if projected.text != text {
            return Err("Markdown round-trip changed visible text; input retained");
        }
        Ok(projected)
    }
    /// Legacy exact-run editing remains for the isolated prototype; the live editor
    /// uses actual native style snapshots rather than inferring formatting from text.
    pub fn edit(&self, new_text: &str) -> Result<Option<SourceEdit>, &'static str> {
        if new_text == self.text {
            return Ok(None);
        }
        if new_text.encode_utf16().count() > MAX_PARAGRAPH_UNITS {
            return Err("Text block limit exceeded");
        }
        let prefix = self
            .text
            .chars()
            .zip(new_text.chars())
            .take_while(|(a, b)| a == b)
            .map(|(ch, _)| ch.len_utf8())
            .sum::<usize>();
        let tail = self.text[prefix..]
            .chars()
            .rev()
            .zip(new_text[prefix..].chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(ch, _)| ch.len_utf8())
            .sum::<usize>();
        let changed = prefix..self.text.len() - tail;
        let replacement = &new_text[prefix..new_text.len() - tail];
        let mut candidates: Vec<_> = self
            .runs
            .iter()
            .filter(|run| run.text.start <= changed.start && changed.end <= run.text.end)
            .collect();
        if changed.is_empty() && candidates.iter().any(|run| run.style == Style::default()) {
            candidates.retain(|run| run.style == Style::default());
        }
        let [run] = candidates.as_slice() else {
            return Err("Use the native styled editing path for cross-style edits");
        };
        if self.source.get(run.source.clone()) != self.text.get(run.text.clone()) {
            return Err("Use native styled editing for escaped source");
        }
        let range = run.source.start + changed.start - run.text.start
            ..run.source.start + changed.end - run.text.start;
        let mut source = self.source.clone();
        source.replace_range(range.clone(), replacement);
        let projected = Self::parse(&source)?;
        if projected.text != new_text {
            return Err("Edit changes Markdown meaning; input retained");
        }
        Ok(Some(SourceEdit {
            range,
            replacement: replacement.into(),
            projected,
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn plain(text: &str) -> Vec<Styled> {
        if text.is_empty() {
            vec![]
        } else {
            vec![Styled {
                utf16: 0..text.encode_utf16().count(),
                style: Style::default(),
            }]
        }
    }
    #[test]
    fn scalar_maps_reject_utf8_and_surrogate_interiors() {
        let text = "a한국😀e\u{301}👩\u{200d}💻";
        for byte in 0..=text.len() {
            if let Some(unit) = byte_to_utf16(text, byte) {
                assert_eq!(utf16_to_byte(text, unit), Some(byte));
            } else {
                assert!(!text.is_char_boundary(byte));
            }
        }
        assert_eq!(utf16_to_byte("a😀z", 2), None);
        assert_eq!(byte_to_utf16(text, 999), None);
    }
    #[test]
    fn source_ranges_and_styles_are_exact() {
        let p = Projection::parse("Plain **한국😀** and *café*.").unwrap();
        assert_eq!(p.text, "Plain 한국😀 and café.");
        for r in &p.runs {
            assert_eq!(p.source[r.source.clone()], p.text[r.text.clone()]);
        }
        assert!(p.runs.iter().any(|r| r.style.bold));
        assert!(p.runs.iter().any(|r| r.style.italic));
    }
    #[test]
    fn native_literals_empty_and_newlines_round_trip() {
        for text in [
            "",
            " ",
            "# title",
            "a &amp; <literal> [x] * _ `",
            "first\rsecond",
            "first\r",
            "\r\r",
            "\r한국😀\r",
            "a\t b",
        ] {
            let p = Projection::from_native(text, &plain(text), "\r\n")
                .unwrap_or_else(|e| panic!("{text:?}: {e}"));
            assert_eq!(p.text, text);
        }
        assert!(Projection::parse("[link](https://example.com)").is_err());
        assert!(Projection::parse("a `code`").is_err());
    }
    #[test]
    fn selection_replacement_and_nested_styles_round_trip() {
        let text = "plain bold italic";
        let styles = vec![
            Styled {
                utf16: 0..6,
                style: Style::default(),
            },
            Styled {
                utf16: 6..10,
                style: Style {
                    bold: true,
                    italic: true,
                },
            },
            Styled {
                utf16: 10..17,
                style: Style {
                    bold: false,
                    italic: true,
                },
            },
        ];
        let p = Projection::from_native(text, &styles, "\n").unwrap();
        assert_eq!(p.text, text);
        assert!(p.runs.iter().any(|r| r.style.bold && r.style.italic));
        assert_eq!(
            Projection::from_native("replacement", &plain("replacement"), "\n")
                .unwrap()
                .text,
            "replacement"
        );
    }
    #[test]
    fn limits_and_invalid_runs_are_inert() {
        assert!(Projection::from_native("\u{fffc}", &plain("\u{fffc}"), "\n").is_err());
        assert!(
            Projection::from_native(
                "😀",
                &[Styled {
                    utf16: 0..1,
                    style: Style::default()
                }],
                "\n"
            )
            .is_err()
        );
        assert!(Projection::from_native(&"x".repeat(MAX_PARAGRAPH_UNITS + 1), &[], "\n").is_err());
    }
    #[test]
    fn existing_simple_edit_preserves_markers() {
        let p = Projection::parse("Plain **bold** and *italic* 한국😀.").unwrap();
        assert_eq!(
            p.edit("Plain 굵은🚀 and italic 한국😀.")
                .unwrap()
                .unwrap()
                .projected
                .source,
            "Plain **굵은🚀** and *italic* 한국😀."
        );
    }
}
