use crate::camera::Camera;
use crate::{Background, LightingProfile, RenderError, RenderOptions};
use litematic_material::{TextureFilter, TextureSet};
use litematic_mesh::{Mesh, MeshPage, NO_TEXTURE, RenderLayer, ambient_occlusion_level, decode_uv};
use litematic_scene::Scene;
use serde::{Deserialize, Serialize};

mod downsample;
mod stream;

use downsample::downsample;
use stream::rasterize_streaming;

/// Render counters and effective output settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderStatistics {
    pub width: u32,
    pub height: u32,
    pub supersample: u32,
    pub internal_width: u32,
    pub internal_height: u32,
    pub tiles: u64,
    pub triangles: u64,
    pub culled_backfaces: u64,
    pub opaque_triangles: u64,
    pub cutout_triangles: u64,
    pub translucent_triangles: u64,
    pub textured_triangles: u64,
    pub pixels_tested: u64,
    pub pixels_written: u64,
}

/// RGBA output plus render diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOutput {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub statistics: RenderStatistics,
}

/// Render a color-only mesh with the selected camera and lighting profile.
pub fn render_mesh(
    scene: &Scene,
    mesh: &Mesh,
    options: &RenderOptions,
) -> Result<RenderOutput, RenderError> {
    render_mesh_internal(scene, mesh, options, None)
}

/// Render a mesh and sample its texture ids from `textures`.
pub fn render_mesh_with_textures(
    scene: &Scene,
    mesh: &Mesh,
    options: &RenderOptions,
    textures: &TextureSet,
) -> Result<RenderOutput, RenderError> {
    render_mesh_internal(scene, mesh, options, Some(textures))
}

fn render_mesh_internal(
    scene: &Scene,
    mesh: &Mesh,
    options: &RenderOptions,
    textures: Option<&TextureSet>,
) -> Result<RenderOutput, RenderError> {
    options.validate()?;
    let camera = Camera::new(&scene.bounds, options)?;

    let mut statistics = RenderStatistics {
        width: options.width,
        height: options.height,
        supersample: options.supersample,
        internal_width: options.internal_width(),
        internal_height: options.internal_height(),
        ..Default::default()
    };

    let streaming = options.force_streaming || table_bytes(mesh) > tile_buffer_bytes(options);
    let rgba = if streaming {
        rasterize_streaming(mesh, &camera, options, &mut statistics, textures)
    } else {
        let mut quads = Vec::new();
        let mut translucent = Vec::new();
        for page in &mesh.pages {
            push_page_quads(
                page,
                &camera,
                options,
                &mut statistics,
                &mut quads,
                &mut translucent,
            );
        }
        rasterize(&quads, &translucent, options, &mut statistics, textures)
    };
    Ok(RenderOutput {
        width: options.width,
        height: options.height,
        rgba,
        statistics,
    })
}

