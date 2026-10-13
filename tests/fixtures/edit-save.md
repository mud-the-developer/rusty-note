# Native editing and safe saves

Use the **pencil** or *Ctrl+E* to edit this source. Save with `Ctrl+S`, then read with `Ctrl+Enter`.

- [ ] Create a new note with the plus button.
- [x] Preserve source text and newline style.

> A detected external change must block Save, not destroy the unsaved buffer.

| Action | Expected |
| --- | --- |
| Save | Previous `.md.bak` retained |
| New | Existing filenames untouched |

```text
first line

last line
```

[Project](https://github.com/mud-the-developer/rusty-note)

## Final heading

한국어와 emoji ✨ remain in the source. The GUI regression edits only copied fixtures, never this file.
