# Rusty Note

A simple **read-only Markdown sticker** for Windows 10/11 x64.
Open a `.md` file, drag the small card onto your desktop, and pin it above other windows.
Native rounded cards, real DirectWrite font metrics, cached Pretext layout and event-driven idle. No Electron, browser,
WebView, JavaScript runtime or GPUI framework.

## Download

[Windows releases](https://github.com/mud-the-developer/rusty-note/releases).
Extract the portable zip and run **rusty-note.exe**. SHA256 checksums are provided.
The executable is unsigned; SmartScreen may warn. No installer or autostart change.

## Simple sticker (default)

```powershell
.\rusty-note.exe "C:\path\note.md"
```

Or launch without arguments and click **↑ / Ctrl+O** to open Markdown.
**P** toggles always-on-top, **◐** changes theme, **×** closes. Drag the header to move;
resize the edges, wheel/PageUp/PageDown scroll, **F5** reloads. The viewer does not edit,
rename, create, save or trash Markdown, even through the old editor commands.
Default size: 360x500. Last file/placement use `%LOCALAPPDATA%\RustyNote\sticker-state.json`.

The old editor remains opt-in with `--editor`; it is not part of the default sticker UI.

## Shared renderer / optional editor features

- Headings, paragraphs, bold/italic/strike, inline code, safe clickable web/mail links.
- Ordered/bullet/nested/task lists, quotes, horizontal rules and whitespace-preserving code.
- Real wrapping table cells with header shading and grid borders.
- Local PNG/JPEG/etc images through Windows WIC, with bounded file/pixel/cache limits.
- Native Unicode Markdown source editor: selection, undo, clipboard and Windows IME.
- New notes, explicit Save, read view, rename, and recoverable local note trash.
- Save/discard/cancel before leaving dirty editing; backups and external-change checks.
- Light/dark themes (including editor), pin, native drag/resize, window/selection/folder state.
- Scrollable note library, native folder picker and folder/open-trash shell actions.
- Tray hide/restore/right-click menu. Failed tray registration cannot hide the window.
- Native folder-change notifications refresh notes without a polling timer.
- Cached native font advances reused on resize; animation timers stop after transitions.

## Optional editor (`--editor`)

Default notes: `%USERPROFILE%\Documents\Rusty Note`.
App state: `%LOCALAPPDATA%\RustyNote\state.json`.
Choose a folder with **Ctrl+O**; it is remembered. `--notes-dir PATH` overrides it.

| Key / control | Action |
| --- | --- |
| ≡ / Tab | Note library in read view |
| ✎ / Ctrl+E | Source edit |
| Ctrl+S | Save, remain editing |
| ✓ / Ctrl+Enter | Save/read or leave with Save/Discard/Cancel |
| + / Ctrl+N | New note |
| F2 / right-click menu | Rename |
| Ctrl+O | Choose notes folder |
| Ctrl+H | Hide to tray; double-click tray icon restores |
| Left/Right | Previous/next note in read view |
| Wheel, Home/End, PageUp/PageDown | Scroll content; wheel over library scrolls notes |
| F5 | Reload selected file after dirty-editor confirmation |
| P / ◐ / × | Pin / theme / close |

Right-click opens the native actions menu (rename, trash, folders, tray, quit).
A trashed note moves to `.rusty-note-trash` inside its folder; restore it manually
or open the trash from the menu. Rename/trash use no-overwrite hard-link operations;
unsupported filesystems fail safely rather than replacing/deleting source data.

Save is **explicit, not autosave**. Each changed save keeps `.md.bak`, syncs a
new temporary file and replaces the note. Original CRLF and Unicode are retained.
Detected external edits block Save and keep the buffer. Cooperating saves use a
`.md.rusty-lock`; a stale lock requires manual review, not automatic deletion.
External writers remain optimistic: checking and replacement are not a transaction.

## Build / validate

Rust 1.99+, Windows MSVC tools and SDK:

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
.\target\release\rusty-note.exe
```

Isolated test/demo (source edits should use copied notes, not original fixtures):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/validate.ps1
.\target\release\rusty-note.exe --notes-dir target/demo/notes --state-file target/demo/state.json
```

Options: `--notes-dir PATH`, `--state-file PATH`, `--light`, `--reduced-motion`.
The GUI harness copies mixed Markdown/image fixtures to new per-theme directories,
checks normal/narrow layouts, table/style structure, native editing/Save/New,
rename prompt, folder notifications, tray callback and idle CPU/paint counts.
Screenshots contain owned HWNDs only. Output stays ignored in `target/`.
Version tags trigger CI checks/build/portable zip/checksum pre-releases.

## Performance / Pretext

A local mixed-fixture Release sample (styled Markdown + local image) showed
~44 MiB working set and ~58 MiB private bytes. No reported CPU-time growth or
additional repaint occurred in a 2.5-second idle interval. Short local accounting
is not a budget guarantee, GPU-memory measurement or comparison against Tinta.

[`gpui-pretext` 0.6.2](https://docs.rs/gpui-pretext/0.6.2/gpui_pretext/) is a pinned,
third-party framework-independent Rust port of [chenglou/pretext](https://github.com/chenglou/pretext),
not the original author's official Rust edition. Real DirectWrite font advances
feed preparation. Styled wrapping reserves conservative advances from fonts used
in each block, then draws exact DirectWrite style ranges. Reflow does not measure fonts.

## Intentional limits / remaining work

- Windows x64 only, unsigned portable build. No background autosave, updater or installer.
- Notes are bounded at 2 MiB and must be UTF-8 text without NUL bytes.
- Remote images are never fetched. Local images must remain under the notes folder;
  each image is limited to 8 MiB / 4 million pixels, with at most eight cached images.
- SVG, math/Mermaid, syntax highlighting, rich-text/WYSIWYG editing and full Tinta parity
  are not supported. Raw HTML is displayed as literal text, never executed.
- Tables use equal-width columns; alignment metadata is parsed but not applied.
  Very many columns in tiny windows need horizontal scrolling in a future version.
- Desktop file-change checks are not exclusive filesystem transactions. Multi-instance,
  exotic filesystem, long-document and live multi-monitor DPI behavior need more coverage.

[Changelog](CHANGELOG.md) · [Validation](docs/validation.md) ·
[Architecture](docs/architecture.md) · MIT license.
