//! Deterministic color-block mesh construction.

mod builder;
mod catalog;
mod color;
mod error;
mod export;
mod greedy;
mod mesh;
mod model;
mod page;
mod shape;
mod view;
mod voxel;

pub use builder::{
    build_mesh, build_mesh_parallel, build_mesh_parallel_with_options, build_mesh_streaming,
    build_mesh_streaming_with_options, build_mesh_with_options,
};
pub use catalog::{
    BlockCatalog, CatalogError, CatalogErrorCode, ColorBlockCatalog, ColorSnapshot, ResolvedBlock,
};
pub use color::{RenderLayer, Rgba};
pub use error::{MeshError, MeshErrorCode};
pub use export::{write_obj, write_ply};
pub use mesh::{
    AO_LEVELS, Mesh, MeshBuildOptions, MeshLimits, MeshPage, MeshStatistics, Quad, UV_SCALE,
    Vertex, ambient_occlusion_level, decode_uv, encode_uv,
};
pub use model::{
    BlockModel, FaceDirection, ModelBox, ModelQuad, NO_TEXTURE, empty_model, face_uv,
    full_cube_model,
};
pub use view::{ResolvedBlockRef, SceneBlockView};

pub const CRATE_NAME: &str = "litematic-mesh";
