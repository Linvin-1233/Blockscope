use super::base64::encode_base64;
use super::options::TerminalOptions;
use crate::png::write_png;
use crate::{RenderError, RenderOutput};

/// Max base64 payload chunk size mandated by Kitty Graphics Protocol.
const KITTY_CHUNK_SIZE: usize = 4096;

/// Encode rendered output into Kitty Graphics Protocol escape sequences.
pub fn encode_kitty(
    output: &RenderOutput,
    options: &TerminalOptions,
) -> Result<Vec<u8>, RenderError> {
    let mut png_bytes = Vec::new();
    write_png(output, &mut png_bytes)?;
    Ok(encode_kitty_png(&png_bytes, options))
}

/// Encode PNG bytes into Kitty Graphics Protocol escape sequences.
pub fn encode_kitty_png(png_bytes: &[u8], options: &TerminalOptions) -> Vec<u8> {
    let base64_data = encode_base64(png_bytes);
    let mut out = Vec::with_capacity(base64_data.len() + 128);

    let chunks: Vec<&str> = base64_data
        .as_bytes()
        .chunks(KITTY_CHUNK_SIZE)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
        .collect();

    if chunks.is_empty() {
        return out;
    }

    // Build control arguments for the first chunk
    let mut control = String::from("a=T,f=100");
    if options.quiet {
        control.push_str(",q=2");
    }
    if let Some(cols) = options.width {
        control.push_str(&format!(",c={cols}"));
    }
    if let Some(rows) = options.height {
        control.push_str(&format!(",r={rows}"));
    }

    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i == chunks.len() - 1;
        let m = if is_last { 0 } else { 1 };

        if i == 0 {
            // First chunk contains format and placement controls
            out.extend_from_slice(format!("\x1b_G{control},m={m};{chunk}\x1b\\").as_bytes());
        } else {
            // Subsequent chunks only need the continuation flag
            out.extend_from_slice(format!("\x1b_Gm={m};{chunk}\x1b\\").as_bytes());
        }
    }

    out.push(b'\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kitty_single_chunk_encoding() {
        let small_png = b"\x89PNG\r\n\x1a\nfake-png-bytes";
        let options = TerminalOptions::default();
        let encoded = encode_kitty_png(small_png, &options);
        let s = std::str::from_utf8(&encoded).unwrap();

        assert!(s.starts_with("\x1b_Ga=T,f=100,q=2,m=0;"));
        assert!(s.ends_with("\x1b\\\n"));
    }

    #[test]
    fn kitty_chunked_encoding_for_large_payload() {
        let large_png = vec![0x42u8; 4000]; // Base64 will exceed 4096 bytes
        let options = TerminalOptions::default();
        let encoded = encode_kitty_png(&large_png, &options);
        let s = std::str::from_utf8(&encoded).unwrap();

        assert!(s.contains("m=1;"));
        assert!(s.contains("m=0;"));
    }

    #[test]
    fn kitty_includes_dimension_controls() {
        let small_png = b"test";
        let options = TerminalOptions {
            width: Some(80),
            height: Some(40),
            ..Default::default()
        };
        let encoded = encode_kitty_png(small_png, &options);
        let s = std::str::from_utf8(&encoded).unwrap();

        assert!(s.contains("c=80"));
        assert!(s.contains("r=40"));
    }
}
