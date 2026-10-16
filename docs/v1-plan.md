# v1.0.0 scope and post-v1 plan

The operator requested a fast first v1 with **essential validation only**. Heavy
stress, broad optimization, mixed-DPI and soak work are explicitly deferred until
after v1. Do not describe that deferred work as a completed pass.

## v1 implemented

- [x] Native Rust/Win32/Direct2D/DirectWrite app, no browser/WebView/JS runtime.
- [x] Default rendered-text editing: paragraphs, ATX headings and simple list/task
  text; literal punctuation, bold/italic, cross-style replacement and empty text.
- [x] Enter/new paragraphs and explicit new sticky-note creation (+ / Ctrl+N).
- [x] Real native RichEdit input/selection/undo with immediate working-source/scene
  updates; edited text blocks have canonical CommonMark spelling.
- [x] Original snapshot, mutable Markdown checkpoints and bounded pending UTF-16
  recovery. Original writes remain explicit, with backup/lock/conflict checks.
- [x] Checkbox clicks share working state, checkpointing and bounded block/task
  undo/redo; no direct per-click original overwrite.
- [x] Owned native local-image fit/zoom/pan view and Esc close, using existing
  canonical-path/file/pixel limits.
- [x] Existing reader/source modes and bounded native Mermaid rendering retained.
- [x] Dependency notices collected, including the third-party Pretext port and
  upstream notices; unsigned portable Windows x64 packaging.

## Essential release gate

- [x] Final cargo fmt, locked tests (86), warnings-denied clippy and Release build.
- [x] Short copied-fixture native smoke in both themes and normal/narrow sizes:
  edit/style/newline/task/undo/recovery/Save/backup/image/new-note workflows.
- [x] Source-integrity and conflict/recovery boundary regressions retained.
- [x] Inspect final owned-window captures and record real brief CPU/memory/paints.
- [ ] Review intended diff, commit/tag/push v1.0.0, check CI and release metadata.
- [ ] Download the actual zip, verify SHA256/four packaged files, and run the same
  short smoke against the downloaded executable.

These checks are not a stress matrix or a claim of full rich-text compatibility.
The default editor handles the basic text blocks above, not arbitrary Markdown
objects. Complex tables, code/diagram structures and paragraphs containing unsupported
inline constructs remain preserved/readable and can use `--editor` source mode.

## Never cut for release speed

Original/unedited source bytes, BOM and surrounding line endings remain protected.
Edited block markup may be normalized. Conflicting original Save refuses detected
external changes. Invalid/unprojectable native input is retained and cannot be
silently saved as stale source. Unknown recovery/state data and temporaries are not
automatically overwritten or deleted. Existing source-editor behavior remains.

## Explicit post-v1 backlog

- Broader Markdown block/inline editing and formatting commands, nested structures,
  complex source spellings, editor/renderer layout parity and caret polish.
- Physical Korean IME/TSF, clipboard interoperability, screen readers/accessibility
  and real keyboard/pointer sessions beyond receiver-local native smoke.
- Mixed-DPI/multi-monitor transitions, multi-instance/state coordination, long
  documents, repeated resize/load/zoom and long-session memory/resource stress.
- Measure and optimize confirmed bottlenecks only; no invented budget compliance.
- Better recovery browsing, atomic source/input companion publication, verified
  cleanup of retained plaintext draft files, and persisted undo history if needed.
- Signing, installer/updater and richer Markdown/table overflow support are separate
  future work. No vault, graph, plugins, sync or drawing editor is part of v1.

Draft and original Save remain distinct. M1 and the input sidecar are not one atomic
transaction; mismatches fail visibly and retain files. Discard clears an active
association but retains plaintext recovery files. Non-cooperating writers and
power-loss/hostile filesystems are not transaction guarantees.

See [README](../README.md), [v1 verification](v1-validation.md), and the historical
[earlier inline slice](inline-main-validation.md) / [deferred stress record](stress-validation.md).
