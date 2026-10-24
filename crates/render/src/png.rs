use crate::{RenderError, RenderOutput};
use std::io::Write;

/// Encode a rendered RGBA buffer as a PNG.
pub fn write_png<W: Write>(output: &RenderOutput, writer: W) -> Result<(), RenderError> {
    let mut encoder = png::Encoder::new(writer, output.width, output.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder
        .write_header()
        .map_err(|error| RenderError::png(error.to_string()))?;
    writer
        .write_image_data(&output.rgba)
        .map_err(|error| RenderError::png(error.to_string()))?;
    writer
        .finish()
        .map_err(|error| RenderError::png(error.to_string()))?;

    Ok(())
}
