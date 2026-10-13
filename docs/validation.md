# Prototype validation

## Build / tests actually run

- `cargo test`: 5 passing unit tests (mixed Markdown, UTF-8 BOM heading,
  prepared-layout cache, animation stop/reduced motion, malformed state defaults).
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo build --release`: passed, executable about 816 KiB.
- Active Rust LSP/structural diagnostics: changed native path confirmed without errors.
- `scripts/validate.ps1`: passed on the real Windows Release binary.

## Native GUI checks

Two isolated notes were copied from runnable Markdown fixtures into
`target/validation/notes`; user notes/global state were not used. Dark/light
windows were rendered at 600x760 and 360x640, scrolled, shown with the library,
and switched via keyboard. Actual owned-HWND captures were inspected; no desktop
or other application pixels were captured.

The fixtures combine headings, lists/tasks, quotes, code, table content,
emphasis/link text, Korean and emoji. Inline style semantics/grid tables are not
claimed: the prototype retains their text as block content.

Checks verified:

- initial motion settles;
- native font-measurement count does not increase on resize;
- selecting another note changes the selection and starts/finishes the animation;
- no idle paint-count growth over 2.5 seconds;
- process CPU and working set/private bytes sampled on the owned process;
- state written outside source notes and owned process closes cleanly.

Actual last local results:

| Theme | Working set MiB | Private MiB | CPU ms / 2500 ms idle | Additional idle paints |
| --- | ---: | ---: | ---: | ---: |
| Dark | 41.84 | 58.08 | 0 | 0 |
| Light | 41.89 | 56.72 | 0 | 0 |

A 0 ms sample is limited by Windows accounting granularity; it is not proof of
zero CPU at every instant. GPU memory/full-system memory, large files, multiple
instances and live multi-monitor DPI migration were not profiled.

## Findings fixed while inspecting captures

- A generated PowerShell fixture originally had an encoding/BOM problem, turning
  the heading into plain text and damaging Korean. Fixtures are now source files
  copied as bytes; the parser strips a leading UTF-8 BOM. A regression test was added.
- The decorative NATIVE CANVAS badge overlapped the title in a narrow library
  layout. It now hides when insufficient space remains; corrected images inspected.
- An unresolved WM_MOUSELEAVE identifier could become a Rust match wildcard.
  The message is explicit (0x02a3); actual native smoke/GUI checks catch message-loop errors.
- Unsafe Win32 operations now have explicit unsafe scopes, not a file-wide
  unsafe-op lint exemption. Win32 ownership/reentrancy still needs ongoing review.

Artifacts (ignored): `target/validation/{dark,light}-*.png`, `*-profile.json`,
`*-metrics.json`, isolated state files and copied notes.

## Current limitations

This is a working rendering/performance prototype, not the finished fancy note
product. No editor/undo/save, tray, directory watcher, images, clickable links,
rich inline runs, native grid tables or Tinta compatibility guarantee yet.
Animations are bounded Direct2D redraws, not retained compositor animations.
The next optimization is cached card surfaces + DirectComposition transitions.
The Pretext dependency is a third-party Rust port, not the official JS package.
