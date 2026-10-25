//! Canonical scene representation shared by the parser and renderer.

mod block_entity;
mod block_state;
mod bounds;
mod diagnostic;
mod error;
mod hash;
mod lod;
mod manifest;
mod metadata;
mod packed;
mod region;
mod scene;
mod statistics;

pub use block_entity::BlockEntity;
pub use block_state::BlockStateKey;
pub use bounds::Bounds;
pub use diagnostic::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
pub use error::{SceneError, SceneErrorCode};
pub use hash::{Sha256Digest, sha256};
pub use lod::{aggregate, aggregate_with_threads};
pub use manifest::{
    MANIFEST_SCHEMA_VERSION, Manifest, ManifestBounds, ManifestDiagnostic, ManifestMetadata,
    ManifestRegion, ManifestStatistics, ParserInfo,
};
pub use metadata::SceneMetadata;
pub use packed::PackedIndices;
pub use region::Region;
pub use scene::Scene;
pub use statistics::BlockStatistics;

pub const CRATE_NAME: &str = "litematic-scene";
