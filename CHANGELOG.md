# Changelog

## 0.3.6 — aligned tables and precise table links

- Apply existing Markdown left/center/right column metadata to headers and wrapped cells.
- DirectWrite aligns actual styled glyphs inside cell bounds; resize stays font-cached.
- Position search line marks from the drawn layout's metrics, including aligned numbers.
- Fix link hit testing in later table columns: check both cell coordinates, not only
  the first text line at a given vertical position. Keep the safe URI dispatch guard.
- Mixed alignment fixture, cached reflow/zoom/search regression and native both-theme
  glyph-position/link-hit/source-byte/idle checks at normal and narrow widths.

## 0.3.5 — read-only document find

- Ctrl+F native find prompt, F3/Shift+F3 result navigation and Escape to clear.
- Count/status and selected matching-line marks across paragraphs, table cells and code.
- Match semantic text rather than raw Markdown syntax; phrases survive visual wrapping.
- Preserve/update results across resize, text-size changes and external refresh.
- Bounded literal matching with ASCII-insensitive case and unchanged source bytes;
  search performs no font preparation or app-state query persistence.
- Mixed reader-find fixture, wrapping/Unicode/limit regressions and native both-theme
  prompt/navigation/no-result/reflow/refresh checks.

## 0.3.4 — text size and recoverable read errors

- Bounded 80–160% reading text size with Ctrl++/Ctrl+-/Ctrl+0, Ctrl+wheel and menu controls.
- Measure the actual scaled DirectWrite fonts; window resize remains cached.
- Persist text size in app state; source Markdown remains untouched.
- Show a distinct clickable error status and a complete native error dialog with the path.
- Keep the last read view while a file is unavailable; F5 clears the resolved error.
- Mixed reader-comfort fixture, scaled-font/layout/state regressions and native
  normal/narrow light/dark zoom, popup/recovery, source-byte and idle checks.

## 0.3.3 — reader controls and resume

- Clear Open label and per-control hover help without a tooltip polling timer.
- Wider draggable scrollbar with track clicking and pointer capture.
- P/T keyboard pin/theme controls and up/down reading scroll.
- Left/right keys no longer reload the single read-only file or reset its position.
- Clamp reading offset after resize, avoiding a blank view beyond the document.
- Persist/restore reading position for the same file; different files start at the top.
- F5 clears resolved read errors without discarding the reading offset.
- Mixed reader-controls fixture, scrollbar/state regressions and native light/dark
  input, close/reopen, file-override, font-cache and source-byte checks.

## 0.3.2 — reference-informed viewer polish

- Used pinned REA to inspect two public reference app structures; documented source findings,
  license restrictions, static-analysis limits and independent adaptations.
- Native Unicode file-drop opening; reject multiple files without changing the selection.
- Preserve/clamp reading position across external refresh and F5.
- Ignore unchanged Markdown folder notifications without reparsing/measuring fonts;
  changed cached local images still invalidate.
- Fit off-screen/oversized saved geometry to the work area with initial monitor DPI;
  recover after display changes and clear device-bound image caches on DPI change.
- Fixed sent-message starvation of the directory-handle wait, using PeekMessage without polling.
- Regression checks for Unicode shell payloads, reload position and negative/small monitor geometry;
  native copied-fixture checks in normal/narrow light/dark windows.
- No reference runtime/code/assets copied; source Markdown remains read-only.

## 0.3.1 — simple Markdown sticker

- Default to a small read-only MD viewer, not a note-management/editor UI.
- Open an existing `.md` via positional path or ↑/Ctrl+O native file dialog.
- Keep only open/pin/theme/close toolbar controls; header drag and F5 reload.
- Separate remembered sticker file/placement state; initial size 360x500.
- Block edit/New/rename/trash/Save commands in viewer mode; source bytes stay untouched.
- Preserve prior editor only behind explicit `--editor`.
- Mixed viewer fixture, read-only core tests and native dark/light read-only/idle checks.

## 0.3.0 — usable native desktop notes

- Rich inline bold/italic/strike/code and safe DirectWrite-hit-tested web/mail links.
- Ordered/nested/task lists, rules, real wrapping table grids and code whitespace.
- Bounded native local image decoding/cache; remote images remain private placeholders.
- Native tray hide/restore/menu and event-driven folder notifications, without polling timers.
- Scrollable note rail, folder picker, remembered notes folder and Documents default.
- Native rename prompt and recoverable local trash with no-overwrite filename validation.
- Editor colors follow light/dark theme; PageUp/PageDown scrolling.
- Bounded UTF-8/NUL-safe reads, save lock and a second source check before replacement.
- Geometry/state-input bounds, missing/last-note empty view and failed-tray hide protection.
- Fifteen core regressions plus fresh per-theme owned-window mixed-document GUI checks.
- GUI checks exercise tables/styles/local PNG, editing/New/Save/backup/read, rename prompt,
  folder notifications and tray callback; package tested in normal/narrow light/dark.

Still Windows x64 and unsigned. Intentional bounds and unimplemented SVG, math/Mermaid,
syntax highlighting, rich-text editing, installer/updater, table alignment/overflow scrolling
and full Tinta parity are documented; no completion/compatibility guarantee is implied.

## 0.2.0 — Windows preview

- Public Rusty Note repository, Rust-native Direct2D/DirectWrite card UI.
- Unicode source editing with native Windows selection/undo/clipboard/IME.
- New-note creation without filename overwrites.
- Ctrl+E edit, Ctrl+S save, Ctrl+Enter reading preview, Ctrl+N new note.
- Pencil/checkmark and plus toolbar actions.
- Explicit save with a sibling `.md.bak` backup and synced temporary-file replacement.
- Block saves if the note changed externally; keep the unsaved buffer.
- Save/discard/cancel prompt before leaving an edited note or closing.
- Prepared Pretext measurements reused during resize; completed animations stop timers.
- Hover repaints only when the hovered control changes.
- Version-tag CI builds a portable Windows x64 zip and SHA256 checksums, publishing a pre-release.

Known limits: no native grid tables, rich-inline rendering, image loading, tray,
file watcher or full Tinta compatibility yet. Unsigned Windows binary; SmartScreen
may warn. This is an early preview, not a claim of finished product quality.

## 0.1.0 — rendering baseline

Initial cached Pretext block renderer, card surfaces, themes, note library,
selection/placement persistence and native idle validation.
