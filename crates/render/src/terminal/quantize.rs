use std::collections::HashMap;

/// Maximum palette capacity supported by modern Sixel terminals.
pub const SIXEL_MAX_COLORS: usize = 256;

/// Quantized palette and pixel indices.
#[derive(Debug, Clone)]
pub struct QuantizedImage {
    /// Palette of RGB colors (at most 256 entries).
    pub palette: Vec<[u8; 3]>,
    /// Index into `palette` for each pixel, or `None` if transparent.
    pub pixels: Vec<Option<u8>>,
}

#[derive(Clone)]
struct ColorCount {
    rgb: [u8; 3],
    count: u64,
}

#[derive(Clone)]
struct BoundingBox {
    colors: Vec<ColorCount>,
    total_count: u64,
}

impl BoundingBox {
    fn from_colors(colors: Vec<ColorCount>) -> Self {
        let total_count = colors.iter().map(|c| c.count).sum();
        Self {
            colors,
            total_count,
        }
    }

    /// Extents along R, G, B channels.
    fn extents(&self) -> (u8, u8, u8) {
        if self.colors.is_empty() {
            return (0, 0, 0);
        }
        let mut min = [255u8; 3];
        let mut max = [0u8; 3];
        for c in &self.colors {
            for i in 0..3 {
                min[i] = min[i].min(c.rgb[i]);
                max[i] = max[i].max(c.rgb[i]);
            }
        }
        (max[0] - min[0], max[1] - min[1], max[2] - min[2])
    }

    /// Split this bounding box into two along its widest color channel.
    fn split(mut self) -> (BoundingBox, BoundingBox) {
        if self.colors.len() <= 1 {
            return (self, BoundingBox::from_colors(Vec::new()));
        }

        let (er, eg, eb) = self.extents();
        let axis = if er >= eg && er >= eb {
            0
        } else if eg >= er && eg >= eb {
            1
        } else {
            2
        };

        self.colors.sort_by_key(|c| c.rgb[axis]);

        let half_count = self.total_count / 2;
        let mut running = 0u64;
        let mut split_idx = 1;

        for (idx, c) in self.colors.iter().enumerate() {
            running += c.count;
            if running >= half_count && idx > 0 {
                split_idx = idx;
                break;
            }
        }

        let right_colors = self.colors.split_off(split_idx);
        (
            BoundingBox::from_colors(self.colors),
            BoundingBox::from_colors(right_colors),
        )
    }

    /// Weighted average color of all entries in this box.
    fn average_color(&self) -> [u8; 3] {
        if self.colors.is_empty() {
            return [0, 0, 0];
        }
        let mut r = 0.0f64;
        let mut g = 0.0f64;
        let mut b = 0.0f64;
        let total = self.total_count.max(1) as f64;

        for c in &self.colors {
            let weight = c.count as f64;
            r += f64::from(c.rgb[0]) * weight;
            g += f64::from(c.rgb[1]) * weight;
            b += f64::from(c.rgb[2]) * weight;
        }

        [
            (r / total).clamp(0.0, 255.0).round() as u8,
            (g / total).clamp(0.0, 255.0).round() as u8,
            (b / total).clamp(0.0, 255.0).round() as u8,
        ]
    }
}

