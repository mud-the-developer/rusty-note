# Rusty Note

Native Rust Markdown cards for Windows: cached Pretext layout, real DirectWrite
font metrics and bounded event-driven animation. No browser/WebView/GPUI runtime.

Repository: https://github.com/mud-the-developer/rusty-note

## Current baseline

The initial version is a read-only rendering prototype with a card canvas,
dark/light themes, note library, pin/resize/drag, selection/placement persistence
and eased transitions. Inline styles, grid tables, editing, tray and file watching
are being developed; do not assume Tinta feature parity.

Pretext integration uses the third-party framework-independent Rust port
[`gpui-pretext` 0.6.2](https://docs.rs/gpui-pretext/0.6.2/gpui_pretext/), pinned in
Cargo.lock, not an official Rust edition of [chenglou/pretext](https://github.com/chenglou/pretext).

## Build

```powershell
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
.\target\release\rusty-note.exe
```

Default notes: `%USERPROFILE%\Desktop\repo\notes`.
Default app state: `%LOCALAPPDATA%\RustyNote\state.json`.

Isolated preview:

```powershell
.\target\release\rusty-note.exe --notes-dir tests/fixtures --state-file target/demo/state.json
```

Options: `--notes-dir PATH`, `--state-file PATH`, `--light`, `--reduced-motion`.
Tab/hamburger opens the library, Left/Right changes notes, wheel/Home/End scroll,
Escape closes the library, F5 reloads. Header controls toggle pin/theme or close.

## Performance / validation

A local two-fixture Release sample showed ~42 MiB working set, 57–58 MiB private
bytes, and no reported CPU-time growth/idle repaint during a 2.5-second interval.
These are short local measurements, not guarantees or a Tinta comparison.

`cargo test` verifies parser/cache/motion behavior. Interactive Windows checks:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/validate.ps1
```

Validation uses fixture copies and owned-HWND screenshots only; generated output
and personal notes/settings are excluded from version control. See
[validation](docs/validation.md) and [architecture](docs/architecture.md).
