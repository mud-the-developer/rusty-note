# Aligned tables

Before the table, **bold text**, *emphasis*, 한국어 and emoji ✨ still read normally.

| Left | Center | Right |
| :--- | :---: | ---: |
| L1 | C1 | 123.45 |
| plain | [center link](https://example.com/center) | [right link](https://example.com/right) |
| **bold left** | *center italic* | **1,234.56** |
| Several left words wrap normally | Several centered words wrap one line at a time | Longer right text wraps against the right padding |
| `left code` | **center bold** and plain | `987.65` |

## Default alignment

| Default | Center | Right |
| --- | :---: | ---: |
| default left | 한국어 가운데 | 오른쪽 ✨ |
| empty neighbors | | |

> After the tables, this quote and its **styled words** keep their own left alignment.

- Lists remain left aligned.
- A [safe surrounding link](https://example.com/after) stays clickable.

```text
  Code indentation stays intact.
```

---

## After everything

The final paragraph is not aligned by the preceding tables. Finding **123.45** should mark the numeric cell without modifying this source.
