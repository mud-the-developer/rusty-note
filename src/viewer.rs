//! Read-only Markdown sticker input. This module never writes source files.
use crate::{edit::read_note, store::Note};
use std::{io, path::Path, time::SystemTime};
pub fn open(path: &Path) -> io::Result<(Note, String)> {
    let path = path.canonicalize()?;
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return Err(io::Error::other("Choose a Markdown (.md) file"));
    }
    let text = read_note(&path)?;
    let title = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let modified = path
        .metadata()?
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH);
    Ok((
        Note {
            path,
            title,
            modified,
        },
        text,
    ))
}
pub fn reload_scroll(scroll: f32, document_height: f32, visible_height: f32) -> f32 {
    let max = (document_height - visible_height).max(0.0);
    if scroll.is_finite() {
        scroll.clamp(0.0, max)
    } else {
        0.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn opening_and_reloading_never_modify_markdown() {
        let root = std::env::temp_dir().join(format!("rusty-viewer-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("sticker.md");
        let source = include_bytes!("../tests/fixtures/sticky-viewer.md");
        fs::write(&path, source).unwrap();
        for _ in 0..3 {
            let (note, text) = open(&path).unwrap();
            assert_eq!(note.title, "sticker");
            assert!(text.contains("한국어"));
            assert_eq!(fs::read(&path).unwrap(), source);
        }
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn reload_keeps_reading_position_but_clamps_shorter_documents() {
        assert_eq!(reload_scroll(420.0, 1800.0, 400.0), 420.0);
        assert_eq!(reload_scroll(420.0, 600.0, 400.0), 200.0);
        assert_eq!(reload_scroll(420.0, 200.0, 400.0), 0.0);
        assert_eq!(reload_scroll(f32::NAN, 800.0, 400.0), 0.0);
    }
    #[test]
    fn non_markdown_input_is_rejected() {
        assert!(open(Path::new("Cargo.toml")).is_err());
    }
}
