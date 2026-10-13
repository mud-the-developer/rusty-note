# Read without losing your place

This **read-only** document combines headings, lists, tables, quotes and code. 한글 파일 이름도 드래그해서 열 수 있습니다.

## Open and keep reading

- Drop exactly one `.md` file into the sticker.
- Keep the original source bytes unchanged.
- External refresh should preserve this reading position.

> A viewer reload should not send the reader back to the top.

| Behavior | Expected result |
| --- | --- |
| Another file changes | No font preparation or scroll reset |
| This Markdown changes | Refresh while keeping the scroll offset |
| Several files are dropped | Keep the current selection and show an error |

## Native work areas

A saved sticker should fit the nearest monitor's usable work area, including a taskbar. Large or old off-screen coordinates must not cause a panic or an invisible window.

```text
Windows native rendering stays in Rust.
No source file is saved by the viewer.
```

## Mixed surrounding content

[Project documentation](https://github.com/mud-the-developer/rusty-note) is a safe web link. **Emphasis** and `inline code` remain visible after a file notification.

1. Open the document.
2. Scroll one page down.
3. Append text outside the app.
4. Check that the reader stays in place.

## Final section

The ending stays present at both normal and narrow widths. The validation driver appends only to its own isolated copy, then restores that copy before checking the viewer did not write anything.
