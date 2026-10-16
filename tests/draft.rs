use rusty_note::{
    document::{Document, Kind},
    draft::Session,
    edit::{EditBuffer, MAX_NOTE_BYTES},
    layout::{Font, FontMeasure, Scene},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const MIXED: &str = include_str!("fixtures/draft-edit.md");
static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

fn root(name: &str) -> PathBuf {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let id = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/v1-m1/tests/{name}-{}-{time:x}-{id}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
fn setup(name: &str, source: &str) -> (PathBuf, Session) {
    let root = root(name);
    let note = root.join("한국어 note.md");
    fs::write(&note, source).unwrap();
    let session = Session::create(&note, &root.join("recovery")).unwrap();
    (note, session)
}
fn replace(session: &mut Session, old: &str, new: &str) {
    let start = session.text().find(old).unwrap();
    session
        .replace_range(start..start + old.len(), new)
        .unwrap();
}
fn active_slot(directory: &Path) -> PathBuf {
    let head: Value =
        serde_json::from_slice(&fs::read(directory.join("current.json")).unwrap()).unwrap();
    directory.join(format!(
        "checkpoint-{}",
        head["revision"].as_u64().unwrap() % 2
    ))
}
fn other_slot(directory: &Path) -> PathBuf {
    let head: Value =
        serde_json::from_slice(&fs::read(directory.join("current.json")).unwrap()).unwrap();
    directory.join(format!(
        "checkpoint-{}",
        (head["revision"].as_u64().unwrap() + 1) % 2
    ))
}
fn assert_source(path: &Path, source: &str) {
    assert_eq!(fs::read(path).unwrap(), source.as_bytes());
}

#[test]
fn edits_and_checkpoints_never_touch_original_and_drop_keeps_last_checkpoint() {
    let (note, mut session) = setup("isolation", MIXED);
    let directory = session.directory().to_owned();
    assert_eq!(session.snapshot(), MIXED);
    assert_eq!(session.text(), MIXED);
    assert!(!session.dirty());
    assert_source(&note, MIXED);
    let start = MIXED.find("[ ]").unwrap() + 1;
    session.replace_range(start..start + 1, "x").unwrap();
    let expected = format!("{}x{}", &MIXED[..start], &MIXED[start + 1..]);
    assert_eq!(session.text(), expected);
    assert!(session.dirty());
    assert_source(&note, MIXED);
    session.checkpoint().unwrap();
    assert_source(&note, MIXED);
    assert_source(&directory.join("snapshot.md"), MIXED);
    assert_source(&active_slot(&directory).join("working.md"), &expected);
    assert!(!note.with_extension("md.bak").exists());
    replace(&mut session, "Final", "Not checkpointed");
    drop(session); // simulate loss of all in-memory state; no Drop cleanup
    let recovered = Session::reopen(&note, &directory).unwrap();
    assert_eq!(recovered.text(), expected);
    assert_eq!(recovered.snapshot(), MIXED);
    assert!(recovered.dirty());
    assert!(!recovered.original_changed().unwrap());
    assert_source(&note, MIXED);
}

#[test]
fn completed_checkpoint_survives_process_exit_without_destructors() {
    const CHILD_ROOT: &str = "RUSTY_NOTE_M1_PROCESS_LOSS_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = fs::canonicalize(root).unwrap();
        let test_root =
            fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("target/v1-m1/tests"))
                .unwrap();
        assert_eq!(root.parent(), Some(test_root.as_path()));
        let note = root.join("process.md");
        fs::write(&note, MIXED).unwrap();
        let mut session = Session::create(&note, &root.join("recovery")).unwrap();
        replace(&mut session, "[ ]", "[x]");
        session.checkpoint().unwrap();
        fs::write(
            root.join("session-path.txt"),
            session.directory().to_str().unwrap(),
        )
        .unwrap();
        replace(&mut session, "Final", "Only in lost memory");
        std::process::exit(0); // deliberately bypass all session/test destructors
    }
    let root = root("process-loss");
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "completed_checkpoint_survives_process_exit_without_destructors",
        ])
        .env(CHILD_ROOT, &root)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let directory = PathBuf::from(fs::read_to_string(root.join("session-path.txt")).unwrap());
    let note = root.join("process.md");
    let mut recovered = Session::reopen(&note, &directory).unwrap();
    assert_eq!(recovered.text(), MIXED.replacen("[ ]", "[x]", 1));
    assert!(recovered.dirty());
    recovered.checkpoint().unwrap();
    assert_source(&note, MIXED);
}