pub(super) fn push_page_quads(
    page: &MeshPage,
    camera: &Camera,
    options: &RenderOptions,
    statistics: &mut RenderStatistics,
    quads: &mut Vec<QuadRender>,
    translucent: &mut Vec<Triangle>,
) {
    for quad in &page.quads {
        let first = &quad.vertices[0];
        let normal = first.normal;
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

        // The mesh builder emits one material and light tuple per quad: all
        // four vertices share color, normal, texture, and shade.
        debug_assert!(
            quad.vertices
                .iter()
                .all(|vertex| vertex.color == first.color
                    && vertex.normal == first.normal
                    && vertex.texture == first.texture
                    && vertex.shade == first.shade)
        );
        let shade = if first.shade {
            options.lighting.face_shade(first.normal)
        } else {
            1.0
        };
        let color = first.color;
        let ao = match options.lighting {
            LightingProfile::FlatV1 => [3u8; 4],
            LightingProfile::SimpleAoV1 => [
                quad.vertices[0].ao,
                quad.vertices[1].ao,
                quad.vertices[2].ao,
                quad.vertices[3].ao,
            ],
        };
        let uvs = [
            quad.vertices[0].uv,
            quad.vertices[1].uv,
            quad.vertices[2].uv,
            quad.vertices[3].uv,
        ];
        let texture = first.texture;
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
                    points,
                    color,
                    shade,
                    ao: [ao[indices[0]], ao[indices[1]], ao[indices[2]]],
                    uvs: [uvs[indices[0]], uvs[indices[1]], uvs[indices[2]]],
                    texture,
                    layer,
                    depth_key,
                });
            }
        }

        if keep && layer != RenderLayer::Translucent {
            quads.push(QuadRender {
                points: projected,
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

/// An opaque or cutout quad, projected once and split into triangles per tile.
/// Colors and texture coordinates are stored compactly; the exact float values
/// are reconstructed when the quad is rasterized into a tile.
#[derive(Debug, Clone)]
pub(super) struct QuadRender {
    points: [[f64; 3]; 4],
    color: [u8; 4],
    shade: f32,
    ao: [u8; 4],
    uvs: [[i16; 2]; 4],
    texture: u16,
    layer: RenderLayer,
}

/// A translucent triangle. Only translucent geometry needs sorting, so this is
/// kept separate from the quad table.
#[derive(Debug, Clone)]
pub(super) struct Triangle {
    pub(super) points: [[f64; 3]; 3],
    color: [u8; 4],
    shade: f32,
    ao: [u8; 3],
    uvs: [[i16; 2]; 3],
    pub(super) texture: u16,
    pub(super) layer: RenderLayer,
    pub(super) depth_key: f64,
}

/// Reconstruct the per-vertex color exactly as the previous float path did.
fn vertex_color(color: [u8; 4], shade: f32, ao: u8) -> [f32; 4] {
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

fn rasterize(
    quads: &[QuadRender],
    translucent: &[Triangle],
    options: &RenderOptions,
    statistics: &mut RenderStatistics,
    textures: Option<&TextureSet>,
) -> Vec<u8> {
    let width = options.internal_width();
    let height = options.internal_height();
    let tile_size = options.tile_size;
    let tiles_x = width.div_ceil(tile_size);
    let tiles_y = height.div_ceil(tile_size);
    statistics.tiles = u64::from(tiles_x) * u64::from(tiles_y);

    let jobs = tile_jobs(width, height, tile_size);
    let quad_bins = bin_quads(quads, width, height, tile_size);
    let triangle_bins = bin_triangles(translucent, width, height, tile_size);
    let threads = worker_count(options.threads, jobs.len());
    let tiles = if threads > 1 {
        render_tiles_parallel(
            quads,
            translucent,
            options,
            &jobs,
            &quad_bins,
            &triangle_bins,
            threads,
            textures,
        )
    } else {
        jobs.iter()
            .map(|job| {
                render_tile(
                    quads,
                    translucent,
                    options,
                    job,
                    &quad_bins[job.index],
                    &triangle_bins[job.index],
                    textures,
                )
            })
            .collect()
    };

    let mut output = vec![0u8; (u64::from(width) * u64::from(height) * 4) as usize];
    for tile in &tiles {
        statistics.pixels_tested += tile.pixels_tested;
        statistics.pixels_written += tile.pixels_written;

        let row_bytes = usize::try_from(tile.width).expect("tile width fits usize") * 4;
        for row in 0..tile.height {
            let source_start = row as usize * row_bytes;
            let target_start =
                ((u64::from(tile.y0 + row) * u64::from(width) + u64::from(tile.x0)) as usize) * 4;
            output[target_start..target_start + row_bytes]
                .copy_from_slice(&tile.rgba[source_start..source_start + row_bytes]);
        }
    }

    if options.supersample > 1 {
        downsample(
            &output,
            width,
            height,
            options.width,
            options.height,
            options.supersample,
            threads,
        )
    } else {
        output
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TileJob {
    index: usize,
    pub(super) x0: u32,
    pub(super) y0: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

pub(super) fn tile_jobs(width: u32, height: u32, tile_size: u32) -> Vec<TileJob> {
    let tiles_x = width.div_ceil(tile_size);
    let tiles_y = height.div_ceil(tile_size);
    let mut jobs = Vec::with_capacity((tiles_x * tiles_y) as usize);

    for tile_y in 0..tiles_y {
        for tile_x in 0..tiles_x {
            let x0 = tile_x * tile_size;
            let y0 = tile_y * tile_size;
            jobs.push(TileJob {
                index: jobs.len(),
                x0,
                y0,
                width: tile_size.min(width - x0),
                height: tile_size.min(height - y0),
            });
        }
    }

    jobs
}

fn table_bytes(mesh: &Mesh) -> u64 {
    mesh.statistics
        .quads
        .saturating_mul(std::mem::size_of::<QuadRender>() as u64)
}

fn tile_buffer_bytes(options: &RenderOptions) -> u64 {
    u64::from(options.internal_width()) * u64::from(options.internal_height()) * 24
}

/// Group quads by the tiles their screen bounds cover, so each tile only visits
/// candidates that can actually write a fragment.
pub(super) fn bin_quads(
    quads: &[QuadRender],
    width: u32,
    height: u32,
    tile_size: u32,
) -> Vec<Vec<u32>> {
    let tiles_x = width.div_ceil(tile_size);
    let tiles_y = height.div_ceil(tile_size);
    let mut bins = vec![Vec::new(); (tiles_x * tiles_y) as usize];
    let tile_size = f64::from(tile_size);
    let max_tile_x = f64::from(tiles_x - 1);
    let max_tile_y = f64::from(tiles_y - 1);

    for (index, quad) in quads.iter().enumerate() {
        let bbox = quad_bounds(&quad.points);
        let min_x = (bbox[0] / tile_size).floor().clamp(0.0, max_tile_x) as u32;
        let max_x = (bbox[2] / tile_size).floor().clamp(0.0, max_tile_x) as u32;
        let min_y = (bbox[1] / tile_size).floor().clamp(0.0, max_tile_y) as u32;
        let max_y = (bbox[3] / tile_size).floor().clamp(0.0, max_tile_y) as u32;

        for tile_y in min_y..=max_y {
            for tile_x in min_x..=max_x {
                bins[(tile_y * tiles_x + tile_x) as usize].push(index as u32);
            }
        }
    }

    bins
}

/// Group translucent triangles by tile; only these need per-tile sorting.
pub(super) fn bin_triangles(
    triangles: &[Triangle],
    width: u32,
    height: u32,
    tile_size: u32,
) -> Vec<Vec<u32>> {
    let tiles_x = width.div_ceil(tile_size);
    let tiles_y = height.div_ceil(tile_size);
    let mut bins = vec![Vec::new(); (tiles_x * tiles_y) as usize];
    let tile_size = f64::from(tile_size);
    let max_tile_x = f64::from(tiles_x - 1);
    let max_tile_y = f64::from(tiles_y - 1);

    for (index, triangle) in triangles.iter().enumerate() {
        let bbox = bounds(&triangle.points);
        let min_x = (bbox[0] / tile_size).floor().clamp(0.0, max_tile_x) as u32;
        let max_x = (bbox[2] / tile_size).floor().clamp(0.0, max_tile_x) as u32;
        let min_y = (bbox[1] / tile_size).floor().clamp(0.0, max_tile_y) as u32;
        let max_y = (bbox[3] / tile_size).floor().clamp(0.0, max_tile_y) as u32;

        for tile_y in min_y..=max_y {
            for tile_x in min_x..=max_x {
                bins[(tile_y * tiles_x + tile_x) as usize].push(index as u32);
            }
        }
    }

    bins
}

pub(super) fn worker_count(requested: u32, jobs: usize) -> usize {
    let available = if requested == 0 {
        std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
    } else {
        requested as usize
    };

    available.clamp(1, jobs.max(1))
}

#[allow(clippy::too_many_arguments)]
fn render_tiles_parallel(
    quads: &[QuadRender],
    translucent: &[Triangle],
    options: &RenderOptions,
    jobs: &[TileJob],
    quad_bins: &[Vec<u32>],
    triangle_bins: &[Vec<u32>],
    threads: usize,
    textures: Option<&TextureSet>,
) -> Vec<TileOutput> {
    let cursor = std::sync::atomic::AtomicUsize::new(0);

    let mut tiles: Vec<TileOutput> = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let cursor = &cursor;
            handles.push(scope.spawn(move || {
                let mut local = Vec::new();
                loop {
                    let index = cursor.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(job) = jobs.get(index) else {
                        break;
                    };
                    local.push(render_tile(
                        quads,
                        translucent,
                        options,
                        job,
                        &quad_bins[index],
                        &triangle_bins[index],
                        textures,
                    ));
                }
                local
            }));
        }

        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("tile worker panicked"))
            .collect()
    });

    // Workers finish in any order; restore tile order so statistics stay stable.
    tiles.sort_unstable_by_key(|tile| tile.index);
    tiles
}

struct TileOutput {
    index: usize,
    x0: u32,
    y0: u32,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    pub(super) pixels_tested: u64,
    pub(super) pixels_written: u64,
}

#[allow(clippy::too_many_arguments)]
fn render_tile(
    quads: &[QuadRender],
    translucent_triangles: &[Triangle],
    options: &RenderOptions,
    job: &TileJob,
    quad_candidates: &[u32],
    triangle_candidates: &[u32],
    textures: Option<&TextureSet>,
) -> TileOutput {
    let pixel_count = (u64::from(job.width) * u64::from(job.height)) as usize;
    let mut colors = vec![[0.0f32; 4]; pixel_count];
    if let Background::Solid(color) = options.background {
        let fill = [
            f32::from(color.r) / 255.0,
            f32::from(color.g) / 255.0,
            f32::from(color.b) / 255.0,
            f32::from(color.a) / 255.0,
        ];
        colors.fill(fill);
    }
    let mut depths = vec![f64::INFINITY; pixel_count];
    let mut counters = TileCounters::default();

    {
        let mut tile = TileBuffer {
            x0: job.x0,
            y0: job.y0,
            width: job.width,
            height: job.height,
            colors: &mut colors,
            depths: &mut depths,
            counters: &mut counters,
            textures,
            filter: options.texture_filter,
        };

        for &index in quad_candidates {
            let quad = &quads[index as usize];
            if !intersects(
                &quad_bounds(&quad.points),
                job.x0,
                job.y0,
                job.width,
                job.height,
            ) {
                continue;
            }
            tile.rasterize_quad(quad);
        }

        let mut translucent: Vec<&Triangle> = triangle_candidates
            .iter()
            .map(|&index| &translucent_triangles[index as usize])
            .filter(|triangle| {
                intersects(
                    &bounds(&triangle.points),
                    job.x0,
                    job.y0,
                    job.width,
                    job.height,
                )
            })
            .collect();
        translucent.sort_by(|left, right| right.depth_key.total_cmp(&left.depth_key));

        for triangle in translucent {
            let colors = triangle.colors();
            let uvs = triangle.uvs();
            tile.rasterize_triangle(
                &triangle.points,
                &colors,
                &uvs,
                triangle.texture,
                triangle.layer,
            );
        }
    }

    let mut rgba = vec![0u8; pixel_count * 4];
    for local_y in 0..job.height {
        for local_x in 0..job.width {
            let color = colors[(local_y * job.width + local_x) as usize];
            let alpha = to_u8(color[3]);
            let [red, green, blue] = if alpha == 0 {
                [0, 0, 0]
            } else {
                [to_u8(color[0]), to_u8(color[1]), to_u8(color[2])]
            };
            let index = ((local_y * job.width + local_x) as usize) * 4;
            rgba[index] = red;
            rgba[index + 1] = green;
            rgba[index + 2] = blue;
            rgba[index + 3] = alpha;
        }
    }

    TileOutput {
        index: job.index,
        x0: job.x0,
        y0: job.y0,
        width: job.width,
        height: job.height,
        rgba,
        pixels_tested: counters.pixels_tested,
        pixels_written: counters.pixels_written,
    }
}

#[derive(Default)]
pub(super) struct TileCounters {
    pixels_tested: u64,
    pixels_written: u64,
}

pub(super) struct TileBuffer<'a> {
    pub(super) x0: u32,
    pub(super) y0: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) colors: &'a mut [[f32; 4]],
    pub(super) depths: &'a mut [f64],
    pub(super) counters: &'a mut TileCounters,
    pub(super) textures: Option<&'a TextureSet>,
    pub(super) filter: TextureFilter,
}

impl TileBuffer<'_> {
    pub(super) fn rasterize_quad(&mut self, quad: &QuadRender) {
        let colors = [
            vertex_color(quad.color, quad.shade, quad.ao[0]),
            vertex_color(quad.color, quad.shade, quad.ao[1]),
            vertex_color(quad.color, quad.shade, quad.ao[2]),
            vertex_color(quad.color, quad.shade, quad.ao[3]),
        ];
        let uvs = [
            decode_uv(quad.uvs[0]),
            decode_uv(quad.uvs[1]),
            decode_uv(quad.uvs[2]),
            decode_uv(quad.uvs[3]),
        ];

        let points = [quad.points[0], quad.points[1], quad.points[2]];
        let triangle_colors = [colors[0], colors[1], colors[2]];
        let triangle_uvs = [uvs[0], uvs[1], uvs[2]];
        self.rasterize_triangle(
            &points,
            &triangle_colors,
            &triangle_uvs,
            quad.texture,
            quad.layer,
        );

        let points = [quad.points[0], quad.points[2], quad.points[3]];
        let triangle_colors = [colors[0], colors[2], colors[3]];
        let triangle_uvs = [uvs[0], uvs[2], uvs[3]];
        self.rasterize_triangle(
            &points,
            &triangle_colors,
            &triangle_uvs,
            quad.texture,
            quad.layer,
        );
    }

    pub(super) fn rasterize_triangle(
        &mut self,
        points: &[[f64; 3]; 3],
        colors: &[[f32; 4]; 3],
        uvs: &[[f32; 2]; 3],
        texture: u16,
        layer: RenderLayer,
    ) {
        let bbox = bounds(points);
        let min_x = bbox[0].floor().max(f64::from(self.x0)) as i32;
        let min_y = bbox[1].floor().max(f64::from(self.y0)) as i32;
        let max_x = bbox[2].ceil().min(f64::from(self.x0 + self.width) - 1.0) as i32;
        let max_y = bbox[3].ceil().min(f64::from(self.y0 + self.height) - 1.0) as i32;

        if max_x < min_x || max_y < min_y {
            return;
        }

        let p0 = points[0];
        let p1 = points[1];
        let p2 = points[2];
        let area = edge(p0, p1, p2);
        if area.abs() < 1e-12 {
            return;
        }

        for y in min_y..=max_y {
            let py = f64::from(y) + 0.5;
            for x in min_x..=max_x {
                self.counters.pixels_tested += 1;
                let px = f64::from(x) + 0.5;
                let w0 = edge(p1, p2, [px, py, 0.0]);
                let w1 = edge(p2, p0, [px, py, 0.0]);
                let w2 = edge(p0, p1, [px, py, 0.0]);

                if area > 0.0 {
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                } else if w0 > 0.0 || w1 > 0.0 || w2 > 0.0 {
                    continue;
                }

                let inverse = 1.0 / area;
                let b0 = w0 * inverse;
                let b1 = w1 * inverse;
                let b2 = w2 * inverse;
                let depth = b0 * p0[2] + b1 * p1[2] + b2 * p2[2];

                let local_x = (x - self.x0 as i32) as u32;
                let local_y = (y - self.y0 as i32) as u32;
                let index = (local_y * self.width + local_x) as usize;

                if depth >= self.depths[index] {
                    continue;
                }

                let mut color = [
                    (colors[0][0] as f64 * b0 + colors[1][0] as f64 * b1 + colors[2][0] as f64 * b2)
                        as f32,
                    (colors[0][1] as f64 * b0 + colors[1][1] as f64 * b1 + colors[2][1] as f64 * b2)
                        as f32,
                    (colors[0][2] as f64 * b0 + colors[1][2] as f64 * b1 + colors[2][2] as f64 * b2)
                        as f32,
                    (colors[0][3] as f64 * b0 + colors[1][3] as f64 * b1 + colors[2][3] as f64 * b2)
                        as f32,
                ];

                if texture != NO_TEXTURE
                    && let Some(textures) = self.textures
                {
                    let uv = [
                        (uvs[0][0] as f64 * b0 + uvs[1][0] as f64 * b1 + uvs[2][0] as f64 * b2)
                            as f32,
                        (uvs[0][1] as f64 * b0 + uvs[1][1] as f64 * b1 + uvs[2][1] as f64 * b2)
                            as f32,
                    ];
                    if let Some(texel) = textures.sample(texture, uv, self.filter) {
                        color[0] *= texel[0];
                        color[1] *= texel[1];
                        color[2] *= texel[2];
                        color[3] *= texel[3];
                    }
                }

                if layer == RenderLayer::Cutout && color[3] < 0.5 {
                    continue;
                }

                if layer == RenderLayer::Translucent {
                    blend(&mut self.colors[index], color);
                } else {
                    self.colors[index] = color;
                    self.depths[index] = depth;
                }
                self.counters.pixels_written += 1;
            }
        }
    }
}

