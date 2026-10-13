# A quieter desktop

A native note, not a miniature browser. **Fast layout**, *soft motion* and a focused canvas.

## Today

- [x] Cache text measurement once.
- [ ] Animate only when something changes.
- [ ] Sleep when the note is still.

> The best animation is the one that stops when it has done its job.

| Layer | Job |
| --- | --- |
| Rust | State and files |
| Pretext | Cached wrapping |
| DirectWrite | Real font metrics |

```rust
let idle = !motion.active();
```

## Final heading

한국어 메모도 같은 네이티브 폰트 측정으로 줄을 바꿉니다. Emoji 👋 and links [remain readable](https://github.com/chenglou/pretext).