#[test]
fn exact_save_preserves_bom_mixed_endings_backup_and_second_save_baseline() {
    let (note, mut session) = setup("two-saves", MIXED);
    let directory = session.directory().to_owned();
    replace(&mut session, "한국어 🦀", "한글 😀");
    let first = session.text().to_owned();
    let outcome = session.save_original().unwrap();
    assert!(outcome.replaced);
    assert!(outcome.recovery_error.is_none());
    assert!(!session.dirty());
    assert_source(&note, &first);
    assert_source(&note.with_extension("md.bak"), MIXED);
    session.checkpoint().unwrap();
    drop(session);
    let mut session = Session::reopen(&note, &directory).unwrap();
    assert!(!session.dirty());
    assert!(!session.original_changed().unwrap());
    replace(&mut session, "[ ]", "[x]");
    let second = session.text().to_owned();
    assert!(session.save_original().unwrap().recovery_error.is_none());
    assert_source(&note, &second);
    assert_source(&note.with_extension("md.bak"), &first);
    assert_source(&directory.join("snapshot.md"), MIXED);
    session.checkpoint().unwrap();
    drop(session);
    let mut recovered = Session::reopen(&note, &directory).unwrap();
    assert_eq!(recovered.snapshot(), MIXED);
    assert_eq!(recovered.text(), second);
    assert!(!recovered.dirty());
    let no_op = recovered.save_original().unwrap();
    assert!(!no_op.replaced);
    assert!(no_op.recovery_error.is_none());
    assert_source(&note, &second);
    assert_source(&note.with_extension("md.bak"), &first);
}

#[test]
fn lf_crlf_mixed_bom_and_empty_source_are_exact_including_inserted_newlines() {
    for source in ["", "a\nb\n", "a\r\nb\r\n", "\u{feff}a\r\nb\nc\r"] {
        let (note, mut session) = setup("endings", source);
        let no_op = session.save_original().unwrap();
        assert!(!no_op.replaced);
        assert!(no_op.recovery_error.is_none());
        assert_source(&note, source);
        assert!(!note.with_extension("md.bak").exists());
        session
            .replace_range(source.len()..source.len(), "\n새\r\n")
            .unwrap();
        let expected = format!("{source}\n새\r\n");
        session.checkpoint().unwrap();
        let directory = session.directory().to_owned();
        drop(session);
        let mut recovered = Session::reopen(&note, &directory).unwrap();
        assert_eq!(recovered.text(), expected);
        recovered.save_original().unwrap();
        assert_source(&note, &expected);
        assert_source(&note.with_extension("md.bak"), source);
    }
}

#[test]
fn invalid_ranges_unicode_boundaries_and_nul_are_inert() {
    let (note, mut session) = setup("invalid-edits", "\u{feff}a한😀\r\n");
    let before = session.text().to_owned();
    let invalid = [
        (2, 1),
        (0, usize::MAX),
        (usize::MAX, usize::MAX),
        (1, 1),
        (0, 2),
        (5, 7),
        (7, 8),
    ];
    for (start, end) in invalid {
        assert!(session.replace_range(start..end, "new").is_err());
        assert_eq!(session.text(), before);
        assert!(!session.dirty());
    }
    assert!(session.replace_range(3..4, "\0").is_err());
    assert_eq!(session.text(), before);
    session.replace_range(4..7, "글").unwrap();
    session.replace_range(7..11, "🦀").unwrap();
    assert_eq!(session.text(), "\u{feff}a글🦀\r\n");
    replace(&mut session, "글🦀", "한😀");
    assert!(!session.dirty());
    session.replace_range(3..4, "").unwrap();
    assert_eq!(session.text(), "\u{feff}한😀\r\n");
    session.replace_range(3..3, "a").unwrap();
    assert_eq!(session.text(), before);
    assert!(!session.dirty());
    session.checkpoint().unwrap();
    assert_source(&note, &before);
}

