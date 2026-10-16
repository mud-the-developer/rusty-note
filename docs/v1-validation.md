# v1.0.1 — essential validation

The operator explicitly chose a quick first v1 with essential checks, then further
validation/iteration after publication. No stress matrix, mixed-DPI/soak exercise or
broad optimization claim is included in this sign-off.

## Local release candidate

Logs: `target/v1.0.0-release/final/`.

- `cargo fmt --check`: passed.
- `cargo test --locked`: **86 passed** (50 unit, 19 draft, 2 icon, 2 projection/session,
  13 inline-note/layout tests). Source/conflict/recovery/limits tests were retained.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo build --locked --release`: passed.
- Active LSP checks of the nine changed production paths: no errors reported.
- All PowerShell scripts parsed; original source-fixture and `icon.png` hashes match.
- Third-party inventory: 52 non-development Windows dependency packages, including
  build dependencies conservatively; 29 distinct full license/notice texts bundled.

## Short actual native workflow smoke

Command: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/validate-v1.ps1`.
Both themes passed with copied tiny notes and isolated state under `target/`.
Final local artifacts:
`target/v1-smoke/4f6af1e14a7c496bb1f1e0e9e9a22aa5/`.

Checks: editable default, heading edit, cross-style text replacement with literal
punctuation/emoji, native bold and its Markdown source, Enter/new paragraph and
retained earlier formatting, task toggle/undo/redo, completed-checkpoint recovery
after stopping only the owned test process, explicit original Save and exact
backup, image fit/zoom/Esc, new empty-note typing/Save, normal/narrow layouts.

Inspected dark `editing.png`/`image-zoom.png` and light `narrow.png`/`new-note.png`.
Parent and native client captures are actual owned HWND renders, combined at their
actual coordinates/clip; raw `*-parent.png` and `*-native.png` remain available.
No desktop pixels or re-rendered/synthesized note text are used for capture.
Unicode injection is not physical keyboard or IME evidence.

An early driver sent WM_CHAR Return without WM_KEYDOWN, which RichEdit ignored.
The final driver sends the normal keydown/character sequence and asserts actual
Markdown paragraph separation and preserved `**Hello**` source, not only no error.
That earlier incomplete check is not counted as a newline pass.

Literal 1.5-second idle observations after the small new-note flow:

| Theme | CPU increase | Parent paints | Working set | Private bytes |
| --- | ---: | ---: | ---: | ---: |
| Dark | 0 ms | 0 | 51,101,696 bytes | 64,708,608 bytes |
| Light | 0 ms | 0 | 51,003,392 bytes | 63,791,104 bytes |

These short local samples are not budget, GPU-memory, foreground-caret or stress
compliance. Native caret/compositor work is not completely counted by WM_PAINT.

The first `v1.0.0` tag stopped in CI at a fixture-selection test that assumed LF
checkout. No artifact was published. The test now uses parser source ranges and
exercises LF/CRLF; the existing tag is preserved and the first published v1 is
`v1.0.1`. This was a test-portability fix, not a runtime feature change.

## Distribution check

The version-tag workflow repeats Cargo checks, then builds an unsigned Windows x64
portable zip containing `rusty-note.exe`, `README.md`, `LICENSE` and
`THIRD_PARTY_NOTICES.txt`, plus a separate `SHA256SUMS.txt`. The release owner checks
CI, stable release metadata, the actual downloaded zip/checksum/contents and the
same short smoke before reporting the uploaded artifact verified. Local build
success alone is not a downloaded-artifact result.

## Deliberately deferred / known limits

Physical IME/TSF, clipboard interoperability, complete accessibility, full Markdown
rich editing, mixed-DPI, multi-instance and long-session/large-note resource stress
remain post-v1 work. Complex blocks and unsupported inline constructs use source
mode rather than lossy flattening. Edited text blocks have canonical Markdown
spelling; unedited document bytes remain outside the conversion.

Undo is bounded and not persisted. Plaintext recovery files remain after clearing
an association. M1 and pending-input publication are not a single transaction;
mismatches retain files and visibly block Save. Non-cooperating external writers,
power loss and exotic filesystems are not transaction guarantees. Windows builds
remain unsigned. No full Mermaid/Tinta compatibility or broad optimization claim.
