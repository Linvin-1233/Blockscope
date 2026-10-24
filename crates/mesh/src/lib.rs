//! Deterministic color-block mesh construction.

mod builder;
mod catalog;
mod color;
mod error;
mod export;
mod mesh;
mod model;
mod view;

pub use builder::build_mesh;
pub use catalog::{BlockCatalog, CatalogError, CatalogErrorCode, ColorBlockCatalog, ResolvedBlock};
pub use color::{RenderLayer, Rgba};
pub use error::{MeshError, MeshErrorCode};
pub use export::{write_obj, write_ply};
pub use mesh::{Mesh, MeshLimits, MeshPage, MeshStatistics, Quad, Vertex};
pub use model::{BlockModel, FaceDirection, ModelQuad, empty_model, full_cube_model};
pub use view::{ResolvedBlockRef, SceneBlockView};

pub const CRATE_NAME: &str = "litematic-mesh";
