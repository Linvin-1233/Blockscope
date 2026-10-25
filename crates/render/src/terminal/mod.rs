//! Terminal image rendering protocols: Kitty, iTerm2, Sixel, and Halfblocks (ANSI Truecolor).

mod base64;
mod detect;
mod halfblocks;
mod iterm2;
mod kitty;
mod options;
mod protocol;
mod quantize;
mod scale;
mod sixel;

pub use detect::{detect_terminal_protocol, detect_terminal_size, enable_windows_virtual_terminal};
pub use halfblocks::encode_halfblocks;
pub use iterm2::{encode_iterm2, encode_iterm2_png};
pub use kitty::{encode_kitty, encode_kitty_png};
pub use options::TerminalOptions;
pub use protocol::TerminalProtocol;
pub use quantize::quantize_rgba;
pub use sixel::encode_sixel;

use crate::{RenderError, RenderOutput};
use std::io::Write;

/// Encode rendered RGBA output into bytes according to the specified terminal protocol.
pub fn encode_terminal(
    output: &RenderOutput,
    options: &TerminalOptions,
) -> Result<Vec<u8>, RenderError> {
    match options.protocol {
        TerminalProtocol::Kitty => encode_kitty(output, options),
        TerminalProtocol::Iterm2 => encode_iterm2(output, options),
        TerminalProtocol::Sixel => encode_sixel(output, options),
        TerminalProtocol::Halfblocks => Ok(encode_halfblocks(output, options).into_bytes()),
    }
}

/// Render an image directly to a terminal output stream.
///
/// Automatically enables Windows console virtual terminal processing if running on Windows.
pub fn render_to_terminal<W: Write>(
    output: &RenderOutput,
    options: &TerminalOptions,
    writer: &mut W,
) -> Result<(), RenderError> {
    enable_windows_virtual_terminal();
    let bytes = encode_terminal(output, options)?;
    writer
        .write_all(&bytes)
        .map_err(|e| RenderError::png(format!("terminal write error: {e}")))?;
    writer
        .flush()
        .map_err(|e| RenderError::png(format!("terminal flush error: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RenderStatistics;

    fn sample_output() -> RenderOutput {
        RenderOutput {
            width: 4,
            height: 4,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255,
                128, 128, 128, 255, 255, 255, 0, 255, 0, 255, 255, 255, 255, 0, 255, 255, 64, 64,
                64, 255, 192, 192, 192, 255, 32, 32, 32, 255, 0, 0, 0, 0, 255, 0, 0, 128, 0, 255,
                0, 128, 0, 0, 255, 128,
            ],
            statistics: RenderStatistics::default(),
        }
    }

    #[test]
    fn encode_terminal_supports_all_protocols() {
        let output = sample_output();

        for protocol in TerminalProtocol::ALL {
            let options = TerminalOptions::new(*protocol);
            let result = encode_terminal(&output, &options);
            assert!(
                result.is_ok(),
                "protocol {protocol:?} failed to encode: {:?}",
                result.err()
            );
            let bytes = result.unwrap();
            assert!(
                !bytes.is_empty(),
                "protocol {protocol:?} produced empty output"
            );
        }
    }

    #[test]
    fn render_to_terminal_writes_to_stream() {
        let output = sample_output();
        let options = TerminalOptions::new(TerminalProtocol::Halfblocks);
        let mut buffer = Vec::new();
        let res = render_to_terminal(&output, &options, &mut buffer);
        assert!(res.is_ok());
        assert!(!buffer.is_empty());
    }
}
