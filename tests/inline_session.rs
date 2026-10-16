use rusty_note::{
    document::{Document, Kind},
    inline_session::{Note, Reference},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn setup(name: &str, source: &str) -> (PathBuf, PathBuf, Note) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/v1-m2-main/tests/{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    let original = root.join("한국어.md");
    let state = root.join("state.json");
    fs::write(&original, source).unwrap();
    let note = Note::create(&original, &state, source).unwrap();
    (original, state, note)
}
fn paragraphs(source: &str) -> Vec<std::ops::Range<usize>> {
    Document::parse(source)
        .blocks
        .iter()
        .filter(|b| b.kind == Kind::Paragraph)
        .filter_map(|b| b.paragraph_source.clone())
        .collect()
}
#[test]
fn duplicate_paragraph_coordinates_and_exact_source_save() {
    let source = "\u{feff}# Before\r\n\r\nSame **bold** 한국어 😀.\r\n\r\nSame **bold** 한국어 😀.\n\nAfter\r\n";
    let (original, state, mut note) = setup("duplicates", source);
    let spans = paragraphs(source);
    assert_eq!(spans.len(), 3);
    note.activate(spans[1].clone()).unwrap();
    let text = note
        .active
        .as_ref()
        .unwrap()
        .projection
        .text
        .replace("bold", "굵은 😀");
    assert!(
        note.sync(text.encode_utf16().collect(), 0..0, false)
            .unwrap()
    );
    let expected = format!(
        "{}Same **굵은 😀** 한국어 😀.{}",
        &source[..spans[1].start],
        &source[spans[1].end..]
    );
    assert_eq!(note.text(), expected);
    assert_eq!(fs::read_to_string(&original).unwrap(), source);
    note.checkpoint().unwrap();
    let reference = note.reference();
    drop(note);
    let mut note = Note::reopen(&original, &state, &reference).unwrap();
    assert!(!note.pending());
    assert_eq!(note.text(), expected);
    assert!(note.save_original().unwrap().replaced);
    assert_eq!(fs::read_to_string(&original).unwrap(), expected);
    assert_eq!(
        fs::read_to_string(original.with_extension("md.bak")).unwrap(),
        source
    );
    drop(note);
    let mut note = Note::reopen(&original, &state, &reference).unwrap();
    assert!(!note.dirty());
    assert!(!note.save_original().unwrap().replaced);
}
#[test]
fn unsupported_input_is_preserved_recovered_and_blocks_stale_save() {
    let source = "Before **bold** text.\n";
    let (original, state, mut note) = setup("pending", source);
    note.activate(paragraphs(source)[0].clone()).unwrap();
    let pending = "Before &amp; bold text.";
    assert!(
        !note
            .sync(pending.encode_utf16().collect(), 1..1, false)
            .unwrap()
    );
    assert!(note.pending());
    assert!(note.save_original().is_err());
    note.checkpoint().unwrap();
    let reference = note.reference();
    drop(note);
    let mut note = Note::reopen(&original, &state, &reference).unwrap();
    assert!(note.pending());
    assert_eq!(
        note.active.as_ref().unwrap().input,
        pending.encode_utf16().collect::<Vec<_>>()
    );
    assert!(note.deactivate().is_err());
    assert!(note.save_original().is_err());
    assert_eq!(fs::read_to_string(&original).unwrap(), source);
    let text = note.active.as_ref().unwrap().projection.text.clone();
    assert!(
        !note
            .sync(text.encode_utf16().collect(), 0..0, false)
            .unwrap()
    );
    assert!(!note.pending());
    note.deactivate().unwrap();
}
#[test]
fn companion_mismatch_retains_valid_markdown_and_blocks_operations() {
    let source = "Original words.\n";
    let (original, state, mut note) = setup("mismatch", source);
    note.activate(paragraphs(source)[0].clone()).unwrap();
    note.sync("Changed words.".encode_utf16().collect(), 0..0, false)
        .unwrap();
    fs::write(
        note.directory().join("inline-input.next"),
        "unknown partial data",
    )
    .unwrap();
    assert!(note.checkpoint().is_err());
    let reference = note.reference();
    drop(note);
    let mut recovered = Note::reopen(&original, &state, &reference).unwrap();
    assert_eq!(recovered.text(), "Changed words.\n");
    assert!(recovered.recovery_error.is_some());
    assert!(recovered.checkpoint().is_err());
    assert!(recovered.save_original().is_err());
    assert_eq!(
        fs::read_to_string(recovered.directory().join("inline-input.next")).unwrap(),
        "unknown partial data"
    );
    assert_eq!(fs::read_to_string(&original).unwrap(), source);
}
#[test]
fn external_change_keeps_working_copy_and_refuses_original_save() {
    let source = "Original words.\n";
    let (original, state, mut note) = setup("external", source);
    note.activate(paragraphs(source)[0].clone()).unwrap();
    note.sync("Working words.".encode_utf16().collect(), 0..0, false)
        .unwrap();
    note.checkpoint().unwrap();
    let reference = note.reference();
    fs::write(&original, "External content\r\n").unwrap();
    assert!(note.original_changed().unwrap());
    assert!(note.save_original().is_err());
    drop(note);
    let mut recovered = Note::reopen(&original, &state, &reference).unwrap();
    assert_eq!(recovered.text(), "Working words.\n");
    assert!(recovered.save_original().is_err());
    assert_eq!(
        fs::read_to_string(&original).unwrap(),
        "External content\r\n"
    );
}
#[test]
fn wrong_note_and_foreign_recovery_directory_are_rejected() {
    let (original, state, note) = setup("identity", "Words.\n");
    let reference = note.reference();
    let other = original.with_file_name("other.md");
    fs::write(&other, "Words.\n").unwrap();
    assert!(Note::reopen(&other, &state, &reference).is_err());
    let foreign_state = state.parent().unwrap().join("foreign/state.json");
    fs::create_dir_all(foreign_state.parent().unwrap().join("inline-drafts")).unwrap();
    assert!(Note::reopen(&original, &foreign_state, &reference).is_err());
    let wrong = Reference {
        original: other,
        directory: reference.directory,
    };
    assert!(Note::reopen(&original, &state, &wrong).is_err());
    assert!(Note::create(&original, &state, "stale viewer content").is_err());
}
#[test]
fn ranges_are_real_top_level_paragraphs_not_matching_text_heuristics() {
    let source =
        "Same text.\n\n> Same text.\n\n- Same text.\n\n```text\nSame text.\n```\n\nSame text.\n";
    let (_, _, mut note) = setup("scopes", source);
    let spans = paragraphs(source);
    assert_eq!(spans.len(), 2);
    assert_eq!(&source[spans[0].clone()], "Same text.");
    assert_eq!(&source[spans[1].clone()], "Same text.");
    let quote = source.find("> Same").unwrap() + 2;
    assert!(note.activate(quote..quote + "Same text.".len()).is_err());
    assert!(note.activate(spans[1].start + 1..spans[1].end).is_err());
    note.activate(spans[1].clone()).unwrap();
    assert!(
        note.sync("Other text.".encode_utf16().collect(), 0..0, false)
            .unwrap()
    );
    assert!(note.text().starts_with("Same text.\n\n> Same text."));
}
#[test]
fn composition_and_unpaired_utf16_never_overwrite_source() {
    let source = "한글 😀 words.\n";
    let (original, state, mut note) = setup("composition", source);
    note.activate(paragraphs(source)[0].clone()).unwrap();
    assert!(!note.sync(vec![0xd83d], 1..1, true).unwrap());
    assert!(note.pending());
    note.checkpoint().unwrap();
    let reference = note.reference();
    drop(note);
    let mut recovered = Note::reopen(&original, &state, &reference).unwrap();
    assert_eq!(recovered.active.as_ref().unwrap().input, vec![0xd83d]);
    assert!(recovered.save_original().is_err());
    assert_eq!(fs::read_to_string(&original).unwrap(), source);
}
#[test]
fn mixed_fixture_source_ranges_agree_with_rendered_blocks() {
    let source = include_str!("fixtures/inline-sticker.md");
    let (original, _, mut note) = setup("fixture", source);
    let document = Document::parse(source);
    for block in document.blocks.iter().filter(|b| b.kind == Kind::Paragraph) {
        let Some(range) = block.paragraph_source.clone() else {
            continue;
        };
        if note.activate(range).is_ok() {
            assert_eq!(note.active.as_ref().unwrap().projection.text, block.text);
        }
    }
    assert_eq!(fs::read_to_string(original).unwrap(), source);
}

#[test]
fn scene_origins_keep_exact_paragraph_ranges_across_reflow_and_zoom() {
    use rusty_note::layout::{Font, FontMeasure, Scene};
    use std::cell::Cell;
    // Synthetic widths are a layout regression fixture, never production metrics.
    struct Meter(Cell<usize>);
    impl FontMeasure for Meter {
        fn width(&self, text: &str, font: Font) -> f64 {
            self.0.set(self.0.get() + 1);
            text.chars().count() as f64 * f64::from(font.size) * 0.55
        }
    }
    let source = include_str!("fixtures/inline-sticker.md");
    let document = Document::parse(source);
    let meter = Meter(Cell::new(0));
    for zoom in [1.0, 1.4] {
        let mut scene = Scene::with_zoom(&document, &meter, zoom);
        assert_eq!(scene.paragraph_sources.len(), 7);
        let prepared = meter.0.get();
        for width in [550.0, 304.0, 550.0] {
            scene.reflow(width);
            assert_eq!(meter.0.get(), prepared);
            for (origin, range) in &scene.paragraph_sources {
                let block = document
                    .blocks
                    .iter()
                    .find(|block| block.paragraph_source.as_ref() == Some(range))
                    .unwrap();
                let lines: Vec<_> = scene
                    .lines
                    .iter()
                    .filter(|line| line.origin == *origin)
                    .collect();
                assert!(!lines.is_empty());
                for line in lines {
                    assert_eq!(&block.text[line.source_start..line.source_end], line.text);
                }
            }
        }
        let duplicate_a = &scene.paragraph_sources[2];
        let duplicate_b = &scene.paragraph_sources[3];
        assert_ne!(duplicate_a.0, duplicate_b.0);
        assert_ne!(duplicate_a.1, duplicate_b.1);
        assert_eq!(
            &source[duplicate_a.1.clone()],
            &source[duplicate_b.1.clone()]
        );
    }
}

#[test]
fn source_size_refusal_keeps_latest_input_pending_instead_of_saving_stale_text() {
    let prefix = "Editable.\n\n```text\n";
    let suffix = "\n```\n";
    let maximum = rusty_note::edit::MAX_NOTE_BYTES as usize;
    let source = format!(
        "{prefix}{}{suffix}",
        "x".repeat(maximum - prefix.len() - suffix.len())
    );
    let (original, _, mut note) = setup("source-limit", &source);
    note.activate(paragraphs(&source)[0].clone()).unwrap();
    assert!(
        !note
            .sync("More Editable.".encode_utf16().collect(), 0..0, false)
            .unwrap()
    );
    assert!(note.pending());
    assert_eq!(
        note.active.as_ref().unwrap().input,
        "More Editable.".encode_utf16().collect::<Vec<_>>()
    );
    assert_eq!(note.text(), source);
    assert!(note.save_original().is_err());
    assert_eq!(fs::read_to_string(original).unwrap(), source);
}

#[test]
fn recovered_selection_cannot_split_a_surrogate_pair() {
    let source = "😀 words.\n";
    let (original, state, mut note) = setup("bad-caret", source);
    note.activate(paragraphs(source)[0].clone()).unwrap();
    note.checkpoint().unwrap();
    let reference = note.reference();
    let path = note.directory().join("inline-input.json");
    let mut data: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    data["paragraph"]["selection"] = serde_json::json!({"start":1,"end":1});
    let bytes = serde_json::to_vec(&data).unwrap();
    fs::write(&path, &bytes).unwrap();
    drop(note);
    let mut recovered = Note::reopen(&original, &state, &reference).unwrap();
    assert!(recovered.recovery_error.is_some());
    assert!(recovered.save_original().is_err());
    assert_eq!(fs::read(path).unwrap(), bytes);
    assert_eq!(fs::read_to_string(original).unwrap(), source);
}

#[test]
fn v1_empty_multiline_checkpoint_and_history() {
    use rusty_note::inline_projection::{Style, Styled};
    let (original, state, mut note) = setup("v1-empty", "");
    note.activate(0..0).unwrap();
    let text = "한글 😀\rSecond line";
    let styles = [Styled {
        utf16: 0..text.encode_utf16().count(),
        style: Style::default(),
    }];
    assert!(
        note.sync_styled(text.encode_utf16().collect(), 0..0, false, &styles)
            .unwrap()
    );
    note.checkpoint().unwrap();
    let reference = note.reference();
    drop(note);
    let mut note = Note::reopen(&original, &state, &reference).unwrap();
    assert!(!note.pending());
    assert_eq!(note.active.as_ref().unwrap().projection.text, text);
    note.deactivate().unwrap();
    let before = note.text().to_owned();
    note.apply_edit(0..0, "Prefix ").unwrap();
    assert!(note.undo(false).unwrap());
    assert_eq!(note.text(), before);
    assert!(note.undo(true).unwrap());
    assert!(note.text().starts_with("Prefix "));
    assert_eq!(fs::read_to_string(original).unwrap(), "");
}

#[test]
fn v1_heading_list_continuation_and_task_ranges_are_exact() {
    use rusty_note::inline_projection::{Style, Styled};
    let source = "\u{feff}# Title\r\n\r\n- [ ] Task **bold**\r\n\r\nAfter\r\n";
    let (original, _, mut note) = setup("v1-structure", source);
    let doc = Document::parse(source);
    let heading = doc
        .blocks
        .iter()
        .find(|b| matches!(b.kind, Kind::Heading(_)))
        .unwrap();
    note.activate(heading.paragraph_source.clone().unwrap())
        .unwrap();
    let text = "Changed";
    let styles = [Styled {
        utf16: 0..text.len(),
        style: Style::default(),
    }];
    note.sync_styled(text.encode_utf16().collect(), 0..0, false, &styles)
        .unwrap();
    note.deactivate().unwrap();
    assert!(note.text().starts_with("\u{feff}# Changed\r\n\r\n"));
    let doc = Document::parse(note.text());
    let list = doc.blocks.iter().find(|b| b.kind == Kind::List).unwrap();
    note.activate(list.paragraph_source.clone().unwrap())
        .unwrap();
    let text = "First\rNext";
    let styles = [Styled {
        utf16: 0..text.len(),
        style: Style::default(),
    }];
    note.sync_styled(text.encode_utf16().collect(), 0..0, false, &styles)
        .unwrap();
    note.deactivate().unwrap();
    assert!(note.text().contains("- [ ] First\r\n- [ ] Next"));
    let doc = Document::parse(note.text());
    let tasks: Vec<_> = doc.blocks.iter().filter_map(|b| b.task.clone()).collect();
    assert_eq!(tasks.len(), 2);
    note.apply_edit(tasks[1].marker.clone(), "x").unwrap();
    assert!(note.text().contains("- [x] Next"));
    assert!(note.undo(false).unwrap());
    assert!(note.text().contains("- [ ] Next"));
    assert!(note.text().ends_with("\r\n\r\nAfter\r\n"));
    assert_eq!(fs::read_to_string(original).unwrap(), source);
}
