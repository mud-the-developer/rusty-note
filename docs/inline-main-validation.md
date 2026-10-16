# Historical M2 inline-editing verification

> This records the earlier, narrower pre-v1 slice. It is not the current v1 feature
> contract or release gate. See [README](../README.md) and [v1 verification](v1-validation.md).

Status: first main-window paragraph slice implemented and directly checked; **not
completed v1**. The default reader and `--editor` source editor are unchanged.
`--inline-edit` is a temporary, explicit development opt-in and cannot be combined
with `--editor`. No WebView, JavaScript runtime or new dependency was added.

## Try an isolated note

Build with the existing Windows MSVC/SDK toolchain, then copy the runnable fixture
into a new ignored directory rather than editing a source fixture:

```powershell
cargo build --locked --release
$demo = Join-Path 'target' ('inline-demo-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $demo | Out-Null
Copy-Item tests/fixtures/inline-sticker.md "$demo/note.md"
Copy-Item tests/fixtures/fixture.png "$demo/fixture.png"
.\target\release\rusty-note.exe --inline-edit "$demo/note.md" --state-file "$demo/state.json"
```

Click a supported paragraph in the actual sticky window. It becomes a borderless
native `RICHEDIT50W` paragraph at that location; the surrounding content stays in
the Direct2D/DirectWrite/Pretext scene. No raw-source/preview split is used.
Native text changes update working Markdown and the scene without pressing Save.
Style decoration does not reload the control or enter its native undo history.
Esc ends a supported paragraph edit and checkpoints it; Ctrl+S or the menu commits
the original with the existing backup/lock/external-change checks.

The normal source editor remains available through `--editor`, not simultaneously
with this mode. Unsupported paragraph clicks do not open embedded links in inline
mode; the ordinary reader's guarded link activation is unchanged.

## Deliberately narrow editing scope

- Supported: existing nonempty, single-source-line, top-level paragraphs with
  plain text and flat `**bold**` / `*italic*` runs, up to 8,192 UTF-16 source units.
  Visual wrapping, Unicode and emoji are supported. Parsed source coordinates and
  scene origins identify paragraphs, including identical repeated paragraphs.
- Edits must remain representable by the conservative projection. No cross-style
  selection replacement, whole-emphasis deletion, ambiguous repeated-text seams,
  nested styles, new formatting commands, escapes, entities, links, inline code,
  heading/list/table/code/diagram editing, empty-paragraph creation or block splits.
- Unsupported/ambiguous native input stays visible, unstyled, and explicitly
  marked pending. The last valid working Markdown remains intact. Original Save
  is blocked rather than committing stale source as though it contained that input.
  Repair or native undo can return to a supported projection.
- Native undo/redo belongs to the active paragraph control. Ending/switching that
  control or restarting does not preserve a whole-note undo history. Clipboard
  interoperability, physical keyboard/pointer sessions and physical Korean IME/TSF
  remain unverified. Unicode message injection is not an IME test. Composition
  notifications defer source decoration and original Save while retaining input.
- RichEdit uses native Windows layout. Its size/theme are calibrated to the
  document fonts, but exact DirectWrite fallback/wrapping/DPI parity is not claimed.

These are development limits, not the final editable-sticky product contract.
Broader editing, default editing, new notes, task toggles and image enlargement
remain tracked in [the v1 plan](v1-plan.md).

## Working state and recovery

Without an explicit state override, the experiment uses
`%LOCALAPPDATA%\RustyNote\inline-state.json`, separate from the reader/source-editor
state files. Its `inline-drafts/` sibling holds M1 snapshots/checkpoints and bounded
`inline-input.json` companion data (maximum 192 KiB). Tests override the state path
under `target/` and never use global app state or user notes.

- Input changes start one 500 ms checkpoint timer, not an unconditional render or
  polling loop. The timer is killed before work, including on failure. A completed
  checkpoint can recover both valid Markdown and exact pending UTF-16 input.
- M1 and companion publication are **not a single atomic transaction**. Source
  length/checksum, note identity, paragraph span and scalar-boundary checks reject
  mismatches. Valid M1 Markdown can still be shown, but editing/Save are blocked
  and unknown companion data is retained for manual review. Checksums detect
  accidental corruption; recovery files are not an authenticated import format.
- Dirty notes survive F5, unrelated folder events and external original edits.
  Save refuses a detected external change. Switching/closing supported dirty edits
  offers Save / Discard / Cancel. Cancelling restores native input focus. Pending
  input may close keeping a completed checkpoint, or be explicitly discarded.
