//! Non-destructive note management. Names cannot escape the chosen directory.
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
pub fn valid_name(name: &str) -> bool {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 160
        || name.ends_with(['.', ' '])
        || name.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\0'])
        || name.chars().any(char::is_control)
    {
        return false;
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    !stem.is_empty()
        && !matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
}
pub fn rename(path: &Path, name: &str) -> io::Result<PathBuf> {
    if !valid_name(name) {
        return Err(io::Error::other(
            "Use a normal filename without path separators or reserved device names",
        ));
    }
    let name = name.trim().strip_suffix(".md").unwrap_or(name.trim());
    let destination = path.with_file_name(format!("{name}.md"));
    if destination == path {
        return Ok(destination);
    }
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "A note with that name already exists",
        ));
    }
    fs::hard_link(path, &destination)?;
    if let Err(error) = fs::remove_file(path) {
        let _ = fs::remove_file(&destination);
        return Err(error);
    }
    Ok(destination)
}
pub fn trash(path: &Path) -> io::Result<PathBuf> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("Note has no directory"))?
        .join(".rusty-note-trash");
    fs::create_dir_all(&directory)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let destination = directory.join(format!("{stamp}-{name}"));
    fs::hard_link(path, &destination)?;
    if let Err(error) = fs::remove_file(path) {
        let _ = fs::remove_file(&destination);
        return Err(error);
    }
    Ok(destination)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_windows_names_are_rejected() {
        for name in [
            "",
            "../escape",
            "folder/note",
            "CON",
            "nul.md",
            "COM1",
            "bad:note",
            "name.",
        ] {
            assert!(!valid_name(name), "{name}");
        }
        assert!(valid_name("프로젝트 notes"));
    }
    #[test]
    fn rename_and_trash_preserve_content_and_do_not_replace() {
        let root = std::env::temp_dir().join(format!("rusty-note-manage-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("first.md");
        fs::write(&path, "# Keep").unwrap();
        fs::write(root.join("exists.md"), "existing").unwrap();
        assert!(rename(&path, "exists").is_err());
        assert!(path.exists());
        let renamed = rename(&path, "second").unwrap();
        assert!(!path.exists());
        let stored = trash(&renamed).unwrap();
        assert!(!renamed.exists());
        assert_eq!(fs::read_to_string(stored).unwrap(), "# Keep");
        fs::remove_dir_all(root).unwrap();
    }
}
