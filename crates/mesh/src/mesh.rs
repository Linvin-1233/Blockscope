use crate::color::RenderLayer;
use serde::{Deserialize, Serialize};

/// Fixed-point scale for texture coordinates. Q12 covers [-8, 8), which is
/// well beyond the [-1, 2] range used by vanilla models.
pub const UV_SCALE: f32 = 4096.0;

/// The four ambient-occlusion levels produced by the mesh builder. Keeping the
/// exact expressions (rather than decimal literals) makes the decoded values
/// bit-identical to the previous floating-point path.
pub const AO_LEVELS: [f32; 4] = [
    0.55,
    0.55 + 0.45 * (1.0 - 2.0 / 3.0),
    0.55 + 0.45 * (1.0 - 1.0 / 3.0),
    0.55 + 0.45,
];

/// Decode a Q12 texture coordinate back to its float form.
#[inline]
pub fn decode_uv(uv: [i16; 2]) -> [f32; 2] {
    [f32::from(uv[0]) / UV_SCALE, f32::from(uv[1]) / UV_SCALE]
}

/// Encode texture coordinates as Q12 fixed point.
pub fn encode_uv(uv: [f32; 2]) -> [i16; 2] {
    [encode_uv_component(uv[0]), encode_uv_component(uv[1])]
}

fn encode_uv_component(value: f32) -> i16 {
    let scaled = (value * UV_SCALE).round();
    scaled.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

/// Decode an ambient-occlusion level. Values outside 0..=3 are treated as the
/// fully lit level.
#[inline]
pub fn ambient_occlusion_level(level: u8) -> f32 {
    AO_LEVELS[usize::from(level).min(AO_LEVELS.len() - 1)]
}

/// A mesh vertex in page-local coordinates.
///
/// Texture coordinates use Q12 fixed point. Quad-uniform attributes (normal,
/// color, texture, layer, shade, and ambient-occlusion levels) are stored once
/// on [`Quad`], keeping vertex storage tight at 16 bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [i16; 2],
}

impl Vertex {
    /// Texture coordinates in the float domain.
    pub fn texture_coords(&self) -> [f32; 2] {
        decode_uv(self.uv)
    }
}

/// A renderable quad with its vertices and uniform material and lighting attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct Quad {
    pub vertices: [Vertex; 4],
    pub normal: [f32; 3],
    pub color: [u8; 4],
    pub ao: [u8; 4],
    pub texture: u16,
    pub layer: RenderLayer,
    pub shade: bool,
}

impl Quad {
    /// Ambient-occlusion factor for vertex `index` in the float domain.
    pub fn ambient_occlusion(&self, index: usize) -> f32 {
        ambient_occlusion_level(self.ao[index])
    }
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
