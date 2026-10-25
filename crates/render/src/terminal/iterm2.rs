use super::base64::encode_base64;
use super::options::TerminalOptions;
use crate::png::write_png;
use crate::{RenderError, RenderOutput};

/// Encode rendered output into iTerm2 Inline Images Protocol (OSC 1337).
pub fn encode_iterm2(
    output: &RenderOutput,
    options: &TerminalOptions,
) -> Result<Vec<u8>, RenderError> {
    let mut png_bytes = Vec::new();
    write_png(output, &mut png_bytes)?;
    Ok(encode_iterm2_png(&png_bytes, options))
}

/// Encode PNG bytes into iTerm2 Inline Images Protocol (OSC 1337).
pub fn encode_iterm2_png(png_bytes: &[u8], options: &TerminalOptions) -> Vec<u8> {
    let base64_data = encode_base64(png_bytes);
    let mut args = format!("inline=1;size={}", png_bytes.len());

    let preserve = if options.preserve_aspect_ratio { 1 } else { 0 };
    args.push_str(&format!(";preserveAspectRatio={preserve}"));

    if let Some(cols) = options.width {
        args.push_str(&format!(";width={cols}"));
    } else {
        args.push_str(";width=auto");
    }

    if let Some(rows) = options.height {
        args.push_str(&format!(";height={rows}"));
    } else {
        args.push_str(";height=auto");
    }

    // Format: ESC ] 1337 ; File = <args> : <base64> BEL \n
    let mut out = Vec::with_capacity(base64_data.len() + args.len() + 32);
    out.extend_from_slice(b"\x1b]1337;File=");
    out.extend_from_slice(args.as_bytes());
    out.push(b':');
    out.extend_from_slice(base64_data.as_bytes());
    out.push(0x07); // BEL terminator
    out.push(b'\n');

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterm2_encoding_structure() {
        let fake_png = b"test-png";
        let options = TerminalOptions::default();
        let encoded = encode_iterm2_png(fake_png, &options);
        let s = std::str::from_utf8(&encoded).unwrap();

        assert!(s.starts_with("\x1b]1337;File=inline=1;size=8;preserveAspectRatio=1"));
        assert!(s.ends_with("\x07\n"));
    }

    #[test]
    fn iterm2_custom_dimensions() {
        let fake_png = b"test";
        let options = TerminalOptions {
            width: Some(100),
            height: Some(50),
            preserve_aspect_ratio: false,
            ..Default::default()
        };
        let encoded = encode_iterm2_png(fake_png, &options);
        let s = std::str::from_utf8(&encoded).unwrap();

        assert!(s.contains("width=100"));
        assert!(s.contains("height=50"));
        assert!(s.contains("preserveAspectRatio=0"));
    }
}
