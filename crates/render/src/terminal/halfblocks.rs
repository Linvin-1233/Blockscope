use super::detect::detect_terminal_size;
use super::options::TerminalOptions;
use super::scale::{fit_dimensions, resize_rgba};
use crate::RenderOutput;

/// Default terminal column width if auto-detection returns none.
const DEFAULT_TERMINAL_COLS: u32 = 80;
/// Default terminal row height if auto-detection returns none.
const DEFAULT_TERMINAL_ROWS: u32 = 40;

/// Render an image into terminal text using Unicode Upper Half Block (▀) and ANSI 24-bit Truecolor.
pub fn encode_halfblocks(output: &RenderOutput, options: &TerminalOptions) -> String {
    if output.width == 0 || output.height == 0 {
        return String::new();
    }

    // Determine target dimensions in character cells
    let detected_size = detect_terminal_size();
    let max_cols = options
        .width
        .or_else(|| detected_size.map(|(c, _)| u32::from(c)))
        .unwrap_or(DEFAULT_TERMINAL_COLS);

    // Leave a small headroom of 2 rows if using detected terminal height
    let max_rows = options
        .height
        .or_else(|| detected_size.map(|(_, r)| u32::from(r).saturating_sub(2).max(10)))
        .unwrap_or(DEFAULT_TERMINAL_ROWS);

    // In halfblocks, 1 cell = 1 pixel wide x 2 pixels high (aspect ratio ~1:1 square)
    let max_pixel_w = max_cols;
    let max_pixel_h = max_rows * 2;

    let (target_w, target_h) = if options.preserve_aspect_ratio {
        fit_dimensions(output.width, output.height, max_pixel_w, max_pixel_h)
    } else {
        (max_pixel_w, max_pixel_h)
    };

    let target_w = target_w.max(1);
    let target_h = target_h.max(1);

    let resized_rgba = if target_w != output.width || target_h != output.height {
        resize_rgba(
            &output.rgba,
            output.width,
            output.height,
            target_w,
            target_h,
        )
    } else {
        output.rgba.clone()
    };

    let mut out = String::with_capacity((target_w * (target_h / 2) * 20) as usize);

    // Process 2 vertical pixels per text row
    let mut y = 0;
    while y < target_h {
        let mut current_fg: Option<[u8; 3]> = None;
        let mut current_bg: Option<[u8; 3]> = None;

        for x in 0..target_w {
            let top_idx = ((u64::from(y) * u64::from(target_w) + u64::from(x)) * 4) as usize;
            let top_rgba = &resized_rgba[top_idx..top_idx + 4];
            let top_opaque = top_rgba[3] >= 128;
            let top_rgb = [top_rgba[0], top_rgba[1], top_rgba[2]];

            let bot_idx = ((u64::from(y + 1) * u64::from(target_w) + u64::from(x)) * 4) as usize;
            let (bot_opaque, bot_rgb) = if y + 1 < target_h && bot_idx + 3 < resized_rgba.len() {
                let bot_rgba = &resized_rgba[bot_idx..bot_idx + 4];
                (bot_rgba[3] >= 128, [bot_rgba[0], bot_rgba[1], bot_rgba[2]])
            } else {
                (false, [0, 0, 0])
            };

            match (top_opaque, bot_opaque) {
                (true, true) => {
                    if current_fg != Some(top_rgb) {
                        out.push_str(&format!(
                            "\x1b[38;2;{};{};{}m",
                            top_rgb[0], top_rgb[1], top_rgb[2]
                        ));
                        current_fg = Some(top_rgb);
                    }
                    if current_bg != Some(bot_rgb) {
                        out.push_str(&format!(
                            "\x1b[48;2;{};{};{}m",
                            bot_rgb[0], bot_rgb[1], bot_rgb[2]
                        ));
                        current_bg = Some(bot_rgb);
                    }
                    out.push('▀');
                }
                (true, false) => {
                    if current_bg.is_some() {
                        out.push_str("\x1b[49m"); // Reset background
                        current_bg = None;
                    }
                    if current_fg != Some(top_rgb) {
                        out.push_str(&format!(
                            "\x1b[38;2;{};{};{}m",
                            top_rgb[0], top_rgb[1], top_rgb[2]
                        ));
                        current_fg = Some(top_rgb);
                    }
                    out.push('▀');
                }
                (false, true) => {
                    if current_bg.is_some() {
                        out.push_str("\x1b[49m"); // Reset background
                        current_bg = None;
                    }
                    if current_fg != Some(bot_rgb) {
                        out.push_str(&format!(
                            "\x1b[38;2;{};{};{}m",
                            bot_rgb[0], bot_rgb[1], bot_rgb[2]
                        ));
                        current_fg = Some(bot_rgb);
                    }
                    out.push('▄'); // Lower half block
                }
                (false, false) => {
                    if current_fg.is_some() || current_bg.is_some() {
                        out.push_str("\x1b[0m");
                        current_fg = None;
                        current_bg = None;
                    }
                    out.push(' ');
                }
            }
        }

        // Reset color state at line end
        out.push_str("\x1b[0m\n");
        y += 2;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RenderStatistics;

    #[test]
    fn halfblocks_renders_two_by_two_pixels() {
        let output = RenderOutput {
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, // Row 0: Red, Green
                0, 0, 255, 255, 255, 255, 255, 255, // Row 1: Blue, White
            ],
            statistics: RenderStatistics::default(),
        };
        let options = TerminalOptions {
            width: Some(2),
            height: Some(1),
            preserve_aspect_ratio: false,
            ..Default::default()
        };
        let result = encode_halfblocks(&output, &options);

        assert!(result.contains("▀"));
        assert!(result.contains("\x1b[38;2;255;0;0m")); // Top-left Red
        assert!(result.contains("\x1b[48;2;0;0;255m")); // Bottom-left Blue
        assert!(result.ends_with("\x1b[0m\n"));
    }

    #[test]
    fn halfblocks_handles_transparency() {
        let output = RenderOutput {
            width: 2,
            height: 2,
            rgba: vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            statistics: RenderStatistics::default(),
        };
        let options = TerminalOptions {
            width: Some(2),
            height: Some(1),
            preserve_aspect_ratio: false,
            ..Default::default()
        };
        let result = encode_halfblocks(&output, &options);
        assert!(result.contains("  ")); // Spaces for transparent pixels
    }
}
