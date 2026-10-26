use super::downsample::downsample;
use super::{
    QuadRender, RenderStatistics, TileBuffer, TileCounters, TileJob, Triangle, bin_quads,
    bin_triangles, bounds, intersects, merge_page_statistics, push_page_quads, quad_bounds,
    tile_jobs, to_u8, worker_count,
};
use crate::{Background, Camera, RenderError, RenderOptions, RenderOutput};
use litematic_material::TextureSet;
use litematic_mesh::MeshPage;
use litematic_scene::Scene;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Barrier, Mutex, OnceLock};

/// Chunk pages by projected geometry size rather than page count: page sizes
/// vary by two orders of magnitude between meshes, and a fixed page count
/// spawns dozens of worker scopes (and idle barriers) for fine-grained pages
/// while a byte-sized budget keeps the transient quad table bounded.
const STREAM_QUADS_PER_CHUNK: usize = 600_000;

/// Incremental rasterizer for meshes that are built and rendered page by page.
///
/// The renderer owns the tile buffers for the whole image but only the pages
/// passed to [`StreamingRenderer::render_pages`]. Callers must pass pages in
/// mesh order and split only at page boundaries, which preserves the draw order
/// and output of the non-streaming renderer.
pub struct StreamingRenderer<'a> {
    camera: Camera,
    options: &'a RenderOptions,
    textures: Option<&'a TextureSet>,
    tiles: Vec<Mutex<StreamTile>>,
    translucent: Vec<Triangle>,
    statistics: RenderStatistics,
    threads: usize,
    width: u32,
    height: u32,
    tile_size: u32,
}

impl<'a> StreamingRenderer<'a> {
    /// Create a renderer for `scene` and its precomputed camera.
    pub fn new(
        scene: &Scene,
        options: &'a RenderOptions,
        textures: Option<&'a TextureSet>,
    ) -> Result<Self, RenderError> {
        options.validate()?;
        let camera = Camera::new(&scene.bounds, options)?;
        Ok(Self::with_camera(camera, options, textures))
    }

    pub(crate) fn with_camera(
        camera: Camera,
        options: &'a RenderOptions,
        textures: Option<&'a TextureSet>,
    ) -> Self {
        let width = options.internal_width();
        let height = options.internal_height();
        let tile_size = options.tile_size;
        let tiles: Vec<Mutex<StreamTile>> = tile_jobs(width, height, tile_size)
            .into_iter()
            .map(|job| Mutex::new(StreamTile::new(job, options.background)))
            .collect();
        let threads = worker_count(options.threads, tiles.len());
        let statistics = RenderStatistics {
            width: options.width,
            height: options.height,
            supersample: options.supersample,
            internal_width: width,
            internal_height: height,
            tiles: tiles.len() as u64,
            ..Default::default()
        };

        Self {
            camera,
            options,
            textures,
            tiles,
            translucent: Vec::new(),
            statistics,
            threads,
            width,
            height,
            tile_size,
        }
    }

    /// Project and rasterize the next contiguous run of mesh pages.
    pub fn render_pages(&mut self, pages: &[MeshPage]) {
        if pages.is_empty() {
            return;
        }

        // Each worker projects a contiguous run of pages, so merging the parts
        // in worker order keeps the draw order identical to the serial path.
        let slices = page_slices(pages, self.threads);
        let parts: Vec<OnceLock<ChunkPart>> = (0..self.threads).map(|_| OnceLock::new()).collect();
        let barrier = Barrier::new(self.threads);
        let cursor = AtomicUsize::new(0);
        {
            let chunk = ChunkContext {
                pages,
                slices: &slices,
                camera: &self.camera,
                options: self.options,
                textures: self.textures,
                tiles: &self.tiles,
                parts: &parts,
                barrier: &barrier,
                cursor: &cursor,
                width: self.width,
                height: self.height,
                tile_size: self.tile_size,
            };

            std::thread::scope(|scope| {
                let mut handles = Vec::with_capacity(self.threads.saturating_sub(1));
                for worker in 1..self.threads {
                    let chunk = &chunk;
                    handles.push(scope.spawn(move || render_chunk(chunk, worker)));
                }
                render_chunk(&chunk, 0);
                for handle in handles {
                    handle.join().expect("stream worker panicked");
                }
            });
        }

        for part in parts {
            let part = part.into_inner().expect("stream chunk part missing");
            merge_page_statistics(&mut self.statistics, &part.statistics);
            self.translucent.extend(part.translucent);
        }
    }

    /// Resolve translucent geometry, assemble tiles, and downsample.
    pub fn finish(self) -> RenderOutput {
        let Self {
            options,
            textures,
            tiles,
            translucent,
            mut statistics,
            threads,
            width,
            height,
            tile_size,
            camera: _,
        } = self;

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

        let rgba = if options.supersample > 1 {
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
        };

        RenderOutput {
            width: options.width,
            height: options.height,
            rgba,
            statistics,
        }
    }
}

