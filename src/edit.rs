//! Conflict-aware saves. Source notes are never overwritten silently.
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_NOTE_BYTES: u64 = 2 * 1024 * 1024;
#[derive(Debug)]
pub struct EditBuffer {
    pub path: PathBuf,
    original: String,
    pub text: String,
}
impl EditBuffer {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut source = String::new();
        fs::File::open(path)?
            .take(MAX_NOTE_BYTES + 1)
            .read_to_string(&mut source)?;
        if source.len() as u64 > MAX_NOTE_BYTES {
            return Err(io::Error::other("Note exceeds 2 MiB"));
        }
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
        if self.text.len() as u64 > MAX_NOTE_BYTES {
            return Err(io::Error::other("Note exceeds 2 MiB"));
        }
        let current = fs::read_to_string(&self.path)?;
        if current != self.original {
            return Err(io::Error::other(
                "File changed outside Rusty Note. Save was blocked; copy your edits before reloading.",
            ));
        }
        if !self.dirty() {
            return Ok(());
        }
        let backup = self.path.with_extension("md.bak");
        fs::write(&backup, current.as_bytes())?;
        let temporary = self
            .path
            .with_extension(format!("md.{}.tmp", std::process::id()));
        let saved = if self.original.contains("\r\n") {
            self.text.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            self.text.clone()
        };
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        if let Err(error) = (|| {
            output.write_all(saved.as_bytes())?;
            output.sync_all()?;
            drop(output);
            fs::rename(&temporary, &self.path)
        })() {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
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
    #[test]
    fn saves_keep_backup_and_reject_external_changes() {
        let root = std::env::temp_dir().join(format!("rusty-note-edit-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
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
        let root = std::env::temp_dir().join(format!("rusty-note-crlf-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
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
    fn new_notes_never_replace_existing_files() {
        let root = std::env::temp_dir().join(format!("rusty-note-new-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
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
