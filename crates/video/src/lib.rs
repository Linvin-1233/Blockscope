//! Animation timeline and ffmpeg encoding for Blockscope video export.
//!
//! This crate deliberately does not depend on the renderer, scene, or mesh
//! crates. It accepts raw RGBA frames so the timeline and encoder can be tested
//! without a GPU or a litematic fixture.

mod encoder;
mod error;
mod ffmpeg;
mod timeline;

pub use encoder::{VideoEncoder, VideoSettings};
pub use error::{VideoError, VideoErrorCode};
pub use ffmpeg::{Encoder, Ffmpeg, MINIMUM_VERSION, Quality, ResolvedEncoder, VideoFormat};
pub use timeline::{Animation, Easing, FramePlan, Keyframe};

/// Default service and CLI caps; `--force-limits` raises them.
pub const MAX_FRAMES: u32 = 1800;
pub const MAX_DURATION_SECONDS: f64 = 60.0;

pub const CRATE_NAME: &str = "blockscope-video";
