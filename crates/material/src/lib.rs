//! Minecraft resource-pack blockstate, model, and texture loading.

mod blockstate;
mod catalog;
mod chest;
mod error;
mod json;
mod model;
mod source;
mod texture;
mod tint;
mod util;

pub use catalog::{
    MaterialDiagnostics, MaterialEntry, MaterialInfo, MaterialKind, MaterialReport, MaterialSet,
    MaterialSnapshot,
};
pub use error::{MaterialError, MaterialErrorCode};
pub use source::{AssetSource, asset_path};
pub use texture::{Texture, TextureFilter, TextureSet};

pub const CRATE_NAME: &str = "litematic-material";
