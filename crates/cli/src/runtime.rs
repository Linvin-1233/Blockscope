//! Parse, mesh, and render orchestration shared by the CLI commands.

use crate::capacity::Capacity;
use crate::{AssetArgs, CliError, read_file_limited};
use litematic_core::{DecodeLimits, LitematicFile, LitematicLimits};
use litematic_material::{AssetSource, MaterialSet, MaterialSnapshot, TextureFilter};
use litematic_mesh::{
    ColorBlockCatalog, Mesh, MeshBuildOptions, MeshLimits, MeshPage, MeshStatistics, PageProgress,
    Rgba, build_mesh_parallel_with_options_and_progress,
    build_mesh_streaming_owned_with_options_and_progress, estimate_mesh_quads,
};
#[cfg(feature = "gpu")]
use litematic_render::GpuRenderSession;
use litematic_render::{
    BackendDecision, Background, RenderBackend, RenderError, RenderOptions, RenderOutput,
    StreamingRenderer, TileProgress, render_mesh, render_mesh_with_progress,
    render_mesh_with_textures, render_mesh_with_textures_and_progress, select_backend,
};
use litematic_scene::Scene;
use std::path::Path;
use std::time::Instant;

/// Upper bound for mesh and rasterizer worker threads.
pub(crate) const MAX_THREADS: u32 = 256;

/// Upper bound for the number of yaw angles in one render request.
pub(crate) const MAX_VIEWS: usize = 64;

/// Buffer small CPU-streamed mesh chunks up to this many quads before
/// rendering, so per-chunk projection and tile barriers are amortized.
pub(crate) const STREAM_RENDER_QUADS: usize = 150_000;

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

/// Build a streaming mesh whose chunks are handed to `sink` by value.
pub(crate) fn build_scene_mesh_streaming_owned<F>(
    scene: &Scene,
    snapshot: Option<&MaterialSnapshot>,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
    progress: Option<PageProgress<'_>>,
    sink: F,
) -> Result<MeshStatistics, CliError>
where
    F: FnMut(Vec<MeshPage>),
{
    match snapshot {
        Some(snapshot) => build_mesh_streaming_owned_with_options_and_progress(
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
            build_mesh_streaming_owned_with_options_and_progress(
                scene, &snapshot, limits, threads, options, progress, sink,
            )
            .map_err(CliError::from)
        }
    }
}

/// Streaming renderer selected for a single view.
pub(crate) enum StreamTarget<'a> {
    Cpu(Box<StreamingRenderer<'a>>),
    #[cfg(feature = "gpu")]
    Gpu(Box<GpuRenderSession>),
}

impl StreamTarget<'_> {
    pub(crate) fn is_cpu(&self) -> bool {
        matches!(self, Self::Cpu(_))
    }

    pub(crate) fn render_pages(&mut self, pages: &[MeshPage]) -> Result<(), RenderError> {
        match self {
            Self::Cpu(renderer) => renderer.render_pages(pages),
            #[cfg(feature = "gpu")]
            Self::Gpu(renderer) => renderer.render_pages(pages),
        }
    }

    pub(crate) fn finish(self) -> Result<RenderOutput, RenderError> {
        match self {
            Self::Cpu(renderer) => (*renderer).finish(),
            #[cfg(feature = "gpu")]
            Self::Gpu(renderer) => (*renderer).finish(),
        }
    }
}

pub(crate) fn stream_target<'a>(
    backend: RenderBackend,
    scene: &Scene,
    options: &'a RenderOptions,
    snapshot: Option<&'a MaterialSnapshot>,
) -> Result<StreamTarget<'a>, CliError> {
    let textures = snapshot.map(|snapshot| snapshot.textures());
    match backend {
        RenderBackend::Cpu => Ok(StreamTarget::Cpu(Box::new(StreamingRenderer::new(
            scene, options, textures,
        )?))),
        #[cfg(feature = "gpu")]
        RenderBackend::Gpu => Ok(StreamTarget::Gpu(Box::new(GpuRenderSession::new(
            scene, options, textures,
        )?))),
        #[cfg(not(feature = "gpu"))]
        RenderBackend::Gpu => Err(CliError::input(
            "BACKEND_UNAVAILABLE: the GPU backend is not enabled in this build",
        )),
        RenderBackend::Auto => Err(CliError::input(
            "INVALID_RENDER_OPTIONS: auto streaming is not supported",
        )),
    }
}

