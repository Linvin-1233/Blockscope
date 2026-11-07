use crate::RenderOptions;
use litematic_mesh::{MeshPage, Quad};

/// One quad uploaded as instance data. The shader expands it into two
/// triangles, so the CPU never builds the six expanded vertices.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct GpuQuad {
    /// Page-local corner positions.
    pub(super) positions: [[f32; 3]; 4],
    /// Q12 texture coordinates, decoded in the shader.
    pub(super) uvs: [[i16; 2]; 4],
    /// Page origin relative to the frame origin.
    pub(super) page_offset: [f32; 3],
    pub(super) color: [u8; 4],
    pub(super) ao: [u8; 4],
    pub(super) shade: f32,
    pub(super) texture: u16,
    /// Low two bits are the render layer; the high six bits are the packed
    /// axis-aligned normal used for per-view GPU backface culling.
    pub(super) flags: u8,
    pub(super) _pad: u8,
}

impl GpuQuad {
    pub(super) fn layer(self) -> u8 {
        self.flags & 0x3
    }

    pub(super) fn is_backface(self, forward: [f64; 3]) -> bool {
        let normal = decode_normal((self.flags >> 2) & 0x3f);
        is_backface(normal, forward)
    }
}

const ATTRIBUTES: [wgpu::VertexAttribute; 14] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x3,
    2 => Float32x3,
    3 => Float32x3,
    4 => Sint16x2,
    5 => Sint16x2,
    6 => Sint16x2,
    7 => Sint16x2,
    8 => Float32x3,
    9 => Unorm8x4,
    10 => Uint8x4,
    11 => Float32,
    12 => Uint16,
    13 => Uint8
];

pub(super) fn layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<GpuQuad>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

/// Encode the visible quads of one page as instance data.
///
/// Backface culling stays on the CPU as a single dot product per quad. The
/// shader performs the camera transform; this pass only rejects quads the
/// camera cannot see and packs the surviving attributes. Translucent quads are
/// left to the existing CPU-projected path.
pub(super) fn encode_page(
    page: &MeshPage,
    forward: Option<[f64; 3]>,
    options: &RenderOptions,
    frame_origin: [i32; 3],
    out: &mut Vec<GpuQuad>,
) -> u64 {
    let page_offset = page_offset(page.origin, frame_origin);
    let mut culled = 0u64;

    for quad in &page.quads {
        if quad.layer == litematic_mesh::RenderLayer::Translucent {
            continue;
        }
        if let Some(forward) = forward
            && is_backface(quad.normal, forward)
        {
            culled += 1;
            continue;
        }
        out.push(encode_quad(quad, page_offset, options));
    }

    culled
}

/// Match the CPU projection cull rule: `dot(normal, forward) >= 0` is hidden.
pub(super) fn is_backface(normal: [f32; 3], forward: [f64; 3]) -> bool {
    let normal = [
        f64::from(normal[0]),
        f64::from(normal[1]),
        f64::from(normal[2]),
    ];
    normal[0] * forward[0] + normal[1] * forward[1] + normal[2] * forward[2] >= 0.0
}

fn encode_quad(quad: &Quad, page_offset: [f32; 3], options: &RenderOptions) -> GpuQuad {
    let ao = match options.lighting {
        crate::LightingProfile::FlatV1 => [3u8; 4],
        crate::LightingProfile::SimpleAoV1 => quad.ao,
    };

    GpuQuad {
        positions: std::array::from_fn(|index| quad.vertices[index].position),
        uvs: std::array::from_fn(|index| quad.vertices[index].uv),
        page_offset,
        color: quad.color,
        ao,
        shade: options.lighting.face_shade(quad.normal),
        texture: quad.texture,
        flags: (quad.layer.index() as u8) | (normal_code(quad.normal) << 2),
        _pad: 0,
    }
}

/// Pack an axis-aligned normal into six bits: two bits per axis, where 0 is
/// -1, 1 is 0, and 2 is +1. Cross quads use axis-aligned normals as well.
fn normal_code(normal: [f32; 3]) -> u8 {
    let mut code = 0u8;
    for (axis, value) in normal.iter().enumerate() {
        let encoded = if *value > 0.5 {
            2
        } else if *value < -0.5 {
            0
        } else {
            1
        };
        code |= encoded << (axis * 2);
    }
    code
}

