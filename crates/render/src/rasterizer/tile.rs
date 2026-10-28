use super::bin::TileJob;
use super::geometry::{bounds, edge, intersects, quad_bounds, to_u8};
use super::project::{QuadRender, Triangle, vertex_color};
use crate::{Background, RenderOptions};
use litematic_material::{TextureFilter, TextureSet};
use litematic_mesh::{NO_TEXTURE, RenderLayer, decode_uv};
#[allow(clippy::too_many_arguments)]
pub(super) fn render_tiles_parallel(
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

pub(super) struct TileOutput {
    pub(super) index: usize,
    pub(super) x0: u32,
    pub(super) y0: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) rgba: Vec<u8>,
    pub(super) pixels_tested: u64,
    pub(super) pixels_written: u64,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_tile(
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
                let points = triangle.points_f64();
                intersects(&bounds(&points), job.x0, job.y0, job.width, job.height)
            })
            .collect();
        translucent.sort_by(|left, right| right.depth_key.total_cmp(&left.depth_key));

        for triangle in translucent {
            let colors = triangle.colors();
            let uvs = triangle.uvs();
            let points = triangle.points_f64();
            if triangle.texture == NO_TEXTURE {
                tile.rasterize_triangle::<false>(
                    &points,
                    &colors,
                    &uvs,
                    triangle.texture,
                    RenderLayer::Translucent,
                );
            } else {
                tile.rasterize_triangle::<true>(
                    &points,
                    &colors,
                    &uvs,
                    triangle.texture,
                    RenderLayer::Translucent,
                );
            }
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
    pub(super) pixels_tested: u64,
    pub(super) pixels_written: u64,
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

/// Extra pixels kept around a row span estimated from edge zero crossings, so
/// the exact per-pixel test only ever trims the span instead of widening it.
const SPAN_MARGIN: f64 = 2.0;

/// One triangle edge prepared for per-row span estimation.
///
/// On a fixed row the edge function is affine in screen x:
/// `w = dx * (py - ay) - dy * (px - ax)`. The sign of its x slope decides
/// whether the edge bounds an inside row from below or above, so a row only
/// visits the pixels between the three crossings instead of the whole bounding
/// box. The per-row values still come from the same expression the exact test
/// uses, keeping the accepted pixel set unchanged.
#[derive(Clone, Copy)]
struct RowEdge {
    ax: f64,
    ay: f64,
    dx: f64,
    dy: f64,
    /// `dx / dy`, used only to estimate the zero crossing of the row.
    slide: f64,
    bound: RowBound,
}

#[derive(Clone, Copy, PartialEq)]
enum RowBound {
    Lower,
    Upper,
    /// `dy` is zero, so the edge accepts or rejects the whole row.
    Constant,
}

impl RowEdge {
    fn new(from: [f64; 3], to: [f64; 3], inside_positive: f64) -> Self {
        let dx = to[0] - from[0];
        let dy = to[1] - from[1];
        let bound = if dy == 0.0 {
            RowBound::Constant
        } else if -inside_positive * dy > 0.0 {
            RowBound::Lower
        } else {
            RowBound::Upper
        };
        Self {
            ax: from[0],
            ay: from[1],
            dx,
            dy,
            slide: if dy == 0.0 { 0.0 } else { dx / dy },
            bound,
        }
    }

    /// Continuous x where this edge changes sign on the row through `py`.
    fn root_x(&self, py: f64) -> f64 {
        self.ax - 0.5 + self.slide * (py - self.ay)
    }
}

impl TileBuffer<'_> {
    pub(super) fn rasterize_quad(&mut self, quad: &QuadRender) {
        let colors = [
            vertex_color(quad.color, quad.shade, quad.ao[0]),
            vertex_color(quad.color, quad.shade, quad.ao[1]),
            vertex_color(quad.color, quad.shade, quad.ao[2]),
            vertex_color(quad.color, quad.shade, quad.ao[3]),
        ];
        // Untextured quads never sample uvs; the culling and shading path only
        // needs the color table.
        let uvs = if quad.texture == NO_TEXTURE {
            [[0.0; 2]; 4]
        } else {
            [
                decode_uv(quad.uvs[0]),
                decode_uv(quad.uvs[1]),
                decode_uv(quad.uvs[2]),
                decode_uv(quad.uvs[3]),
            ]
        };

        if quad.texture == NO_TEXTURE {
            self.rasterize_quad_triangles::<false>(quad, &colors, &uvs);
        } else {
            self.rasterize_quad_triangles::<true>(quad, &colors, &uvs);
        }
    }

    fn rasterize_quad_triangles<const TEXTURED: bool>(
        &mut self,
        quad: &QuadRender,
        colors: &[[f32; 4]; 4],
        uvs: &[[f32; 2]; 4],
    ) {
        let points = [quad.points[0], quad.points[1], quad.points[2]];
        let triangle_colors = [colors[0], colors[1], colors[2]];
        let triangle_uvs = [uvs[0], uvs[1], uvs[2]];
        self.rasterize_triangle::<TEXTURED>(
            &points,
            &triangle_colors,
            &triangle_uvs,
            quad.texture,
            quad.layer,
        );

        let points = [quad.points[0], quad.points[2], quad.points[3]];
        let triangle_colors = [colors[0], colors[2], colors[3]];
        let triangle_uvs = [uvs[0], uvs[2], uvs[3]];
        self.rasterize_triangle::<TEXTURED>(
            &points,
            &triangle_colors,
            &triangle_uvs,
            quad.texture,
            quad.layer,
        );
    }

    pub(super) fn rasterize_triangle<const TEXTURED: bool>(
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

        // Barycentric weights, depth, and color interpolation stay in the same
        // order as the full bounding-box scan; only the visited pixel span is
        // narrowed, so accepted fragments are bit-identical.
        let inverse = 1.0 / area;
        let positive = area > 0.0;
        let inside_positive = if positive { 1.0 } else { -1.0 };
        let edge0 = RowEdge::new(p1, p2, inside_positive);
        let edge1 = RowEdge::new(p2, p0, inside_positive);
        let edge2 = RowEdge::new(p0, p1, inside_positive);
        let depth0 = p0[2];
        let depth1 = p1[2];
        let depth2 = p2[2];

        let local_x0 = self.x0 as i32;
        let local_y0 = self.y0 as i32;
        let width = self.width as usize;
        let textures = self.textures;
        let sampled_texture = if TEXTURED {
            textures.and_then(|textures| textures.get(texture))
        } else {
            None
        };
        let filter = self.filter;
        let mut tested = 0u64;
        let mut written = 0u64;

        let row_start = (min_y - local_y0) as usize;
        let row_count = (max_y - min_y + 1) as usize;
        let colors_rows = self
            .colors
            .chunks_mut(width)
            .skip(row_start)
            .take(row_count);
        let depths_rows = self
            .depths
            .chunks_mut(width)
            .skip(row_start)
            .take(row_count);
        let mut row_py = f64::from(min_y) + 0.5;

        for (colors_row, depths_row) in colors_rows.zip(depths_rows) {
            let py = row_py;
            row_py += 1.0;

            // Row-constant term of each edge function.
            let t0 = edge0.dx * (py - edge0.ay);
            let t1 = edge1.dx * (py - edge1.ay);
            let t2 = edge2.dx * (py - edge2.ay);

            let mut lo = min_x;
            let mut hi = max_x;
            let mut empty = false;
            for (edge, t) in [(&edge0, t0), (&edge1, t1), (&edge2, t2)] {
                match edge.bound {
                    RowBound::Lower => {
                        lo = lo.max((edge.root_x(py) - SPAN_MARGIN).ceil() as i32);
                    }
                    RowBound::Upper => {
                        hi = hi.min((edge.root_x(py) + SPAN_MARGIN).floor() as i32);
                    }
                    RowBound::Constant => {
                        if inside_positive * t < 0.0 {
                            empty = true;
                        }
                    }
                }
            }
            if empty {
                continue;
            }

            let mut lo = lo.clamp(min_x, max_x);
            let mut hi = hi.clamp(min_x, max_x);
            if hi < lo {
                // The crossing estimates disagreed; every bounding-box row of a
                // triangle is reachable, so widen instead of trusting them.
                lo = min_x;
                hi = max_x;
            }

            let passes = |x: i32| -> bool {
                let px = f64::from(x) + 0.5;
                let w0 = t0 - edge0.dy * (px - edge0.ax);
                let w1 = t1 - edge1.dy * (px - edge1.ax);
                let w2 = t2 - edge2.dy * (px - edge2.ax);
                if positive {
                    w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0
                } else {
                    w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0
                }
            };

            // The edge functions are monotone in x, so the accepted pixels of
            // a row form one interval; walk the two conservative endpoints in.
            while lo <= hi && !passes(lo) {
                lo += 1;
            }
            while hi >= lo && !passes(hi) {
                hi -= 1;
            }
            if hi < lo {
                continue;
            }

            let start = (lo - local_x0) as usize;
            let end = (hi - local_x0) as usize;
            let mut px = f64::from(lo) + 0.5;

            for (color_slot, depth_slot) in colors_row[start..=end]
                .iter_mut()
                .zip(&mut depths_row[start..=end])
            {
                tested += 1;
                let w0 = t0 - edge0.dy * (px - edge0.ax);
                let w1 = t1 - edge1.dy * (px - edge1.ax);
                let w2 = t2 - edge2.dy * (px - edge2.ax);
                px += 1.0;

                let b0 = w0 * inverse;
                let b1 = w1 * inverse;
                let b2 = w2 * inverse;
                let depth = b0 * depth0 + b1 * depth1 + b2 * depth2;

                if depth >= *depth_slot {
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

                if let Some(texture) = sampled_texture {
                    let uv = [
                        (uvs[0][0] as f64 * b0 + uvs[1][0] as f64 * b1 + uvs[2][0] as f64 * b2)
                            as f32,
                        (uvs[0][1] as f64 * b0 + uvs[1][1] as f64 * b1 + uvs[2][1] as f64 * b2)
                            as f32,
                    ];
                    if let Some(texel) = texture.sample(uv, filter) {
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
                    blend(color_slot, color);
                } else {
                    *color_slot = color;
                    *depth_slot = depth;
                }
                written += 1;
            }
        }

        self.counters.pixels_tested += tested;
        self.counters.pixels_written += written;
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