#[test]
fn note_limits_apply_to_creation_edits_and_recovery_without_truncation() {
    let root = root("limits");
    let note = root.join("bounded.md");
    let recovery = root.join("recovery");
    for invalid in [
        vec![0xff],
        b"bad\0source".to_vec(),
        vec![b'a'; MAX_NOTE_BYTES as usize + 1],
    ] {
        fs::write(&note, &invalid).unwrap();
        assert!(Session::create(&note, &recovery).is_err());
        assert_eq!(fs::read(&note).unwrap(), invalid);
    }
    let maximum = "a".repeat(MAX_NOTE_BYTES as usize);
    fs::write(&note, &maximum).unwrap();
    let mut session = Session::create(&note, &recovery).unwrap();
    assert!(session.replace_range(0..0, "b").is_err());
    assert_eq!(session.text(), maximum);
    assert!(session.replace_range(0..1, "한").is_err());
    assert_eq!(session.text(), maximum);
    session.replace_range(0..3, "한").unwrap();
    assert_eq!(session.text().len() as u64, MAX_NOTE_BYTES);
    session.checkpoint().unwrap();
    let directory = session.directory().to_owned();
    let saved = session.text().to_owned();
    drop(session);
    assert_eq!(Session::reopen(&note, &directory).unwrap().text(), saved);
    assert_source(&note, &maximum);
    let working = active_slot(&directory).join("working.md");
    for invalid in [
        vec![0xff],
        b"bad\0source".to_vec(),
        vec![b'a'; MAX_NOTE_BYTES as usize + 1],
    ] {
        fs::write(&working, &invalid).unwrap();
        assert!(Session::reopen(&note, &directory).is_err());
        assert_eq!(fs::read(&working).unwrap(), invalid);
    }
}

#[test]
fn external_conflict_survives_checkpoint_and_recovery_without_overwrite() {
    let (note, mut session) = setup("external-conflict", "original\r\n");
    let directory = session.directory().to_owned();
    replace(&mut session, "original", "working");
    session.checkpoint().unwrap();
    fs::write(&note, "external\n").unwrap();
    assert!(session.original_changed().unwrap());
    assert!(session.save_original().is_err());
    assert_eq!(session.text(), "working\r\n");
    assert!(session.dirty());
    assert!(!note.with_extension("md.bak").exists());
    drop(session);
    let mut recovered = Session::reopen(&note, &directory).unwrap();
    assert!(recovered.original_changed().unwrap());
    assert_eq!(recovered.text(), "working\r\n");
    recovered.checkpoint().unwrap();
    assert!(recovered.save_original().is_err());
    assert_source(&note, "external\n");
    fs::remove_file(&note).unwrap(); // only this test's isolated source
    let mut recovered = Session::reopen(&note, &directory).unwrap();
    assert!(recovered.original_changed().is_err());
    assert!(recovered.save_original().is_err());
    assert!(!note.exists());
    assert_eq!(recovered.text(), "working\r\n");
}

#[test]
fn save_lock_and_backup_io_failure_retain_work_and_allow_retry() {
    let (note, mut session) = setup("save-failures", "original");
    let directory = session.directory().to_owned();
    replace(&mut session, "original", "working");
    let lock = note.with_extension("md.rusty-lock");
    fs::write(&lock, "external lock").unwrap();
    assert!(session.save_original().is_err());
    assert_source(&note, "original");
    assert_source(&lock, "external lock");
    assert_eq!(
        Session::reopen(&note, &directory).unwrap().text(),
        "working"
    );
    fs::remove_file(lock).unwrap();
    let backup = note.with_extension("md.bak");
    fs::create_dir(&backup).unwrap();
    assert!(session.save_original().is_err());
    assert!(session.dirty());
    assert_source(&note, "original");
    assert_eq!(
        Session::reopen(&note, &directory).unwrap().text(),
        "working"
    );
    fs::remove_dir(&backup).unwrap();
    // A pre-existing Save temporary must never be truncated or deleted.
    let temporary = note.with_extension(format!("md.{}.tmp", std::process::id()));
    fs::write(&temporary, "unknown").unwrap();
    assert!(session.save_original().is_err());
    assert_source(&temporary, "unknown");
    assert_source(&note, "original");
    fs::remove_file(&temporary).unwrap();
    session.save_original().unwrap();
    assert!(!session.dirty());
    assert_source(&note, "working");
    assert_source(&backup, "original");
}

