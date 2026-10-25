use super::downsample::downsample;
use super::{
    QuadRender, RenderStatistics, TileBuffer, TileCounters, TileJob, Triangle, bin_quads,
    bin_triangles, bounds, intersects, push_page_quads, quad_bounds, tile_jobs, to_u8,
    worker_count,
};
use crate::{Background, Camera, RenderOptions};
use litematic_material::TextureSet;
use litematic_mesh::Mesh;

const STREAM_PAGES_PER_CHUNK: usize = 8;

pub(super) fn rasterize_streaming(
    mesh: &Mesh,
    camera: &Camera,
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
    let tiles: Vec<std::sync::Mutex<StreamTile>> = jobs
        .into_iter()
        .map(|job| std::sync::Mutex::new(StreamTile::new(job, options.background)))
        .collect();
    let threads = worker_count(options.threads, tiles.len());

    let mut quads = Vec::new();
    let mut translucent = Vec::new();
    let mut page_index = 0;

    while page_index < mesh.pages.len() {
        let end = (page_index + STREAM_PAGES_PER_CHUNK).min(mesh.pages.len());
        quads.clear();
        for page in &mesh.pages[page_index..end] {
            push_page_quads(
                page,
                camera,
                options,
                statistics,
                &mut quads,
                &mut translucent,
            );
        }
        page_index = end;

        if quads.is_empty() {
            continue;
        }

        let quad_bins = bin_quads(&quads, width, height, tile_size);
        let touched: Vec<usize> = quad_bins
            .iter()
            .enumerate()
            .filter(|(_, bin)| !bin.is_empty())
            .map(|(index, _)| index)
            .collect();
        render_stream_chunk(
            &tiles, &quads, &quad_bins, &touched, options, threads, textures,
        );
    }

    if !translucent.is_empty() {
        let triangle_bins = bin_triangles(&translucent, width, height, tile_size);
        let touched: Vec<usize> = triangle_bins
            .iter()
            .enumerate()
            .filter(|(_, bin)| !bin.is_empty())
            .map(|(index, _)| index)
            .collect();
        render_stream_translucent(
            &tiles,
            &translucent,
            &triangle_bins,
            &touched,
            options,
            threads,
            textures,
        );
    }

    let mut output = vec![0u8; (u64::from(width) * u64::from(height) * 4) as usize];
    for tile in tiles {
        let tile = tile.into_inner().expect("stream tile mutex poisoned");
        statistics.pixels_tested += tile.counters.pixels_tested;
        statistics.pixels_written += tile.counters.pixels_written;
        copy_tile(&tile.job, &tile.colors, &mut output, width);
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

struct StreamTile {
    job: TileJob,
    colors: Vec<[f32; 4]>,
    depths: Vec<f64>,
    counters: TileCounters,
}

impl StreamTile {
    fn new(job: TileJob, background: Background) -> Self {
        let count = (u64::from(job.width) * u64::from(job.height)) as usize;
        let mut colors = vec![[0.0f32; 4]; count];
        if let Background::Solid(color) = background {
            let fill = [
                f32::from(color.r) / 255.0,
                f32::from(color.g) / 255.0,
                f32::from(color.b) / 255.0,
                f32::from(color.a) / 255.0,
            ];
            colors.fill(fill);
        }

        Self {
            job,
            colors,
            depths: vec![f64::INFINITY; count],
            counters: TileCounters::default(),
        }
    }
}

fn render_stream_chunk(
    tiles: &[std::sync::Mutex<StreamTile>],
    quads: &[QuadRender],
    bins: &[Vec<u32>],
    touched: &[usize],
    options: &RenderOptions,
    threads: usize,
    textures: Option<&TextureSet>,
) {
    let work = |tile_index: usize| {
        let mut guard = tiles[tile_index]
            .lock()
            .expect("stream tile mutex poisoned");
        let StreamTile {
            job,
            colors,
            depths,
            counters,
        } = &mut *guard;
        let job = *job;
        let mut buffer = TileBuffer {
            x0: job.x0,
            y0: job.y0,
            width: job.width,
            height: job.height,
            colors,
            depths,
            counters,
            textures,
            filter: options.texture_filter,
        };

        for &index in &bins[tile_index] {
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
            buffer.rasterize_quad(quad);
        }
    };

    run_parallel(touched, threads, work);
}

fn render_stream_translucent(
    tiles: &[std::sync::Mutex<StreamTile>],
    triangles: &[Triangle],
    bins: &[Vec<u32>],
    touched: &[usize],
    options: &RenderOptions,
    threads: usize,
    textures: Option<&TextureSet>,
) {
    let work = |tile_index: usize| {
        let mut guard = tiles[tile_index]
            .lock()
            .expect("stream tile mutex poisoned");
        let StreamTile {
            job,
            colors,
            depths,
            counters,
        } = &mut *guard;
        let job = *job;
        let mut buffer = TileBuffer {
            x0: job.x0,
            y0: job.y0,
            width: job.width,
            height: job.height,
            colors,
            depths,
            counters,
            textures,
            filter: options.texture_filter,
        };

        let mut translucent: Vec<&Triangle> = bins[tile_index]
            .iter()
            .map(|&index| &triangles[index as usize])
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
            buffer.rasterize_triangle(
                &triangle.points,
                &colors,
                &uvs,
                triangle.texture,
                triangle.layer,
            );
        }
    };

    run_parallel(touched, threads, work);
}

fn run_parallel<F>(indices: &[usize], threads: usize, work: F)
where
    F: Fn(usize) + Sync,
{
    if threads <= 1 || indices.len() <= 1 {
        for &index in indices {
            work(index);
        }
        return;
    }

    let cursor = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            let cursor = &cursor;
            let work = &work;
            scope.spawn(move || {
                loop {
                    let position = cursor.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(&index) = indices.get(position) else {
                        break;
                    };
                    work(index);
                }
            });
        }
    });
}

fn encode_tile(colors: &[[f32; 4]], width: u32, height: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (u64::from(width) * u64::from(height) * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let color = colors[(y * width + x) as usize];
            let alpha = to_u8(color[3]);
            let [red, green, blue] = if alpha == 0 {
                [0, 0, 0]
            } else {
                [to_u8(color[0]), to_u8(color[1]), to_u8(color[2])]
            };
            let index = ((y * width + x) * 4) as usize;
            rgba[index] = red;
            rgba[index + 1] = green;
            rgba[index + 2] = blue;
            rgba[index + 3] = alpha;
        }
    }
    rgba
}

fn copy_tile(job: &TileJob, colors: &[[f32; 4]], output: &mut [u8], width: u32) {
    let rgba = encode_tile(colors, job.width, job.height);
    let row_bytes = job.width as usize * 4;
    for row in 0..job.height {
        let source = row as usize * row_bytes;
        let target =
            ((u64::from(job.y0 + row) * u64::from(width) + u64::from(job.x0)) as usize) * 4;
        output[target..target + row_bytes].copy_from_slice(&rgba[source..source + row_bytes]);
    }
}
