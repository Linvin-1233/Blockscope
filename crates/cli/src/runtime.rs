//! Parse, mesh, and render orchestration shared by the CLI commands.

use crate::capacity::Capacity;
use crate::{AssetArgs, CliError, read_file_limited};
use litematic_core::{DecodeLimits, LitematicFile, LitematicLimits};
use litematic_material::{AssetSource, MaterialSet, MaterialSnapshot, TextureFilter};
use litematic_mesh::{
    ColorBlockCatalog, Mesh, MeshBuildOptions, MeshLimits, MeshPage, MeshStatistics, PageProgress,
    Rgba, build_mesh_parallel_with_options_and_progress,
    build_mesh_streaming_with_options_and_progress,
};
use litematic_render::{
    Background, RenderOptions, RenderOutput, TileProgress, render_mesh, render_mesh_with_progress,
    render_mesh_with_textures, render_mesh_with_textures_and_progress,
};
use litematic_scene::Scene;
use std::path::Path;
use std::time::Instant;

/// Upper bound for mesh and rasterizer worker threads.
pub(crate) const MAX_THREADS: u32 = 256;

/// Upper bound for the number of yaw angles in one render request.
pub(crate) const MAX_VIEWS: usize = 64;

/// Keep one batch of rendered views under this much output memory.
const MAX_VIEW_BATCH_BYTES: u64 = 256 * 1024 * 1024;

fn build_scene_mesh_with_progress(
    scene: &Scene,
    snapshot: Option<&MaterialSnapshot>,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
    progress: Option<PageProgress<'_>>,
) -> Result<Mesh, CliError> {
    match snapshot {
        Some(snapshot) => build_mesh_parallel_with_options_and_progress(
            scene, snapshot, limits, threads, options, progress,
        )
        .map_err(CliError::from),
        None => {
            let catalog = ColorBlockCatalog::new().map_err(CliError::internal)?;
            let states = scene
                .regions
                .iter()
                .flat_map(|region| region.palette.iter().cloned());
            let snapshot = catalog.snapshot(states);
            build_mesh_parallel_with_options_and_progress(
                scene, &snapshot, limits, threads, options, progress,
            )
            .map_err(CliError::from)
        }
    }
}

pub(crate) fn build_scene_mesh_streaming<F>(
    scene: &Scene,
    snapshot: Option<&MaterialSnapshot>,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
    progress: Option<PageProgress<'_>>,
    sink: F,
) -> Result<MeshStatistics, CliError>
where
    F: FnMut(&[MeshPage]),
{
    match snapshot {
        Some(snapshot) => build_mesh_streaming_with_options_and_progress(
            scene, snapshot, limits, threads, options, progress, sink,
        )
        .map_err(CliError::from),
        None => {
            let catalog = ColorBlockCatalog::new().map_err(CliError::internal)?;
            let states = scene
                .regions
                .iter()
                .flat_map(|region| region.palette.iter().cloned());
            let snapshot = catalog.snapshot(states);
            build_mesh_streaming_with_options_and_progress(
                scene, &snapshot, limits, threads, options, progress, sink,
            )
            .map_err(CliError::from)
        }
    }
}

/// Render views in deterministic batches and hand each output to `sink`.
///
/// Batches are limited by both the thread budget and a fixed output-byte
/// budget, so a multi-view request cannot retain an unbounded number of RGBA
/// buffers. `sink` is called in input order, which lets callers write and drop
/// each view before the next batch is rendered.
pub(crate) fn render_views_with<F>(
    scene: &Scene,
    mesh: &Mesh,
    snapshot: Option<&MaterialSnapshot>,
    options: &[RenderOptions],
    total_threads: u32,
    progress: Option<TileProgress<'_>>,
    mut sink: F,
) -> Result<(Vec<f64>, f64), CliError>
where
    F: FnMut(RenderOutput, f64, &RenderOptions) -> Result<(), CliError>,
{
    if options.is_empty() {
        return Ok((Vec::new(), 0.0));
    }

    let total_threads = effective_threads(total_threads);
    let max_output_bytes = options
        .iter()
        .map(|options| {
            u64::from(options.width)
                .saturating_mul(u64::from(options.height))
                .saturating_mul(4)
        })
        .max()
        .unwrap_or(0);
    let batch_budget = view_batch_budget(max_output_bytes, total_threads);

    let mut times = Vec::with_capacity(options.len());
    let wall_started = Instant::now();
    let mut sink_ms = 0.0;
    let mut start = 0;

    while start < options.len() {
        let batch_size = total_threads.min(options.len() - start).min(batch_budget);
        let end = start + batch_size;
        let batch = &options[start..end];
        let per_view = (total_threads / batch.len()).max(1) as u32;

        if batch.len() == 1 {
            let mut options = batch[0].clone();
            options.threads = per_view;
            let render_started = Instant::now();
            let output = render_one(scene, mesh, snapshot, &options, progress)?;
            let render_ms = render_started.elapsed().as_secs_f64() * 1000.0;
            let sink_started = Instant::now();
            sink(output, render_ms, &batch[0])?;
            sink_ms += sink_started.elapsed().as_secs_f64() * 1000.0;
            times.push(render_ms);
        } else {
            let batch_results: Vec<Result<(RenderOutput, f64), CliError>> =
                std::thread::scope(|scope| {
                    let handles: Vec<_> = batch
                        .iter()
                        .map(|base| {
                            let mut options = base.clone();
                            options.threads = per_view;
                            scope.spawn(move || {
                                let render_started = Instant::now();
                                let output = render_one(scene, mesh, snapshot, &options, progress)?;
                                Ok((output, render_started.elapsed().as_secs_f64() * 1000.0))
                            })
                        })
                        .collect();
                    handles
                        .into_iter()
                        .map(|handle| handle.join().expect("view render worker panicked"))
                        .collect()
                });

            for (index, result) in batch_results.into_iter().enumerate() {
                let (output, render_ms) = result?;
                let sink_started = Instant::now();
                sink(output, render_ms, &batch[index])?;
                sink_ms += sink_started.elapsed().as_secs_f64() * 1000.0;
                times.push(render_ms);
            }
        }

        start = end;
    }

    let render_ms = (wall_started.elapsed().as_secs_f64() * 1000.0 - sink_ms).max(0.0);
    Ok((times, render_ms))
}

