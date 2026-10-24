//! CPU color-block rasterizer and PNG output.

mod camera;
mod error;
mod options;
mod png;
mod rasterizer;

pub use camera::Camera;
pub use error::{RenderError, RenderErrorCode};
pub use options::{Background, LightingProfile, RenderOptions};
pub use png::write_png;
pub use rasterizer::{RenderOutput, RenderStatistics, render_mesh};

pub const CRATE_NAME: &str = "litematic-render";
