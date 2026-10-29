use crate::camera::Camera;
use crate::{RenderBackend, RenderError, RenderOptions};
use litematic_material::TextureSet;
use litematic_mesh::Mesh;
use litematic_scene::Scene;
use serde::{Deserialize, Serialize};

mod bin;
mod downsample;
mod geometry;
mod gpu;
mod project;
mod spill;
mod stream;
mod tile;

use bin::{bin_quads, bin_triangles, effective_tile_size, tile_jobs, worker_count};
use downsample::downsample;
use gpu::{GpuFailure, try_render as try_gpu_render};
use project::{QuadRender, Triangle, project_mesh_pages};
pub use stream::StreamingRenderer;
use stream::chunk_ranges;
use tile::{TileProgressSink, render_tile, render_tiles_parallel};

/// Auto selects the GPU at or above this mesh quad count. Below it the device
/// setup and readback cost more than the CPU tile loop.
const GPU_AUTO_MIN_QUADS: u64 = 50_000;

/// Receives `(completed_tiles, total_tiles)` from the rasterizer.
pub type TileProgress<'a> = &'a (dyn Fn(u64, u64) + Sync);

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
    /// Backend that produced the image. `Auto` resolves to `Cpu` or `Gpu`.
    #[serde(default)]
    pub backend_used: RenderBackend,
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
    render_mesh_internal(scene, mesh, options, None, None)
}

/// Render a color-only mesh and report rasterizer tile completion.
///
/// The callback is not invoked when [`will_stream`] is true.
pub fn render_mesh_with_progress(
    scene: &Scene,
    mesh: &Mesh,
    options: &RenderOptions,
    progress: TileProgress<'_>,
) -> Result<RenderOutput, RenderError> {
    render_mesh_internal(scene, mesh, options, None, Some(progress))
}

/// Render a mesh and sample its texture ids from `textures`.
pub fn render_mesh_with_textures(
    scene: &Scene,
    mesh: &Mesh,
    options: &RenderOptions,
    textures: &TextureSet,
) -> Result<RenderOutput, RenderError> {
    render_mesh_internal(scene, mesh, options, Some(textures), None)
}

/// Render a textured mesh and report rasterizer tile completion.
///
/// The callback is not invoked when [`will_stream`] is true.
pub fn render_mesh_with_textures_and_progress(
    scene: &Scene,
    mesh: &Mesh,
    options: &RenderOptions,
    textures: &TextureSet,
    progress: TileProgress<'_>,
) -> Result<RenderOutput, RenderError> {
    render_mesh_internal(scene, mesh, options, Some(textures), Some(progress))
}

/// Whether rendering this mesh will use the page-streaming rasterizer.
///
/// Streaming revisits tiles across page chunks, so callers should show
/// indeterminate progress instead of a tile percentage in that case.
pub fn will_stream(mesh: &Mesh, options: &RenderOptions) -> bool {
    options.force_streaming || table_bytes(mesh) > tile_buffer_bytes(options)
}

fn render_mesh_internal(
    scene: &Scene,
    mesh: &Mesh,
    options: &RenderOptions,
    textures: Option<&TextureSet>,
    progress: Option<TileProgress<'_>>,
) -> Result<RenderOutput, RenderError> {
    options.validate()?;
    let camera = Camera::new(&scene.bounds, options)?;

    // Auto only pays GPU initialization when the mesh is large enough to
    // amortize device setup and readback.
    let backend = options.backend;
    let auto = backend == RenderBackend::Auto;
    if backend != RenderBackend::Cpu {
        if options.force_streaming {
            if backend == RenderBackend::Gpu {
                return Err(RenderError::invalid(
                    "the GPU backend does not support mesh streaming",
                ));
            }
        } else if backend == RenderBackend::Gpu || mesh.statistics.quads >= GPU_AUTO_MIN_QUADS {
            match try_gpu_render(scene, mesh, &camera, options, textures) {
                Ok(output) => return Ok(output),
                Err(GpuFailure::Unavailable(_)) if auto => {}
                Err(failure) => return Err(failure.into_error()),
            }
        }
    }

    let streaming = will_stream(mesh, options);
    if streaming {
        let mut renderer = StreamingRenderer::with_camera(camera, options, textures);
        for range in chunk_ranges(&mesh.pages) {
            renderer.render_pages(&mesh.pages[range])?;
        }
        return renderer.finish();
    }

    let mut statistics = RenderStatistics {
        width: options.width,
        height: options.height,
        supersample: options.supersample,
        internal_width: options.internal_width(),
        internal_height: options.internal_height(),
        backend_used: RenderBackend::Cpu,
        ..Default::default()
    };
    let threads = worker_count(options.threads, mesh.pages.len());
    let (quads, translucent) = project_mesh_pages(mesh, &camera, options, &mut statistics, threads);
    let rgba = rasterize(
        &quads,
        &translucent,
        options,
        &mut statistics,
        textures,
        progress,
    );
    Ok(RenderOutput {
        width: options.width,
        height: options.height,
        rgba,
        statistics,
    })
}

fn rasterize(
    quads: &[QuadRender],
    translucent: &[Triangle],
    options: &RenderOptions,
    statistics: &mut RenderStatistics,
    textures: Option<&TextureSet>,
    progress: Option<TileProgress<'_>>,
) -> Vec<u8> {
    let width = options.internal_width();
    let height = options.internal_height();
    let tile_size = effective_tile_size(width, height, options.tile_size);
    let tiles_x = width.div_ceil(tile_size);
    let tiles_y = height.div_ceil(tile_size);
    statistics.tiles = u64::from(tiles_x) * u64::from(tiles_y);

    let jobs = tile_jobs(width, height, tile_size);
    let quad_bins = bin_quads(quads, width, height, tile_size);
    let triangle_bins = bin_triangles(translucent, width, height, tile_size);
    let threads = worker_count(options.threads, jobs.len());
    let total_tiles = jobs.len() as u64;
    let sink = progress.map(|callback| {
        callback(0, total_tiles);
        TileProgressSink::new(callback, total_tiles)
    });
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
            sink.as_ref(),
        )
    } else {
        jobs.iter()
            .map(|job| {
                let tile = render_tile(
                    quads,
                    translucent,
                    options,
                    job,
                    &quad_bins[job.index],
                    &triangle_bins[job.index],
                    textures,
                );
                if let Some(sink) = &sink {
                    sink.advance();
                }
                tile
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

fn table_bytes(mesh: &Mesh) -> u64 {
    mesh.statistics
        .quads
        .saturating_mul(std::mem::size_of::<QuadRender>() as u64)
}

fn tile_buffer_bytes(options: &RenderOptions) -> u64 {
    u64::from(options.internal_width()) * u64::from(options.internal_height()) * 24
}

#[cfg(test)]
mod tests {
    use super::effective_tile_size;

    #[test]
    fn tile_size_keeps_small_outputs_parallel() {
        assert_eq!(effective_tile_size(256, 256, 64), 8);
        assert_eq!(effective_tile_size(512, 512, 64), 16);
        assert_eq!(effective_tile_size(1024, 1024, 64), 32);
        assert_eq!(effective_tile_size(2048, 2048, 64), 64);
        assert_eq!(effective_tile_size(96, 96, 64), 8);
        assert_eq!(effective_tile_size(64, 64, 1), 1);
    }
}
