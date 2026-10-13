# Rusty Note

- Keep the app native Rust/Windows; do not introduce a WebView/JS runtime silently.
- Pretext is the third-party Rust port `gpui-pretext` pinned to 0.6.2, not an official Rust edition of the original JS library.
- Font measurement must be real DirectWrite in production. Character-count metrics are test fixtures only.
- Idle must have no animation timers or unconditional render loop. Measure process CPU, working set/private bytes and idle paint counts; do not invent budget compliance.
- Never modify user Markdown notes during validation. Use `tests/fixtures/` and isolated `target/` state/output paths.
- Add mixed runnable Markdown fixtures and parser/layout regression tests for changes. Exercise the built app at normal/narrow widths in light/dark themes; inspect actual owned-window captures and record limitations.
- Run cargo fmt, tests and clippy with warnings denied. Generated output stays in target/ or out/.
- Do not claim Tinta-compatible Markdown rendering until supported/tested. The current preview has a native source editor and a block renderer. Save must preserve source/newlines, keep backups and reject detected external changes.
- Python tooling, if needed, must use uv venv and uv pip; no global Python installs.
