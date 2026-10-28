//! CPU color-block rasterizer and PNG output.

mod camera;
mod error;
mod options;
mod png;
mod rasterizer;
pub mod terminal;

pub use camera::Camera;
pub use error::{RenderError, RenderErrorCode};
pub use litematic_material::TextureFilter;
pub use options::{Background, LightingProfile, RenderOptions};
pub use png::write_png;
pub use rasterizer::{
    RenderOutput, RenderStatistics, StreamingRenderer, TileProgress, render_mesh,
    render_mesh_with_progress, render_mesh_with_textures, render_mesh_with_textures_and_progress,
    will_stream,
};
pub use terminal::{
    TerminalOptions, TerminalProtocol, detect_terminal_protocol, detect_terminal_size,
    encode_halfblocks, encode_iterm2, encode_iterm2_png, encode_kitty, encode_kitty_png,
    encode_sixel, encode_terminal, render_to_terminal,
};

pub const CRATE_NAME: &str = "litematic-render";
