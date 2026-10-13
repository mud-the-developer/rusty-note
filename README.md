# Rusty Note

Native Rust Markdown sticky-note cards for Windows. Cached Pretext layout,
real DirectWrite font metrics and bounded event-driven animation. No Electron,
Chromium, WebView or GPUI runtime.

## Download

[Windows previews](https://github.com/mud-the-developer/rusty-note/releases).
Extract the x64 zip and run `rusty-note.exe`. Windows 10/11 with x64 hardware is
the current target. The executable is unsigned; SmartScreen may warn. Checksums
are included. This is a **preview**, not a finished/full Markdown editor.

## Features

- Native rounded card canvas, dark/light themes, note library and eased transitions.
- Pin, resize, drag, selected-note and window-state persistence.
- Native Unicode source editor: selection, undo, clipboard and Windows IME.
- New notes, explicit save and reading preview.
- Save/discard/cancel before leaving unsaved edits.
- Each changed save writes `.md.bak`, syncs a temporary file, then replaces the note.
- External file changes block the save rather than silently overwriting them.
- Pretext preparation caches native font advances; resize uses arithmetic reflow.
- Animation timers stop when complete; idle waits in the Windows message loop.

## Use

Default folder: `%USERPROFILE%\Desktop\repo\notes`.
Default state: `%LOCALAPPDATA%\RustyNote\state.json`.

Header: ≡ library, ✎ edit / ✓ save and read, + new, P pin, ◐ theme, × close.
Keys: **Ctrl+E** edit, **Ctrl+S** save, **Ctrl+Enter** read, **Ctrl+N** new.
Tab/hamburger opens the library in reading mode. Left/Right switches notes;
wheel/Home/End scroll; Escape closes the library; F5 reloads.

Options: `--notes-dir PATH`, `--state-file PATH`, `--light`, `--reduced-motion`.
Files larger than 2 MiB are skipped/rejected. Changes are **not autosaved**.
Backups stay beside notes; restore the `.md.bak` manually if needed.

## Build and validate

Rust 1.99+ with Windows MSVC build tools/SDK:

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
.\target\release\rusty-note.exe
```

Isolated demo and interactive validation:

```powershell
.\target\release\rusty-note.exe --notes-dir tests/fixtures --state-file target/demo/state.json
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/validate.ps1
```

GUI tests edit **only copied fixtures**, bind/capture owned HWNDs (never desktop
pixels), exercise normal/narrow dark/light layouts, cached resize, note switching,
source Save/New and idle CPU/paint counts. Output stays ignored in `target/`.
CI checks build/tests/Clippy. Version tags trigger portable zip/checksum pre-releases.

## Performance measurements

A local small-fixture Release sample: ~42 MiB working set, ~57–58 MiB private
bytes; reported CPU time did not grow during a 2.5-second idle interval and no
idle repaint occurred. These are short local samples, not memory guarantees,
GPU/full-system measurements or a comparison against Tinta.

## Pretext / limitations

[`gpui-pretext` 0.6.2](https://docs.rs/gpui-pretext/0.6.2/gpui_pretext/) is a pinned,
third-party framework-independent Rust port of [chenglou/pretext](https://github.com/chenglou/pretext),
not an official Rust edition. The production adapter measures actual DirectWrite
layouts. No JS/browser runtime or GPUI framework is included.

Rendering currently supports block text/kinds (headings, lists/tasks, quotes,
code and table-row content). Rich inline emphasis/link interactions, proper grid
tables, images, native tray, notifications/file watching and full Tinta parity
are not implemented. The source editor is deliberately plain-text Markdown.
Save conflict checking is optimistic: concurrent external writes in the final
replacement window still need stronger filesystem locking/versioning.

See [changelog](CHANGELOG.md), [validation](docs/validation.md) and
[architecture](docs/architecture.md). MIT license.
