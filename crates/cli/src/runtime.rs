//! Parse, mesh, and render orchestration shared by the CLI commands.

use crate::capacity::Capacity;
use crate::{AssetArgs, CliError, read_file_limited};
use litematic_core::{DecodeLimits, LitematicFile, LitematicLimits};
use litematic_material::{AssetSource, MaterialSet, MaterialSnapshot, TextureFilter};
use litematic_mesh::{
    BlockCatalog, ColorBlockCatalog, Mesh, MeshBuildOptions, MeshLimits, MeshPage, MeshStatistics,
    Rgba, build_mesh_parallel_with_options, build_mesh_streaming_with_options, count_mesh_quads,
    estimate_mesh_quads,
};
use litematic_render::{
    Background, RenderOptions, RenderOutput, render_mesh, render_mesh_with_textures,
};
use litematic_scene::{Scene, aggregate_with_threads};
use std::path::Path;
use std::time::Instant;

/// Upper bound for mesh, LOD, and rasterizer worker threads.
pub(crate) const MAX_THREADS: u32 = 256;

/// Pages sampled to decide whether an exact quad count is worth running.
const AUTO_LOD_SAMPLE_PAGES: usize = 128;

fn build_scene_mesh(
    scene: &Scene,
    snapshot: Option<&MaterialSnapshot>,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
) -> Result<Mesh, CliError> {
    match snapshot {
        Some(snapshot) => {
            build_mesh_parallel_with_options(scene, snapshot, limits, threads, options)
                .map_err(CliError::from)
        }
        None => {
            let catalog = ColorBlockCatalog::new().map_err(CliError::internal)?;
            let states = scene
                .regions
                .iter()
                .flat_map(|region| region.palette.iter().cloned());
            let snapshot = catalog.snapshot(states);
            build_mesh_parallel_with_options(scene, &snapshot, limits, threads, options)
                .map_err(CliError::from)
        }
    }
}

/// Count against the limit first so an oversized exact attempt is not meshed.
///
/// Greedy merging is skipped because a merged page can fit budgets the
/// unmerged count rejects.
fn count_and_build_mesh<C>(
    scene: &Scene,
    catalog: &C,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
) -> Result<Option<Mesh>, CliError>
where
    C: BlockCatalog + Sync,
{
    if !options.greedy {
        let estimate = estimate_mesh_quads(
            scene,
            catalog,
            limits,
            AUTO_LOD_SAMPLE_PAGES,
            threads,
            options,
        )
        .map_err(CliError::from)?;
        let budget = limits.max_quads as u64;
        if estimate.saturating_mul(10) > budget.saturating_mul(11)
            && count_mesh_quads(scene, catalog, limits, threads, options)
                .map_err(CliError::from)?
                .is_none()
        {
            return Ok(None);
        }
    }

    build_mesh_parallel_with_options(scene, catalog, limits, threads, options)
        .map(Some)
        .map_err(CliError::from)
}

fn count_and_build_scene_mesh(
    scene: &Scene,
    materials: Option<&MaterialSet>,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
) -> Result<(Option<Mesh>, Option<MaterialSnapshot>), CliError> {
    match materials {
        Some(materials) => {
            let snapshot = material_snapshot(scene, materials);
            let mesh = count_and_build_mesh(scene, &snapshot, limits, threads, options)?;
            Ok((mesh, Some(snapshot)))
        }
        None => {
            let catalog = ColorBlockCatalog::new().map_err(CliError::internal)?;
            let states = scene
                .regions
                .iter()
                .flat_map(|region| region.palette.iter().cloned());
            let snapshot = catalog.snapshot(states);
            let mesh = count_and_build_mesh(scene, &snapshot, limits, threads, options)?;
            Ok((mesh, None))
        }
    }
}

pub(crate) fn build_scene_mesh_streaming<F>(
    scene: &Scene,
    snapshot: Option<&MaterialSnapshot>,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
    sink: F,
) -> Result<MeshStatistics, CliError>
where
    F: FnMut(&[MeshPage]),
{
    match snapshot {
        Some(snapshot) => {
            build_mesh_streaming_with_options(scene, snapshot, limits, threads, options, sink)
                .map_err(CliError::from)
        }
        None => {
            let catalog = ColorBlockCatalog::new().map_err(CliError::internal)?;
            let states = scene
                .regions
                .iter()
                .flat_map(|region| region.palette.iter().cloned());
            let snapshot = catalog.snapshot(states);
            build_mesh_streaming_with_options(scene, &snapshot, limits, threads, options, sink)
                .map_err(CliError::from)
        }
    }
}

/// Render independent views concurrently, splitting the thread budget.
///
/// Returns outputs and per-view render times in input order, plus the wall time
/// for the whole batch.
pub(crate) fn render_views(
    scene: &Scene,
    mesh: &Mesh,
    snapshot: Option<&MaterialSnapshot>,
    options: &[RenderOptions],
    total_threads: u32,
) -> Result<(Vec<RenderOutput>, Vec<f64>, f64), CliError> {
    if options.is_empty() {
        return Ok((Vec::new(), Vec::new(), 0.0));
    }

    let total_threads = effective_threads(total_threads);
    let mut outputs = Vec::with_capacity(options.len());
    let mut times = Vec::with_capacity(options.len());
    let wall_started = Instant::now();
    let mut start = 0;

    while start < options.len() {
        let end = (start + total_threads.min(options.len() - start)).min(options.len());
        let batch = &options[start..end];
        let per_view = (total_threads / batch.len()).max(1) as u32;

        if batch.len() == 1 {
            let mut options = batch[0].clone();
            options.threads = per_view;
            let render_started = Instant::now();
            let output = render_one(scene, mesh, snapshot, &options)?;
            outputs.push(output);
            times.push(render_started.elapsed().as_secs_f64() * 1000.0);
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
                                let output = render_one(scene, mesh, snapshot, &options)?;
                                Ok((output, render_started.elapsed().as_secs_f64() * 1000.0))
                            })
                        })
                        .collect();
                    handles
                        .into_iter()
                        .map(|handle| handle.join().expect("view render worker panicked"))
                        .collect()
                });

            for result in batch_results {
                let (output, render_ms) = result?;
                outputs.push(output);
                times.push(render_ms);
            }
        }

        start = end;
    }

    Ok((
        outputs,
        times,
        wall_started.elapsed().as_secs_f64() * 1000.0,
    ))
}

