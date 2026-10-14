//! Warm paper and charcoal, with copper reserved for readable accents.
#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub background: u32,
    pub card: u32,
    pub panel: u32,
    pub ink: u32,
    pub muted: u32,
    pub accent: u32,
    pub line: u32,
    pub error: u32,
}
impl Palette {
    pub fn new(light: bool) -> Self {
        if light {
            Self {
                background: 0xe7ded5,
                card: 0xfbf6ef,
                panel: 0xf0e6da,
                ink: 0x302722,
                muted: 0x604a3d,
                accent: 0x87391d,
                line: 0xcdbcaa,
                error: 0xa12f27,
            }
        } else {
            Self {
                background: 0x191716,
                card: 0x24201d,
                panel: 0x302922,
                ink: 0xf5eadc,
                muted: 0xcfb7a2,
                accent: 0xf0a574,
                line: 0x6b5143,
                error: 0xffaa88,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blend(foreground: u32, background: u32, alpha: f64) -> u32 {
        [16, 8, 0].into_iter().fold(0, |rgb, shift| {
            let fg = f64::from((foreground >> shift) & 255);
            let bg = f64::from((background >> shift) & 255);
            rgb | (((fg * alpha + bg * (1.0 - alpha)).round() as u32) << shift)
        })
    }
    fn luminance(rgb: u32) -> f64 {
        [16, 8, 0]
            .into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(shift, weight)| {
                let value = f64::from((rgb >> shift) & 255) / 255.0;
                weight
                    * if value <= 0.04045 {
                        value / 12.92
                    } else {
                        ((value + 0.055) / 1.055).powf(2.4)
                    }
            })
            .sum()
    }
    fn contrast(foreground: u32, background: u32) -> f64 {
        let a = luminance(foreground);
        let b = luminance(background);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn contrast_calculation_matches_black_white_reference() {
        assert!((contrast(0, 0xffffff) - 21.0).abs() < 0.001);
        assert_eq!(contrast(0x123456, 0x123456), 1.0);
        assert_eq!(blend(0xffffff, 0, 0.2), 0x333333);
    }

    #[test]
    fn both_themes_keep_small_text_readable_on_composited_surfaces() {
        for light in [true, false] {
            let p = Palette::new(light);
            let sidebar = blend(p.background, p.card, 0.85);
            let active = blend(p.accent, p.background, 0.18);
            let selected = blend(p.accent, sidebar, 0.15);
            let search_card = blend(p.accent, p.card, 0.2);
            let search_panel = blend(p.accent, p.panel, 0.2);
            let mut minimum = f64::INFINITY;
            for (label, fg, backgrounds) in [
                (
                    "body",
                    p.ink,
                    vec![
                        p.background,
                        p.card,
                        p.panel,
                        sidebar,
                        selected,
                        search_card,
                        search_panel,
                        blend(p.line, p.background, 0.7),
                    ],
                ),
                (
                    "muted/quote",
                    p.muted,
                    vec![
                        p.background,
                        p.card,
                        p.panel,
                        sidebar,
                        search_card,
                        search_panel,
                    ],
                ),
                (
                    "copper/code",
                    p.accent,
                    vec![p.card, p.panel, active, selected, search_card, search_panel],
                ),
                ("badge", blend(p.accent, p.card, 0.85), vec![p.card]),
                ("error", p.error, vec![p.card]),
            ] {
                for bg in backgrounds {
                    let ratio = contrast(fg, bg);
                    assert!(
                        ratio >= 4.5,
                        "light={light}, {label}: #{fg:06x} on #{bg:06x} = {ratio:.2}:1"
                    );
                    minimum = minimum.min(ratio);
                }
            }
            println!(
                "{} minimum tested text contrast: {minimum:.2}:1",
                if light { "Light" } else { "Dark" }
            );
        }
    }
}
