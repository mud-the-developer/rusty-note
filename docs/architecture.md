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

Fenced Mermaid is admitted separately by `diagram`: only `graph`/`flowchart` TD/TB/LR
with rectangle, rounded, or diamond nodes, solid directed edges, and admitted labels
enter the native bounded flow layout. DirectWrite measures every displayed label.
Unsupported directives, HTML/styles, cycles, subgraphs, other diagram types, and
layouts that do not fit or collide remain searchable literal code; the source is
never edited. An admission allowlist precedes `mermaid-rs-renderer` 0.3.1 parsing;
its upstream font metrics, layout and rendering are not used. Preparation and cache budgets bound
work per scene/document. Search and scrolling reuse cached geometry, and idle has
no Mermaid timer or unconditional remeasurement/render loop.
Images keep aspect ratio inside bounded card boxes. Device-loss/cache lifetime
and long-document stress still need continued coverage.

## Editing and source integrity

The default editable sticky uses native RichEdit over the rendered scene. Parsed
text-body byte ranges and structural prefixes are associated with scene origins;
DirectWrite hit testing supplies the clicked caret. Native bold/italic runs are
serialized by `pulldown-cmark-to-cmark` only for the edited block, with parser
round-trip checks; unedited document bytes are not HTML/RTF round-tripped.
Empty slots, new paragraphs, simple heading/list text and task toggles share the
working session. `--read-only` and `--editor` retain their separate modes.

`inline_session` binds M1 draft sessions to supported paragraph/pending UTF-16
input. Original Save remains explicit and uses the shared conflict-aware commit.
Per-state recovery associations are bounded and validated. Malformed state and
unknown temporaries are retained, not overwritten. Companion-input and M1
publication are not a single atomic transaction; mismatches visibly block Save.
Draft data is plaintext and retained after association discard. See
[v1 scope and essential validation](v1-validation.md).

A native Windows EDIT child still supplies the optional source editor's Unicode,
IME, selection, undo and clipboard.
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
DefWindowProc rather than panic, except reentrant modal-owner close is ignored to
avoid destroying a borrowed app. RichEdit paint notifications do not invalidate
the D2D parent; child redraws after parent paints are queued/coalesced once, not a
render loop. GDI/editor/tray/watch resources have owned drops.
State input is bounded and geometry clamped; explicit CLI folder overrides remembered
folder. Win32 unsafe code still requires review, not security-certification claims.

Local-image enlargement uses an owned `image_viewer` window with its own bounded
WIC/Direct2D resources, fit/zoom/pan and event-driven paints. It never modifies or
fetches image sources. Block/task undo deltas are bounded and not restart-persistent.

## Future scope

Exact styled advance mapping (rather than conservative wrapping reservations),
horizontally scrollable tables, syntax/math/SVG, broader Mermaid syntax, retained DirectComposition
surfaces, system motion preferences, multi-instance/DPI/filesystem stress and signed
installer/updater. These are not silently implied by the current desktop version.
