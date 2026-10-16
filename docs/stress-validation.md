# Historical repeat smoke — stress and optimization deferred

> This pre-v1 same-code record is not the v1 sign-off. The operator explicitly
> chose essential checks only for v1; see [v1 verification](v1-validation.md).

This was only repeated regression smoke of a preserved Release executable and a
rebuilt Release executable from the **same application code**. It was not the
requested large stress matrix, an optimization pass, or a measurement of an
optimized app variant. At the user's choice, further stress testing and
optimization are deferred for v1.0.0 in favor of releasing the current working app.
All generated notes and captures are under `target/`. No user notes or source
fixtures were modified.

## Recorded commands

The earlier smoke pass used Windows PowerShell (`pwsh`). These commands record
provenance, not a completed stress recipe or a request for further testing:

```powershell
cargo test --locked
cargo build --locked --release
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/validate-mermaid.ps1 -Binary target/stress-baseline/20261016T001330/rusty-note.exe
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/validate-mermaid.ps1 -Binary target/release/rusty-note.exe
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/validate-sticker.ps1 -Binary target/release/rusty-note.exe
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/validate.ps1 -Binary target/release/rusty-note.exe
```

The Mermaid and sticker drivers run dark and light owned windows, normal and
narrow sizes, resize/scroll/find/zoom, Unicode labels, malformed and resource-
limited Mermaid fallback, source-integrity checks, unchanged F5, external
refresh, and idle settling. The existing fixture generators use isolated
`target/` copies. Each operation is serialized; diagnostic writes, captures,
and sleeps are outside operation assertions. The validation process guards are
owned-process cleanup with bounded waits; no concurrent GUI suites were run.

## Environment and artifacts

- Windows native executable; Direct2D + DirectWrite; `gpui-pretext 0.6.2`.
- Preserved executable: `target/stress-baseline/20261016T001330/rusty-note.exe`.
- Preserved SHA-256: `357165ed909debb3fe3e3be237c43a8fe21084f4cfd82384b1f810f9ad0c5b56`.
- Preserved-app Mermaid artifacts: `target/mermaid-validation/1d3c40901e014189a14415108680dc05`.
- Rebuilt-app Mermaid artifacts: `target/mermaid-validation/92421107579c4e3dba0786bb28ec556a`.
- Preserved-app sticker artifacts: `target/sticker-validation/369fd3c5501a424cb3c6539e36c8f59a`.
- Rebuilt-app sticker artifacts: `target/sticker-validation/61bac92ecc114eecb30d4db29ffb6dac`.
- Rebuilt-app general-validation artifacts: `target/validation`.

## Same-code smoke observations

| Workload | Preserved app | Rebuilt same app | Observation |
|---|---:|---:|---|
| Mermaid DirectWrite measurement count (dark/light) | 3970 / 3970 | 3970 / 3970 | same recorded count |
| Mermaid idle CPU, 2.5 s | 0 / 0 ms | 0 / 0 ms | reported zero CPU growth |
| Mermaid idle working set (dark/light) | 52.66 / 50.30 MiB | 50.28 / 50.11 MiB | short local sample |
| Mermaid idle private bytes (dark/light) | 60.18 / 59.09 MiB | 61.51 / 58.37 MiB | short local sample |
| Sticker idle CPU, 1.5 s | 0 / 0 ms | 0 / 0 ms | reported zero CPU growth |
| Sticker alignment idle CPU, 1.5 s | 0 / 0 ms | 0 / 0 ms | reported zero CPU growth |

Counts are requests recorded by the existing validation metrics and are not
claimed to be native-call counts beyond the DirectWrite measurement counter's
instrumented semantics. The existing focused regression suite covers bounded
reads (>2 MiB rejection), Unicode/grapheme text, long/unbroken fallback,
Mermaid admission limits, cached reflow, search, and source hashes.

## Deferred and unverified work

The requested statistically repeated 64 KiB/256 KiB/1 MiB/2 MiB process-profile
matrix and distinct-DPI monitor transitions were not performed. The measurements
above are single serialized GUI runs, not cold-cache measurements, percentile
claims or evidence of performance-budget compliance. Run-to-run memory differences
do not measure an optimization effect.

This smoke pass neither establishes nor rules out worthwhile optimization
opportunities. No application optimization changes were made; resource limits and
fallback behavior were retained. Broader stress testing, large-document/mixed-DPI
profiling and optimization remain deferred at the user's choice, not completed.
