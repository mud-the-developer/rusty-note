# Rusty Note

**Rusty Note v1.0.1** is a small, native **editable Markdown sticky note** for
Windows 10/11 x64. Click rendered text to edit it in place, keep a note above other
windows, check tasks and enlarge local images. No browser, WebView, Electron,
JavaScript runtime or GPUI framework.

## Download

[Download the Windows portable release](https://github.com/mud-the-developer/rusty-note/releases/latest).
Compare the zip's SHA256 with `SHA256SUMS.txt`, extract it and run `rusty-note.exe`.
The executable is **unsigned**; SmartScreen may warn. Checksums verify integrity,
not publisher identity. No installer, autostart change or automatic updater.
The zip includes the executable, this README, `LICENSE` and `THIRD_PARTY_NOTICES.txt`.

## Use a sticky note

```powershell
.\rusty-note.exe "C:\path\note.md"
```

Or run without arguments: **Open / Ctrl+O**, drop a `.md`, or **+ / Ctrl+N** for a
new note. New notes are created beside the open note; with no open note the default
is `%USERPROFILE%\Documents\Rusty Note`. Existing filenames are never overwritten.
Drag the header to move the note and resize its edges. **P** pins it above other
windows; **T / ◐** switches warm-paper/charcoal themes.

### Edit where you read

- Click ordinary paragraphs, ATX headings or simple list/task text to edit there.
  Surrounding blocks stay rendered; input updates working Markdown immediately.
- Select text and use **Ctrl+B / Ctrl+I** for bold/italic. Plain punctuation,
  cross-style replacement, empty blocks and Enter/new paragraphs are supported.
  Edited text blocks get canonical Markdown spelling; unedited document bytes,
  structural prefixes and surrounding line endings are kept.
- **Esc** finishes the current block. **Ctrl+Z / Ctrl+Y** use native active-block
  undo/redo, then bounded block/task history. Undo history is not kept across restart.
- Click a task box to toggle its Markdown check state. Task changes share the same
  working draft, undo and explicit Save flow; clicking does not autosave the original.
- Click a local image for an owned native viewer. Wheel zooms, drag/arrows pan,
  Home/Space fits, and Esc closes only the image window.
- **Ctrl+S / Save** explicitly commits the original. Leaving dirty work offers
  Save / Discard / Cancel. Input that cannot safely become Markdown is retained
  and original Save is blocked rather than writing stale source.

This is a **basic block editor**, not a complete rich-text/WYSIWYG Markdown suite.
Inline links/code/strike, nested/complex structures, tables, code blocks and diagrams
may need the opt-in source editor. Such content remains rendered/preserved rather
than being silently flattened. Existing nested styles render; unsupported editing
is clearly refused. This is not Tinta or Obsidian parity.

### Read comfortably

**Ctrl++ / Ctrl+- / Ctrl+0**, Ctrl+wheel and the menu set 80–160% document text size.
**Ctrl+F**, F3 / Shift+F3 and Esc find, navigate and clear matching-line marks.
Search is literal, ASCII-case-insensitive, bounded to 512 query bytes / 4096 hits;
no regex, image-alt or cross-block search. Resize reuses measured font advances.

External original changes refresh a clean view but never silently replace a dirty
draft. **F5** reloads clean content; dirty work must first be saved/discarded.
Click the colored status for complete error details.

## Working copies and source safety

Editing changes memory and an app-local recovery draft, **not the original per key**.
A bounded 500 ms edit-triggered checkpoint keeps completed drafts recoverable after
restart. Explicit Save keeps `.md.bak`, syncs a temporary replacement and checks
for detected external changes under a cooperating `.md.rusty-lock`.
Non-cooperating writers remain an optimistic race, not a filesystem transaction.
A stale lock needs manual review; do not delete it blindly.

Default editable state: `%LOCALAPPDATA%\RustyNote\inline-state.json`.
Its `inline-drafts/` sibling stores plaintext snapshots, working files and exact
pending input. **Discard clears the active association, not these recovery files**;
they remain for manual review. The last unfinished checkpoint interval can still
be lost on a crash. Source/input companion publication is not one atomic transaction;
inconsistent recovery is retained and visibly blocked, not guessed or overwritten.
Power-loss, exotic filesystems and multi-instance coordination are not guaranteed.

## Reader and advanced source modes

```powershell
.\rusty-note.exe --read-only "C:\path\note.md"
.\rusty-note.exe --editor --notes-dir "C:\path\notes"
```

`--read-only` keeps the former non-mutating reader and `sticker-state.json`.
`--editor` keeps the native full-source editor/library and `state.json`: Ctrl+E
edits, Ctrl+S saves, Ctrl+Enter returns to reading, Ctrl+N creates, F2 renames.
That legacy mode has explicit Save and backups, not the inline draft checkpoint UI.
Choose only one mode. `--inline-edit` remains an alias for the default editable mode.
`--state-file PATH`, `--notes-dir PATH`, `--light` and `--reduced-motion` are also available.

## Rendering and limits

The shared native renderer supports headings, bold/italic/strike/code, guarded
web/mail links, lists/tasks, quotes, rules, aligned wrapping tables and bounded local
images. Fenced Mermaid supports a bounded `graph`/`flowchart` TD/TB/LR subset with
real DirectWrite labels. Unsupported/unfit Mermaid remains searchable code; no
browser, diagram editor or script execution is introduced.
[Exact Mermaid limits](docs/mermaid-validation.md).

- UTF-8 notes without NUL, maximum 2 MiB; an active inline block is bounded to
  8,192 UTF-16 units. Unsupported/over-limit native input is not silently saved.
- Local image paths must remain under the note directory; maximum 8 MiB / 4 million
  pixels each, bounded caches. No remote images, SVG, math or syntax highlighting.
- Tables use equal-width columns; very narrow cells can clip. No horizontal table
  scrolling. Complex Markdown editing uses the source mode, not an HTML round-trip.
- Physical Korean IME/TSF, clipboard interoperability, complete accessibility,
  long-session/multi-instance behavior and mixed-DPI transitions need more testing.
- Large-document/repeated-load stress and broad optimization are intentionally
  deferred until after v1. Short local idle samples are not budget compliance.

[`gpui-pretext` 0.6.2](https://docs.rs/gpui-pretext/0.6.2/gpui_pretext/) is a pinned,
third-party Rust port of [chenglou/pretext](https://github.com/chenglou/pretext), not
an official Rust edition. Production document measurement uses actual DirectWrite
layouts. RichEdit owns native input/layout; exact renderer/fallback/DPI parity is
not claimed. The icon and light/dark copper palette are embedded/shared.

## Build and essential validation

Rust 1.99+, Windows MSVC tools and SDK:

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/validate-v1.ps1
```

The short v1 smoke uses isolated `target/` note copies, normal/narrow light/dark
owned-window captures, edit/newline/task/undo/recovery/Save/image/new-note checks
and brief real idle CPU/memory/paint observations. It never validates against user
notes. Generated output remains in `target/` or `out/`.
[Release validation](docs/v1-validation.md) · [Post-v1 plan](docs/v1-plan.md).

Version tags rerun Cargo checks/build and publish the portable zip + SHA256SUMS.
Normal non-v0 tags are stable; v0 and hyphenated tags are pre-releases. Downloaded
assets are checksum-checked separately; publication alone is not a runtime test.

[Changelog](CHANGELOG.md) · [Architecture](docs/architecture.md) ·
[Third-party notices](THIRD_PARTY_NOTICES.txt) · MIT license.