fn view_batch_budget(max_output_bytes: u64, total_threads: usize) -> usize {
    MAX_VIEW_BATCH_BYTES
        .checked_div(max_output_bytes)
        .map(|value| (value as usize).max(1))
        .unwrap_or(total_threads)
}

fn render_one(
    scene: &Scene,
    mesh: &Mesh,
    snapshot: Option<&MaterialSnapshot>,
    options: &RenderOptions,
    progress: Option<TileProgress<'_>>,
) -> Result<RenderOutput, CliError> {
    let progress = progress.filter(|_| !litematic_render::will_stream(mesh, options));
    match snapshot {
        Some(snapshot) => match progress {
            Some(progress) => render_mesh_with_textures_and_progress(
                scene,
                mesh,
                options,
                snapshot.textures(),
                progress,
            )
            .map_err(CliError::from),
            None => render_mesh_with_textures(scene, mesh, options, snapshot.textures())
                .map_err(CliError::from),
        },
        None => match progress {
            Some(progress) => {
                render_mesh_with_progress(scene, mesh, options, progress).map_err(CliError::from)
            }
            None => render_mesh(scene, mesh, options).map_err(CliError::from),
        },
    }
}

fn effective_threads(requested: u32) -> usize {
    if requested == 0 {
        std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
    } else {
        requested as usize
    }
}

/// Limits used for parsing and meshing. `--force-limits` removes the caps but
/// leaves a warning so an operator can see the risk in the logs.
pub(crate) fn decode_limits(force: bool, ignore_block_entity_nbt: bool) -> DecodeLimits {
    if force {
        DecodeLimits {
            max_input_bytes: 1 << 40,
            max_decompressed_bytes: 1 << 40,
            max_total_bytes: 1 << 40,
            ignore_block_entity_nbt,
            ..DecodeLimits::default()
        }
    } else {
        DecodeLimits {
            ignore_block_entity_nbt,
            ..DecodeLimits::default()
        }
    }
}

pub(crate) fn scene_limits(force: bool) -> LitematicLimits {
    if force {
        LitematicLimits {
            max_volume_per_region: 1_000_000_000_000,
            max_total_volume: 1_000_000_000_000,
            ..LitematicLimits::default()
        }
    } else {
        LitematicLimits::default()
    }
}

pub(crate) fn mesh_limits(force: bool) -> MeshLimits {
    if force {
        MeshLimits {
            max_pages: usize::MAX,
            max_quads: usize::MAX,
            max_vertices: usize::MAX,
        }
    } else {
        let default = MeshLimits::default();
        let budget = usize::try_from(Capacity::detect().suggested_max_quads).unwrap_or(usize::MAX);
        MeshLimits {
            max_pages: default.max_pages,
            max_quads: default.max_quads.min(budget),
            max_vertices: default.max_vertices.min(budget.saturating_mul(4)),
        }
    }
}

pub(crate) fn apply_render_limits(force: bool, options: &mut RenderOptions) {
    if force {
        options.max_pixels = u64::MAX;
        options.max_supersampled_pixels = u64::MAX;
    }
}

