# Rusty Note 0.2 preview validation

## Actually checked

- Cargo format check, tests, Clippy with warnings denied and x64 Release build.
- Unit tests cover mixed Markdown, BOM headings, cached Pretext resize, stopping
  motion/reduced motion, malformed state, backup/conflict saves, no-overwrite new
  files and preserving CRLF without marking an unchanged editor dirty.
- Real native GUI `scripts/validate.ps1`: dark/light, 600x760 and 360x640, scrolling,
  library and note changes, measurements unchanged on resize, no idle paint growth,
  native editor text replacement/Save/backup/read mode and New note creation.
- Active changed-path Rust diagnostics reported no errors before final packaging.
- CI baseline passed publicly; the release workflow checks/tests/builds again.

Only fixture copies under `target/validation/notes` are edited. Original fixtures
and user notes are not touched by the harness. Captures target owned HWNDs only,
never the desktop. Native source-editor captures were inspected in both themes.

A local small-fixture sample after editor integration: ~42 MiB working set,
~57–58 MiB private bytes, no reported CPU-time increase over 2.5 s idle and no
additional paints. This is short Windows accounting, not zero-CPU/memory guarantees,
GPU/full-system accounting, long-document profiling or a Tinta comparison.

## Bugs caught / fixes

- UTF-8 BOM could turn a heading into plain text; parser strips it and tests cover it.
- PowerShell literal fixture encoding damaged Korean; copy UTF-8 fixture files as bytes.
- Narrow library badge/title overlap; hide the decorative badge when space is short.
- Undefined WM_MOUSELEAVE could be a Rust match wildcard; explicit message constant.
- Win32 unsafe operations now have explicit scopes; no file-wide unsafe-op exemption.
- Mouse movement caused redundant repaints; repaint only when the hover region changes.
- Cross-process EDIT tests must use WM_SETTEXT, not SetWindowText caption semantics.
- Windows editor input needs CRLF. Normalize the displayed buffer and preserve the
  original file's CRLF when saving; untouched CRLF files are not falsely dirty.

## Source safety

Save is explicit. Native selection/undo/clipboard/IME come from Windows EDIT.
Save compares the disk text with the original, blocks detected external changes,
writes a sibling `.md.bak`, syncs a new temporary file and replaces the note.
Failing saves keep the editor/buffer. New files use create_new and cannot overwrite
an existing Note-NNN.md. Closing/leaving dirty editing asks Save/Discard/Cancel.

The conflict check is optimistic, not a filesystem transaction: writes between
comparison and replacement remain a race. Multiple editors/instances and exotic
filesystems still need stress testing. Backups are one previous version, not a
history database. No automatic source save or deletion was added.

## Distribution

Public repository: https://github.com/mud-the-developer/rusty-note
Version-tag workflow builds portable Windows x64 zip + SHA256SUMS and a pre-release.
No installer/association/autostart change or signing certificate is used. The
binary is unsigned and SmartScreen may warn. Release notes must state preview status.

## Still incomplete

Rich inline styling/clickable links, proper grid tables, images, tray, folder
watching, system reduced-motion detection, live DPI migration and full Tinta parity.
The source editor is a native plain-text Markdown editor. Native Direct2D animations
are bounded redraws, not retained DirectComposition surfaces yet.

Artifacts: ignored `target/validation/*-{wide,narrow,bottom,library,editor}.png`,
profile/metrics JSON, state and fixture copies. No user-note contents are published.