fn decode_normal(code: u8) -> [f32; 3] {
    std::array::from_fn(|axis| f32::from((code >> (axis * 2)) & 0x3) - 1.0)
}

fn page_offset(page_origin: [i32; 3], frame_origin: [i32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| {
        (i64::from(page_origin[axis]) - i64::from(frame_origin[axis])) as f32
    })
}

#[cfg(test)]
mod tests {
    use super::{GpuQuad, encode_page, is_backface};
    use crate::{LightingProfile, RenderOptions};
    use litematic_mesh::{MeshPage, Quad, RenderLayer, Vertex};

    fn quad(normal: [f32; 3]) -> Quad {
        Quad {
            vertices: [
                Vertex {
                    position: [0.0, 0.0, 0.0],
                    uv: [0, 0],
                },
                Vertex {
                    position: [1.0, 0.0, 0.0],
                    uv: [4096, 0],
                },
                Vertex {
                    position: [1.0, 1.0, 0.0],
                    uv: [4096, 4096],
                },
                Vertex {
                    position: [0.0, 1.0, 0.0],
                    uv: [0, 4096],
                },
            ],
            normal,
            color: [10, 20, 30, 255],
            ao: [3, 2, 1, 0],
            texture: 7,
            layer: RenderLayer::Cutout,
            shade: true,
        }
    }

    #[test]
    fn instance_stays_compact() {
        assert!(
            std::mem::size_of::<GpuQuad>() <= 96,
            "GpuQuad must stay at or below 96 bytes"
        );
        assert_eq!(std::mem::align_of::<GpuQuad>(), 4);
    }

    #[test]
    fn instance_layout_matches_the_record() {
        let layout = super::layout();
        assert_eq!(layout.array_stride, 92);
        assert_eq!(layout.step_mode, wgpu::VertexStepMode::Instance);
        let offsets: Vec<u64> = layout.attributes.iter().map(|attr| attr.offset).collect();
        assert_eq!(
            offsets,
            vec![0, 12, 24, 36, 48, 52, 56, 60, 64, 76, 80, 84, 88, 90]
        );
    }

    #[test]
    fn backface_test_matches_the_projection_rule() {
        let forward = [0.0, 0.0, -1.0];
        assert!(is_backface([0.0, 0.0, -1.0], forward));
        assert!(!is_backface([0.0, 0.0, 1.0], forward));
        assert!(is_backface([0.0, 0.0, 0.0], forward));
    }

    #[test]
    fn encode_page_culls_and_preserves_attributes() {
        let page = MeshPage {
            origin: [16, 0, -16],
            quads: vec![quad([0.0, 0.0, 1.0]), quad([0.0, 0.0, -1.0])],
        };
        let options = RenderOptions::default();
        let mut encoded = Vec::new();

        let culled = encode_page(
            &page,
            Some([0.0, 0.0, -1.0]),
            &options,
            [0, 0, 0],
            &mut encoded,
        );

        assert_eq!(culled, 1);
        assert_eq!(encoded.len(), 1);
        let quad = &encoded[0];
        assert_eq!(quad.page_offset, [16.0, 0.0, -16.0]);
        assert_eq!(quad.positions[1], [1.0, 0.0, 0.0]);
        assert_eq!(quad.uvs[2], [4096, 4096]);
        assert_eq!(quad.color, [10, 20, 30, 255]);
        assert_eq!(quad.ao, [3, 2, 1, 0]);
        assert_eq!(quad.texture, 7);
        assert_eq!(quad.layer(), RenderLayer::Cutout.index() as u8);
        assert!(quad.shade >= 0.55 && quad.shade <= 1.0);
    }

    #[test]
    fn flat_lighting_overrides_ao() {
        let page = MeshPage {
            origin: [0, 0, 0],
            quads: vec![quad([0.0, 0.0, 1.0])],
        };
        let options = RenderOptions {
            lighting: LightingProfile::FlatV1,
            ..Default::default()
        };
        let mut encoded = Vec::new();

        encode_page(
            &page,
            Some([0.0, 0.0, -1.0]),
            &options,
            [0, 0, 0],
            &mut encoded,
        );

        assert_eq!(encoded[0].ao, [3, 3, 3, 3]);
        assert_eq!(encoded[0].shade, 1.0);
    }
}
