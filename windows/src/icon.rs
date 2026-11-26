//! Notification-area artwork: an outlined water drop rendered as alpha
//! coverage, so the adapter can tint it for light or dark taskbars at any DPI.

/// Samples per pixel along each axis.
const SAMPLES: u32 = 4;

/// A drop: the convex hull of a circle and a tip point above its centre.
#[derive(Debug, Clone, Copy)]
struct Drop {
    x: f64,
    y: f64,
    radius: f64,
    tip: f64,
}

impl Drop {
    fn fit(size: f64) -> Self {
        let margin = size * 0.06;
        let radius = size * 0.31;
        Self {
            x: size / 2.0,
            y: size - margin - radius,
            radius,
            tip: margin,
        }
    }

    /// The same shape moved inward by `distance` on every side.
    fn inset(self, distance: f64) -> Self {
        let sine = self.radius / (self.y - self.tip);
        Self {
            radius: self.radius - distance,
            tip: distance.mul_add(1.0 / sine, self.tip),
            ..self
        }
    }

    fn contains(self, x: f64, y: f64) -> bool {
        let (dx, dy) = (x - self.x, y - self.y);
        if dx.hypot(dy) <= self.radius {
            return true;
        }
        // Between the tip and the tangent points, the hull is a triangle.
        let height = self.y - self.tip;
        let ratio = self.radius / height;
        let tangent = self.radius.mul_add(-ratio, self.y);
        // Above the tip the half-width below is negative, so nothing matches.
        if y > tangent {
            return false;
        }
        let half_width = self.radius * ratio.mul_add(-ratio, 1.0).sqrt();
        dx.abs() <= half_width * (y - self.tip) / (tangent - self.tip)
    }
}

/// Alpha coverage from 0 to 255 for a `size` × `size` outlined drop, in rows
/// from the top.
#[must_use]
pub fn drop_alpha(size: u32) -> Vec<u8> {
    let scale = f64::from(size);
    let outer = Drop::fit(scale);
    let inner = outer.inset((scale / 12.0).max(1.25));
    let step = 1.0 / f64::from(SAMPLES);
    let mut alpha = Vec::with_capacity((size * size) as usize);
    for row in 0..size {
        for column in 0..size {
            let mut covered = 0u32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = f64::from(sx).mul_add(step, f64::from(column)) + step / 2.0;
                    let y = f64::from(sy).mul_add(step, f64::from(row)) + step / 2.0;
                    covered += u32::from(outer.contains(x, y) && !inner.contains(x, y));
                }
            }
            let level = covered * 255 / (SAMPLES * SAMPLES);
            alpha.push(u8::try_from(level).unwrap_or(u8::MAX));
        }
    }
    alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(alpha: &[u8], size: u32, x: u32, y: u32) -> u8 {
        alpha[usize::try_from(y * size + x).unwrap_or(usize::MAX)]
    }

    /// The pixel row or column of an icon of `size` containing a coordinate.
    fn pixel(position: f64, size: u32) -> u32 {
        (0..size)
            .take_while(|p| f64::from(*p) <= position)
            .last()
            .unwrap_or(0)
    }

    #[test]
    fn drop_is_an_outline_symmetric_about_its_axis() {
        for size in [16, 20, 24, 32, 48, 64] {
            let alpha = drop_alpha(size);
            assert_eq!(alpha.len(), usize::try_from(size * size).unwrap_or(0));
            for y in 0..size {
                for x in 0..size {
                    let mirrored = at(&alpha, size, size - 1 - x, y);
                    assert!(
                        at(&alpha, size, x, y).abs_diff(mirrored) <= 16,
                        "{size} {x},{y}"
                    );
                }
            }
            let drop = Drop::fit(f64::from(size));
            let centre = pixel(drop.y, size);
            assert_eq!(at(&alpha, size, size / 2, centre), 0, "hollow at {size}");
            assert_eq!(at(&alpha, size, 0, 0), 0, "corner at {size}");
            let bottom = pixel(drop.y + drop.radius - 0.75, size);
            assert!(at(&alpha, size, size / 2, bottom) > 128, "rim at {size}");
            let tip = pixel(drop.tip + 1.5, size);
            assert!(at(&alpha, size, size / 2, tip) > 0, "tip at {size}");
            assert_eq!(at(&alpha, size, size / 2, 0), 0, "above tip at {size}");
        }
    }

    /// The drop at the 100% and 150% tray sizes. Update these deliberately
    /// when the artwork changes.
    #[test]
    fn tray_sizes_keep_their_silhouette() {
        let small = [
            "                ",
            "       ..       ",
            "      .##.      ",
            "      ####      ",
            "     .#..#.     ",
            "    .##  ##.    ",
            "    +#.  .#+    ",
            "   .#.    .#.   ",
            "   ##      ##   ",
            "   #+      +#   ",
            "   #+      +#   ",
            "   #+      +#   ",
            "   .#+    +#.   ",
            "    +#+..+#+    ",
            "     .####.     ",
            "                ",
        ];
        let large = [
            "                        ",
            "           ..           ",
            "           ##           ",
            "          .##.          ",
            "         .####.         ",
            "         +####+         ",
            "        .##..##.        ",
            "       .##+  +##.       ",
            "       +##    ##+       ",
            "      .##.    .##.      ",
            "      ##+      +##      ",
            "     +##        ##+     ",
            "     ##.        .##     ",
            "    .##          ##.    ",
            "    +#+          +#+    ",
            "    +#+          +#+    ",
            "    .##          ##.    ",
            "    .##.        .##.    ",
            "     +##        ##+     ",
            "     .###.    .###.     ",
            "      .###++++###.      ",
            "       .+######+.       ",
            "         .++++.         ",
            "                        ",
        ];
        assert_eq!(silhouette(16), small);
        assert_eq!(silhouette(24), large);
    }

    /// Coverage as text: blank, faint, half and solid.
    fn silhouette(size: u32) -> Vec<String> {
        drop_alpha(size)
            .chunks(usize::try_from(size).unwrap_or(0))
            .map(|row| {
                row.iter()
                    .map(|alpha| match alpha {
                        0 => ' ',
                        1..=84 => '.',
                        85..=169 => '+',
                        _ => '#',
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn hull_includes_the_circle_and_tapers_to_the_tip() {
        let drop = Drop::fit(100.0);
        assert!(drop.contains(50.0, drop.y));
        assert!(drop.contains(50.0, drop.tip + 1.0));
        assert!(!drop.contains(50.0, drop.tip - 1.0));
        assert!(!drop.contains(40.0, drop.tip + 2.0));
        assert!(!drop.contains(50.0 + drop.radius + 1.0, drop.y));
        let inner = drop.inset(5.0);
        assert!((inner.radius - (drop.radius - 5.0)).abs() < 1e-9);
        assert!(inner.tip > drop.tip + 5.0);
    }
}