fn render_one(
    scene: &Scene,
    mesh: &Mesh,
    snapshot: Option<&MaterialSnapshot>,
    options: &RenderOptions,
) -> Result<RenderOutput, CliError> {
    match snapshot {
        Some(snapshot) => render_mesh_with_textures(scene, mesh, options, snapshot.textures())
            .map_err(CliError::from),
        None => render_mesh(scene, mesh, options).map_err(CliError::from),
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
        eprintln!("WARNING: resource limits are disabled; a large input can exhaust memory");
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LodMode {
    Fixed(u32),
    Auto,
}

pub(crate) fn parse_lod(value: &str) -> Result<LodMode, CliError> {
    if value.eq_ignore_ascii_case("auto") {
        return Ok(LodMode::Auto);
    }

    let factor = value.parse::<u32>().map_err(|_| {
        CliError::input(format!(
            "INVALID_RENDER_OPTIONS: invalid lod '{value}', expected 1-16 or auto"
        ))
    })?;
    if !(1..=16).contains(&factor) {
        return Err(CliError::input(
            "INVALID_RENDER_OPTIONS: lod must be between 1 and 16, or auto",
        ));
    }
    Ok(LodMode::Fixed(factor))
}

pub(crate) struct PreparedScene {
    pub(crate) scene: Scene,
    pub(crate) mesh: Mesh,
    pub(crate) snapshot: Option<MaterialSnapshot>,
    pub(crate) factor: u32,
    pub(crate) lod_ms: f64,
    pub(crate) mesh_ms: f64,
}

pub(crate) fn prepare_scene(
    scene: Scene,
    materials: Option<&MaterialSet>,
    lod: LodMode,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
) -> Result<PreparedScene, CliError> {
    if threads > MAX_THREADS {
        return Err(CliError::input(format!(
            "INVALID_RENDER_OPTIONS: threads must be at most {MAX_THREADS}"
        )));
    }

    match lod {
        LodMode::Fixed(factor) => {
            let lod_started = Instant::now();
            let scene = apply_lod(scene, factor, threads)?;
            let lod_ms = lod_started.elapsed().as_secs_f64() * 1000.0;

            let mesh_started = Instant::now();
            let snapshot = materials.map(|materials| material_snapshot(&scene, materials));
            let mesh = build_scene_mesh(&scene, snapshot.as_ref(), limits, threads, options)?;
            let mesh_ms = mesh_started.elapsed().as_secs_f64() * 1000.0;

            Ok(PreparedScene {
                scene,
                mesh,
                snapshot,
                factor,
                lod_ms,
                mesh_ms,
            })
        }
        LodMode::Auto => prepare_auto(scene, materials, limits, threads, options),
    }
}

fn prepare_auto(
    scene: Scene,
    materials: Option<&MaterialSet>,
    base_limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
) -> Result<PreparedScene, CliError> {
    let capacity = Capacity::detect();
    let budget_quads = usize::try_from(capacity.suggested_max_quads).unwrap_or(usize::MAX);
    let limits = MeshLimits {
        max_quads: budget_quads,
        max_vertices: budget_quads.saturating_mul(4),
        max_pages: base_limits.max_pages,
    };

    let mut lod_total = 0.0;
    let mut mesh_total = 0.0;
    let mut last_error = None;

    for factor in [1u32, 2, 4, 8, 16] {
        let lod_started = Instant::now();
        let aggregated = if factor == 1 {
            None
        } else {
            Some(aggregate_with_threads(&scene, factor, threads).map_err(CliError::from)?)
        };
        lod_total += lod_started.elapsed().as_secs_f64() * 1000.0;

        let attempt = {
            let candidate = aggregated.as_ref().unwrap_or(&scene);
            let mesh_started = Instant::now();
            let result =
                count_and_build_scene_mesh(candidate, materials, &limits, threads, options);
            mesh_total += mesh_started.elapsed().as_secs_f64() * 1000.0;
            result
        };

        match attempt {
            Ok((Some(mesh), snapshot)) => {
                let scene = aggregated.unwrap_or(scene);
                return Ok(PreparedScene {
                    scene,
                    mesh,
                    snapshot,
                    factor,
                    lod_ms: lod_total,
                    mesh_ms: mesh_total,
                });
            }
            Ok((None, _)) => continue,
            Err(error) if is_mesh_limit_error(&error) => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        CliError::input("MESH_LIMIT_EXCEEDED: no LOD factor fits the memory budget")
    }))
}

fn is_mesh_limit_error(error: &CliError) -> bool {
    error.message.starts_with("MESH_LIMIT_EXCEEDED")
        || error.message.starts_with("PAGE_LIMIT_EXCEEDED")
}

pub(crate) fn apply_lod(scene: Scene, lod: u32, threads: u32) -> Result<Scene, CliError> {
    if lod <= 1 {
        return Ok(scene);
    }
    aggregate_with_threads(&scene, lod, threads).map_err(CliError::from)
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
