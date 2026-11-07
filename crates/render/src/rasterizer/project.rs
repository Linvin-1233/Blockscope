use super::RenderStatistics;
use super::geometry::{bounds, dot3};
use crate::camera::Camera;
use crate::{LightingProfile, RenderOptions, RenderPrecision};
use litematic_mesh::{Mesh, MeshPage, NO_TEXTURE, RenderLayer, ambient_occlusion_level, decode_uv};

pub(super) fn project_mesh_pages(
    mesh: &Mesh,
    camera: &Camera,
    options: &RenderOptions,
    statistics: &mut RenderStatistics,
    threads: usize,
) -> (QuadTable, Vec<Triangle>) {
    project_pages(&mesh.pages, camera, options, statistics, threads)
}

/// Project an arbitrary page slice and return a table matching the requested
/// precision.
pub(super) fn project_pages(
    pages: &[MeshPage],
    camera: &Camera,
    options: &RenderOptions,
    statistics: &mut RenderStatistics,
    threads: usize,
) -> (QuadTable, Vec<Triangle>) {
    match options.precision {
        RenderPrecision::Exact => {
            let (quads, translucent) =
                project_pages_typed::<[f64; 3]>(pages, camera, options, statistics, threads);
            (QuadTable::Exact(quads), translucent)
        }
        RenderPrecision::Fast => {
            let (quads, translucent) =
                project_pages_typed::<[f32; 3]>(pages, camera, options, statistics, threads);
            (QuadTable::Fast(quads), translucent)
        }
    }
}

fn project_pages_typed<P: QuadPoint>(
    pages: &[MeshPage],
    camera: &Camera,
    options: &RenderOptions,
    statistics: &mut RenderStatistics,
    threads: usize,
) -> (Vec<QuadRender<P>>, Vec<Triangle>) {
    if threads <= 1 || pages.len() <= 1 {
        let mut quads = Vec::new();
        let mut translucent = Vec::new();
        for page in pages {
            push_page_quads::<P>(
                page,
                camera,
                options,
                statistics,
                &mut quads,
                &mut translucent,
            );
        }
        return (quads, translucent);
    }

    let workers = threads.clamp(1, pages.len());
    let chunk_size = pages.len().div_ceil(workers);
    let chunk_results: Vec<(Vec<QuadRender<P>>, Vec<Triangle>, RenderStatistics)> =
        std::thread::scope(|scope| {
            let handles: Vec<_> = pages
                .chunks(chunk_size)
                .map(|chunk| {
                    scope.spawn(move || {
                        let mut local_quads = Vec::new();
                        let mut local_translucent = Vec::new();
                        let mut local_stats = RenderStatistics::default();
                        for page in chunk {
                            push_page_quads::<P>(
                                page,
                                camera,
                                options,
                                &mut local_stats,
                                &mut local_quads,
                                &mut local_translucent,
                            );
                        }
                        (local_quads, local_translucent, local_stats)
                    })
                })
                .collect();

            handles
                .into_iter()
                .map(|handle| handle.join().expect("page projection worker panicked"))
                .collect()
        });

    let total_quads: usize = chunk_results.iter().map(|(q, _, _)| q.len()).sum();
    let total_translucent: usize = chunk_results.iter().map(|(_, t, _)| t.len()).sum();
    let mut quads = Vec::with_capacity(total_quads);
    let mut translucent = Vec::with_capacity(total_translucent);

    for (chunk_quads, chunk_translucent, chunk_stats) in chunk_results {
        quads.extend(chunk_quads);
        translucent.extend(chunk_translucent);
        merge_page_statistics(statistics, &chunk_stats);
    }

    (quads, translucent)
}

pub(super) fn merge_page_statistics(target: &mut RenderStatistics, source: &RenderStatistics) {
    target.culled_backfaces += source.culled_backfaces;
    target.triangles += source.triangles;
    target.opaque_triangles += source.opaque_triangles;
    target.cutout_triangles += source.cutout_triangles;
    target.translucent_triangles += source.translucent_triangles;
    target.textured_triangles += source.textured_triangles;
}

