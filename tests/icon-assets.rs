#[test]
fn windows_icon_has_eight_rgba_png_sizes_and_an_rgba_master() {
    let ico = include_bytes!("../assets/rusty-note.ico");
    let word = |at| u16::from_le_bytes(ico[at..at + 2].try_into().unwrap());
    let dword = |at| u32::from_le_bytes(ico[at..at + 4].try_into().unwrap()) as usize;
    let png_size = |png: &[u8], at| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
    assert_eq!((word(0), word(2), word(4)), (0, 1, 8));
    let mut end = 6 + 16 * 8;
    for (i, size) in [16, 20, 24, 32, 48, 64, 128, 256].into_iter().enumerate() {
        let entry = 6 + i * 16;
        let dimension = if size == 256 { 0 } else { size as u8 };
        assert_eq!(&ico[entry..entry + 2], &[dimension, dimension]);
        assert_eq!((word(entry + 4), word(entry + 6)), (1, 32));
        assert_eq!(dword(entry + 12), end);
        let frame = &ico[end..end + dword(entry + 8)];
        assert_eq!(&frame[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!((png_size(frame, 16), png_size(frame, 20)), (size, size));
        assert_eq!(&frame[24..26], &[8, 6]); // 8-bit RGBA; the generator checks transparent corners.
        end += frame.len();
    }
    assert_eq!(end, ico.len());
    let master = include_bytes!("../assets/rusty-note.png");
    assert_eq!((png_size(master, 16), png_size(master, 20)), (1024, 1024));
    assert_eq!(&master[24..26], &[8, 6]);
}

#[test]
fn public_source_png_contains_only_image_and_color_chunks() {
    let source = include_bytes!("../assets/icon-source.png");
    assert_eq!(&source[..8], b"\x89PNG\r\n\x1a\n");
    let mut pos = 8;
    while pos + 12 <= source.len() {
        let length = u32::from_be_bytes(source[pos..pos + 4].try_into().unwrap()) as usize;
        let kind = &source[pos + 4..pos + 8];
        assert!(
            matches!(
                kind,
                b"IHDR"
                    | b"IDAT"
                    | b"IEND"
                    | b"sRGB"
                    | b"iCCP"
                    | b"cHRM"
                    | b"gAMA"
                    | b"pHYs"
                    | b"PLTE"
                    | b"tRNS"
            ),
            "Unexpected PNG metadata chunk"
        );
        pos += length + 12;
        assert!(pos <= source.len());
        if kind == b"IEND" {
            assert_eq!(pos, source.len());
            return;
        }
    }
    panic!("Source PNG has no IEND");
}
