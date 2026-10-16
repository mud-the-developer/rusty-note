//! Explicit, exact-source working copies. No UI, timers, global storage or Drop cleanup.
//!
//! A caller supplies the recovery root (production: app-local state; tests: target/).
//! Each new session owns a new directory with an immutable `snapshot.md`. Two bounded
//! checkpoint slots hold plain `baseline.md`, `working.md` and a small `record.json`.
//! Only an atomically replaced `current.json` publishes a completed slot. Failed or
//! interrupted writes never truncate the active slot. Unknown/partial data is left
//! for manual review, not automatically repaired or deleted.
//!
//! Files are synced before publication. This is process-loss recovery on a local
//! filesystem with atomic rename, not a power-loss or hostile-filesystem transaction.
use crate::edit::{MAX_NOTE_BYTES, SaveLock, read_note, save_source};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    ops::Range,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_METADATA_BYTES: u64 = 16 * 1024;
const SLOT_FILES: [&str; 3] = ["baseline.md", "working.md", "record.json"];
static NEXT_SESSION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stamp {
    bytes: u64,
    checksum: u64,
}
impl Stamp {
    fn of(text: &str) -> Self {
        // FNV-1a detects accidental corruption; it is not authentication. Recovery
        // directories must be private to the user, not an untrusted import format.
        Self {
            bytes: text.len() as u64,
            checksum: text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            }),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    session: String,
    original: PathBuf,
    snapshot: Stamp,
    revision: u64,
    pending_save: bool,
    baseline: Stamp,
    working: Stamp,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Head {
    revision: u64,
}

/// `Ok` from Save means the original matched the working source at the commit
/// point. A later recovery-write failure is a warning, NOT a failed original Save.
#[derive(Debug)]
pub struct SaveOutcome {
    pub replaced: bool,
    pub recovery_error: Option<io::Error>,
}

#[derive(Debug)]
pub struct Session {
    directory: PathBuf,
    original: PathBuf,
    snapshot: String,
    baseline: String,
    working: String,
    revision: u64,
    edit_version: u64,
}
impl Session {
    /// Create a separate session even for an already-open note. Never writes the note.
    pub fn create(original: &Path, recovery_root: &Path) -> io::Result<Self> {
        let original = fs::canonicalize(original)?;
        let snapshot = read_note(&original)?;
        fs::create_dir_all(recovery_root)?;
        let root = fs::canonicalize(recovery_root)?;
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        for _ in 0..100 {
            let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
            let directory = root.join(format!("draft-{}-{time:x}-{id:x}", std::process::id()));
            match fs::create_dir(&directory) {
                Ok(()) => {
                    let mut session = Self {
                        directory,
                        original,
                        baseline: snapshot.clone(),
                        working: snapshot.clone(),
                        snapshot,
                        revision: 0,
                        edit_version: 0,
                    };
                    write_new(
                        &session.directory.join("snapshot.md"),
                        session.snapshot.as_bytes(),
                    )?;
                    session.checkpoint_inner(false)?;
                    return Ok(session);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::other("No available draft session directory"))
    }

    /// Reopen only for the explicitly supplied note identity, even if its bytes
    /// changed or the file is missing. No writes, deletion, or automatic merging.
    pub fn reopen(original: &Path, directory: &Path) -> io::Result<Self> {
        require_directory(directory)?;
        let directory = fs::canonicalize(directory)?;
        let original = note_identity(original)?;
        let head: Head = read_json(&directory.join("current.json"))?;
        if head.revision == 0 {
            return Err(invalid("Invalid checkpoint revision"));
        }
        let snapshot = read_source(&directory.join("snapshot.md"))?;
        let (record, mut baseline, working) =
            read_checkpoint(&directory, &original, &snapshot, head.revision)?;
        // Save publishes its intent before touching the original. If replacement
        // completed but its final checkpoint did not, identical source is safe to
        // adopt as the saved baseline. Otherwise keep the old baseline and let the
        // normal external-change check block Save (also when the note is missing).
        if record.pending_save && read_note(&original).is_ok_and(|source| source == working) {
            baseline = working.clone();
        }
        Ok(Self {
            directory,
            original,
            snapshot,
            baseline,
            working,
            revision: head.revision,
            edit_version: 0,
        })
    }

    /// Completed on-disk publication identity for bounded companion metadata.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// In-memory coordinate epoch. Checkpoints do not invalidate source coordinates.
    pub fn edit_version(&self) -> u64 {
        self.edit_version
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn original_path(&self) -> &Path {
        &self.original
    }
    /// The unchanged starting source, including BOM and original line endings.
    pub fn snapshot(&self) -> &str {
        &self.snapshot
    }
    pub fn text(&self) -> &str {
        &self.working
    }
    /// Relative to the last successful Save, not to the immutable starting snapshot.
    pub fn dirty(&self) -> bool {
        self.working != self.baseline
    }
    pub fn original_changed(&self) -> io::Result<bool> {
        Ok(read_note(&self.original)? != self.baseline)
    }

    /// Replace a UTF-8 byte range only. BOM/newlines are ordinary source bytes;
    /// no renderer round-trip or newline normalization occurs. Invalid edits are inert.
    pub fn replace_range(&mut self, range: Range<usize>, replacement: &str) -> io::Result<()> {
        if self.working.get(range.clone()).is_none() || replacement.contains('\0') {
            return Err(invalid("Invalid UTF-8 edit range or NUL in replacement"));
        }
        let length = (self.working.len() - (range.end - range.start))
            .checked_add(replacement.len())
            .ok_or_else(|| invalid("Note exceeds 2 MiB"))?;
        if length as u64 > MAX_NOTE_BYTES {
            return Err(invalid("Note exceeds 2 MiB"));
        }
        self.edit_version = self
            .edit_version
            .checked_add(1)
            .ok_or_else(|| invalid("Edit version limit"))?;
        self.working.replace_range(range, replacement);
        Ok(())
    }

    /// Persist memory without changing the original. Reopening the same directory
    /// in multiple handles is allowed for reading; stale writers are rejected.
    pub fn checkpoint(&mut self) -> io::Result<()> {
        require_directory(&self.directory)?;
        let _lock = SaveLock::acquire(&self.directory.join("current.json"))?;
        self.check_revision()?;
        self.checkpoint_inner(false)
    }

    /// Explicit original Save: checkpoint the intent first, reuse the source
    /// editor's lock/backup/conflict-aware commit, then checkpoint the new baseline.
    /// An error before replacement retains both memory and completed recovery data.
    /// Non-cooperating external writers remain an optimistic race.
    pub fn save_original(&mut self) -> io::Result<SaveOutcome> {
        require_directory(&self.directory)?;
        let _lock = SaveLock::acquire(&self.directory.join("current.json"))?;
        self.check_revision()?;
        self.checkpoint_inner(true)?;
        let replaced = save_source(&self.original, &self.baseline, &self.working)?;
        self.baseline.clone_from(&self.working);
        Ok(SaveOutcome {
            replaced,
            recovery_error: self.checkpoint_inner(false).err(),
        })
    }

    fn check_revision(&self) -> io::Result<()> {
        let head: Head = read_json(&self.directory.join("current.json"))?;
        if head.revision != self.revision {
            return Err(invalid("Session checkpoint changed; reopen before writing"));
        }
        if read_source(&self.directory.join("snapshot.md"))? != self.snapshot {
            return Err(invalid("Session snapshot changed; manual review required"));
        }
        Ok(())
    }

    fn checkpoint_inner(&mut self, pending_save: bool) -> io::Result<()> {
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("Revision limit"))?;
        let slot = slot_path(&self.directory, revision);
        // Only recycle a completely verified, inactive checkpoint. Partial/unknown
        // files (including leftovers from a crash) are retained for manual review.
        if slot.try_exists()? {
            require_directory(&slot)?;
            let old: Record = read_json(&slot.join("record.json"))?;
            if old.revision == 0 || old.revision > revision || old.revision % 2 != revision % 2 {
                return Err(invalid("Unknown inactive checkpoint"));
            }
            read_checkpoint(
                &self.directory,
                &self.original,
                &self.snapshot,
                old.revision,
            )?;
            for name in SLOT_FILES {
                fs::remove_file(slot.join(name))?;
            }
            fs::remove_dir(&slot)?;
        }
        let record = Record {
            version: 1,
            session: session_name(&self.directory)?.to_owned(),
            original: self.original.clone(),
            snapshot: Stamp::of(&self.snapshot),
            revision,
            pending_save,
            baseline: Stamp::of(&self.baseline),
            working: Stamp::of(&self.working),
        };
        // Validate serialization/metadata bounds before creating any partial slot.
        let metadata = json_bytes(&record)?;
        fs::create_dir(&slot)?;
        write_new(&slot.join("baseline.md"), self.baseline.as_bytes())?;
        write_new(&slot.join("working.md"), self.working.as_bytes())?;
        write_new(&slot.join("record.json"), &metadata)?;
        let temporary = self.directory.join("current.next");
        let bytes = json_bytes(&Head { revision })?;
        // create_new never truncates an unknown/stale temporary. Only a temporary
        // created by THIS call may be removed on failure; never cleanup on Drop.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, self.directory.join("current.json"))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        self.revision = revision;
        Ok(())
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn note_identity(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let absolute = std::path::absolute(path)?;
            let parent = absolute
                .parent()
                .ok_or_else(|| invalid("Missing note parent"))?;
            let name = absolute
                .file_name()
                .ok_or_else(|| invalid("Missing note filename"))?;
            Ok(fs::canonicalize(parent)?.join(name))
        }
        Err(error) => Err(error),
    }
}
fn session_name(directory: &Path) -> io::Result<&str> {
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("Invalid session directory name"))?;
    if !name.starts_with("draft-")
        || name.len() > 96
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(invalid("Invalid session directory name"));
    }
    Ok(name)
}
fn plain_metadata(path: &Path) -> io::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    let linked = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::MetadataExt;
        linked || metadata.file_attributes() & 0x400 != 0 // any reparse point
    };
    if linked {
        return Err(invalid("Recovery links/reparse points are not supported"));
    }
    Ok(metadata)
}
fn require_directory(path: &Path) -> io::Result<()> {
    if !plain_metadata(path)?.is_dir() {
        return Err(invalid("Expected a recovery directory"));
    }
    Ok(())
}
fn require_file(path: &Path) -> io::Result<()> {
    if !plain_metadata(path)?.is_file() {
        return Err(invalid("Expected a regular recovery file"));
    }
    Ok(())
}
fn read_source(path: &Path) -> io::Result<String> {
    require_file(path)?;
    read_note(path)
}
fn read_json<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    require_file(path)?;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_METADATA_BYTES {
        return Err(invalid("Recovery metadata exceeds 16 KiB"));
    }
    serde_json::from_slice(&bytes).map_err(|error| invalid(&error.to_string()))
}
fn json_bytes(value: &impl Serialize) -> io::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() as u64 > MAX_METADATA_BYTES {
        return Err(invalid("Recovery metadata exceeds 16 KiB"));
    }
    Ok(bytes)
}
fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
fn slot_path(directory: &Path, revision: u64) -> PathBuf {
    directory.join(format!("checkpoint-{}", revision % 2))
}
fn read_checkpoint(
    directory: &Path,
    original: &Path,
    snapshot: &str,
    revision: u64,
) -> io::Result<(Record, String, String)> {
    let slot = slot_path(directory, revision);
    require_directory(&slot)?;
    // No recursive removal, and no deleting unexpected files in an otherwise valid slot.
    let mut count = 0;
    for entry in fs::read_dir(&slot)? {
        let name = entry?.file_name();
        count += 1;
        if count > SLOT_FILES.len() || !SLOT_FILES.iter().any(|known| name == *known) {
            return Err(invalid("Unknown checkpoint files; manual review required"));
        }
    }
    if count != SLOT_FILES.len() {
        return Err(invalid("Incomplete checkpoint; manual review required"));
    }
    let record: Record = read_json(&slot.join("record.json"))?;
    if record.version != 1
        || record.revision != revision
        || record.session != session_name(directory)?
        || record.original != original
        || record.snapshot != Stamp::of(snapshot)
    {
        return Err(invalid("Recovery identity/version/snapshot mismatch"));
    }
    let baseline = read_source(&slot.join("baseline.md"))?;
    let working = read_source(&slot.join("working.md"))?;
    if record.baseline != Stamp::of(&baseline) || record.working != Stamp::of(&working) {
        return Err(invalid("Incomplete or changed checkpoint source"));
    }
    Ok((record, baseline, working))
}