/// Split a chunk into contiguous page slices of roughly equal quad counts, so
/// projection workers finish together instead of waiting on a page-dense tail.
fn page_slices(pages: &[MeshPage], threads: usize) -> Vec<(usize, usize)> {
    let total: usize = pages.iter().map(|page| page.quads.len()).sum();
    let target = total.div_ceil(threads).max(1);
    let mut slices = Vec::with_capacity(threads);
    let mut start = 0;
    let mut quads = 0;

    for (index, page) in pages.iter().enumerate() {
        quads += page.quads.len();
        if slices.len() + 1 < threads && quads >= target * (slices.len() + 1) {
            slices.push((start, index + 1));
            start = index + 1;
        }
    }

    while slices.len() + 1 < threads {
        slices.push((start, start));
    }
    slices.push((start, pages.len()));
    slices
}

pub(super) fn chunk_ranges(pages: &[MeshPage]) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut quads = 0;

    for (index, page) in pages.iter().enumerate() {
        quads += page.quads.len();
        if quads >= STREAM_QUADS_PER_CHUNK {
            ranges.push(start..index + 1);
            start = index + 1;
            quads = 0;
        }
    }

    if start < pages.len() {
        ranges.push(start..pages.len());
    }

    ranges
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

/// Projected geometry for one worker's slice of a streamed page chunk.
struct ChunkPart {
    quads: Vec<QuadRender>,
    bins: Vec<Vec<u32>>,
    translucent: Vec<Triangle>,
    statistics: RenderStatistics,
}

struct ChunkContext<'a> {
    pages: &'a [MeshPage],
    slices: &'a [(usize, usize)],
    camera: &'a Camera,
    options: &'a RenderOptions,
    textures: Option<&'a TextureSet>,
    tiles: &'a [Mutex<StreamTile>],
    parts: &'a [OnceLock<ChunkPart>],
    barrier: &'a Barrier,
    cursor: &'a AtomicUsize,
    width: u32,
    height: u32,
    tile_size: u32,
}

/// Project and bin this worker's pages, then join the tile raster pass.
///
/// The projection used to run on the main thread while the workers were idle;
/// splitting it across the same threads keeps every core busy for the whole
/// chunk and leaves the per-tile draw order unchanged.
fn render_chunk(chunk: &ChunkContext<'_>, worker: usize) {
    let (start, end) = chunk.slices[worker];
    let pages = &chunk.pages[start..end];

    // Reserving the page quad count avoids repeated growth copies; culling
    // only ever makes this an upper bound.
    let mut quads = Vec::with_capacity(pages.iter().map(|page| page.quads.len()).sum());
    let mut translucent = Vec::new();
    let mut statistics = RenderStatistics::default();
    for page in pages {
        push_page_quads(
            page,
            chunk.camera,
            chunk.options,
            &mut statistics,
            &mut quads,
            &mut translucent,
        );
    }

    let bins = bin_quads(&quads, chunk.width, chunk.height, chunk.tile_size);
    let part = ChunkPart {
        quads,
        bins,
        translucent,
        statistics,
    };
    assert!(
        chunk.parts[worker].set(part).is_ok(),
        "stream chunk part set twice"
    );

    chunk.barrier.wait();

    let touched: Vec<usize> = (0..chunk.tiles.len())
        .filter(|&index| {
            chunk
                .parts
                .iter()
                .any(|part| part.get().is_some_and(|part| !part.bins[index].is_empty()))
        })
        .collect();

    loop {
        let position = chunk.cursor.fetch_add(1, Ordering::Relaxed);
        let Some(&tile_index) = touched.get(position) else {
            break;
        };
        render_chunk_tile(chunk, tile_index);
    }
}

fn render_chunk_tile(chunk: &ChunkContext<'_>, tile_index: usize) {
    let mut guard = chunk.tiles[tile_index]
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
        textures: chunk.textures,
        filter: chunk.options.texture_filter,
    };

    for part in chunk.parts {
        let Some(part) = part.get() else {
            continue;
        };
        for &index in &part.bins[tile_index] {
            let quad = &part.quads[index as usize];
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
    }
}

fn render_stream_translucent(
    tiles: &[Mutex<StreamTile>],
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
    let workers = threads.min(indices.len());
    if workers <= 1 {
        for &index in indices {
            work(index);
        }
        return;
    }

    let cursor = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let cursor = &cursor;
            let work = &work;
            scope.spawn(move || {
                loop {
                    let position = cursor.fetch_add(1, Ordering::Relaxed);
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