pub(super) fn push_page_quads<P: QuadPoint>(
    page: &MeshPage,
    camera: &Camera,
    options: &RenderOptions,
    statistics: &mut RenderStatistics,
    quads: &mut Vec<QuadRender<P>>,
    translucent: &mut Vec<Triangle>,
) {
    for quad in &page.quads {
        let normal = quad.normal;
        if dot3(
            [
                f64::from(normal[0]),
                f64::from(normal[1]),
                f64::from(normal[2]),
            ],
            camera.forward(),
        ) >= 0.0
        {
            statistics.culled_backfaces += 1;
            continue;
        }

        let mut projected = [[0.0f64; 3]; 4];
        for (index, vertex) in quad.vertices.iter().enumerate() {
            let world = [
                f64::from(page.origin[0]) + f64::from(vertex.position[0]),
                f64::from(page.origin[1]) + f64::from(vertex.position[1]),
                f64::from(page.origin[2]) + f64::from(vertex.position[2]),
            ];
            projected[index] = camera.project(world);
        }

        let shade = if quad.shade {
            options.lighting.face_shade(quad.normal)
        } else {
            1.0
        };
        let color = quad.color;
        let ao = match options.lighting {
            LightingProfile::FlatV1 => [3u8; 4],
            LightingProfile::SimpleAoV1 => quad.ao,
        };
        let uvs = [
            quad.vertices[0].uv,
            quad.vertices[1].uv,
            quad.vertices[2].uv,
            quad.vertices[3].uv,
        ];
        let texture = quad.texture;
        let layer = quad.layer;
        let mut keep = false;

        for indices in [[0usize, 1, 2], [0, 2, 3]] {
            let points = [
                projected[indices[0]],
                projected[indices[1]],
                projected[indices[2]],
            ];
            let bbox = bounds(&points);
            if bbox[0] > bbox[2] || bbox[1] > bbox[3] {
                continue;
            }
            keep = true;

            match layer {
                RenderLayer::Opaque => statistics.opaque_triangles += 1,
                RenderLayer::Cutout => statistics.cutout_triangles += 1,
                RenderLayer::Translucent => statistics.translucent_triangles += 1,
            }
            if texture != NO_TEXTURE {
                statistics.textured_triangles += 1;
            }
            statistics.triangles += 1;

            if layer == RenderLayer::Translucent {
                let depth_key = (points[0][2] + points[1][2] + points[2][2]) / 3.0;
                translucent.push(Triangle {
                    // Screen coordinates are bounded by the output size, so f32
                    // keeps subpixel precision while halving the point storage.
                    points: points.map(|point| point.map(|value| value as f32)),
                    color,
                    shade,
                    ao: [ao[indices[0]], ao[indices[1]], ao[indices[2]]],
                    uvs: [uvs[indices[0]], uvs[indices[1]], uvs[indices[2]]],
                    texture,
                    depth_key,
                });
            }
        }

        if keep && layer != RenderLayer::Translucent {
            quads.push(QuadRender {
                points: projected.map(P::from_f64),
                color,
                shade,
                ao,
                uvs,
                texture,
                layer,
            });
        }
    }
}

/// Project only translucent quads into the retained triangle list.
///
/// The instanced opaque/cutout path no longer projects those quads on the CPU.
/// This companion keeps the translucent path unchanged without paying for
/// opaque projection.
#[cfg(feature = "gpu")]
pub(super) fn push_page_translucent(
    page: &MeshPage,
    camera: &Camera,
    options: &RenderOptions,
    statistics: &mut RenderStatistics,
    translucent: &mut Vec<Triangle>,
) {
    for quad in &page.quads {
        if quad.layer != RenderLayer::Translucent {
            continue;
        }

        let normal = quad.normal;
        if dot3(
            [
                f64::from(normal[0]),
                f64::from(normal[1]),
                f64::from(normal[2]),
            ],
            camera.forward(),
        ) >= 0.0
        {
            statistics.culled_backfaces += 1;
            continue;
        }

        let mut projected = [[0.0f64; 3]; 4];
        for (index, vertex) in quad.vertices.iter().enumerate() {
            let world = [
                f64::from(page.origin[0]) + f64::from(vertex.position[0]),
                f64::from(page.origin[1]) + f64::from(vertex.position[1]),
                f64::from(page.origin[2]) + f64::from(vertex.position[2]),
            ];
            projected[index] = camera.project(world);
        }

        let shade = if quad.shade {
            options.lighting.face_shade(quad.normal)
        } else {
            1.0
        };
        let color = quad.color;
        let ao = match options.lighting {
            LightingProfile::FlatV1 => [3u8; 4],
            LightingProfile::SimpleAoV1 => quad.ao,
        };
        let uvs = [
            quad.vertices[0].uv,
            quad.vertices[1].uv,
            quad.vertices[2].uv,
            quad.vertices[3].uv,
        ];

        for indices in [[0usize, 1, 2], [0, 2, 3]] {
            let points = [
                projected[indices[0]],
                projected[indices[1]],
                projected[indices[2]],
            ];
            let bbox = bounds(&points);
            if bbox[0] > bbox[2] || bbox[1] > bbox[3] {
                continue;
            }

            statistics.translucent_triangles += 1;
            if quad.texture != NO_TEXTURE {
                statistics.textured_triangles += 1;
            }
            statistics.triangles += 1;
            let depth_key = (points[0][2] + points[1][2] + points[2][2]) / 3.0;
            translucent.push(Triangle {
                points: points.map(|point| point.map(|value| value as f32)),
                color,
                shade,
                ao: [ao[indices[0]], ao[indices[1]], ao[indices[2]]],
                uvs: [uvs[indices[0]], uvs[indices[1]], uvs[indices[2]]],
                texture: quad.texture,
                depth_key,
            });
        }
    }
}

