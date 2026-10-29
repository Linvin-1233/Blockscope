use super::super::project::{QuadRender, Triangle, vertex_color};
use litematic_mesh::RenderLayer;

pub(super) const FLAG_CUTOUT: u8 = 1;

/// Interleaved vertex consumed by the offscreen pipeline.
///
/// The struct is deliberately compact: the largest chunk holds 600k quads, so
/// every extra field costs megabytes of staging memory. The UVs stay in the
/// mesh's Q12 format and are converted after interpolation by the shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct GpuVertex {
    pub(super) position: [f32; 3],
    pub(super) color: [f32; 4],
    pub(super) uv: [i16; 2],
    pub(super) texture: u16,
    pub(super) flags: u8,
    _pad: u8,
}

const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x4,
    2 => Sint16x2,
    3 => Uint16,
    4 => Uint8
];

pub(super) fn layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<GpuVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

/// Maps projected screen coordinates into the clip-space range wgpu expects.
pub(super) struct Viewport {
    width: f64,
    height: f64,
    depth_min: f64,
    depth_span: f64,
}

impl Viewport {
    pub(super) fn new(width: u32, height: u32, depth_min: f64, depth_max: f64) -> Self {
        Self {
            width: f64::from(width),
            height: f64::from(height),
            depth_min,
            depth_span: (depth_max - depth_min).max(1e-6),
        }
    }

    fn position(&self, point: [f64; 3]) -> [f32; 3] {
        [
            (point[0] / self.width * 2.0 - 1.0) as f32,
            (1.0 - point[1] / self.height * 2.0) as f32,
            ((point[2] - self.depth_min) / self.depth_span).clamp(0.0, 0.999_999) as f32,
        ]
    }
}

pub(super) fn push_quad(vertices: &mut Vec<GpuVertex>, quad: &QuadRender, viewport: &Viewport) {
    let flags = if quad.layer == RenderLayer::Cutout {
        FLAG_CUTOUT
    } else {
        0
    };
    for index in [0usize, 1, 2, 0, 2, 3] {
        vertices.push(GpuVertex {
            position: viewport.position(quad.points[index]),
            color: vertex_color(quad.color, quad.shade, quad.ao[index]),
            uv: quad.uvs[index],
            texture: quad.texture,
            flags,
            _pad: 0,
        });
    }
}

pub(super) fn push_triangle(
    vertices: &mut Vec<GpuVertex>,
    triangle: &Triangle,
    viewport: &Viewport,
) {
    let colors = triangle.colors();
    let points = triangle.points_f64();
    for index in 0..3 {
        vertices.push(GpuVertex {
            position: viewport.position(points[index]),
            color: colors[index],
            uv: triangle.uvs[index],
            texture: triangle.texture,
            flags: 0,
            _pad: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{GpuVertex, Viewport, layout};

    #[test]
    fn vertex_record_stays_compact() {
        assert_eq!(std::mem::size_of::<GpuVertex>(), 36);
        assert_eq!(layout().array_stride, 36);
        assert_eq!(layout().attributes.len(), 5);
    }

    #[test]
    fn maps_screen_top_left_to_clip_space() {
        let viewport = Viewport::new(100, 50, -2.0, 2.0);
        let top_left = viewport.position([0.0, 0.0, -2.0]);
        assert_eq!(top_left, [-1.0, 1.0, 0.0]);
        let bottom_right = viewport.position([100.0, 50.0, 2.0]);
        assert_eq!(bottom_right[0], 1.0);
        assert_eq!(bottom_right[1], -1.0);
        assert!(bottom_right[2] < 1.0);
    }
}