/// Deterministic page sample used by the `auto` policy before a streaming mesh
/// exists. Thirty-two pages keep the estimate cheap even for huge scenes.
pub(crate) const POLICY_SAMPLE_PAGES: usize = 32;

pub(crate) fn estimate_scene_quads(
    scene: &Scene,
    snapshot: Option<&MaterialSnapshot>,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
) -> Result<u64, CliError> {
    match snapshot {
        Some(snapshot) => estimate_mesh_quads(
            scene,
            snapshot,
            limits,
            POLICY_SAMPLE_PAGES,
            threads,
            options,
        )
        .map_err(CliError::from),
        None => {
            let catalog = ColorBlockCatalog::new().map_err(CliError::internal)?;
            let states = scene
                .regions
                .iter()
                .flat_map(|region| region.palette.iter().cloned());
            let snapshot = catalog.snapshot(states);
            estimate_mesh_quads(
                scene,
                &snapshot,
                limits,
                POLICY_SAMPLE_PAGES,
                threads,
                options,
            )
            .map_err(CliError::from)
        }
    }
}

/// Resolve `Auto` for a complete mesh so every view uses the same backend and
/// multi-view GPU batching sees an explicit `Gpu` value.
pub(crate) fn resolve_view_backends(
    scene: &Scene,
    mesh: &Mesh,
    snapshot: Option<&MaterialSnapshot>,
    options: &[RenderOptions],
) -> Vec<RenderOptions> {
    // A single view keeps `Auto` so the library entry point can fall back to
    // the CPU when session setup fails. Multi-view jobs must resolve before
    // batching so the shared GPU session is selected.
    if options.len() <= 1 {
        return options.to_vec();
    }
    let textures = snapshot
        .map(|snapshot| snapshot.textures().bytes())
        .unwrap_or(0);
    let mut resolved = options.to_vec();
    let auto = options
        .iter()
        .find(|options| options.backend == RenderBackend::Auto);
    if let Some(auto) = auto {
        let decision = select_backend(
            scene,
            auto,
            mesh.statistics.quads,
            false,
            textures,
            options.len(),
        );
        for options in &mut resolved {
            if options.backend == RenderBackend::Auto {
                options.backend = decision.backend;
            }
        }
    }
    resolved
}