/// A projected screen point stored in the requested precision.
pub(super) trait QuadPoint: Copy + Send {
    fn from_f64(value: [f64; 3]) -> Self;
    fn to_f64(self) -> [f64; 3];
}

impl QuadPoint for [f64; 3] {
    #[inline]
    fn from_f64(value: [f64; 3]) -> Self {
        value
    }

    #[inline]
    fn to_f64(self) -> [f64; 3] {
        self
    }
}

impl QuadPoint for [f32; 3] {
    #[inline]
    fn from_f64(value: [f64; 3]) -> Self {
        value.map(|component| component as f32)
    }

    #[inline]
    fn to_f64(self) -> [f64; 3] {
        self.map(f64::from)
    }
}

/// An opaque or cutout quad, projected once and split into triangles per tile.
/// Colors and texture coordinates are stored compactly; the exact float values
/// are reconstructed when the quad is rasterized into a tile.
#[derive(Debug, Clone)]
pub(super) struct QuadRender<P> {
    pub(super) points: [P; 4],
    pub(super) color: [u8; 4],
    pub(super) shade: f32,
    pub(super) ao: [u8; 4],
    pub(super) uvs: [[i16; 2]; 4],
    pub(super) texture: u16,
    pub(super) layer: RenderLayer,
}

/// One projected table whose point storage follows `RenderPrecision`.
pub(super) enum QuadTable {
    Exact(Vec<QuadRender<[f64; 3]>>),
    Fast(Vec<QuadRender<[f32; 3]>>),
}

/// A translucent triangle. Only translucent geometry needs sorting, so this is
/// kept separate from the quad table.
#[derive(Debug, Clone)]
pub(super) struct Triangle {
    /// Projected screen coordinates. Bounded by the output size, so f32 is
    /// enough for subpixel precision and costs half of the f64 representation.
    pub(super) points: [[f32; 3]; 3],
    color: [u8; 4],
    shade: f32,
    ao: [u8; 3],
    pub(super) uvs: [[i16; 2]; 3],
    pub(super) texture: u16,
    /// The average depth is kept as f64 so the sort order is independent of the
    /// compact point representation.
    pub(super) depth_key: f64,
}

/// Reconstruct the per-vertex color exactly as the previous float path did.
pub(super) fn vertex_color(color: [u8; 4], shade: f32, ao: u8) -> [f32; 4] {
    let shade = f64::from(shade);
    let ao = f64::from(ambient_occlusion_level(ao));
    [
        (f64::from(color[0]) / 255.0 * shade * ao) as f32,
        (f64::from(color[1]) / 255.0 * shade * ao) as f32,
        (f64::from(color[2]) / 255.0 * shade * ao) as f32,
        f32::from(color[3]) / 255.0,
    ]
}

impl Triangle {
    /// Fixed record size used by the translucent spill files.
    pub(super) const RECORD_BYTES: usize = 76;

