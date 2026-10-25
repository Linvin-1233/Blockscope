/// Calculate dimensions fitting inside max bounds while preserving aspect ratio.
pub(crate) fn fit_dimensions(orig_w: u32, orig_h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if orig_w == 0 || orig_h == 0 || max_w == 0 || max_h == 0 {
        return (0, 0);
    }
    if orig_w <= max_w && orig_h <= max_h {
        return (orig_w, orig_h);
    }

    let scale_x = f64::from(max_w) / f64::from(orig_w);
    let scale_y = f64::from(max_h) / f64::from(orig_h);
    let scale = scale_x.min(scale_y);

    let fit_w = (f64::from(orig_w) * scale).round().max(1.0) as u32;
    let fit_h = (f64::from(orig_h) * scale).round().max(1.0) as u32;

    (fit_w.min(max_w), fit_h.min(max_h))
}

/// Resize an RGBA image buffer to target dimensions using area-weighted box sampling.
pub(crate) fn resize_rgba(input: &[u8], in_w: u32, in_h: u32, out_w: u32, out_h: u32) -> Vec<u8> {
    if in_w == 0 || in_h == 0 || out_w == 0 || out_h == 0 {
        return Vec::new();
    }
    if in_w == out_w && in_h == out_h {
        return input.to_vec();
    }

    let mut output = vec![0u8; (u64::from(out_w) * u64::from(out_h) * 4) as usize];
    let x_ratio = f64::from(in_w) / f64::from(out_w);
    let y_ratio = f64::from(in_h) / f64::from(out_h);

    for out_y in 0..out_h {
        let src_y_start = (f64::from(out_y) * y_ratio).floor() as u32;
        let src_y_end = ((f64::from(out_y + 1) * y_ratio).ceil() as u32)
            .min(in_h)
            .max(src_y_start + 1);

        for out_x in 0..out_w {
            let src_x_start = (f64::from(out_x) * x_ratio).floor() as u32;
            let src_x_end = ((f64::from(out_x + 1) * x_ratio).ceil() as u32)
                .min(in_w)
                .max(src_x_start + 1);

            let mut r_sum = 0.0f64;
            let mut g_sum = 0.0f64;
            let mut b_sum = 0.0f64;
            let mut a_sum = 0.0f64;
            let mut count = 0.0f64;

            for sy in src_y_start..src_y_end {
                for sx in src_x_start..src_x_end {
                    let idx = ((u64::from(sy) * u64::from(in_w) + u64::from(sx)) * 4) as usize;
                    if idx + 3 < input.len() {
                        let a = f64::from(input[idx + 3]) / 255.0;
                        r_sum += f64::from(input[idx]) * a;
                        g_sum += f64::from(input[idx + 1]) * a;
                        b_sum += f64::from(input[idx + 2]) * a;
                        a_sum += a;
                        count += 1.0;
                    }
                }
            }

            let out_idx = ((u64::from(out_y) * u64::from(out_w) + u64::from(out_x)) * 4) as usize;
            if count > 0.0 && a_sum > 0.0 {
                let final_a = (a_sum / count).clamp(0.0, 1.0);
                output[out_idx] = (r_sum / a_sum).clamp(0.0, 255.0).round() as u8;
                output[out_idx + 1] = (g_sum / a_sum).clamp(0.0, 255.0).round() as u8;
                output[out_idx + 2] = (b_sum / a_sum).clamp(0.0, 255.0).round() as u8;
                output[out_idx + 3] = (final_a * 255.0).round() as u8;
            } else {
                output[out_idx] = 0;
                output[out_idx + 1] = 0;
                output[out_idx + 2] = 0;
                output[out_idx + 3] = 0;
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_dimensions_preserves_aspect_ratio() {
        assert_eq!(fit_dimensions(1024, 1024, 80, 40), (40, 40));
        assert_eq!(fit_dimensions(800, 400, 80, 80), (80, 40));
        assert_eq!(fit_dimensions(400, 800, 80, 80), (40, 80));
        assert_eq!(fit_dimensions(50, 50, 100, 100), (50, 50));
    }

    #[test]
    fn resize_rgba_scales_correctly() {
        // 2x2 image: red, green, blue, white
        let input = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let resized = resize_rgba(&input, 2, 2, 1, 1);
        assert_eq!(resized.len(), 4);
        // Average of colors with equal alpha
        assert_eq!(resized[3], 255);
    }
}
