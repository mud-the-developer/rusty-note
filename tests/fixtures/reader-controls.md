# A small reader that feels finished

Open this **read-only** sticker, drag the right-hand scroll thumb and use ↑/↓ to read. 한국어와 emoji ✨ remain unchanged in the source.

## Clear controls

- Open opens Markdown without creating a note.
- P pins/unpins the sticker.
- T changes the theme.
- Hovered controls explain themselves in the status bar.

> Left and right arrow keys must not reload the same file or lose the reading position.

| Control | Expected behavior |
| --- | --- |
| Thumb drag | Follow the pointer, including beyond the track |
| Track click | Jump toward the clicked position |
| Close and reopen | Restore reading offset for the same file |
| Different file | Start at the top |

## A mixed long section

Use **emphasis**, *italic text* and `inline code` while checking narrow layout. This paragraph wraps across several lines without covering the scrollbar.

```rust
fn source_is_read_only() {
    // App state is separate from Markdown.
    println!("Keep the original bytes.");
}
```

1. Scroll to this code.
2. Close the sticker.
3. Reopen using the saved isolated state.
4. Check this reading position survived.

## More surrounding content

[Documentation](https://github.com/mud-the-developer/rusty-note) remains a safe clickable link. Quotes and lists before and after a resized table must survive.

> Enlarging the window clamps the scroll instead of leaving a blank canvas.

## Final heading

The final **paragraph** remains readable. Native validation edits and saves only copied test input and isolated state, never source fixtures or user notes.
