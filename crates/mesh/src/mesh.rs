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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

/// How non-grid vertex positions are handled during mesh packing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PositionPacking {
    /// Keep fractional positions in the page float table.
    #[default]
    Exact,
    /// Round positions to a 1/1440 block grid, which covers common model
    /// denominators such as 1/9, 1/5, and 1/16.
    Quantize,
}

/// Position storage for a mesh quad.
///
/// Model and cube geometry is always a multiple of 1/16 block, so it fits the
/// grid form exactly. Fluid surfaces and rotated model elements keep f32
/// positions in `MeshPage::float_positions` unless quantization is enabled.
#[derive(Debug, Clone, PartialEq)]
pub enum QuadPositions {
    /// Four page-local positions in `1 / scale` block units.
    Grid {
        scale: u16,
        positions: [[i16; 3]; 4],
    },
    /// Index into `MeshPage::float_positions`.
    Float(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuadNormal {
    /// One of the six axis directions, stored as signed unit components.
    Axis([i8; 3]),
    /// Index into `MeshPage::float_normals`.
    Float(u32),
}

/// A renderable quad with its vertices and uniform material and lighting attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct Quad {
    pub positions: QuadPositions,
    pub normal: QuadNormal,
    pub uvs: [[i16; 2]; 4],
    pub color: [u8; 4],
    pub ao: [u8; 4],
    pub texture: u16,
    pub layer: RenderLayer,
    pub shade: bool,
}

impl Quad {
    /// Pack one quad, appending float positions or normals when the vertices
    /// are not on the 1/16 grid or the normal is not axis-aligned.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        vertices: [Vertex; 4],
        normal: [f32; 3],
        color: [u8; 4],
        ao: [u8; 4],
        texture: u16,
        layer: RenderLayer,
        shade: bool,
        float_positions: &mut Vec<[[f32; 3]; 4]>,
        float_normals: &mut Vec<[f32; 3]>,
    ) -> Self {
        Self::new_with_packing(
            vertices,
            normal,
            color,
            ao,
            texture,
            layer,
            shade,
            float_positions,
            float_normals,
            PositionPacking::Exact,
        )
    }

    /// Pack one quad using the requested position precision.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_packing(
        vertices: [Vertex; 4],
        normal: [f32; 3],
        color: [u8; 4],
        ao: [u8; 4],
        texture: u16,
        layer: RenderLayer,
        shade: bool,
        float_positions: &mut Vec<[[f32; 3]; 4]>,
        float_normals: &mut Vec<[f32; 3]>,
        packing: PositionPacking,
    ) -> Self {
        let positions = match grid_positions(&vertices, packing) {
            Some((scale, positions)) => QuadPositions::Grid { scale, positions },
            None => {
                let index =
                    u32::try_from(float_positions.len()).expect("float position index fits u32");
                float_positions.push(vertices.map(|vertex| vertex.position));
                QuadPositions::Float(index)
            }
        };
        let normal = match axis_normal(normal) {
            Some(axis) => QuadNormal::Axis(axis),
            None => {
                let index =
                    u32::try_from(float_normals.len()).expect("float normal index fits u32");
                float_normals.push(normal);
                QuadNormal::Float(index)
            }
        };
        Self {
            positions,
            normal,
            uvs: vertices.map(|vertex| vertex.uv),
            color,
            ao,
            texture,
            layer,
            shade,
        }
    }

    /// Page-local vertex positions reconstructed exactly.
    pub fn positions(&self, page: &MeshPage) -> [[f32; 3]; 4] {
        self.positions_with(&page.float_positions)
    }

    /// Page-local vertex positions with an explicit float table.
    pub fn positions_with(&self, float_positions: &[[[f32; 3]; 4]]) -> [[f32; 3]; 4] {
        match self.positions {
            QuadPositions::Grid { scale, positions } => {
                positions.map(|position| position.map(|value| f32::from(value) / f32::from(scale)))
            }
            QuadPositions::Float(index) => float_positions[index as usize],
        }
    }

    pub fn uvs(&self) -> [[i16; 2]; 4] {
        self.uvs
    }

    /// Unit normal reconstructed exactly for axis directions.
    pub fn normal(&self, page: &MeshPage) -> [f32; 3] {
        self.normal_with(&page.float_normals)
    }

    pub fn normal_with(&self, float_normals: &[[f32; 3]]) -> [f32; 3] {
        match self.normal {
            QuadNormal::Axis(axis) => axis.map(f32::from),
            QuadNormal::Float(index) => float_normals[index as usize],
        }
    }

    /// Ambient-occlusion factor for vertex `index` in the float domain.
    pub fn ambient_occlusion(&self, index: usize) -> f32 {
        ambient_occlusion_level(self.ao[index])
    }
}

fn axis_normal(normal: [f32; 3]) -> Option<[i8; 3]> {
    let mut axis = [0i8; 3];
    for (component, packed) in normal.iter().zip(&mut axis) {
        *packed = if *component == 0.0 {
            // Equality canonicalizes -0.0 to +0.0. Rendering is unchanged and
            // keeping the sign would push many axis normals into the float table.
            0
        } else if *component == 1.0 {
            1
        } else if *component == -1.0 {
            -1
        } else {
            return None;
        };
    }
    Some(axis)
}

