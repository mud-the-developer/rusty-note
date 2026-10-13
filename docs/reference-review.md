# Reference review with REA

Keep this app a small **read-only native Windows Markdown sticker**. Reference
projects inform behavior, not a migration to their runtime or a source-code copy.

## Tooling actually run

Pinned `rea-agents@6.4.0` was installed only under ignored `target/rea`, with npm
lifecycle scripts disabled. No global/MCP-client configuration changes, reference
app launch, paid model calls or Ghidra installation were performed. Node/REA are
investigation tools, not application dependencies.

```text
rea analyze-javascript-application <md-viewer>/app --json
rea analyze-javascript-application <sticky-markdown-note> --json
```

Both commands completed. Evidence JSON stays in `target/rea/` (about 3 and 9.6MB).
REA reports syntax-derived relationships, not proven runtime behavior:

| Reference | Inert analysis | Coverage limit |
| --- | --- | --- |
| md-viewer | 8 JS files, 2,679 AST nodes, 88 findings, zero parse failures | Older split Electron packages are not fully recognized; zero BrowserWindow findings does not mean no windows |
| sticky-markdown-note | 5 JS files, 7,422 AST nodes, 102 findings; 3 window sites and 42 IPC operations | Runtime registration/enforcement is unverified |

Selected source bodies were also read to verify behavior suggested by the analysis.
No full native binary decompilation or REA Windows UI automation is claimed: native
function inspection needs a configured provider, and REA's documented native UI
observation is macOS-only. Windows checks use our owned-HWND harness instead.

## References and independent adaptations

- [md-viewer/md-viewer](https://github.com/md-viewer/md-viewer),
  `e99b88fc1c33e91ba45614d05b875048a828d331`, Apache-2.0.
  `app/main/viewers.js` and `app/renderer/modules/renderer.js`: preserve scroll
  during refresh and normalize file identity. Adapted reading-position retention
  and unchanged-content detection. Did **not** copy polling `fs.watchFile`,
  unrestricted link dispatch or unsanitized HTML rendering.
- [seongmini/sticky-markdown-note](https://github.com/seongmini/sticky-markdown-note),
  `b1fc5c9ecb2141d1db8f0bf5c916d91f7b0a635a`, MIT.
  `src/main/main.js`: small frameless per-file windows, window-state memory,
  delayed presentation and file identity. Kept the simple sticker shape; did not
  add its editor, autosave, updater or permissive Node/web preferences.
- [snownico0722/PaperTodo](https://github.com/snownico0722/PaperTodo),
  `a1cc3a0c266a393d5987c9b3117138e94644e1a8`, restricted noncommercial license.
  Read `app.manifest` and selected `src/WindowWorkAreaHelper.cs` bodies for general
  work-area/DPI principles. **No code/assets were copied or added as dependencies.**
  Our independent Win32 restoration uses documented monitor/work-area APIs and
  clips oversized geometry before clamping, avoiding an invalid clamp/panic.

## Changes and actual verification

- One Unicode `.md` file can be dropped into the viewer; multi-file payloads are
  rejected without replacing the current document or writing source files.
- External refresh and F5 keep the reading offset; shorter content clamps it.
- Unrelated directory events do not rebuild Markdown/fonts or reset the scroll.
  Cached local image changes still invalidate their bitmap.
- Saved off-screen/oversized windows fit the nearest monitor work area, with
  initial monitor DPI scaling. Display changes trigger visible-window recovery.
- Device/DPI changes clear target-bound bitmap caches.
- Found and fixed a native message-wait bug: a sent message could wake
  MsgWaitForMultipleObjectsEx then leave GetMessage blocked past file events.
  PeekMessage now returns to the handle wait; no polling timer/render loop added.

Core tests cover scroll clamping, negative monitor coordinates, oversized geometry
and real shell-drop parsing. Native Release checks cover Unicode drop handling,
multiple-file refusal, off-screen restoration, external reload/read-position,
unchanged-event font counts, read-only source bytes and idle paint counts in both
normal/narrow light/dark views. Screenshots are owned-window captures only.

A synthetic cross-process HGLOBAL drop was **not valid** (HGLOBAL is process-local).
The final harness requests a bounded payload allocated inside the receiving app,
then exercises its actual WM_DROPFILES handler. This tests the handler, not physical
Explorer dragging. Live multi-monitor DPI transitions and interactive Explorer
input remain manual coverage gaps. Validation edits only copied Markdown fixtures.
