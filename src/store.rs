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
    .join("Desktop/repo/notes")
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
        }
    }
}
impl State {
    pub fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temporary, path)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_state_is_safe() {
        let state: State = serde_json::from_str("{}").unwrap();
        assert_eq!(state.width, 460);
        assert!(state.pinned);
    }
}