fn blend(destination: &mut [f32; 4], source: [f32; 4]) {
    let source_alpha = source[3];
    if source_alpha <= 0.0 {
        return;
    }

    let destination_alpha = destination[3];
    let out_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if out_alpha <= 0.0 {
        return;
    }

    let destination_factor = destination_alpha * (1.0 - source_alpha);
    let inverse = 1.0 / out_alpha;
    destination[0] = (source[0] * source_alpha + destination[0] * destination_factor) * inverse;
    destination[1] = (source[1] * source_alpha + destination[1] * destination_factor) * inverse;
    destination[2] = (source[2] * source_alpha + destination[2] * destination_factor) * inverse;
    destination[3] = out_alpha;
}

pub(super) fn intersects(bbox: &[f64; 4], x0: u32, y0: u32, width: u32, height: u32) -> bool {
    let max_x = f64::from(x0 + width);
    let max_y = f64::from(y0 + height);
    bbox[2] >= f64::from(x0) && bbox[0] < max_x && bbox[3] >= f64::from(y0) && bbox[1] < max_y
}

pub(super) fn bounds(points: &[[f64; 3]; 3]) -> [f64; 4] {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for point in points {
        min_x = min_x.min(point[0]);
        max_x = max_x.max(point[0]);
        min_y = min_y.min(point[1]);
        max_y = max_y.max(point[1]);
    }

    [min_x, min_y, max_x, max_y]
}

pub(super) fn quad_bounds(points: &[[f64; 3]; 4]) -> [f64; 4] {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for point in points {
        min_x = min_x.min(point[0]);
        max_x = max_x.max(point[0]);
        min_y = min_y.min(point[1]);
        max_y = max_y.max(point[1]);
    }

    [min_x, min_y, max_x, max_y]
}

fn edge(a: [f64; 3], b: [f64; 3], point: [f64; 3]) -> f64 {
    (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0])
}

fn dot3(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

pub(super) fn to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
