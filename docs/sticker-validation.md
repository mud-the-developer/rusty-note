# Simple read-only sticker checks

## 0.3.5 document find

Twenty-seven core tests passed. Original semantic source IDs/ranges map literal
matches onto wrapped lines, including table cells and code. Queries are capped at
512 UTF-8 bytes and 4096 displayed hits; tests cover empty/overlong/no-result input,
English case, Korean literals, wrapping and unchanged font-call counts.

The native owned-window harness opens the Ctrl+F prompt through its menu-command
route, sets the native Unicode EDIT and confirms it. Both themes check 3 matches,
F3 and previous/wrapped navigation, resize without measuring fonts, zoom, a controlled
external fixture append adding a fourth match, no results, 2 Korean matches and Esc
clear. Copied source bytes are unchanged afterward. Normal/narrow captures of the
paragraph/table/code/Unicode/no-result views were inspected. Query strings are not
written to app state or diagnostic metrics; only counts/index/active flags are.

Limitations: selected matching **lines**, not exact substring glyphs, are marked.
Non-ASCII case folding, regex, image-alt and cross-block phrase matching are not
implemented. The initial absent-word assertion was corrected because the fixture
literally contained that word; final tests use a genuinely absent query.
Native Ctrl dispatch/Shift key state is reviewed; the prompt/menu and direct F3 route
are automated without global keyboard injection. Existing DPI/accessibility gaps remain.

## 0.3.4 text comfort and errors

Twenty-five core tests passed. Scaled Scene fonts and mixed tables/code are checked
at normal/narrow widths, with no extra font calls on resize. State keeps the bounded
zoom preference, with old JSON still defaulting to 100%. Native tests exercise
larger/reset/limit menu commands, 140% owned-window captures including tables/code,
120% close/reopen preference, source bytes, idle paints and the missing-file path.

The driver temporarily renames only a copied fixture, checks the last read view
survives, opens/captures the full owned native message, restores the fixture and
retries with F5. It locates exactly one native dialog button instead of assuming
IDOK: this host's single confirmation button had ID2. Early driver failures were
not app passes; final checks dismiss that button and require clean owned-process close.
Only app-owned windows/popup pixels are captured. No original fixture/user file,
global state or unowned dialog is altered.

Ctrl keyboard/wheel dispatch is implemented and reviewed; native menu-command
routes are automated, not global keyboard injection. Live DPI/assistive technology
and simultaneous state saves remain manual/stress gaps. Text-size changes preserve
an approximate reading area; exact logical paragraph anchors are not claimed.

## 0.3.3 reader usability

Twenty-four core tests passed, including scrollbar endpoint/grab-offset mapping
(with subpixel float tolerance) and backward-compatible reading-state round trips.
Native copied-fixture tests exercise P/T and arrow keys, no left/right reload,
actual captured scrollbar dragging without extra font measurements, source-byte
protection, idle paints and close/reopen reading resume. A different file override
must reset scroll. Mixed `reader-controls.md` combines heading/list/quote/table/code,
emphasis, links, Korean and emoji; both normal/narrow light/dark captures were checked.

Open/help and enlarged scrollbar captures were inspected, including the final
paragraph after dragging and restored table position. For hover, the test briefly
moves the pointer only inside the verified owned HWND (no click), then immediately
restores its original coordinates. A synthetic move alone triggered WM_MOUSELEAVE
because the real pointer was elsewhere; that was a driver issue, not a UI pass.

No source autosave/settings panel/updater/installer was added. Physical Explorer
input, assistive-technology completeness, multi-instance state-save stress and live
multi-monitor DPI transitions remain outside this check. Outputs remain ignored.

## 0.3.2 reference-informed polish

See [reference review](reference-review.md) for actual REA commands, pinned reference
commits and license/coverage limits. The final native harness covers oversized off-screen
state, Unicode receiver-allocated shell-drop payloads, multi-file refusal, unchanged
folder-event font counts and changed-document scroll retention. New mixed
`viewer-polish.md` is copied under ignored target output before controlled external edits.
Dark/light 600x760 and 360x500 captures, including the retained scroll/table position,
were inspected. Twenty-two core tests cover native payload parsing and geometry/scroll bounds.
A process-local HGLOBAL cannot be used by a cross-process synthetic driver; the final
harness allocates it inside the receiver before calling the real WM_DROPFILES handler.
Physical Explorer dragging and live multi-monitor DPI transitions remain manual coverage.
The driver restores only its own source copy, then verifies the app wrote neither that
file nor the original fixture, backups or new notes.

## Existing read-only behavior

Default app is now the MD viewer, not an editor. The native file dialog/positional
path selects one Markdown file; toolbar is open/pin/theme/close. Separate sticker
state restores its last file and placement. Existing editor is explicit `--editor`.

Ran core read-only input tests plus `scripts/validate-sticker.ps1` against the built
Windows Release app. Dark/light, normal/narrow 360x500 owned-window captures were
inspected. Mixed headings/emphasis/lists/quotes/table/code/link/Korean survived.

The GUI test hashes source bytes before/after, reloads, attempts all legacy mutating
commands and verifies no EDIT child, new Markdown or backup appears. Cached resize
font calls do not increase; idle paint count is stable. Short 1.5-second sample
reported 0ms CPU increase and ~43MiB working set, not a performance guarantee.
All generated output stays in ignored target/sticker-validation/<GUID>/.

Native file-picker interaction is manual; positional file loading is exercised.
No browser/mail launch or user note/state is used in validation. Optional-editor
regressions still run with `scripts/validate.ps1` using isolated copied notes.