pub(crate) fn warn_forced(force: bool) {
    if force {
        crate::ui::errln!(
            "{}warning:{:#} resource limits are disabled; a large input can exhaust memory",
            crate::ui::WARNING,
            crate::ui::WARNING
        );
    }
}

/// Resolve every block state used by the scene once, so meshing and rendering
/// share one immutable material set.
pub(crate) fn material_snapshot(scene: &Scene, materials: &MaterialSet) -> MaterialSnapshot {
    let states = scene
        .regions
        .iter()
        .flat_map(|region| region.palette.iter().cloned());
    materials.snapshot(states)
}

pub(crate) fn load_materials(args: &AssetArgs) -> Result<Option<MaterialSet>, CliError> {
    if args.client_jar.is_none() && args.assets.is_none() && args.resource_pack.is_none() {
        return Ok(None);
    }

    let mut source = AssetSource::new();
    if let Some(path) = &args.client_jar {
        source.add_jar(path).map_err(CliError::from)?;
    }
    if let Some(path) = &args.assets {
        source.add_dir(path).map_err(CliError::from)?;
    }
    if let Some(path) = &args.resource_pack {
        if path.is_dir() {
            source.add_overlay_dir(path).map_err(CliError::from)?;
        } else {
            source.add_overlay_zip(path).map_err(CliError::from)?;
        }
    }

    MaterialSet::open(source).map(Some).map_err(CliError::from)
}

pub(crate) fn parse_texture_filter(value: &str) -> Result<TextureFilter, CliError> {
    match value {
        "nearest" => Ok(TextureFilter::Nearest),
        "linear" => Ok(TextureFilter::Linear),
        other => Err(CliError::input(format!(
            "INVALID_RENDER_OPTIONS: unknown texture filter '{other}'"
        ))),
    }
}

pub(crate) fn parse_background(value: &str) -> Result<Background, CliError> {
    if value == "transparent" {
        return Ok(Background::Transparent);
    }

    let color = Rgba::from_hex(value)
        .map_err(|error| CliError::input(format!("INVALID_RENDER_OPTIONS: {error}")))?;
    Ok(Background::Solid(color))
}

pub(crate) struct PreparedScene {
    pub(crate) scene: Scene,
    pub(crate) mesh: Mesh,
    pub(crate) snapshot: Option<MaterialSnapshot>,
    pub(crate) mesh_ms: f64,
}

pub(crate) fn prepare_scene(
    scene: Scene,
    materials: Option<&MaterialSet>,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
) -> Result<PreparedScene, CliError> {
    prepare_scene_with_progress(scene, materials, limits, threads, options, None)
}

pub(crate) fn prepare_scene_with_progress(
    scene: Scene,
    materials: Option<&MaterialSet>,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
    progress: Option<PageProgress<'_>>,
) -> Result<PreparedScene, CliError> {
    if threads > MAX_THREADS {
        return Err(CliError::input(format!(
            "INVALID_RENDER_OPTIONS: threads must be at most {MAX_THREADS}"
        )));
    }

    let mesh_started = Instant::now();
    let snapshot = materials.map(|materials| material_snapshot(&scene, materials));
    let mesh = build_scene_mesh_with_progress(
        &scene,
        snapshot.as_ref(),
        limits,
        threads,
        options,
        progress,
    )?;
    let mesh_ms = mesh_started.elapsed().as_secs_f64() * 1000.0;

    Ok(PreparedScene {
        scene,
        mesh,
        snapshot,
        mesh_ms,
    })
}

/// Geometry commands never read block entity contents.
pub(crate) const GEOMETRY_IGNORES_BLOCK_ENTITY_NBT: bool = true;

pub(crate) fn load_scene(
    path: &Path,
    force_limits: bool,
    ignore_block_entity_nbt: bool,
) -> Result<(Vec<u8>, Scene), CliError> {
    let limits = decode_limits(force_limits, ignore_block_entity_nbt);
    let input = read_file_limited(path, limits.max_input_bytes)?;

    let file = LitematicFile::parse_bytes(&input, &limits, &scene_limits(force_limits))
        .map_err(CliError::from)?;

    let scene = file.into_scene().map_err(CliError::from)?;
    Ok((input, scene))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_batch_budget_keeps_default_resolutions_fully_parallel() {
        // 1024x1024 RGBA and 2048x2048 RGBA both fit more than 12 view buffers.
        assert!(view_batch_budget(4 * 1024 * 1024, 12) >= 12);
        assert!(view_batch_budget(16 * 1024 * 1024, 12) >= 12);
    }

    #[test]
    fn view_batch_budget_caps_large_outputs() {
        // 4096x4096 RGBA is 64 MiB; the byte budget permits four at a time.
        assert_eq!(view_batch_budget(64 * 1024 * 1024, 12), 4);
        assert_eq!(view_batch_budget(0, 12), 12);
    }
}