- **Discard clears the active association, not the files.** Plaintext snapshots,
  working copies and input companions remain for manual review. Automatic cleanup,
  multi-note recovery browsing and multi-instance state coordination are not done.
- Another note's retained association is never silently overwritten: reopening that
  note or explicitly discarding its association is required before creating a new
  session. Paths outside the current recovery root are rejected.
- Malformed/oversized state or unknown `*.json.tmp` files are not overwritten.
  A malformed state visibly blocks inline editing. Correct the retained data or
  explicitly choose a new state path; the program does not guess or delete it.
- The last uncompleted checkpoint interval can still be lost on process loss.
  Power-loss, hostile/exotic filesystems and non-cooperating external writers are
  not transactional guarantees. No large-document/mixed-DPI stress or optimization
  claim is made.

## Direct verification

Current checks passed in `target/v1-m2-main/final/`:

- `cargo fmt --check`, `cargo test --locked`: **84 tests** (50 unit, 19 M1 draft,
  2 icon, 2 projection/session, 11 inline-note/layout integration).
- `cargo clippy --locked --all-targets -- -D warnings`, Release build.
- Main-window harness: `scripts/validate-inline-sticker.ps1`, both themes at
  600x760 and 360x500, plus 140% zoom and a malformed-state case. It uses actual
  paragraph clicks and native Unicode/undo messages, not an alternate edit path.
  Tested duplicate targeting, BOM/CRLF exact splices, working-only edits, native
  styles/caret selection, undo/redo, checkpoint/forced owned-process stop/reopen,
  pending-input Save refusal/reopen/discard, explicit original Save/backup,
  external-change refusal, cancelled file drop and a reentrant owner-close while
  its modal dialog holds application state.
- Existing reader/find/alignment, Mermaid, native source-editor Save/backup/New/
  rename/theme, and standalone RichEdit prototype harnesses all passed.
- Original fixtures and the user's `icon.png` SHA256 values were unchanged.

Final main-window artifacts:
`target/v1-m2-main/validation-9c35e12dc72d46f8ac52874a786680e6/`.
Inspected both-theme `normal-edited.png`, `narrow-zoom.png`,
`pending-recovered.png` and `external-conflict.png`, including native glyph/font
and color assertions. Window dimensions are physical bounds; client bounds differ
by the native frame. These are not mixed-DPI transition tests.

The capture method never reads the desktop. D2D parent `PrintWindow` alone does not
reliably include RichEdit. Therefore `*-parent.png` retains the actual parent HWND
capture; a diagnostic receiver-local `WM_PRINTCLIENT` renders the actual owned
RichEdit client to a bounded DIB (`*-native.png`). `*.png` combines those owned
surfaces at their actual HWND screen-coordinate delta, clipped to the actual
native window region. No text/layout is synthesized for captures. The harness
requires real visible ink, not only a successful text/model assertion. Caret
compositor drawing is not completely represented by these snapshots/counters.

Literal 3-second main-window idle observations after the cancel/refocus path:

| Theme | CPU increase | Parent/native WM_PAINT increase | Working set | Private bytes |
| --- | ---: | ---: | ---: | ---: |
| Dark | 0 ms | 0 / 0 | 54,755,328 bytes | 60,985,344 bytes |
| Light | 0 ms | 0 / 0 | 54,472,704 bytes | 61,583,360 bytes |

Both had logical native focus but were not foreground during these samples.
They do not prove foreground-caret budgets, GPU/system memory use or general
performance compliance. The separate standalone prototype has its own recorded
foreground-caret observations, not substituted for main-window measurements.

Earlier failed runs remain under `target/v1-m2-main/`. Review caught and fixed
parent invalidation on RichEdit paint notifications, focus/visibility after zoom,
reentrant modal owner close, and lost native input focus after Cancel. Nested GDI
repaint/cross-process capture attempts also exposed unreliable snapshots; the final
child repaint is queued/coalesced outside parent painting, and capture uses the
receiver-local native client route described above. Earlier failures are not
counted as passes.

Other fresh artifacts: reader `target/sticker-validation/4888637629b84c08a84fdcf797aa7cb8/`,
Mermaid `target/mermaid-validation/153eceea2c7f4a5a9dbe7fb36b05d5f9/`, source editor
`target/validation/`, and standalone prototype
`target/v1-m2/validation-419f7551b7674be79df8d510c7f7d789/`.

No commit, tag, push or release was made for this slice. Full v1 remains pending.