#[test]
fn failed_checkpoint_publication_keeps_prior_completed_checkpoint_and_unknown_files() {
    let (note, mut session) = setup("failed-checkpoint", "original");
    replace(&mut session, "original", "completed");
    session.checkpoint().unwrap();
    let directory = session.directory().to_owned();
    let head = fs::read(directory.join("current.json")).unwrap();
    let obstacle = directory.join("current.next");
    fs::write(&obstacle, "unknown recovery data").unwrap();
    replace(&mut session, "completed", "unpublished");
    assert!(session.checkpoint().is_err()); // fails after writing the inactive slot
    assert_eq!(session.text(), "unpublished");
    assert_eq!(fs::read(directory.join("current.json")).unwrap(), head);
    assert_eq!(
        Session::reopen(&note, &directory).unwrap().text(),
        "completed"
    );
    assert_source(&obstacle, "unknown recovery data");
    assert_source(&note, "original");
    // The test explicitly resolves its own obstacle; a verified orphan slot can be reused.
    fs::remove_file(obstacle).unwrap();
    session.checkpoint().unwrap();
    assert_eq!(
        Session::reopen(&note, &directory).unwrap().text(),
        "unpublished"
    );
    assert!(fs::read_dir(&directory).unwrap().count() <= 4); // snapshot, head, two slots
}

#[test]
fn simulated_interrupted_partial_slot_never_hides_the_completed_checkpoint() {
    let (note, mut session) = setup("partial-slot", "original");
    replace(&mut session, "original", "completed");
    session.checkpoint().unwrap();
    let directory = session.directory().to_owned();
    drop(session);
    let partial = other_slot(&directory);
    // Simulate interruption while replacing the inactive slot, before record/head publication.
    fs::remove_file(partial.join("record.json")).unwrap();
    fs::write(partial.join("working.md"), b"partial\xf0\x9f").unwrap();
    let mut recovered = Session::reopen(&note, &directory).unwrap();
    assert_eq!(recovered.text(), "completed");
    assert!(recovered.checkpoint().is_err());
    assert_eq!(
        fs::read(partial.join("working.md")).unwrap(),
        b"partial\xf0\x9f"
    );
    assert_source(&note, "original");
}

#[test]
fn committed_save_reports_recovery_warning_and_reopens_without_spurious_conflict() {
    let (note, mut session) = setup("committed-warning", "original");
    let directory = session.directory().to_owned();
    // Pre-Save writes the other slot. Post-Save recycling of this old slot must
    // fail, retaining unknown data, AFTER the original has actually been replaced.
    let obstacle = active_slot(&directory).join("user-file.txt");
    fs::write(&obstacle, "do not delete").unwrap();
    replace(&mut session, "original", "saved");
    let outcome = session.save_original().unwrap();
    assert!(outcome.replaced);
    assert!(outcome.recovery_error.is_some());
    assert!(!session.dirty());
    assert!(!session.original_changed().unwrap());
    assert_source(&note, "saved");
    assert_source(&note.with_extension("md.bak"), "original");
    assert_source(&obstacle, "do not delete");
    let recovered = Session::reopen(&note, &directory).unwrap();
    assert_eq!(recovered.text(), "saved");
    assert!(!recovered.dirty());
    assert!(!recovered.original_changed().unwrap());
    fs::remove_file(obstacle).unwrap();
    // Same live handle can checkpoint later edits against its truthful saved baseline.
    replace(&mut session, "saved", "second");
    session.checkpoint().unwrap();
    drop(session);
    let mut recovered = Session::reopen(&note, &directory).unwrap();
    assert!(recovered.dirty());
    assert!(!recovered.original_changed().unwrap());
    recovered.save_original().unwrap();
    assert_source(&note, "second");
    assert_source(&note.with_extension("md.bak"), "saved");
}

#[test]
fn pending_save_with_later_external_change_still_blocks_recovery_save() {
    let (note, mut session) = setup("pending-conflict", "original");
    let directory = session.directory().to_owned();
    let obstacle = active_slot(&directory).join("unknown");
    fs::write(&obstacle, "keep").unwrap();
    replace(&mut session, "original", "saved");
    assert!(session.save_original().unwrap().recovery_error.is_some());
    fs::write(&note, "external after save").unwrap();
    drop(session);
    fs::remove_file(obstacle).unwrap();
    let mut recovered = Session::reopen(&note, &directory).unwrap();
    assert_eq!(recovered.text(), "saved");
    assert!(recovered.original_changed().unwrap());
    assert!(recovered.save_original().is_err());
    assert_source(&note, "external after save");
    assert_source(&note.with_extension("md.bak"), "original");
}

