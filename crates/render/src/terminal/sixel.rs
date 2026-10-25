use super::options::TerminalOptions;
use super::quantize::{QuantizedImage, quantize_rgba};
use super::scale::{fit_dimensions, resize_rgba};
use crate::{RenderError, RenderOutput};

/// Encode rendered output into DEC Sixel graphics escape sequences.
pub fn encode_sixel(
    output: &RenderOutput,
    options: &TerminalOptions,
) -> Result<Vec<u8>, RenderError> {
    if output.width == 0 || output.height == 0 {
        return Ok(Vec::new());
    }

    // Determine target pixel dimensions
    let mut target_w = output.width;
    let mut target_h = output.height;

    // Convert cell dimensions to pixel bounds (typical terminal cell is ~8x16 pixels)
    let max_pixel_w = options
        .pixel_width
        .or_else(|| options.width.map(|cols| cols * 8));
    let max_pixel_h = options
        .pixel_height
        .or_else(|| options.height.map(|rows| rows * 16));

    if let (Some(mw), Some(mh)) = (max_pixel_w, max_pixel_h) {
        if options.preserve_aspect_ratio {
            let (fw, fh) = fit_dimensions(output.width, output.height, mw, mh);
            target_w = fw;
            target_h = fh;
        } else {
            target_w = mw;
            target_h = mh;
        }
    } else if let Some(mw) = max_pixel_w
        && mw < target_w
    {
        let scale = f64::from(mw) / f64::from(target_w);
        target_w = mw;
        if options.preserve_aspect_ratio {
            target_h = (f64::from(target_h) * scale).round().max(1.0) as u32;
        }
    } else if let Some(mh) = max_pixel_h
        && mh < target_h
    {
        let scale = f64::from(mh) / f64::from(target_h);
        target_h = mh;
        if options.preserve_aspect_ratio {
            target_w = (f64::from(target_w) * scale).round().max(1.0) as u32;
        }
    }

    let rgba_buffer = if target_w != output.width || target_h != output.height {
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

    let QuantizedImage { palette, pixels } = quantize_rgba(&rgba_buffer);
    let mut out = Vec::with_capacity(rgba_buffer.len() / 2 + 1024);

    // DCS Sixel header: ESC P q (standard DEC sixel)
    out.extend_from_slice(b"\x1bPq");

    // Output palette definitions: #<index>;2;<r%>;<g%>;<b%>
    for (idx, rgb) in palette.iter().enumerate() {
        let r_pct = (u32::from(rgb[0]) * 100 + 127) / 255;
        let g_pct = (u32::from(rgb[1]) * 100 + 127) / 255;
        let b_pct = (u32::from(rgb[2]) * 100 + 127) / 255;
        out.extend_from_slice(format!("#{idx};2;{r_pct};{g_pct};{b_pct}").as_bytes());
    }

    let bands = target_h.div_ceil(6);

    for band in 0..bands {
        let y0 = band * 6;
        let mut first_color_in_band = true;

        for color_idx in 0..palette.len() {
            let color_idx = color_idx as u8;

            // Check if this color is used in this 6-row band
            let mut color_used = false;

            for row in 0..6 {
                let y = y0 + row;
                if y >= target_h {
                    break;
                }
                let row_offset = (u64::from(y) * u64::from(target_w)) as usize;
                for x in 0..target_w as usize {
                    if pixels[row_offset + x] == Some(color_idx) {
                        color_used = true;
                        break;
                    }
                }
                if color_used {
                    break;
                }
            }

            if !color_used {
                continue;
            }

            if !first_color_in_band {
                out.push(b'$'); // Carriage return without line feed
            } else {
                first_color_in_band = false;
            }

            out.extend_from_slice(format!("#{color_idx}").as_bytes());

            // Build 6-bit vertical slice for each column
            let mut line_chars = Vec::with_capacity(target_w as usize);
            for x in 0..target_w {
                let mut bits = 0u8;
                for row in 0..6 {
                    let y = y0 + row;
                    if y < target_h {
                        let idx = (u64::from(y) * u64::from(target_w) + u64::from(x)) as usize;
                        if pixels[idx] == Some(color_idx) {
                            bits |= 1 << row;
                        }
                    }
                }
                line_chars.push(bits + 63); // Sixel value 0..63 mapped to ASCII 63..126
            }

            // Trim trailing empty sixel characters (63 = '?')
            while line_chars.last() == Some(&b'?') {
                line_chars.pop();
            }

            // Write with RLE compression: !<count><char>
            let mut i = 0;
            while i < line_chars.len() {
                let ch = line_chars[i];
                let mut run_len = 1;
                while i + run_len < line_chars.len() && line_chars[i + run_len] == ch {
                    run_len += 1;
                }

                if run_len >= 3 {
                    out.extend_from_slice(format!("!{run_len}{}", ch as char).as_bytes());
                } else {
                    for _ in 0..run_len {
                        out.push(ch);
                    }
                }

                i += run_len;
            }
        }

        out.push(b'-'); // Sixel newline (advance 6 vertical pixels and return to column 0)
    }

    // DCS Terminator: ESC \
    out.extend_from_slice(b"\x1b\\\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RenderStatistics;
    use crate::terminal::quantize::{SIXEL_MAX_COLORS, quantize_rgba};

    #[test]
    fn sixel_encodes_basic_image() {
        let output = RenderOutput {
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
            statistics: RenderStatistics::default(),
        };
        let options = TerminalOptions::default();
        let encoded = encode_sixel(&output, &options).unwrap();
        let s = std::str::from_utf8(&encoded).unwrap();

        assert!(s.starts_with("\x1bPq"));
        assert!(s.contains("#0;2;"));
        assert!(s.ends_with("\x1b\\\n"));
    }

    #[test]
    fn sixel_emits_pixels_with_full_palette() {
        let width = 20;
        let height = 15;
        let mut rgba = Vec::with_capacity(width * height * 4);
        for i in 0..(width * height) {
            // 300 distinct colors; the median cut fills all 256 palette slots.
            rgba.extend_from_slice(&[
                (i % 256) as u8,
                ((i / 256) * 85) as u8,
                ((i * 37) % 256) as u8,
                255,
            ]);
        }

        let output = RenderOutput {
            width: width as u32,
            height: height as u32,
            rgba: rgba.clone(),
            statistics: RenderStatistics::default(),
        };

        let quantized = quantize_rgba(&rgba);
        assert_eq!(quantized.palette.len(), SIXEL_MAX_COLORS);

        let encoded = encode_sixel(&output, &TerminalOptions::default()).unwrap();
        let s = std::str::from_utf8(&encoded).unwrap();

        // 256 '#' are palette definitions; extra ones come from pixel data.
        assert!(
            s.matches('#').count() > SIXEL_MAX_COLORS,
            "sixel must emit pixel data when the palette is full"
        );
    }
}
