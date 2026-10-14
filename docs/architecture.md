# Native desktop architecture

Rust + Win32 + Direct2D/DirectWrite, with pinned third-party `gpui-pretext` 0.6.2
and pulldown-cmark. No browser/WebView/JS/GPUI runtime, application async runtime
or unbounded render loop.

## Rendering

`document` emits styled UTF-8 ranges, structural lists, table cells, code, rules
and local image descriptors. Raw HTML stays literal. The production font adapter
measures actual native DirectWrite text layouts, including font fallback.

Pretext prepares each block/cell once using PreWrap. Styled wrapping reserves the
largest measured advance among fonts used in that block; this is deliberately
conservative. Arithmetic reflow creates line/run geometry and equal-width cell
rows. Resize does not measure fonts. DirectWrite draws actual UTF-16 styled ranges;
table lines retain cell bounds and parsed alignment. DirectWrite applies alignment
to the actual styled glyphs; the same layout supplies search marks and link hit tests
in every column. The core coarse link lookup still uses cached prepared advances.
Link activation uses native text hit-testing, and shell dispatch permits only
HTTP/HTTPS/mailto without control bytes. No shell command strings are executed.

WIC loads only canonical local paths inside the notes directory, bounded to
8MiB files/4M pixels/max8 bitmaps. Remote/absolute/traversal paths never fetch.
Images keep aspect ratio inside bounded card boxes. Device-loss/cache lifetime
and long-document stress still need continued coverage.

## Editing and source integrity

A native Windows EDIT child supplies Unicode, IME, selection, undo and clipboard.
Explicit Save keeps a `.md.bak`, syncs a new temporary file and replaces the source;
CRLF survives. Cooperating saves use create_new `.md.rusty-lock`, with manual stale
lock review. Disk text is checked before and immediately before replacement.
External programs do not cooperate with this lock: final TOCTOU remains optimistic,
not a filesystem transaction or guarantee of exclusive external-write safety.

Names reject path/device/control characters. Rename/trash use create-only hard
links then unlink source; unsupported filesystems fail safely. Trash is recoverable
inside `.rusty-note-trash`. No background source save/deletion was introduced.

## Native lifecycle

Completed 220ms motions kill their timer; idle blocks. A directory notification
handle joins the message wait via MsgWaitForMultipleObjectsEx, not a polling timer.
Refresh preserves selection paths and never discards an active editor buffer.
Tray hide is allowed only after icon registration; double-click restores. Native
folder and rename prompts are separate Win32 windows/dialogs.

App ownership is stored in window data, freed at WM_NCDESTROY; the main loop checks
window lifetime before dereferencing it. RefCell reentrant callbacks fall back to
DefWindowProc rather than panic. GDI/editor/tray/watch resources have owned drops.
State input is bounded and geometry clamped; explicit CLI folder overrides remembered
folder. Win32 unsafe code still requires review, not security-certification claims.

## Future scope

Exact styled advance mapping (rather than conservative wrapping reservations),
horizontally scrollable tables, syntax/math/diagrams/SVG, retained DirectComposition
surfaces, system motion preferences, multi-instance/DPI/filesystem stress and signed
installer/updater. These are not silently implied by the current desktop version.
