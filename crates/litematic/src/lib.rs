//! Litematic parsing and canonical scene construction.

mod container;
mod data;
mod error;
mod limits;
mod scene_builder;

pub use container::{LitematicFile, LitematicMetadata, RawRegion};
pub use data::RegionData;
pub use error::{LitematicError, LitematicErrorCode};
pub use limits::LitematicLimits;
pub use litematic_nbt::DecodeLimits;

pub const CRATE_NAME: &str = "litematic-core";