#[test]
fn sessions_for_same_and_different_notes_are_isolated_and_stale_handles_rejected() {
    let (note, mut first) = setup("sessions", "original");
    let recovery = first.directory().parent().unwrap();
    let mut second = Session::create(&note, recovery).unwrap();
    let other_note = note.with_file_name("other.md");
    fs::write(&other_note, "original").unwrap(); // same bytes, different identity
    let mut other = Session::create(&other_note, recovery).unwrap();
    assert_ne!(first.directory(), second.directory());
    assert_ne!(first.directory(), other.directory());
    assert!(Session::reopen(&other_note, first.directory()).is_err());
    let mut stale = Session::reopen(&note, first.directory()).unwrap();
    replace(&mut first, "original", "first");
    replace(&mut second, "original", "second");
    replace(&mut other, "original", "other");
    first.checkpoint().unwrap();
    assert!(stale.checkpoint().is_err());
    assert!(stale.save_original().is_err());
    second.checkpoint().unwrap();
    other.checkpoint().unwrap();
    first.save_original().unwrap();
    assert!(second.save_original().is_err());
    assert_eq!(
        Session::reopen(&note, second.directory()).unwrap().text(),
        "second"
    );
    assert_source(&other_note, "original");
    other.save_original().unwrap();
    assert_source(&note, "first");
    assert_source(&other_note, "other");
    assert!(Session::reopen(&other_note, first.directory()).is_err());
    assert_eq!(first.snapshot(), "original");
    assert_eq!(second.snapshot(), "original");
}

#[test]
fn malformed_metadata_wrong_identity_and_changed_checkpoint_bytes_are_retained() {
    let (note, session) = setup("malformed", MIXED);
    let directory = session.directory().to_owned();
    let record_path = active_slot(&directory).join("record.json");
    let valid = fs::read(&record_path).unwrap();
    let value: Value = serde_json::from_slice(&valid).unwrap();
    for (key, invalid) in [
        ("version", json!(2)),
        ("session", json!("draft-wrong")),
        ("original", json!(note.with_file_name("wrong.md"))),
        ("revision", json!(0)),
        ("snapshot", json!({"bytes": 0, "checksum": 0})),
        ("pending_save", json!("true")),
        ("extra", json!("unknown field")),
    ] {
        let mut bad = value.clone();
        bad[key] = invalid;
        let bytes = serde_json::to_vec(&bad).unwrap();
        fs::write(&record_path, &bytes).unwrap();
        assert!(Session::reopen(&note, &directory).is_err());
        assert_eq!(fs::read(&record_path).unwrap(), bytes);
    }
    for bytes in [b"{".to_vec(), vec![b' '; 16 * 1024 + 1], vec![0xff]] {
        fs::write(&record_path, &bytes).unwrap();
        assert!(Session::reopen(&note, &directory).is_err());
        assert_eq!(fs::read(&record_path).unwrap(), bytes);
    }
    fs::write(&record_path, &valid).unwrap();
    for name in ["baseline.md", "working.md"] {
        let path = active_slot(&directory).join(name);
        fs::write(&path, "tampered").unwrap();
        assert!(Session::reopen(&note, &directory).is_err());
        assert_source(&path, "tampered");
        fs::write(&path, MIXED).unwrap();
    }
    let snapshot = directory.join("snapshot.md");
    fs::write(&snapshot, "tampered snapshot").unwrap();
    assert!(Session::reopen(&note, &directory).is_err());
    assert_source(&snapshot, "tampered snapshot");
    fs::write(&snapshot, MIXED).unwrap();
    let head = directory.join("current.json");
    let valid_head = fs::read(&head).unwrap();
    for bytes in [
        b"{\"revision\":0}".to_vec(),
        b"{\"revision\":1,\"path\":\"../other\"}".to_vec(),
        vec![b' '; 16 * 1024 + 1],
    ] {
        fs::write(&head, &bytes).unwrap();
        assert!(Session::reopen(&note, &directory).is_err());
        assert_eq!(fs::read(&head).unwrap(), bytes);
    }
    fs::write(&head, valid_head).unwrap();
    assert!(Session::reopen(&note, &directory).is_ok());
    assert_source(&note, MIXED);
}

