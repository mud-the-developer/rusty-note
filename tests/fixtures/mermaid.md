# Native Mermaid

Branching labels stay native, including 한국어 and Unicode.

```mermaid
flowchart TD
    A(시작 Start) --> B{선택?}
    B -->|예| C[완료 ✅]
    B -->|No| D[다시 시도]
```

## Left to right

```mermaid
graph LR
    A[입력 α] --> B[출력 β]
```

## Merge and chain

```mermaid
flowchart TD
    A[Read] --> B[Check]
    A --> C[Review]
    B --> D[Keep source]
    C --> D
```

```mermaid
graph LR; A[Open]-->B[Read]-->C[Close]
```

## Unsupported stays code

```mermaid
sequenceDiagram
    Alice->>Bob: Keep this readable
```

```mermaid
flowchart TD
    A[Literal] --> B[No execution]
    click A "https://example.com/never-opened"
```

```mermaid
%%{init: {"securityLevel": "loose"}}%%
graph TD; A[<b>No HTML</b>]-->B
```

```mermaid
graph TD
    A[unclosed --> B
```

## Ordinary Markdown survives

```rust
fn main() {
    println!("graph TD; A-->B is ordinary code");
}
```

~~~text
  whitespace and blank lines

  stay readable
~~~

- [x] Source is read-only in sticker mode.
- **Bold**, *italic*, `inline code`, and a [safe link](https://example.com/markdown).

> A quote after native diagrams.

| Left | Center | Right |
| :--- | :---: | ---: |
| keep | 한국어 | 123.45 |

![Local fixture](fixture.png)

---

Final paragraph: Mermaid never changes the Markdown source bytes.