fn grid_positions(
    vertices: &[Vertex; 4],
    packing: PositionPacking,
) -> Option<(u16, [[i16; 3]; 4])> {
    match packing {
        PositionPacking::Exact => pack_positions(vertices, 16, false),
        // 1440 is the least common multiple of the common model denominators
        // (2, 3, 4, 5, 8, 9, 16), so vanilla fluid and plant fractions stay
        // exact. Arbitrary rotations land within 1/2880 block.
        PositionPacking::Quantize => pack_positions(vertices, 1440, true),
    }
}

/// Pack page-local positions at `scale`, either exactly or by rounding.
fn pack_positions(
    vertices: &[Vertex; 4],
    scale: u16,
    quantize: bool,
) -> Option<(u16, [[i16; 3]; 4])> {
    let mut positions = [[0i16; 3]; 4];
    for (vertex, position) in vertices.iter().zip(&mut positions) {
        for (axis, slot) in position.iter_mut().enumerate() {
            let scaled = vertex.position[axis] * f32::from(scale);
            if !scaled.is_finite() {
                return None;
            }
            let packed = if quantize {
                let rounded = scaled.round();
                if rounded < f32::from(i16::MIN) || rounded > f32::from(i16::MAX) {
                    return None;
                }
                rounded as i16
            } else {
                if scaled.fract() != 0.0
                    || scaled < f32::from(i16::MIN)
                    || scaled > f32::from(i16::MAX)
                {
                    return None;
                }
                let packed = scaled as i16;
                // Division must reproduce the original f32 value. Signed zero
                // canonicalizes to +0.0, which does not change rendering.
                if f32::from(packed) / f32::from(scale) != vertex.position[axis] {
                    return None;
                }
                packed
            };
            *slot = packed;
        }
    }
    Some((scale, positions))
}

/// A 16x16x16 mesh page. Vertex positions are local to `origin`.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshPage {
    pub origin: [i32; 3],
    pub quads: Vec<Quad>,
    /// Positions for quads whose vertices are not a multiple of 1/16 block.
    pub float_positions: Vec<[[f32; 3]; 4]>,
    /// Normals for quads that are not one of the six axis directions.
    pub float_normals: Vec<[f32; 3]>,
}

impl MeshPage {
    /// Page-local vertex positions for `quad`, reconstructed exactly.
    pub fn quad_positions(&self, quad: &Quad) -> [[f32; 3]; 4] {
        quad.positions(self)
    }

    /// Unit normal for `quad`, reconstructed exactly for axis directions.
    pub fn quad_normal(&self, quad: &Quad) -> [f32; 3] {
        quad.normal(self)
    }
}

/// A deterministic mesh split into pages.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub pages: Vec<MeshPage>,
    pub statistics: MeshStatistics,
}

impl Mesh {
    /// Estimated heap bytes held by page vectors and quad storage.
    pub fn heap_bytes(&self) -> u64 {
        let mut total = self.pages.capacity() as u64 * std::mem::size_of::<MeshPage>() as u64;
        for page in &self.pages {
            total += page.quads.capacity() as u64 * std::mem::size_of::<Quad>() as u64;
            total += page.float_positions.capacity() as u64
                * std::mem::size_of::<[[f32; 3]; 4]>() as u64;
            total += page.float_normals.capacity() as u64 * std::mem::size_of::<[f32; 3]>() as u64;
        }
        total
    }
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

/// Optional mesh construction behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MeshBuildOptions {
    /// Merge coplanar unit faces with identical material, lighting, and UV
    /// mapping into larger quads. Disabled by default because it only pays off
    /// on builds with contiguous surfaces.
    pub greedy: bool,
    /// Round positions to a 1/1440 block grid instead of storing fractional
    /// fluid and plant vertices in the page float table. This lowers mesh
    /// memory but can move a small number of edge pixels.
    pub quantize_positions: bool,
}

impl MeshBuildOptions {
    /// Enable conservative greedy merging.
    pub const GREEDY: Self = Self {
        greedy: true,
        quantize_positions: false,
    };
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

#[cfg(test)]
mod tests {
    use super::{PositionPacking, Quad, QuadNormal, QuadPositions, Vertex, grid_positions};

    #[test]
    fn quad_stays_compact() {
        // Grid geometry is the dominant case. Keep accidental field growth
        // visible; the old f32-vertex form was 88 bytes.
        assert_eq!(std::mem::size_of::<QuadPositions>(), 28);
        assert_eq!(std::mem::size_of::<QuadNormal>(), 8);
        assert_eq!(std::mem::size_of::<Quad>(), 64);
    }

    #[test]
    fn quantized_positions_use_the_common_denominator_scale() {
        let vertices = [
            Vertex {
                position: [0.2, 0.8, 0.0],
                uv: [0, 0],
            },
            Vertex {
                position: [1.0, 0.8, 0.0],
                uv: [0, 0],
            },
            Vertex {
                position: [1.0, 1.0, 0.0],
                uv: [0, 0],
            },
            Vertex {
                position: [0.2, 1.0, 0.0],
                uv: [0, 0],
            },
        ];

        assert!(grid_positions(&vertices, PositionPacking::Exact).is_none());
        let (scale, positions) = grid_positions(&vertices, PositionPacking::Quantize).unwrap();
        assert_eq!(scale, 1440);
        assert_eq!(positions[0], [288, 1152, 0]);
        assert_eq!(positions[1], [1440, 1152, 0]);
    }
}
