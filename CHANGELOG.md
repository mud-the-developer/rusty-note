# Changelog

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
