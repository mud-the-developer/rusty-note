# Rusty Note 0.3 validation

This is the original integrated-editor baseline. Current read-only reader checks,
including table alignment and column-aware native link hit testing, are recorded in
[sticker validation](sticker-validation.md).

## Actual checks

- Cargo fmt, Clippy with warnings denied, fifteen passing core regressions and MSVC Release build.
- Active changed-path Rust diagnostics: ten modules checked without errors before final packaging.
- Built native app and mixed `complete.md` + local PNG at 600x760/360x640 in both themes.
- Actual owned-HWND captures inspected: rich styles, Korean/emoji, heading wraps,
  ordered/nested/task lists, quotes, table grid/cell wraps, code indentation/blank lines,
  local PNG, remote-image placeholder, final heading/paragraph and theme-matched editor.
- Cached font measurement unchanged on resize; finished animations stopped.
- Native edit/Save/backup/read/New; async-driven native rename prompt preserved file content.
- New disk file appeared through native directory notification, without a polling timer.
- Hide/restore via owned tray callback, clean close and isolated state output.
- No idle paint growth; latest short local sample: dark WS44.41/private57.51MiB,
  light WS44.43/private57.07MiB, reported CPU0ms/2500ms interval.

Only new GUID-per-theme fixture copies under `target/validation` are edited. Source
fixtures and user notes/global state are not touched. Captures include owned HWNDs
only, never desktop/other app pixels. Generated screenshots, metrics, state, logs
and exports remain ignored. `tests/fixtures/fixture.png` is a permanent small
runnable image asset, not a screenshot/export.

Core regressions cover BOM/mixed/styled nested structures, real table cells and
surrounding content, cached resize/motion, source backup/conflict/CRLF, NUL/oversize
rejection and held save-lock refusal, filename/device traversal protection,
no-overwrite rename/trash integrity and URI scheme restrictions.

## Fixes found during native validation

- BOM headings and PowerShell text encoding; copy fixture bytes and parse BOM safely.
- Narrow header/library overlap; decorative badge hides when space is short.
- WM_MOUSELEAVE must be explicit, never a Rust wildcard match.
- Repaint only when hover control changes.
- Cross-process EDIT tests use WM_SETTEXT (SetWindowText caption behavior is insufficient).
- CRLF display/save normalization prevents false dirty state and preserves disk newlines.
- Directory/message wait must not dereference app data after WM_NCDESTROY.
- Fresh copied notes prevent prior New/save tests from polluting the next theme/profile.
- Styled UTF-8 slicing is skipped when normalized line text cannot map to source.
- Failed tray registration cannot hide the only visible window.
- Removing the last note resets the read canvas instead of retaining deleted contents.

## Boundaries / unverified areas

Tray callback and real registration are exercised; Explorer restart re-registration
is not implemented. Folder picker and note-trash native confirmation need broader
interactive coverage (core trash preservation tested). Actual link activation is
not automated because it would launch an external browser/mail app; native ranges
and URI guards are covered. Multiple instances, exotic filesystems, image/device-loss,
large files, live monitor DPI and full GPU/system memory were not comprehensively profiled.

Tiny-width many-column table alignment/horizontal scrolling, SVG, diagrams/math,
syntax highlighting, rich-text edit, installer/updater/signing are not claimed.
Raw HTML is literal; remote image fetching is intentionally disabled. Source Save
checks remain optimistic against external writers, not transactional locking.

## Distribution

Public https://github.com/mud-the-developer/rusty-note. The v1 release workflow
requires a version tag matching `Cargo.toml`, then packages an unsigned Windows x64
portable zip and `SHA256SUMS.txt` after fmt, locked tests, warnings-denied Clippy and
a locked Release build. Staging/output stays under ignored `out/`; the zip contains
`rusty-note.exe`, `README.md`, `LICENSE` and reviewed `THIRD_PARTY_NOTICES.txt`.
`v1.0.0` and other non-v0, non-hyphenated version tags publish as stable/latest;
`v0.*` and hyphenated pre-release tags remain pre-releases.

This historical baseline and CI publication do not verify a downloaded v1 artifact.
The release owner must checksum-verify downloaded artifacts and run them through
the native harness before reporting deployment verified. Further stress testing
and optimization are deferred at the user's choice; large-document and mixed-DPI
profiling remain incomplete, as detailed in the [repeat smoke record](stress-validation.md).
