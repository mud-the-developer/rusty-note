# Find without editing the file

This **needle phrase** appears in a paragraph that should wrap across several narrow lines. 한국어 찾기 also works literally.

## Mixed surrounding content

- Lists and *emphasis* remain readable.
- F3 moves to the next result.
- Shift+F3 moves back; Escape clears the result marks.

> A quote preserves the reading-only behavior and surrounding layout.

| Place | Search content |
| --- | --- |
| Cell | a second needle phrase in a wrapping table cell |
| Unicode | 한국어 찾기 in another cell |

```text
The third needle phrase is inside code.
    Indentation stays intact.
```

## Final content

A [safe link](https://github.com/mud-the-developer/rusty-note) and emoji ✨ remain visible. A query absent from this file reports no results without changing the source bytes.