#[test]
fn recovery_file_directories_and_unknown_session_data_are_not_deleted() {
    let (note, session) = setup("unknown", "source");
    let directory = session.directory().to_owned();
    let unknown = directory.join("unknown-user-file");
    fs::write(&unknown, "keep").unwrap();
    let working = active_slot(&directory).join("working.md");
    fs::remove_file(&working).unwrap();
    fs::create_dir(&working).unwrap();
    fs::write(working.join("keep"), "nested user file").unwrap();
    drop(session);
    assert!(Session::reopen(&note, &directory).is_err());
    assert_source(&unknown, "keep");
    assert_source(&working.join("keep"), "nested user file");
    assert_source(&note, "source");
}

#[test]
fn session_lock_blocks_checkpoint_without_touching_source_or_unknown_lock() {
    let (note, mut session) = setup("session-lock", "source");
    let lock = session.directory().join("current.md.rusty-lock");
    fs::write(&lock, "stale lock for manual review").unwrap();
    replace(&mut session, "source", "working");
    assert!(session.checkpoint().is_err());
    assert!(session.save_original().is_err());
    assert_source(&lock, "stale lock for manual review");
    assert_source(&note, "source");
    assert_eq!(session.text(), "working");
    // Reading the last completed checkpoint is possible even after an interrupted lock owner.
    assert_eq!(
        Session::reopen(&note, session.directory()).unwrap().text(),
        "source"
    );
}

#[test]
fn source_editor_noop_and_normalizing_save_keep_existing_behavior() {
    let (note, _) = setup("editor-compatibility", MIXED);
    let mut editor = EditBuffer::open(&note).unwrap();
    assert_eq!(editor.text, MIXED.replace("\r\n", "\n"));
    assert!(!editor.dirty());
    editor.save().unwrap();
    assert_source(&note, MIXED);
    assert!(!note.with_extension("md.bak").exists());
    editor.text.push_str("\nMore\n");
    editor.save().unwrap();
    let expected = format!("{}\nMore\n", MIXED.replace("\r\n", "\n")).replace('\n', "\r\n");
    assert_source(&note, &expected);
    assert_source(&note.with_extension("md.bak"), MIXED);
    assert!(!editor.dirty());
    editor.save().unwrap();
    assert_source(&note.with_extension("md.bak"), MIXED);
    fs::write(&note, "external").unwrap();
    assert!(editor.save().is_err()); // even a no-op Save checks external changes
    assert_source(&note, "external");
}

#[test]
fn source_editor_rejects_nul_and_crlf_expansion_past_limit() {
    let (note, _) = setup("editor-limits", "a\r\n");
    let mut editor = EditBuffer::open(&note).unwrap();
    editor.text = "bad\0text".into();
    assert!(editor.save().is_err());
    editor.text = "\n".repeat(MAX_NOTE_BYTES as usize);
    assert!(editor.save().is_err());
    assert_source(&note, "a\r\n");
    assert!(!note.with_extension("md.bak").exists());
}

#[test]
fn mixed_fixture_parser_and_layout_survive_source_edits_without_round_trip() {
    struct FixtureMeasure;
    impl FontMeasure for FixtureMeasure {
        fn width(&self, text: &str, font: Font) -> f64 {
            text.chars().count() as f64 * f64::from(font.size) * 0.5
        }
    }
    let (note, mut session) = setup("mixed-layout", MIXED);
    replace(&mut session, "[ ]", "[x]");
    replace(&mut session, "Draft 한국어 🦀", "Edited 한국어 🦀");
    let document = Document::parse(session.text());
    for kind in [
        Kind::Heading(1),
        Kind::List,
        Kind::Quote,
        Kind::Table,
        Kind::Image,
        Kind::Code,
    ] {
        assert!(document.blocks.iter().any(|block| block.kind == kind));
    }
    assert!(document.blocks.iter().any(|block| block.text.contains("☑")));
    let code: Vec<_> = document
        .blocks
        .iter()
        .filter_map(|block| block.fence.as_ref())
        .collect();
    assert_eq!(code.len(), 3);
    for fence in code {
        let exact = &session.text()[fence.source.clone()];
        assert!(MIXED.contains(exact)); // opaque blocks and source ranges remain exact
    }
    let mut scene = Scene::new(&document, &FixtureMeasure);
    for width in [544.0, 304.0] {
        scene.reflow(width);
        assert!(!scene.lines.is_empty());
        assert!(!scene.find("Final paragraph").is_empty());
        assert!(!scene.find("Edited 한국어").is_empty());
    }
    session.checkpoint().unwrap();
    assert_source(&note, MIXED);
}
