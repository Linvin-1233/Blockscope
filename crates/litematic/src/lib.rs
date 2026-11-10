//! Litematic parsing and canonical scene construction.

mod container;
mod data;
mod error;
mod limits;
mod scene_builder;

pub use blockscope_nbt::DecodeLimits;
pub use container::{LitematicFile, LitematicMetadata, RawRegion};
pub use data::RegionData;
pub use error::{LitematicError, LitematicErrorCode};
pub use limits::LitematicLimits;

pub const CRATE_NAME: &str = "blockscope-core";
