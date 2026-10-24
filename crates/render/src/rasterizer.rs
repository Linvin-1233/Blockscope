use crate::camera::Camera;
use crate::{Background, LightingProfile, RenderError, RenderOptions};
use litematic_mesh::{Mesh, RenderLayer};
use litematic_scene::Scene;
use serde::{Deserialize, Serialize};

/// Render counters and effective output settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderStatistics {
    pub width: u32,
    pub height: u32,
    pub tiles: u64,
    pub triangles: u64,
    pub culled_backfaces: u64,
    pub opaque_triangles: u64,
    pub cutout_triangles: u64,
    pub translucent_triangles: u64,
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

/// Render a mesh with the selected camera and lighting profile.
pub fn render_mesh(
    scene: &Scene,
    mesh: &Mesh,
    options: &RenderOptions,
) -> Result<RenderOutput, RenderError> {
    options.validate()?;
    let camera = Camera::new(&scene.bounds, options)?;

    let mut triangles = Vec::new();
    let mut statistics = RenderStatistics {
        width: options.width,
        height: options.height,
        ..Default::default()
    };

    for page in &mesh.pages {
        for quad in &page.quads {
            let normal = quad.vertices[0].normal;
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

            let mut triangle_points = [[[0.0f64; 3]; 3]; 2];
            let mut triangle_colors = [[[0.0f32; 4]; 3]; 2];
            let mut projected = [[0.0f64; 3]; 4];
            let mut vertex_colors = [[0.0f32; 4]; 4];

            for (index, vertex) in quad.vertices.iter().enumerate() {
                let world = [
                    f64::from(page.origin[0]) + f64::from(vertex.position[0]),
                    f64::from(page.origin[1]) + f64::from(vertex.position[1]),
                    f64::from(page.origin[2]) + f64::from(vertex.position[2]),
                ];
                projected[index] = camera.project(world);

                let shade = f64::from(options.lighting.face_shade(vertex.normal));
                let ao = match options.lighting {
                    LightingProfile::FlatV1 => 1.0,
                    LightingProfile::SimpleAoV1 => f64::from(vertex.ao),
                };
                vertex_colors[index] = [
                    (f64::from(vertex.color[0]) / 255.0 * shade * ao) as f32,
                    (f64::from(vertex.color[1]) / 255.0 * shade * ao) as f32,
                    (f64::from(vertex.color[2]) / 255.0 * shade * ao) as f32,
                    f32::from(vertex.color[3]) / 255.0,
                ];
            }

            for (triangle_index, indices) in [[0usize, 1, 2], [0, 2, 3]].into_iter().enumerate() {
                for (slot, index) in indices.into_iter().enumerate() {
                    triangle_points[triangle_index][slot] = projected[index];
                    triangle_colors[triangle_index][slot] = vertex_colors[index];
                }
                let points = triangle_points[triangle_index];
                let bbox = bounds(points);
                if bbox[0] > bbox[2] || bbox[1] > bbox[3] {
                    continue;
                }
                let depth_key = (points[0][2] + points[1][2] + points[2][2]) / 3.0;
                let triangle = Triangle {
                    points,
                    colors: triangle_colors[triangle_index],
                    layer: quad.layer,
                    depth_key,
                    bbox,
                };

                match quad.layer {
                    RenderLayer::Opaque => statistics.opaque_triangles += 1,
                    RenderLayer::Cutout => statistics.cutout_triangles += 1,
                    RenderLayer::Translucent => statistics.translucent_triangles += 1,
                }
                statistics.triangles += 1;
                triangles.push(triangle);
            }
        }
    }

    let rgba = rasterize(&triangles, options, &mut statistics);
    Ok(RenderOutput {
        width: options.width,
        height: options.height,
        rgba,
        statistics,
    })
}

#[derive(Debug, Clone)]
struct Triangle {
    points: [[f64; 3]; 3],
    colors: [[f32; 4]; 3],
    layer: RenderLayer,
    depth_key: f64,
    bbox: [f64; 4],
}