/// Quantize RGBA pixels to a palette of up to 256 colors.
pub fn quantize_rgba(rgba: &[u8]) -> QuantizedImage {
    let pixel_count = rgba.len() / 4;
    let mut color_histogram: HashMap<[u8; 3], u64> = HashMap::new();

    for chunk in rgba.as_chunks::<4>().0 {
        if chunk[3] >= 128 {
            *color_histogram
                .entry([chunk[0], chunk[1], chunk[2]])
                .or_insert(0) += 1;
        }
    }

    if color_histogram.is_empty() {
        return QuantizedImage {
            palette: vec![[0, 0, 0]],
            pixels: vec![None; pixel_count],
        };
    }

    // Fast path: Exact palette if <= 256 unique colors
    if color_histogram.len() <= SIXEL_MAX_COLORS {
        let mut palette: Vec<[u8; 3]> = color_histogram.keys().copied().collect();
        // Sort for deterministic output
        palette.sort();

        let color_to_idx: HashMap<[u8; 3], u8> = palette
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, i as u8))
            .collect();

        let pixels = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| {
                if chunk[3] >= 128 {
                    color_to_idx.get(&[chunk[0], chunk[1], chunk[2]]).copied()
                } else {
                    None
                }
            })
            .collect();

        return QuantizedImage { palette, pixels };
    }

    // Median Cut algorithm. Sort so the box order and split points do not
    // depend on HashMap iteration order.
    let mut counts: Vec<ColorCount> = color_histogram
        .into_iter()
        .map(|(rgb, count)| ColorCount { rgb, count })
        .collect();
    counts.sort_by_key(|c| c.rgb);

    let mut boxes = vec![BoundingBox::from_colors(counts)];

    while boxes.len() < SIXEL_MAX_COLORS {
        // Find box with greatest spread and more than 1 color
        let mut best_idx = None;
        let mut max_spread = 0u32;

        for (idx, b) in boxes.iter().enumerate() {
            if b.colors.len() > 1 {
                let (er, eg, eb) = b.extents();
                let spread = u32::from(er).max(u32::from(eg)).max(u32::from(eb));
                if spread > max_spread {
                    max_spread = spread;
                    best_idx = Some(idx);
                }
            }
        }

        let Some(idx) = best_idx else {
            break;
        };

        let target = boxes.swap_remove(idx);
        let (left, right) = target.split();
        if !left.colors.is_empty() {
            boxes.push(left);
        }
        if !right.colors.is_empty() {
            boxes.push(right);
        }
    }

    let mut palette: Vec<[u8; 3]> = boxes.iter().map(BoundingBox::average_color).collect();
    palette.sort();

    // Map each pixel to closest palette color
    let pixels = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| {
            if chunk[3] >= 128 {
                let rgb = [chunk[0], chunk[1], chunk[2]];
                Some(nearest_color(&rgb, &palette))
            } else {
                None
            }
        })
        .collect();

    QuantizedImage { palette, pixels }
}

/// Find index of the closest color in palette by Euclidean distance.
fn nearest_color(rgb: &[u8; 3], palette: &[[u8; 3]]) -> u8 {
    let mut best_idx = 0;
    let mut min_dist = u64::MAX;

    for (idx, p) in palette.iter().enumerate() {
        let dr = i64::from(rgb[0]) - i64::from(p[0]);
        let dg = i64::from(rgb[1]) - i64::from(p[1]);
        let db = i64::from(rgb[2]) - i64::from(p[2]);
        let dist = (dr * dr + dg * dg + db * db) as u64;
        if dist < min_dist {
            min_dist = dist;
            best_idx = idx;
            if dist == 0 {
                break;
            }
        }
    }

    best_idx as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_exact_palette_for_small_colorset() {
        let rgba = vec![
            255, 0, 0, 255, // red
            0, 255, 0, 255, // green
            0, 0, 0, 0, // transparent
        ];
        let quantized = quantize_rgba(&rgba);
        assert_eq!(quantized.palette.len(), 2);
        assert_eq!(quantized.pixels[0], Some(1)); // [255, 0, 0] sorted
        assert_eq!(quantized.pixels[1], Some(0)); // [0, 255, 0] sorted
        assert_eq!(quantized.pixels[2], None);
    }

    #[test]
    fn quantize_caps_at_max_colors() {
        // Generate 300 different colors
        let mut rgba = Vec::new();
        for i in 0..300u32 {
            rgba.push((i % 256) as u8);
            rgba.push(((i * 7) % 256) as u8);
            rgba.push(((i * 13) % 256) as u8);
            rgba.push(255);
        }
        let quantized = quantize_rgba(&rgba);
        assert!(quantized.palette.len() <= SIXEL_MAX_COLORS);
        assert_eq!(quantized.pixels.len(), 300);
    }

    #[test]
    fn quantize_is_deterministic_for_median_cut() {
        let mut rgba = Vec::new();
        for i in 0..300u32 {
            rgba.extend_from_slice(&[
                (i % 256) as u8,
                ((i / 256) * 85) as u8,
                ((i * 37) % 256) as u8,
                255,
            ]);
        }

        let first = quantize_rgba(&rgba);
        assert_eq!(first.palette.len(), SIXEL_MAX_COLORS);

        for _ in 0..8 {
            let next = quantize_rgba(&rgba);
            assert_eq!(first.palette, next.palette);
            assert_eq!(first.pixels, next.pixels);
        }
    }
}
