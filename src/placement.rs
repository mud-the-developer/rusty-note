//! Keep a restored sticker on its nearest monitor's work area.
//! Independent implementation using native work-area bounds, not reference-app code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
impl Placement {
    pub fn fit(self, left: i32, top: i32, right: i32, bottom: i32) -> Self {
        let work_width = right.saturating_sub(left).max(1);
        let work_height = bottom.saturating_sub(top).max(1);
        let width = self.width.clamp(1, work_width);
        let height = self.height.clamp(1, work_height);
        Self {
            x: self.x.clamp(left, right.saturating_sub(width).max(left)),
            y: self.y.clamp(top, bottom.saturating_sub(height).max(top)),
            width,
            height,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_saved_window_fits_small_work_area_without_panicking() {
        assert_eq!(
            Placement {
                x: 32000,
                y: -32000,
                width: 1600,
                height: 1600
            }
            .fit(0, 0, 800, 560),
            Placement {
                x: 0,
                y: 0,
                width: 800,
                height: 560
            }
        );
    }
    #[test]
    fn restore_supports_negative_monitor_coordinates_and_taskbar() {
        let placed = Placement {
            x: -2500,
            y: 900,
            width: 360,
            height: 500,
        }
        .fit(-1920, 0, 0, 1040);
        assert_eq!(placed.x, -1920);
        assert_eq!(placed.y, 540);
        assert_eq!(placed.width, 360);
    }
    #[test]
    fn valid_placement_is_unchanged() {
        let placed = Placement {
            x: 80,
            y: 70,
            width: 360,
            height: 500,
        };
        assert_eq!(placed.fit(0, 0, 1920, 1040), placed);
    }
}
