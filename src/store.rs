use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub struct Note {
    pub path: PathBuf,
    pub title: String,
    pub modified: SystemTime,
}
pub fn default_notes_dir() -> PathBuf {
    PathBuf::from(
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .unwrap_or_default(),
    )
    .join("Documents/Rusty Note")
}
pub fn list_notes(dir: &Path) -> io::Result<Vec<Note>> {
    let mut notes = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_file()
            || !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            continue;
        }
        let metadata = entry.metadata()?;
        if metadata.len() > 2 * 1024 * 1024 {
            continue;
        } // explicit bounded prototype input
        notes.push(Note {
            title: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path,
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        });
    }
    notes.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.path.cmp(&b.path)));
    Ok(notes)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub pinned: bool,
    pub light: bool,
    pub selected: Option<PathBuf>,
    pub notes_dir: Option<PathBuf>,
    pub scroll: f32,
    pub zoom: f32,
    pub inline_draft: Option<crate::inline_session::Reference>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            x: 160,
            y: 100,
            width: 460,
            height: 700,
            pinned: true,
            light: false,
            selected: None,
            notes_dir: None,
            scroll: 0.0,
            zoom: 1.0,
            inline_draft: None,
        }
    }
}
impl State {
    pub fn load(path: &Path) -> Self {
        Self::load_checked(path).unwrap_or_default()
    }
    pub fn load_checked(path: &Path) -> io::Result<Self> {
        use std::io::Read;
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(64 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "State exceeds 64 KiB; recovery data retained",
            ));
        }
        let mut state: Self = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        state.width = state.width.clamp(340, 1600);
        state.height = state.height.clamp(420, 1600);
        state.x = state.x.clamp(-32000, 32000);
        state.y = state.y.clamp(-32000, 32000);
        state.zoom = crate::layout::reading_zoom(state.zoom);
        state.scroll = if state.scroll.is_finite() {
            state.scroll.max(0.0)
        } else {
            0.0
        };
        Ok(state)
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        use std::io::Write;
        match Self::load_checked(path) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(io::Error::new(
                    error.kind(),
                    format!("Existing state needs review; not overwritten: {error}"),
                ));
            }
        }
        let bytes = serde_json::to_vec_pretty(self)?;
        if bytes.len() > 64 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "State exceeds 64 KiB",
            ));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn test_root(name: &str) -> PathBuf {
        let time = std::time::SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/v1-m2-main/state-tests/{name}-{}-{time:x}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn malformed_state_is_safe() {
        let state: State = serde_json::from_str("{}").unwrap();
        assert_eq!(state.width, 460);
        assert!(state.pinned);
        assert_eq!(state.scroll, 0.0);
        assert_eq!(state.zoom, 1.0);
    }
    #[test]
    fn reading_position_round_trips_without_changing_old_state_defaults() {
        let root = std::env::temp_dir().join(format!("rusty-reading-state-{}", std::process::id()));
        let path = root.join("state.json");
        let state = State {
            scroll: 420.0,
            zoom: 1.4,
            selected: Some(PathBuf::from("view.md")),
            ..Default::default()
        };
        state.save(&path).unwrap();
        let loaded = State::load(&path);
        assert_eq!(loaded.scroll, 420.0);
        assert_eq!(loaded.zoom, 1.4);
        assert_eq!(loaded.selected, state.selected);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_or_oversized_state_is_not_overwritten() {
        let root = test_root("malformed");
        let path = root.join("state.json");
        for bytes in [
            b"{broken".to_vec(),
            br#"{"inline_draft":{"original":17}}"#.to_vec(),
            vec![b' '; 64 * 1024 + 1],
        ] {
            fs::write(&path, &bytes).unwrap();
            assert!(State::load_checked(&path).is_err());
            assert!(State::default().save(&path).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert!(!path.with_extension("json.tmp").exists());
        }
    }
    #[test]
    fn unknown_state_temporary_is_retained_without_changing_active_state() {
        let root = test_root("temporary");
        let path = root.join("state.json");
        State::default().save(&path).unwrap();
        let before = fs::read(&path).unwrap();
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, "unknown recovery pointer").unwrap();
        let state = State {
            scroll: 100.0,
            ..State::default()
        };
        assert!(state.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(
            fs::read_to_string(temporary).unwrap(),
            "unknown recovery pointer"
        );
    }
    #[test]
    fn inline_association_round_trips_and_old_state_defaults_are_compatible() {
        let root = test_root("association");
        let path = root.join("state.json");
        fs::write(&path, "{}").unwrap();
        assert!(State::load_checked(&path).unwrap().inline_draft.is_none());
        let reference = crate::inline_session::Reference {
            original: root.join("note.md"),
            directory: root.join("inline-drafts/draft-test"),
        };
        let state = State {
            inline_draft: Some(reference.clone()),
            ..State::default()
        };
        state.save(&path).unwrap();
        let restored = State::load_checked(&path).unwrap().inline_draft.unwrap();
        assert_eq!(restored.original, reference.original);
        assert_eq!(restored.directory, reference.directory);
    }
}
