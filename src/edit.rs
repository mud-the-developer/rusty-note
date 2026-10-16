//! Conflict-aware saves. Source notes are never overwritten silently.
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_NOTE_BYTES: u64 = 2 * 1024 * 1024;
pub fn read_note(path: &Path) -> io::Result<String> {
    let mut text = String::new();
    fs::File::open(path)?
        .take(MAX_NOTE_BYTES + 1)
        .read_to_string(&mut text)?;
    validate_note(&text)?;
    Ok(text)
}
pub(crate) fn validate_note(text: &str) -> io::Result<()> {
    if text.len() as u64 > MAX_NOTE_BYTES || text.contains('\0') {
        return Err(io::Error::other(
            "Unsupported note: exceeds 2 MiB or contains NUL bytes",
        ));
    }
    Ok(())
}
pub(crate) struct SaveLock(PathBuf);
impl SaveLock {
    pub(crate) fn acquire(path: &Path) -> io::Result<Self> {
        let lock = path.with_extension("md.rusty-lock");
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock)
            .map_err(|_| {
                io::Error::other(
                    "Another save is in progress or a stale .rusty-lock needs manual review",
                )
            })?;
        Ok(Self(lock))
    }
}
impl Drop for SaveLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
/// Commit exact UTF-8 source, without newline conversion. An error means replacement
/// did not complete; no fallible work follows the rename. Cooperating writers use
/// SaveLock; the final content check is still optimistic for other writers.
pub(crate) fn save_source(path: &Path, original: &str, saved: &str) -> io::Result<bool> {
    validate_note(saved)?;
    let _lock = SaveLock::acquire(path)?;
    let current = read_note(path)?;
    if current != original {
        return Err(io::Error::other(
            "File changed outside Rusty Note. Save was blocked; copy your edits before reloading.",
        ));
    }
    if saved == original {
        return Ok(false);
    }
    let backup = path.with_extension("md.bak");
    let mut backup_file = fs::File::create(&backup)?;
    backup_file.write_all(current.as_bytes())?;
    backup_file.sync_all()?;
    drop(backup_file);
    let temporary = path.with_extension(format!("md.{}.tmp", std::process::id()));
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    if let Err(error) = (|| {
        output.write_all(saved.as_bytes())?;
        output.sync_all()?;
        drop(output);
        if read_note(path)? != original {
            return Err(io::Error::other(
                "File changed during Save; replacement blocked",
            ));
        }
        fs::rename(&temporary, path)
    })() {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(true)
}
#[derive(Debug)]
pub struct EditBuffer {
    pub path: PathBuf,
    original: String,
    pub text: String,
}
impl EditBuffer {
    pub fn open(path: &Path) -> io::Result<Self> {
        let source = read_note(path)?;
        Ok(Self {
            path: path.to_owned(),
            original: source.clone(),
            text: source.replace("\r\n", "\n"),
        })
    }
    pub fn dirty(&self) -> bool {
        self.text != self.original.replace("\r\n", "\n")
    }
    pub fn save(&mut self) -> io::Result<()> {
        validate_note(&self.text)?;
        // The source editor intentionally normalizes newlines on an actual edit,
        // but a no-op Save must keep even mixed endings byte-for-byte unchanged.
        let saved = if !self.dirty() {
            self.original.clone()
        } else if self.original.contains("\r\n") {
            self.text.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            self.text.clone()
        };
        save_source(&self.path, &self.original, &saved)?;
        self.original = saved;
        Ok(())
    }
}
pub fn create_note(directory: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(directory)?;
    for index in 1..10000 {
        let path = directory.join(format!("Note-{index:03}.md"));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(b"# New note\n\n")?;
                file.sync_all()?;
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("No available note filename"))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn test_root(name: &str) -> PathBuf {
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/v1-m1/tests/edit-{name}-{}-{time:x}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn saves_keep_backup_and_reject_external_changes() {
        let root = test_root("save");
        let path = root.join("safe.md");
        fs::write(&path, "# Old\n한국어").unwrap();
        let mut buffer = EditBuffer::open(&path).unwrap();
        buffer.text = "# New\n한국어".into();
        buffer.save().unwrap();
        assert_eq!(
            fs::read_to_string(path.with_extension("md.bak")).unwrap(),
            "# Old\n한국어"
        );
        buffer.text = "unsaved".into();
        fs::write(&path, "external").unwrap();
        assert!(buffer.save().is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "external");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn crlf_is_not_dirty_and_is_preserved_on_save() {
        let root = test_root("crlf");
        let path = root.join("crlf.md");
        fs::write(&path, "# Old\r\n\r\nText\r\n").unwrap();
        let mut buffer = EditBuffer::open(&path).unwrap();
        assert!(!buffer.dirty());
        buffer.text.push_str("More\n");
        buffer.save().unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# Old\r\n\r\nText\r\nMore\r\n"
        );
        assert!(!buffer.dirty());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn bounded_reads_reject_binary_and_oversize_notes() {
        let root = test_root("bounded");
        let path = root.join("bounded.md");
        fs::write(&path, b"bad\0note").unwrap();
        assert!(read_note(&path).is_err());
        fs::write(&path, vec![b'x'; MAX_NOTE_BYTES as usize + 1]).unwrap();
        assert!(read_note(&path).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn existing_save_lock_does_not_change_note_or_backup() {
        let root = test_root("lock");
        let path = root.join("locked.md");
        fs::write(&path, "original").unwrap();
        fs::write(path.with_extension("md.rusty-lock"), "").unwrap();
        let mut buffer = EditBuffer::open(&path).unwrap();
        buffer.text = "new".into();
        assert!(buffer.save().is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
        assert!(!path.with_extension("md.bak").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn new_notes_never_replace_existing_files() {
        let root = test_root("new");
        fs::write(root.join("Note-001.md"), "keep").unwrap();
        let path = create_note(&root).unwrap();
        assert_ne!(path.file_name().unwrap(), "Note-001.md");
        assert_eq!(
            fs::read_to_string(root.join("Note-001.md")).unwrap(),
            "keep"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
