# Simple read-only sticker checks

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
