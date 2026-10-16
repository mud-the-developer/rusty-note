//! Experimental main-window binding: exact Markdown plus bounded pending native input.
//! Original Save is explicit. Companion input and M1 publication are not one atomic
//! transaction: mismatches block editing/Save and retain all recovery files.
use crate::{
    document::{Document, Kind},
    draft::{SaveOutcome, Session},
    inline_projection::{MAX_PARAGRAPH_UNITS, Projection},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    ops::Range,
    path::{Path, PathBuf},
};

const MAX_INPUT_BYTES: u64 = 192 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub original: PathBuf,
    pub directory: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Active {
    pub source: Range<usize>,
    pub projection: Projection,
    pub input: Vec<u16>,
    pub selection: Range<usize>,
    pub error: Option<String>,
    epoch: u64,
    initial_source: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    version: u32,
    original: PathBuf,
    source_bytes: usize,
    source_stamp: u64,
    paragraph: Option<ParagraphInput>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParagraphInput {
    range: Range<usize>,
    source: String,
    utf16: Vec<u16>,
    selection: Range<usize>,
}

struct Change {
    start: usize,
    before: String,
    after: String,
}
pub struct Note {
    session: Session,
    pub active: Option<Active>,
    pub recovery_error: Option<String>,
    undo: Vec<Change>,
    redo: Vec<Change>,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn stamp(source: &str) -> u64 {
    source.bytes().fold(0xcbf29ce484222325, |value, byte| {
        (value ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
fn plain(path: &Path) -> io::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    let linked = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::MetadataExt;
        linked || metadata.file_attributes() & 0x400 != 0
    };
    if linked {
        return Err(invalid("Recovery links/reparse points are not supported"));
    }
    Ok(metadata)
}
pub fn recovery_root(state_file: &Path) -> io::Result<PathBuf> {
    let state_file = std::path::absolute(state_file)?;
    Ok(state_file
        .parent()
        .ok_or_else(|| invalid("Missing state directory"))?
        .join("inline-drafts"))
}

impl Note {
    pub fn create(original: &Path, state_file: &Path, expected: &str) -> io::Result<Self> {
        let session = Session::create(original, &recovery_root(state_file)?)?;
        if session.text() != expected {
            return Err(invalid(
                "Original changed before editing; reload before retrying",
            ));
        }
        let mut note = Self {
            session,
            active: None,
            recovery_error: None,
            undo: Vec::new(),
            redo: Vec::new(),
        };
        note.checkpoint()?;
        Ok(note)
    }
    pub fn reopen(original: &Path, state_file: &Path, reference: &Reference) -> io::Result<Self> {
        let root = fs::canonicalize(recovery_root(state_file)?)?;
        if !plain(&reference.directory)?.is_dir() {
            return Err(invalid("Invalid recovery directory"));
        }
        let directory = fs::canonicalize(&reference.directory)?;
        if directory.parent() != Some(root.as_path()) {
            return Err(invalid(
                "Recovery directory is outside this state directory",
            ));
        }
        let session = Session::reopen(original, &directory)?;
        if session.original_path() != reference.original {
            return Err(invalid("Recovery note association does not match"));
        }
        let mut note = Self {
            session,
            active: None,
            recovery_error: None,
            undo: Vec::new(),
            redo: Vec::new(),
        };
        if let Err(error) = note.restore_input() {
            // Keep the valid M1 Markdown visible, but never silently replace or
            // save over an unrecognised pending-input companion.
            note.recovery_error = Some(format!(
                "Input recovery needs review; files retained: {error}"
            ));
        }
        Ok(note)
    }
    pub fn reference(&self) -> Reference {
        Reference {
            original: self.session.original_path().to_owned(),
            directory: self.session.directory().to_owned(),
        }
    }
    pub fn text(&self) -> &str {
        self.session.text()
    }
    pub fn directory(&self) -> &Path {
        self.session.directory()
    }
    pub fn original_changed(&self) -> io::Result<bool> {
        self.session.original_changed()
    }
    pub fn pending(&self) -> bool {
        self.recovery_error.is_some()
            || self
                .active
                .as_ref()
                .is_some_and(|active| active.error.is_some())
    }
    pub fn dirty(&self) -> bool {
        self.session.dirty() || self.pending()
    }
    fn ready(&self) -> io::Result<()> {
        if let Some(error) = &self.recovery_error {
            return Err(invalid(error));
        }
        Ok(())
    }
    pub fn activate(&mut self, source: Range<usize>) -> io::Result<()> {
        self.ready()?;
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.error.is_some())
        {
            return Err(invalid(
                "Repair pending paragraph input before changing paragraphs",
            ));
        }
        let projection = self.projection_at(&source)?;
        self.active = Some(Active {
            source,
            input: projection.text.encode_utf16().collect(),
            initial_source: projection.source.clone(),
            projection,
            selection: 0..0,
            error: None,
            epoch: self.session.edit_version(),
        });
        Ok(())
    }
    fn projection_at(&self, source: &Range<usize>) -> io::Result<Projection> {
        let text = self
            .text()
            .get(source.clone())
            .ok_or_else(|| invalid("Stale paragraph coordinates"))?;
        let document = Document::parse(self.text());
        let first = document.blocks.iter().find(|block| {
            block
                .paragraph_source
                .as_ref()
                .is_some_and(|range| range.start == source.start && range.end <= source.end)
        });
        let Some(block) = first else {
            let before = &self.text()[..source.start];
            if source.is_empty()
                && (before
                    .trim_matches(['\u{feff}', '\r', '\n', ' ', '\t'])
                    .is_empty()
                    || (before.ends_with('\n')
                        && (source.end == self.text().len()
                            || self.text()[source.end..].starts_with(['\r', '\n']))
                        && !document.blocks.iter().any(|b| {
                            b.fence.as_ref().is_some_and(|f| {
                                f.source.start < source.start && source.start < f.source.end
                            })
                        })))
            {
                return Projection::parse("").map_err(invalid);
            }
            return Err(invalid("This is not an editable text block"));
        };
        let prefix = if block.kind == Kind::List {
            self.continuation_for(block.source_start, source.start)
        } else {
            String::new()
        };
        let projection = Projection::parse_with_prefix(text, &prefix).map_err(invalid)?;
        if block.paragraph_source.as_ref() == Some(source)
            && block
                .text
                .get(block.edit_prefix.len()..)
                .map(|value| value.replace('\n', "\r"))
                != Some(projection.text.clone())
        {
            return Err(invalid("Source text and displayed block do not agree"));
        }
        Ok(projection)
    }
    /// Store latest native input first, even if no safe Markdown projection exists.
    /// Return whether the last valid working Markdown changed.
    pub fn sync(
        &mut self,
        input: Vec<u16>,
        selection: Range<usize>,
        composing: bool,
    ) -> io::Result<bool> {
        self.sync_inner(input, selection, composing, None)
    }
    pub fn sync_styled(
        &mut self,
        input: Vec<u16>,
        selection: Range<usize>,
        composing: bool,
        styles: &[crate::inline_projection::Styled],
    ) -> io::Result<bool> {
        self.sync_inner(input, selection, composing, Some(styles))
    }
    pub fn retain_input(
        &mut self,
        input: Vec<u16>,
        selection: Range<usize>,
        error: String,
    ) -> io::Result<bool> {
        if input.len() > MAX_PARAGRAPH_UNITS
            || input.contains(&0)
            || selection.start > selection.end
            || selection.end > input.len()
        {
            return Err(invalid("Invalid pending input"));
        }
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| invalid("No active text block"))?;
        active.input = input;
        active.selection = selection;
        active.error = Some(error);
        Ok(false)
    }
    fn sync_inner(
        &mut self,
        input: Vec<u16>,
        selection: Range<usize>,
        composing: bool,
        styles: Option<&[crate::inline_projection::Styled]>,
    ) -> io::Result<bool> {
        self.ready()?;
        let prefix = self.continuation_prefix();
        let newline = self.newline();
        if input.len() > MAX_PARAGRAPH_UNITS
            || input.contains(&0)
            || selection.start > selection.end
            || selection.end > input.len()
        {
            return Err(invalid("Invalid or oversized native paragraph input"));
        }
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| invalid("No active paragraph"))?;
        active.input = input;
        active.selection = selection;
        if composing {
            active.error = Some("Native composition pending; input retained".into());
            return Ok(false);
        }
        if active.epoch != self.session.edit_version()
            || self.session.text().get(active.source.clone())
                != Some(active.projection.source.as_str())
        {
            active.error = Some("Stale source coordinates; input retained".into());
            return Ok(false);
        }
        let edit = String::from_utf16(&active.input)
            .map_err(|_| "Incomplete UTF-16 input retained".to_owned())
            .and_then(|text| {
                if let Some(styles) = styles {
                    let projected = Projection::from_native_in(&text, styles, newline, &prefix)
                        .map_err(str::to_owned)?;
                    if projected.source == active.projection.source {
                        Ok(None)
                    } else {
                        Ok(Some(crate::inline_projection::SourceEdit {
                            range: 0..active.projection.source.len(),
                            replacement: projected.source.clone(),
                            projected,
                        }))
                    }
                } else {
                    active.projection.edit(&text).map_err(str::to_owned)
                }
            });
        match edit {
            Ok(Some(edit)) => {
                let old_length = active.projection.source.len();
                if let Err(error) = self.session.replace_range(
                    active.source.start + edit.range.start..active.source.start + edit.range.end,
                    &edit.replacement,
                ) {
                    active.error = Some(format!(
                        "Working source rejected the edit; input retained: {error}"
                    ));
                    return Ok(false);
                }
                active.source.end = active.source.end - old_length + edit.projected.source.len();
                active.projection = edit.projected;
                active.epoch = self.session.edit_version();
                active.error = None;
                Ok(true)
            }
            Ok(None) => {
                active.error = None;
                Ok(false)
            }
            Err(error) => {
                active.error = Some(error);
                Ok(false)
            }
        }
    }
    pub fn deactivate(&mut self) -> io::Result<()> {
        self.ready()?;
        if self.pending() {
            return Err(invalid(
                "Pending native input is retained; repair it before changing paragraphs",
            ));
        }
        let active = self.active.take();
        if active.is_none() {
            return Ok(());
        }
        if let Err(error) = self.checkpoint() {
            self.active = active;
            return Err(error);
        }
        if let Some(active) = active {
            self.record(Change {
                start: active.source.start,
                before: active.initial_source,
                after: active.projection.source,
            });
        }
        Ok(())
    }
    pub fn newline(&self) -> &'static str {
        if self.text().contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        }
    }
    fn continuation_for(&self, start: usize, body: usize) -> String {
        let mut prefix = self.text().get(start..body).unwrap_or("").to_owned();
        if let Some(index) = prefix.find("[x]").or_else(|| prefix.find("[X]")) {
            prefix.replace_range(index + 1..index + 2, " ");
        }
        prefix
    }
    pub fn continuation_prefix(&self) -> String {
        let Some(active) = &self.active else {
            return String::new();
        };
        Document::parse(self.text())
            .blocks
            .iter()
            .find(|b| {
                b.kind == Kind::List
                    && b.paragraph_source
                        .as_ref()
                        .is_some_and(|r| r.start == active.source.start)
            })
            .map(|b| self.continuation_for(b.source_start, active.source.start))
            .unwrap_or_default()
    }
    fn record(&mut self, change: Change) {
        if change.before == change.after {
            return;
        }
        self.redo.clear();
        self.undo.push(change);
        // shortcut: bounded block deltas, not persisted undo; deeper history is post-v1.
        while self.undo.len() > 64
            || self
                .undo
                .iter()
                .map(|c| c.before.len() + c.after.len())
                .sum::<usize>()
                > 4 * 1024 * 1024
        {
            self.undo.remove(0);
        }
    }
    pub fn apply_edit(&mut self, range: Range<usize>, replacement: &str) -> io::Result<()> {
        self.deactivate()?;
        let before = self
            .text()
            .get(range.clone())
            .ok_or_else(|| invalid("Invalid source edit"))?
            .to_owned();
        self.session.replace_range(range.clone(), replacement)?;
        self.record(Change {
            start: range.start,
            before,
            after: replacement.into(),
        });
        Ok(())
    }
    pub fn undo(&mut self, redo: bool) -> io::Result<bool> {
        self.deactivate()?;
        let change = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        let Some(change) = change else {
            return Ok(false);
        };
        let (old, new) = if redo {
            (&change.before, &change.after)
        } else {
            (&change.after, &change.before)
        };
        let result =
            if self.text().get(change.start..change.start + old.len()) == Some(old.as_str()) {
                self.session
                    .replace_range(change.start..change.start + old.len(), new)
            } else {
                Err(invalid("History coordinates do not match"))
            };
        if let Err(error) = result {
            if redo {
                self.redo.push(change)
            } else {
                self.undo.push(change)
            }
            return Err(error);
        }
        if redo {
            self.undo.push(change)
        } else {
            self.redo.push(change)
        }
        Ok(true)
    }
    pub fn checkpoint(&mut self) -> io::Result<()> {
        self.ready()?;
        self.session.checkpoint()?;
        self.write_input()
    }
    pub fn save_original(&mut self) -> io::Result<SaveOutcome> {
        self.ready()?;
        if self.pending() {
            return Err(invalid(
                "Cannot Save unprojected input; repair or discard it first",
            ));
        }
        self.checkpoint()?;
        let mut outcome = self.session.save_original()?;
        if let Err(error) = self.write_input()
            && outcome.recovery_error.is_none()
        {
            outcome.recovery_error = Some(error);
        }
        Ok(outcome)
    }
    fn write_input(&self) -> io::Result<()> {
        let input = Input {
            version: 1,
            original: self.session.original_path().to_owned(),
            source_bytes: self.text().len(),
            source_stamp: stamp(self.text()),
            paragraph: self.active.as_ref().map(|active| ParagraphInput {
                range: active.source.clone(),
                source: active.projection.source.clone(),
                utf16: active.input.clone(),
                selection: active.selection.clone(),
            }),
        };
        let bytes = serde_json::to_vec(&input)?;
        if bytes.len() as u64 > MAX_INPUT_BYTES {
            return Err(invalid("Input recovery metadata exceeds its bound"));
        }
        let temporary = self.directory().join("inline-input.next");
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, self.directory().join("inline-input.json"))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
    fn restore_input(&mut self) -> io::Result<()> {
        let path = self.directory().join("inline-input.json");
        if !plain(&path)?.is_file() {
            return Err(invalid("Expected regular pending-input metadata"));
        }
        let mut bytes = Vec::new();
        fs::File::open(&path)?
            .take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_INPUT_BYTES {
            return Err(invalid("Oversized pending-input metadata"));
        }
        let input: Input =
            serde_json::from_slice(&bytes).map_err(|error| invalid(&error.to_string()))?;
        if input.version != 1
            || input.original != self.session.original_path()
            || input.source_bytes != self.text().len()
            || input.source_stamp != stamp(self.text())
        {
            return Err(invalid("Working Markdown and pending input do not match"));
        }
        if let Some(input) = input.paragraph {
            let projection = self.projection_at(&input.range)?;
            if input.source != projection.source
                || input.utf16.len() > MAX_PARAGRAPH_UNITS
                || input.utf16.contains(&0)
                || input.selection.start > input.selection.end
                || input.selection.end > input.utf16.len()
            {
                return Err(invalid("Invalid recovered paragraph input"));
            }
            let decoded = String::from_utf16(&input.utf16);
            if let Ok(text) = &decoded
                && (crate::inline_projection::utf16_to_byte(text, input.selection.start).is_none()
                    || crate::inline_projection::utf16_to_byte(text, input.selection.end).is_none())
            {
                return Err(invalid("Recovered selection cuts a surrogate pair"));
            }
            let projected = decoded.is_ok_and(|text| text == projection.text);
            self.active = Some(Active {
                source: input.range,
                initial_source: projection.source.clone(),
                projection,
                input: input.utf16,
                selection: input.selection,
                error: (!projected)
                    .then(|| "Recovered unprojected input; repair before Save".into()),
                epoch: self.session.edit_version(),
            });
        }
        Ok(())
    }
}