    /// Serialize one spill record, including its destination tile.
    pub(super) fn encode_record(&self, tile_index: u32) -> [u8; Self::RECORD_BYTES] {
        let mut out = [0u8; Self::RECORD_BYTES];
        out[0..4].copy_from_slice(&tile_index.to_le_bytes());
        let mut offset = 4;
        for point in &self.points {
            for value in point {
                out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                offset += 4;
            }
        }
        out[offset..offset + 4].copy_from_slice(&self.color);
        offset += 4;
        out[offset..offset + 4].copy_from_slice(&self.shade.to_le_bytes());
        offset += 4;
        out[offset..offset + 3].copy_from_slice(&self.ao);
        offset += 3;
        for uv in &self.uvs {
            for value in uv {
                out[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
                offset += 2;
            }
        }
        out[offset..offset + 2].copy_from_slice(&self.texture.to_le_bytes());
        offset += 2;
        out[offset..offset + 8].copy_from_slice(&self.depth_key.to_le_bytes());
        offset += 8;
        debug_assert_eq!(offset, 73);
        out
    }

    /// Parse a spill record. The remaining bytes are padding.
    pub(super) fn decode_record(bytes: &[u8]) -> (u32, Self) {
        assert_eq!(bytes.len(), Self::RECORD_BYTES);
        let tile_index = u32::from_le_bytes(bytes[0..4].try_into().expect("tile index"));
        let mut offset = 4;
        let mut points = [[0.0f32; 3]; 3];
        for point in &mut points {
            for value in point {
                *value = f32::from_le_bytes(
                    bytes[offset..offset + 4]
                        .try_into()
                        .expect("point component"),
                );
                offset += 4;
            }
        }
        let color = bytes[offset..offset + 4].try_into().expect("color");
        offset += 4;
        let shade = f32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("shade"));
        offset += 4;
        let ao = bytes[offset..offset + 3].try_into().expect("ao");
        offset += 3;
        let mut uvs = [[0i16; 2]; 3];
        for uv in &mut uvs {
            for value in uv {
                *value = i16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("uv"));
                offset += 2;
            }
        }
        let texture = u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("texture"));
        offset += 2;
        let depth_key = f64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("depth"));
        (
            tile_index,
            Self {
                points,
                color,
                shade,
                ao,
                uvs,
                texture,
                depth_key,
            },
        )
    }

    /// Encode a depth-bucket record. The tile field is unused by the GPU
    /// translucent spill and kept for the shared record layout.
    #[cfg(feature = "gpu")]
    pub(super) fn encode_depth_record(&self) -> [u8; Self::RECORD_BYTES] {
        self.encode_record(0)
    }

    #[cfg(feature = "gpu")]
    pub(super) fn decode_depth_record(bytes: &[u8]) -> Self {
        Self::decode_record(bytes).1
    }

    #[cfg(all(test, feature = "gpu"))]
    pub(super) fn for_test(depth_key: f64) -> Self {
        Self {
            points: [[0.0; 3]; 3],
            color: [255; 4],
            shade: 1.0,
            ao: [3; 3],
            uvs: [[0; 2]; 3],
            texture: NO_TEXTURE,
            depth_key,
        }
    }

    /// Expand the compact screen points for binning and rasterization.
    pub(super) fn points_f64(&self) -> [[f64; 3]; 3] {
        self.points.map(|point| point.map(f64::from))
    }

    pub(super) fn colors(&self) -> [[f32; 4]; 3] {
        [
            vertex_color(self.color, self.shade, self.ao[0]),
            vertex_color(self.color, self.shade, self.ao[1]),
            vertex_color(self.color, self.shade, self.ao[2]),
        ]
    }

    pub(super) fn uvs(&self) -> [[f32; 2]; 3] {
        [
            decode_uv(self.uvs[0]),
            decode_uv(self.uvs[1]),
            decode_uv(self.uvs[2]),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::{QuadRender, Triangle};

    #[test]
    fn translucent_triangle_stays_compact() {
        // Twelve million retained triangles are the dominant textured path
        // cost; keep a regression from silently widening this struct.
        assert!(std::mem::size_of::<Triangle>() <= 72);
    }

    #[test]
    fn fast_quad_table_is_compact() {
        assert_eq!(std::mem::size_of::<QuadRender<[f64; 3]>>(), 128);
        assert_eq!(std::mem::size_of::<QuadRender<[f32; 3]>>(), 80);
    }

    #[cfg(feature = "gpu")]
    #[test]
    fn depth_record_round_trips() {
        let triangle = Triangle::for_test(1.25);
        let record = triangle.encode_depth_record();
        let decoded = Triangle::decode_depth_record(&record);

        assert_eq!(decoded.depth_key, triangle.depth_key);
        assert_eq!(decoded.points, triangle.points);
        assert_eq!(decoded.color, triangle.color);
        assert_eq!(decoded.shade, triangle.shade);
        assert_eq!(decoded.ao, triangle.ao);
        assert_eq!(decoded.uvs, triangle.uvs);
        assert_eq!(decoded.texture, triangle.texture);
    }
}
