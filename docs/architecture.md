# Native card rendering spike

## Decision

Use a small Windows-native stack: Rust + Win32 + Direct2D/DirectWrite with
`gpui-pretext` 0.6.2. The original JS Pretext needs browser/font measurement during
prepare. A framework-free Rust port makes a WebView unnecessary for this spike.
The port is explicitly third-party. Published crate/version/license were verified
against crates.io before adopting it; search summaries about publication were stale.

No GPUI framework, browser runtime, async application runtime or GPU abstraction
stack is pulled in. Direct2D manages the native rendering target.

## Data flow

1. Read bounded Markdown from the chosen notes directory, never mutate it.
2. `pulldown-cmark` emits block content/kinds (heading, list/task, quote, code, table row).
3. Configure a native font per block. `TextMeasure` delegates to actual DirectWrite
   text layout metrics, including Windows font fallback.
4. Pretext prepares cached segment/grapheme advances once for the selected document.
5. Resize calls Pretext's arithmetic line walker; repeated widths skip reflow.
6. Render only lines intersecting the viewport, plus the card/header/library.
7. Interactions start a bounded 220 ms cubic ease-out motion. Stop the timer when
   complete. The normal message loop blocks when idle.

Native window messages use a `RefCell` with non-panicking reentrant borrow handling;
window data ownership is released on non-client destruction. Win32 bindings are
unsafe and need continued review: this is a prototype, not a security-certified GUI.

## Animation tradeoff

Current animation is native Direct2D alpha/position redraw on an active-only timer.
This is **not yet a retained DirectComposition animation graph**. A future version
can snapshot prepared card surfaces and let the compositor animate opacity/offset,
reducing active CPU work without redoing text layout. Idle remains event-driven.

## What comes next

- Rich inline runs with correct styled advance measurement, code whitespace and grid tables.
- Prepared line/surface reuse across transitions and a bounded cache for recently viewed notes.
- Compositor-driven card/rail transitions and reduced-motion system preference.
- Native edit/undo/save flow with source integrity tests, tray and directory notifications.
- Long-document, multi-monitor DPI, GPU-memory and multi-note-instance profiling.

These are design directions, not implemented features. First establish a reliable
native renderer and measurable budgets before expanding the product.