pub(crate) fn backend_decision(
    scene: &Scene,
    options: &RenderOptions,
    quads: u64,
    streaming: bool,
    snapshot: Option<&MaterialSnapshot>,
    views: usize,
) -> BackendDecision {
    let textures = snapshot
        .map(|snapshot| snapshot.textures().bytes())
        .unwrap_or(0);
    select_backend(scene, options, quads, streaming, textures, views)
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
    let auto_requested = options
        .iter()
        .any(|options| options.backend == RenderBackend::Auto);
    let resolved = resolve_view_backends(scene, mesh, snapshot, options);
    let options = resolved.as_slice();

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
    let all_gpu = options
        .iter()
        .all(|options| options.backend == RenderBackend::Gpu);
    let batch_budget = if all_gpu {
        gpu_view_batch_budget(
            options,
            snapshot.map(|snapshot| snapshot.textures().bytes()),
        )
    } else {
        view_batch_budget(max_output_bytes, total_threads)
    };

    let mut times = Vec::with_capacity(options.len());
    let wall_started = Instant::now();
    let mut sink_ms = 0.0;
    let mut start = 0;

    while start < options.len() {
        let batch_size = batch_budget
            .min(options.len() - start)
            .min(total_threads.max(1));
        let end = start + batch_size;
        let batch = &options[start..end];
        let per_view = (total_threads / batch.len()).max(1) as u32;

        #[cfg(feature = "gpu")]
        if all_gpu && batch.len() > 1 {
            let render_started = Instant::now();
            let session = GpuRenderSession::new_views(
                scene,
                batch,
                snapshot.map(|snapshot| snapshot.textures()),
            );
            match session {
                Ok(mut session) => {
                    session.render_pages(&mesh.pages)?;
                    let outputs = session.finish_views()?;
                    let render_ms =
                        render_started.elapsed().as_secs_f64() * 1000.0 / batch.len() as f64;
                    for (index, output) in outputs.into_iter().enumerate() {
                        let sink_started = Instant::now();
                        sink(output, render_ms, &batch[index])?;
                        sink_ms += sink_started.elapsed().as_secs_f64() * 1000.0;
                        times.push(render_ms);
                    }
                    start = end;
                    continue;
                }
                Err(error) if auto_requested => {
                    crate::ui::errln!(
                        "{}warning:{:#} GPU setup failed, falling back to CPU: {}",
                        crate::ui::WARNING,
                        crate::ui::WARNING,
                        error
                    );
                    // Fall through to the CPU loop with a CPU copy of this
                    // batch so a pre-draw failure never loses output.
                    for (index, base) in batch.iter().enumerate() {
                        let mut options = base.clone();
                        options.backend = RenderBackend::Cpu;
                        options.threads = per_view;
                        let render_started = Instant::now();
                        let output = render_one(scene, mesh, snapshot, &options, progress)?;
                        let render_ms = render_started.elapsed().as_secs_f64() * 1000.0;
                        let sink_started = Instant::now();
                        sink(output, render_ms, &batch[index])?;
                        sink_ms += sink_started.elapsed().as_secs_f64() * 1000.0;
                        times.push(render_ms);
                    }
                    start = end;
                    continue;
                }
                Err(error) => return Err(CliError::from(error)),
            }
        }

        if batch.len() == 1 {
            let mut options = batch[0].clone();
            // Let the library re-run the policy for a single view so a
            // pre-draw GPU failure can still fall back to the CPU.
            if auto_requested {
                options.backend = RenderBackend::Auto;
            }
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

/// GPU views need a color, depth, and WBOIT target at internal resolution in
/// addition to the output buffer. Batch by the adapter working set so the
/// session never asks for more than the 60 percent VRAM budget, and keep the
/// old output-byte ceiling for callers that only have CPU memory pressure.
fn gpu_view_batch_budget(options: &[RenderOptions], texture_bytes: Option<u64>) -> usize {
    let per_view = options
        .iter()
        .map(|options| {
            let internal =
                u64::from(options.internal_width()) * u64::from(options.internal_height());
            // Color + depth + two Rgba16Float WBOIT targets, one aligned
            // readback buffer, the output RGBA buffer, and a small allowance.
            let row_stride = (u64::from(options.internal_width()) * 4).div_ceil(256) * 256;
            let readback = row_stride * u64::from(options.internal_height());
            let output = u64::from(options.width)
                .saturating_mul(u64::from(options.height))
                .saturating_mul(4);
            internal
                .saturating_mul(24)
                .saturating_add(readback)
                .saturating_add(output)
                .saturating_add(8 * 1024 * 1024)
        })
        .max()
        .unwrap_or(1);
    let output_bytes = options
        .iter()
        .map(|options| {
            u64::from(options.width)
                .saturating_mul(u64::from(options.height))
                .saturating_mul(4)
        })
        .max()
        .unwrap_or(1);

    #[cfg(feature = "gpu")]
    let budget = litematic_render::gpu_working_set_bytes()
        .map(|budget| {
            budget.saturating_sub(
                texture_bytes
                    .unwrap_or(0)
                    .saturating_add(3 * 32 * 1024 * 1024),
            )
        })
        .filter(|budget| *budget > 0)
        .unwrap_or(MAX_VIEW_BATCH_BYTES);
    #[cfg(not(feature = "gpu"))]
    let budget = {
        let _ = texture_bytes;
        MAX_VIEW_BATCH_BYTES
    };

    let by_vram = budget.checked_div(per_view).unwrap_or(1).max(1) as usize;
    let by_output = view_batch_budget(output_bytes, 1);
    by_vram.min(by_output)
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
