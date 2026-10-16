# Native Mermaid validation

Rusty Note renders a deliberately small native subset of fenced Mermaid. Supported
input is `graph`/`flowchart` with `TD`, `TB`, or `LR` direction; rectangle, rounded,
or diamond nodes; solid directed edges; and admitted plain labels. DAG edges must
join adjacent ranks. Labels are measured with DirectWrite and the bounded layout
must fit without collisions. Otherwise the block's complete code body remains
literal, searchable code rather than being shrunk or having text omitted.

Unsupported directives, HTML, styles, cycles, subgraphs, labelled constructs outside
the admission rules, and other diagram types remain code. This is not full Mermaid
compatibility and does not include source editing. An admission allowlist precedes
upstream parsing; upstream font metrics, layout and rendering are never used. Search, scrolling, and resize reuse cached
geometry; idle has no Mermaid animation timer or unconditional remeasurement loop.

## Evidence

The existing isolated Windows owned-window run used the copied fixture
`tests/fixtures/mermaid.md` and generated artifacts in:

`target/mermaid-validation/64071f4f4fff43f9870f5902ac47338a`

It exercised dark and light themes at 600px and 360px widths, Unicode search,
zoom, fallback and resource-limit cases, source-byte hashes, and owned-window PNG
captures. Representative captures include `dark-600-top.png`, `light-360-top.png`,
`dark-360-fallback.png`, and `light-360-unsafe.png`. The run reported four prepared
and rendered diagrams, ten arrows, thirteen nodes in the wide fixture, and intact
table/image content. Its measured 2.5-second idle interval reported 0 ms CPU growth
and 0 additional paints. Dark reported 52,645,888 bytes working set and 63,606,784
private bytes; light reported 52,580,352 and 63,062,016 respectively. These are
literal observations from one Windows run, not performance-budget claims.

The focused recorded checks contain 41 unit tests and 2 integration tests passing,
plus format, clippy, and release-build logs. Captures are owned-window captures;
visual coverage is limited to the fixture, two widths, and two themes. DPI,
long documents, unusual fonts, and broader Mermaid syntax remain risks. Validation
copies fixtures into `target/` and does not modify user notes or source fixtures.

## Final direct review — 2026-10-16

Verdict: **PASS for the documented bounded subset**. The changed parser, layout,
DirectWrite drawing, callers, tests, dependency features and actual owned-window
captures were reviewed directly. No unresolved correctness, execution-safety or
source-integrity blocker was found in this scope. This is not a security
certification or a full Mermaid-compatibility claim.

Fresh command outputs are in `target/mermaid-final-review/`:

| Check | Result / evidence |
| --- | --- |
| `cargo fmt --check` | Passed, `fmt.log` |
| `cargo test --locked` | 41 unit + 2 integration tests passed, `test.log` |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed, `clippy.log` |
| `cargo build --locked --release` | Passed, `build.log` |
| Active LSP checks of all five Rust paths | No errors/warnings; spelling information about the existing Win32 `TME` name only |
| `git diff --check` | Passed, `diff-check.log` |
| Additional standalone invariant probes | 3 passed, `invariants.rs` / `invariants.log` |
| Source hashes | All 13 source fixtures and the user's `icon.png` unchanged, `source-integrity.log` |
| Native Mermaid, sticker and editor harnesses | All passed; `mermaid-gui.log`, `sticker-gui.log`, `editor-gui.log` |

The additional probes use explicitly synthetic test-only widths. They cover
lossless fallback tokens/source ranges across six widths and three zoom levels,
Unicode/graphemes, long rejected code, finite diagram bounds, search preservation,
no font calls on reflow, fence language selection, and CRLF/BOM offsets. Production
DirectWrite behavior is separately covered by the native test and GUI harness.

Fresh Mermaid owned-window captures and numeric geometry are in
`target/mermaid-validation/1db6016a291a429e9563415569306c4d/`.
Direct inspection covered both themes at 600x760 and 360x500, Unicode-labelled
branching nodes, LR, merge/chain, 140% zoom, malformed/unsafe/unsupported fallback,
and surrounding Markdown/table/image content. Inspected files include
`dark-600-top.png`, `light-600-top.png`, `dark-360-unicode.png`,
`light-360-unicode.png`, `dark-600-merge.png`, `light-360-lr.png`,
`light-zoom-narrow.png`, `dark-360-fallback.png`, `light-360-unsafe.png`,
`light-360-invalid.png`, and `dark-600-bottom.png`. Viewport clipping while scrolled
is expected; content remains reachable by scrolling. An unfit LR chain falls back
to code rather than hiding nodes.

The latest 2.5-second idle samples are recorded literally below. In particular,
the nonzero dark CPU observation is retained, not retried away. The short sample
does not identify its cause or establish a CPU/memory budget or GPU-memory use.

| Theme | CPU time increase | Additional paints | Working set (bytes) | Private bytes |
| --- | ---: | ---: | ---: | ---: |
| Dark | 15.625 ms | 0 | 50,671,616 | 61,284,352 |
| Light | 0 ms | 0 | 50,307,072 | 64,548,864 |

Fresh sticker/find/alignment regressions are in
`target/sticker-validation/ed7faf5b595449698c3b51345591ac00/`.
Representative aligned-link and code-search captures were inspected. The native
optional-editor Save/backup/New/rename and theme checks also passed; their latest
captures are under `target/validation/`. Only isolated copied notes were edited.

### Exact admission and layout limits

- Only `graph`/`flowchart` TD/TB/LR and the allowlisted plain-label forms are
  admitted. For example, `A[Start] --> B{Ready}` and `B -->|Yes| C(Done)`.
  Comments, escaping, entities, nested shapes, quoted punctuation outside that
  allowlist, and labelled chains are not supported. Node IDs are ASCII letters /
  underscore followed by at most 31 letters, digits or underscores.
- Per block: at most 8,192 UTF-8 bytes, 96 statements, 32 nodes, 48 edges,
  256 bytes per label and 4,096 total label bytes. Duplicate edges, self-loops,
  cycles and edges skipping topological ranks fall back to code.
- Per scene: at most 32 admitted parse attempts, 65,536 source bytes, 128 nodes,
  192 edges and 16,384 label bytes. Failed admitted attempts consume the attempt
  and source budget. Over-limit content remains code, not an omitted block.
- Native node width is at least 80 logical pixels times the reading zoom, with at
  most 8 wrapped node-label lines and 4 edge-label lines. Diagram height is capped
  at 8,192 logical pixels. Unfit labels and colliding label/connector layouts fall
  back to code using cached source preparation.

Remaining unverified areas: broader Mermaid syntax, live multi-monitor DPI changes,
unusual font installations, long-document stress and complete accessibility.
No plugin system, drawing editor, WebView/JS, source rewriting or syntax-highlighting
feature was added.