fn rasterize(
    triangles: &[Triangle],
    options: &RenderOptions,
    statistics: &mut RenderStatistics,
) -> Vec<u8> {
    let width = options.width;
    let height = options.height;
    let tile_size = options.tile_size;
    let tiles_x = width.div_ceil(tile_size);
    let tiles_y = height.div_ceil(tile_size);
    statistics.tiles = u64::from(tiles_x) * u64::from(tiles_y);

    let mut output = vec![0u8; (u64::from(width) * u64::from(height) * 4) as usize];

    for tile_y in 0..tiles_y {
        for tile_x in 0..tiles_x {
            let tile_x0 = tile_x * tile_size;
            let tile_y0 = tile_y * tile_size;
            let tile_w = tile_size.min(width - tile_x0);
            let tile_h = tile_size.min(height - tile_y0);
            let pixel_count = (u64::from(tile_w) * u64::from(tile_h)) as usize;

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

            {
                let mut tile = TileBuffer {
                    x0: tile_x0,
                    y0: tile_y0,
                    width: tile_w,
                    height: tile_h,
                    colors: &mut colors,
                    depths: &mut depths,
                    statistics: &mut *statistics,
                };

                for triangle in triangles {
                    if triangle.layer == RenderLayer::Translucent {
                        continue;
                    }
                    if !intersects(triangle, tile_x0, tile_y0, tile_w, tile_h) {
                        continue;
                    }
                    tile.rasterize(triangle);
                }

                let mut translucent: Vec<&Triangle> = triangles
                    .iter()
                    .filter(|triangle| {
                        triangle.layer == RenderLayer::Translucent
                            && intersects(triangle, tile_x0, tile_y0, tile_w, tile_h)
                    })
                    .collect();
                translucent.sort_by(|left, right| right.depth_key.total_cmp(&left.depth_key));

                for triangle in translucent {
                    tile.rasterize(triangle);
                }
            }

            for local_y in 0..tile_h {
                for local_x in 0..tile_w {
                    let color = colors[(local_y * tile_w + local_x) as usize];
                    let alpha = to_u8(color[3]);
                    let [red, green, blue] = if alpha == 0 {
                        [0, 0, 0]
                    } else {
                        [to_u8(color[0]), to_u8(color[1]), to_u8(color[2])]
                    };
                    let image_x = tile_x0 + local_x;
                    let image_y = tile_y0 + local_y;
                    let index =
                        (u64::from(image_y) * u64::from(width) + u64::from(image_x)) as usize * 4;
                    output[index] = red;
                    output[index + 1] = green;
                    output[index + 2] = blue;
                    output[index + 3] = alpha;
                }
            }
        }
    }

    output
}

struct TileBuffer<'a> {
    x0: u32,
    y0: u32,
    width: u32,
    height: u32,
    colors: &'a mut [[f32; 4]],
    depths: &'a mut [f64],
    statistics: &'a mut RenderStatistics,
}

impl TileBuffer<'_> {
    fn rasterize(&mut self, triangle: &Triangle) {
        let min_x = triangle.bbox[0].floor().max(f64::from(self.x0)) as i32;
        let min_y = triangle.bbox[1].floor().max(f64::from(self.y0)) as i32;
        let max_x = triangle.bbox[2]
            .ceil()
            .min(f64::from(self.x0 + self.width) - 1.0) as i32;
        let max_y = triangle.bbox[3]
            .ceil()
            .min(f64::from(self.y0 + self.height) - 1.0) as i32;

        if max_x < min_x || max_y < min_y {
            return;
        }

        let p0 = triangle.points[0];
        let p1 = triangle.points[1];
        let p2 = triangle.points[2];
        let area = edge(p0, p1, p2);
        if area.abs() < 1e-12 {
            return;
        }

        for y in min_y..=max_y {
            let py = f64::from(y) + 0.5;
            for x in min_x..=max_x {
                self.statistics.pixels_tested += 1;
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

                let color = [
                    (triangle.colors[0][0] as f64 * b0
                        + triangle.colors[1][0] as f64 * b1
                        + triangle.colors[2][0] as f64 * b2) as f32,
                    (triangle.colors[0][1] as f64 * b0
                        + triangle.colors[1][1] as f64 * b1
                        + triangle.colors[2][1] as f64 * b2) as f32,
                    (triangle.colors[0][2] as f64 * b0
                        + triangle.colors[1][2] as f64 * b1
                        + triangle.colors[2][2] as f64 * b2) as f32,
                    (triangle.colors[0][3] as f64 * b0
                        + triangle.colors[1][3] as f64 * b1
                        + triangle.colors[2][3] as f64 * b2) as f32,
                ];

                if triangle.layer == RenderLayer::Cutout && color[3] < 0.5 {
                    continue;
                }

                if triangle.layer == RenderLayer::Translucent {
                    blend(&mut self.colors[index], color);
                } else {
                    self.colors[index] = color;
                    self.depths[index] = depth;
                }
                self.statistics.pixels_written += 1;
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

fn intersects(triangle: &Triangle, x0: u32, y0: u32, width: u32, height: u32) -> bool {
    let max_x = f64::from(x0 + width);
    let max_y = f64::from(y0 + height);
    triangle.bbox[2] >= f64::from(x0)
        && triangle.bbox[0] < max_x
        && triangle.bbox[3] >= f64::from(y0)
        && triangle.bbox[1] < max_y
}

fn bounds(points: [[f64; 3]; 3]) -> [f64; 4] {
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

fn to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
