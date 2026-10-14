# Comfortable Markdown reading

This paragraph combines **bold**, *italic*, ~~strike~~ and `inline code`. 한국어와 emoji ✨ should remain readable when the text grows.

## Text size controls

- Ctrl++ enlarges document text.
- Ctrl+- reduces document text.
- Ctrl+0 returns to the default.
- Ctrl+wheel also changes the size.

> The card controls stay the same size; only document fonts and line layout change.

| Content | Expected behavior |
| --- | --- |
| Paragraph | Uses real native font measurements at the selected size |
| Table cell | Wraps without overwriting its neighbor |
| Window resize | Uses the already prepared font metrics |

```rust
fn keep_source() {
    println!("Original Markdown is unchanged.");
}
```

## Read errors

If a copied test file temporarily disappears, the old read view remains available.
The error status opens the complete message instead of silently clipping the path.
Restore the copied file and press F5 to recover without editing the source.

1. Open this document.
2. Enlarge the text and inspect headings, tables and code.
3. Reopen and check the text size preference survived.

[Project](https://github.com/mud-the-developer/rusty-note)

## Ending

Surrounding content still survives at a narrow width. The **source bytes** and
line endings are never changed by text-size or error-recovery controls.
