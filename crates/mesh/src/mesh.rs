use crate::color::RenderLayer;
use serde::{Deserialize, Serialize};

/// A mesh vertex in page-local coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [u8; 4],
    pub uv: [f32; 2],
    pub ao: f32,
}

/// A renderable quad with its material layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Quad {
    pub vertices: [Vertex; 4],
    pub layer: RenderLayer,
}

/// A 16x16x16 mesh page. Vertex positions are local to `origin`.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshPage {
    pub origin: [i32; 3],
    pub quads: Vec<Quad>,
}

/// A deterministic mesh split into pages.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub pages: Vec<MeshPage>,
    pub statistics: MeshStatistics,
}

/// Mesh counters used by the CLI and golden tests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshStatistics {
    pub pages: u64,
    pub quads: u64,
    pub vertices: u64,
    pub blocks_visited: u64,
    pub blocks_rendered: u64,
    pub unknown_blocks: u64,
    pub opaque_quads: u64,
    pub cutout_quads: u64,
    pub translucent_quads: u64,
}

/// Hard limits for a single mesh build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshLimits {
    pub max_pages: usize,
    pub max_quads: usize,
    pub max_vertices: usize,
}

impl Default for MeshLimits {
    fn default() -> Self {
        Self {
            max_pages: 65_536,
            max_quads: 5_000_000,
            max_vertices: 20_000_000,
        }
    }
}

impl MeshLimits {
    pub fn small() -> Self {
        Self {
            max_pages: 2,
            max_quads: 20,
            max_vertices: 80,
        }
    }
}

impl MeshStatistics {
    pub(crate) fn add_quad(&mut self, layer: RenderLayer) {
        self.quads += 1;
        self.vertices += 4;
        match layer {
            RenderLayer::Opaque => self.opaque_quads += 1,
            RenderLayer::Cutout => self.cutout_quads += 1,
            RenderLayer::Translucent => self.translucent_quads += 1,
        }
    }
}
