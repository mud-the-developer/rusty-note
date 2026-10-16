use rusty_note::{draft::Session, inline_projection::Projection};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn isolated() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/v1-m2/tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn mixed_fixture_source_splice_checkpoint_reopen_and_original_isolation() {
    let root = isolated();
    // Deliberately exercise BOM and mixed newlines outside the editable range.
    let fixture = include_str!("fixtures/inline-edit.md");
    let original = format!(
        "\u{feff}{}\r\nUnsupported: &copy; [x](y) \\*z\\*\n",
        fixture.replacen('\n', "\r\n", 3)
    );
    let path = root.join("original.md");
    fs::write(&path, &original).unwrap();
    let mut session = Session::create(&path, &root.join("drafts")).unwrap();
    let start = original.find("Plain text,").unwrap();
    let end = start + original[start..].find('\n').unwrap();
    let p = Projection::parse(&original[start..end]).unwrap();
    let next = p.text.replace("bold words", "굵은 😀 글자");
    let edit = p.edit(&next).unwrap().unwrap();
    session
        .replace_range(
            start + edit.range.start..start + edit.range.end,
            &edit.replacement,
        )
        .unwrap();
    assert_eq!(&session.text()[..start], &original[..start]);
    assert!(session.text().ends_with(&original[end..]));
    assert!(session.text().contains("**굵은 😀 글자**"));
    assert!(session.dirty());
    session.checkpoint().unwrap();
    let reopened = Session::reopen(&path, session.directory()).unwrap();
    assert_eq!(reopened.text(), session.text());
    assert_eq!(reopened.snapshot(), original);
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}

#[test]
fn unsupported_projection_leaves_session_and_complete_surrounding_source_unchanged() {
    let fixture = include_str!("fixtures/inline-edit.md").replace("\r\n", "\n");
    for source in [fixture.clone(), fixture.replace('\n', "\r\n")] {
        let root = isolated();
        let path = root.join("original.md");
        fs::write(&path, &source).unwrap();
        let session = Session::create(&path, &root.join("drafts")).unwrap();
        let document = rusty_note::document::Document::parse(&source);
        let unsupported = document
            .blocks
            .iter()
            .find(|block| block.text.contains("literal"))
            .unwrap()
            .paragraph_source
            .clone()
            .unwrap();
        assert!(Projection::parse(&source[unsupported]).is_err());
        let range = document
            .blocks
            .iter()
            .find(|block| block.text.starts_with("Plain text,"))
            .unwrap()
            .paragraph_source
            .clone()
            .unwrap();
        let p = Projection::parse(&source[range]).unwrap();
        assert!(
            p.edit(
                &p.text
                    .replace("bold words and italic words", "cross-style replacement")
            )
            .is_err()
        );
        assert_eq!(session.text(), source);
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
    }
}
