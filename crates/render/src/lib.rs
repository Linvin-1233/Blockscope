//! Color-block rasterizer and PNG output.

mod backend;
mod camera;
mod error;
mod options;
mod png;
mod rasterizer;
pub mod terminal;

pub use backend::RenderBackend;
pub use camera::Camera;
pub use error::{RenderError, RenderErrorCode};
pub use litematic_material::TextureFilter;
pub use options::{Background, LightingProfile, RenderOptions};
pub use png::write_png;
#[cfg(feature = "gpu")]
pub use rasterizer::GpuFrameSession;
#[cfg(feature = "gpu")]
pub use rasterizer::GpuRenderSession;
#[cfg(feature = "gpu")]
pub use rasterizer::gpu_working_set_bytes;
pub use rasterizer::{
    BackendDecision, BackendPolicyInput, GpuAdapterKind, GpuStageTimings, RenderOutput,
    RenderStatistics, StreamingRenderer, TileProgress, decide_backend, render_mesh,
    render_mesh_with_progress, render_mesh_with_textures, render_mesh_with_textures_and_progress,
    select_backend, will_stream,
};
pub use terminal::{
    TerminalOptions, TerminalProtocol, cell_footprint, detect_terminal_pixels,
    detect_terminal_protocol, detect_terminal_size, encode_halfblocks, encode_iterm2,
    encode_iterm2_png, encode_kitty, encode_kitty_png, encode_sixel, encode_terminal,
    fit_terminal_cells, render_to_terminal,
};

pub const CRATE_NAME: &str = "litematic-render";
