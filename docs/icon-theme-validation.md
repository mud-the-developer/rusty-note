# Rusty icon and warm theme validation

Local v0.3.7 Windows x64 MSVC Release candidate, 2026-10-15. This records local
validation, not full accessibility/performance certification. Published assets require
a separate checksum check and native run.

## Assets and integration

- Preserved the supplied `icon.png` (2048px RGBA PNG) locally. The public source copy
  `assets/icon-source.png` has identical image/color chunks but omits the original
  generation-metadata chunk. A regression rejects extra metadata/trailing data.
- Retained its pixel-art rust texture and folded-paper silhouette; removed the baked
  checkerboard, original outer shadow and lower-right sparkle. Rebuilt the charcoal tile
  as a continuous glass squircle with a translucent gradient, two light rims, copper
  reflection and softened paper shadow. This is static icon artwork, not runtime blur.
- Production assets: `assets/rusty-note.png` (1024px RGBA) and `assets/rusty-note.ico`
  (16, 20, 24, 32, 48, 64, 128, 256px RGBA PNG frames).
- `build.rs` uses the existing Windows SDK x64 resource compiler; resource group 1
  is linked into the executable. There are no new Rust/runtime dependencies.
- Window large/small and tray icons load that module resource. Shared resource handles
  live for the process and are not destroyed by individual consumers.
- The GUI harness extracted both executable icons and inspected both class icons
  from owned HWNDs. All pixels matched production artwork loaded through Win32.
  Direct .NET ICO-to-bitmap decoding misread small PNG frames during investigation;
  validation now uses the real Windows loader instead.

To reproduce the refinement without overwriting the supplied source:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/refine-icon.ps1
# Inspect target/icon-assets first, then deliberately promote production assets:
Copy-Item target/icon-assets/rusty-note.png,target/icon-assets/rusty-note.ico assets/
```

The generator checks the paper mask, glass lighting, transparent corners/translucent
plate/opaque paper at all eight sizes and ICO size decoding. Its sampler is specific to this supplied pixel artwork.
Generated previews/diagnostics stay under `target/icon-assets/`.

## Palette and contrast

Reader and optional native editor share `src/theme.rs`; GDI RGB-to-BGR conversion
has its own regression test. No note IO, layout or font-measurement behavior changed.

| Theme | Card | Ink | Copper accent | Minimum tested text contrast |
| --- | --- | --- | --- | --- |
| Light | `#FBF6EF` | `#302722` | `#87391D` | 4.58:1 |
| Dark | `#24201D` | `#F5EADC` | `#F0A574` | 4.66:1 |

Tests use sRGB relative luminance, with black/white 21:1 as a reference. They include
muted quotes/status, body/code/table text, active controls, selected library rows,
20%-tinted search surfaces, error text and the 85%-opacity small badge. Every tested
nominal text pair is at least 4.5:1; this is not a claim of full WCAG compliance.
The GUI harness also checked actual captured card/background RGB values.

## Executed checks and evidence

- `cargo fmt --check`; 33 unit tests plus two icon-asset integration tests;
  `cargo clippy --locked --all-targets -- -D warnings`; MSVC Release build.
- Active changed-path LSP checks: fourteen Rust/PowerShell/documentation files,
  no warnings/errors.
- Both native validation scripts passed: read-only opening/drop/refresh/resume,
  zoom/find/aligned-table/link hit testing; optional edit/Save/backup/New/rename,
  folder notifications and tray registration/hide/restore.
- New mixed `tests/fixtures/rusty-theme.md` has parser and cached-reflow regressions.
  Inspected actual owned-window captures at 600x760 and 360x640 in both themes,
  including body/Korean text, code, quote, table, selected library and native editor.
- Captures: `target/validation/{light,dark}-rusty-{wide,narrow,search,bottom}.png`,
  `{light,dark}-editor.png`, `{light,dark}-library.png` and `exe-icon-{large,small}.png`.
- Reader evidence: `target/sticker-validation/9d1ef69f9e2d40e8b637249f7b4a0f3b/`.
  Logs: `target/glass-release-{tests,clippy,build,native,sticker}.log`.
- Only isolated fixture copies/state under `target/` were used; source-byte guards
  passed. User notes and global app state were not modified.

### Short local idle samples

| Mode/theme | Interval | CPU-time increase | Additional idle paints | Working set | Private bytes |
| --- | --- | --- | --- | --- | --- |
| Editor/dark | 2500ms | 0ms reported | 0 | 44.70 MiB | 57.64 MiB |
| Editor/light | 2500ms | 0ms reported | 0 | 44.79 MiB | 58.83 MiB |
| Reader table/dark | 1500ms | 0ms reported | 0 | 45.84 MiB | 57.87 MiB |
| Reader table/light | 1500ms | 15.625ms reported | 0 | 45.85 MiB | 57.38 MiB |

These are short fixture-specific process measurements, not idle budget compliance,
GPU/system-memory accounting or a long-duration stress test. No timers/render loop
were introduced by these changes.

## Limits

Windows native menus, prompts, editor selection and its scrollbar still use system
control styling; they were not reskinned. No high-contrast mode, HDR/color-management,
live multi-monitor DPI or Explorer icon-cache/restart testing was performed. The narrow
optional-editor library still leaves little reading width (existing layout). No
signing, installer work or user-note migration was performed.
