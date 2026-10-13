# Changelog

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
