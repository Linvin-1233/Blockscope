use clap::{Args, Parser, Subcommand};
use litematic_core::{DecodeLimits, LitematicError, LitematicFile, LitematicLimits};
use litematic_material::{AssetSource, MaterialSet, MaterialSnapshot, TextureFilter};
use litematic_mesh::{ColorBlockCatalog, Mesh, MeshError, MeshLimits, Rgba, build_mesh_parallel};
use litematic_render::{Background, RenderError, RenderOptions};
use litematic_scene::{Manifest, Scene, SceneError, aggregate_with_threads, sha256};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

mod capacity;
mod commands;
mod file_io;
mod serve;
mod supervise;

pub(crate) use file_io::read_file_limited;

/// Upper bound for mesh, LOD, and rasterizer worker threads.
pub(crate) const MAX_THREADS: u32 = 256;

/// Litematic parser and render service.
#[derive(Debug, Parser)]
#[command(
    name = "blockscope",
    version,
    about = "Blockscope - headless Litematic image renderer",
    after_help = "Default limits: input 64 MiB, decompressed 256 MiB, NBT depth 64, regions 1024, total volume 200M."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Inspect a litematic and print its manifest.
    Inspect(InspectArgs),
    /// Validate a litematic without printing the full manifest.
    Validate(ValidateArgs),
    /// Print material statistics.
    Stats(StatsArgs),
    /// Build a color-block mesh and optionally export it.
    Mesh(commands::mesh::MeshArgs),
    /// Render a color-block mesh to PNG.
    Render(commands::render::RenderArgs),
    /// Measure parse, mesh, render, and PNG timings.
    Bench(commands::bench::BenchArgs),
    /// Report model and texture coverage from a resource pack.
    Materials(commands::materials::MaterialsArgs),
    /// Print detected CPU and memory capacity with suggested pool settings.
    Capacity(commands::capacity::CapacityArgs),
    /// Run as a long-lived worker speaking newline-delimited JSON on stdio.
    Serve(serve::ServeArgs),
    /// Spawn and route jobs across a pool of worker processes.
    Supervise(supervise::SuperviseArgs),
}

#[derive(Debug, Args, Clone, Copy, Default)]
pub(crate) struct LimitArgs {
    /// Disable parser, mesh, and render size limits. Can exhaust memory.
    #[arg(long)]
    force_limits: bool,
    /// Skip TileEntities payloads. Rendering does not use container contents;
    /// block data before a truncated block-entity list is kept. Always on for
    /// mesh, bench, and render.
    #[arg(long)]
    ignore_block_entity_nbt: bool,
}

#[derive(Debug, Args)]
struct InspectArgs {
    file: PathBuf,
    /// Print the full manifest as JSON.
    #[arg(long)]
    json: bool,
    /// Print compact JSON; implies --json.
    #[arg(long)]
    compact: bool,
    #[command(flatten)]
    limits: LimitArgs,
}

#[derive(Debug, Args)]
struct ValidateArgs {
    file: PathBuf,
    #[command(flatten)]
    limits: LimitArgs,
}

#[derive(Debug, Args)]
struct StatsArgs {
    file: PathBuf,
    /// Print statistics as JSON.
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    limits: LimitArgs,
}

#[derive(Debug, Args, Clone, Default)]
struct AssetArgs {
    /// Read blockstates, models, and textures directly from a client jar.
    #[arg(long)]
    client_jar: Option<PathBuf>,
    /// Read assets from an extracted assets/resource-pack directory.
    #[arg(long)]
    assets: Option<PathBuf>,
    /// Optional resource-pack overlay directory or zip.
    #[arg(long)]
    resource_pack: Option<PathBuf>,
    /// Texture filter: nearest or linear.
    #[arg(long, default_value = "nearest")]
    texture_filter: String,
}

impl AssetArgs {
    /// Rebuild the asset flags for a child worker process.
    fn cli_args(&self) -> Vec<std::ffi::OsString> {
        let mut args = Vec::new();
        for (flag, value) in [
            ("--client-jar", &self.client_jar),
            ("--assets", &self.assets),
            ("--resource-pack", &self.resource_pack),
        ] {
            if let Some(path) = value {
                args.push(flag.into());
                args.push(path.as_os_str().to_os_string());
            }
        }
        args.push("--texture-filter".into());
        args.push(self.texture_filter.clone().into());
        args
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(error.exit_code)
        }
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Inspect(args) => inspect(args),
        Command::Validate(args) => validate(args),
        Command::Stats(args) => stats(args),
        Command::Mesh(args) => commands::mesh::run(args),
        Command::Render(args) => commands::render::run(args),
        Command::Bench(args) => commands::bench::run(args),
        Command::Materials(args) => commands::materials::run(args),
        Command::Capacity(args) => commands::capacity::run(args),
        Command::Serve(args) => serve::run(args),
        Command::Supervise(args) => supervise::run(args),
    }
}

fn inspect(args: InspectArgs) -> Result<(), CliError> {
    warn_forced(args.limits.force_limits);
    let (input, scene) = load_scene(
        &args.file,
        args.limits.force_limits,
        args.limits.ignore_block_entity_nbt,
    )?;
    let manifest = Manifest::from_scene(&scene, sha256(&input), env!("CARGO_PKG_VERSION"));

    if args.json || args.compact {
        let json = if args.compact {
            manifest.to_json()
        } else {
            manifest.to_json_pretty()
        }
        .map_err(CliError::internal)?;
        println!("{json}");
    } else {
        print_summary(&args.file, &manifest);
    }

    Ok(())
}

fn validate(args: ValidateArgs) -> Result<(), CliError> {
    warn_forced(args.limits.force_limits);
    let (_, scene) = load_scene(
        &args.file,
        args.limits.force_limits,
        args.limits.ignore_block_entity_nbt,
    )?;

    println!("OK: {}", args.file.display());
    println!(
        "  litematic: v{} (data {})",
        scene.metadata.litematic_version, scene.metadata.minecraft_data_version
    );
    println!("  regions: {}", scene.regions.len());
    println!("  bounds: {:?}", scene.bounds.size());
    println!(
        "  volume: {} (air {}, non-air {})",
        scene.statistics.volume, scene.statistics.air, scene.statistics.non_air
    );

    if !scene.diagnostics.is_empty() {
        println!("  diagnostics: {}", scene.diagnostics.len());
        for diagnostic in &scene.diagnostics {
            println!(
                "    {} [{}] {}",
                diagnostic.severity.as_str(),
                diagnostic.code.as_str(),
                diagnostic.message
            );
        }
    }

    Ok(())
}

fn stats(args: StatsArgs) -> Result<(), CliError> {
    warn_forced(args.limits.force_limits);
    let (input, scene) = load_scene(
        &args.file,
        args.limits.force_limits,
        args.limits.ignore_block_entity_nbt,
    )?;

    if args.json {
        let manifest = Manifest::from_scene(&scene, sha256(&input), env!("CARGO_PKG_VERSION"));
        let json =
            serde_json::to_string_pretty(&manifest.statistics).map_err(CliError::internal)?;
        println!("{json}");
        return Ok(());
    }

    let statistics = &scene.statistics;
    println!("volume: {}", statistics.volume);
    println!("air: {}", statistics.air);
    println!("non-air: {}", statistics.non_air);
    println!("block entities: {}", statistics.block_entities);
    println!("entities: {}", statistics.entities);
    println!("blocks:");

    let mut blocks: Vec<_> = statistics.blocks.iter().collect();
    blocks.sort_by(|left, right| {
        right
            .1
            .cmp(left.1)
            .then_with(|| left.0.canonical().cmp(&right.0.canonical()))
    });

    for (state, count) in blocks {
        println!("{count:>10}  {}", state.canonical());
    }

    Ok(())
}

fn build_scene_mesh(
    scene: &Scene,
    snapshot: Option<&MaterialSnapshot>,
    limits: &MeshLimits,
    threads: u32,
) -> Result<Mesh, CliError> {
    match snapshot {
        Some(snapshot) => {
            build_mesh_parallel(scene, snapshot, limits, threads).map_err(CliError::from)
        }
        None => {
            let catalog = ColorBlockCatalog::new().map_err(CliError::internal)?;
            let states = scene
                .regions
                .iter()
                .flat_map(|region| region.palette.iter().cloned());
            let snapshot = catalog.snapshot(states);
            build_mesh_parallel(scene, &snapshot, limits, threads).map_err(CliError::from)
        }
    }
}

/// Limits used for parsing and meshing. `--force-limits` removes the caps but
/// leaves a warning so an operator can see the risk in the logs.
fn decode_limits(force: bool, ignore_block_entity_nbt: bool) -> DecodeLimits {
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

fn scene_limits(force: bool) -> LitematicLimits {
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

fn mesh_limits(force: bool) -> MeshLimits {
    if force {
        MeshLimits {
            max_pages: usize::MAX,
            max_quads: usize::MAX,
            max_vertices: usize::MAX,
        }
    } else {
        let default = MeshLimits::default();
        let budget =
            usize::try_from(capacity::Capacity::detect().suggested_max_quads).unwrap_or(usize::MAX);
        MeshLimits {
            max_pages: default.max_pages,
            max_quads: default.max_quads.min(budget),
            max_vertices: default.max_vertices.min(budget.saturating_mul(4)),
        }
    }
}

fn apply_render_limits(force: bool, options: &mut RenderOptions) {
    if force {
        options.max_pixels = u64::MAX;
        options.max_supersampled_pixels = u64::MAX;
    }
}

fn warn_forced(force: bool) {
    if force {
        eprintln!("WARNING: resource limits are disabled; a large input can exhaust memory");
    }
}

/// Resolve every block state used by the scene once, so meshing and rendering
/// share one immutable material set.
fn material_snapshot(scene: &Scene, materials: &MaterialSet) -> MaterialSnapshot {
    let states = scene
        .regions
        .iter()
        .flat_map(|region| region.palette.iter().cloned());
    materials.snapshot(states)
}

fn load_materials(args: &AssetArgs) -> Result<Option<MaterialSet>, CliError> {
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

fn parse_texture_filter(value: &str) -> Result<TextureFilter, CliError> {
    match value {
        "nearest" => Ok(TextureFilter::Nearest),
        "linear" => Ok(TextureFilter::Linear),
        other => Err(CliError::input(format!(
            "INVALID_RENDER_OPTIONS: unknown texture filter '{other}'"
        ))),
    }
}

fn parse_background(value: &str) -> Result<Background, CliError> {
    if value == "transparent" {
        return Ok(Background::Transparent);
    }

    let color = Rgba::from_hex(value)
        .map_err(|error| CliError::input(format!("INVALID_RENDER_OPTIONS: {error}")))?;
    Ok(Background::Solid(color))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LodMode {
    Fixed(u32),
    Auto,
}

fn parse_lod(value: &str) -> Result<LodMode, CliError> {
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

struct PreparedScene {
    scene: Scene,
    mesh: Mesh,
    snapshot: Option<MaterialSnapshot>,
    factor: u32,
    lod_ms: f64,
    mesh_ms: f64,
}

fn prepare_scene(
    scene: Scene,
    materials: Option<&MaterialSet>,
    lod: LodMode,
    limits: &MeshLimits,
    threads: u32,
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
            let mesh = build_scene_mesh(&scene, snapshot.as_ref(), limits, threads)?;
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
        LodMode::Auto => prepare_auto(scene, materials, limits, threads),
    }
}

fn prepare_auto(
    scene: Scene,
    materials: Option<&MaterialSet>,
    base_limits: &MeshLimits,
    threads: u32,
) -> Result<PreparedScene, CliError> {
    let capacity = capacity::Capacity::detect();
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
            let snapshot = materials.map(|materials| material_snapshot(candidate, materials));
            let mesh_started = Instant::now();
            let result = build_scene_mesh(candidate, snapshot.as_ref(), &limits, threads);
            mesh_total += mesh_started.elapsed().as_secs_f64() * 1000.0;
            result.map(|mesh| (mesh, snapshot))
        };

        match attempt {
            Ok((mesh, snapshot)) => {
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

fn apply_lod(scene: Scene, lod: u32, threads: u32) -> Result<Scene, CliError> {
    if lod <= 1 {
        return Ok(scene);
    }
    aggregate_with_threads(&scene, lod, threads).map_err(CliError::from)
}

/// Geometry commands never read block entity contents.
const GEOMETRY_IGNORES_BLOCK_ENTITY_NBT: bool = true;

fn load_scene(
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

fn print_summary(path: &Path, manifest: &Manifest) {
    println!("file: {}", path.display());
    println!(
        "litematic: v{} (data {})",
        manifest.parser.litematic_version, manifest.parser.minecraft_data_version
    );

    if let Some(name) = &manifest.metadata.name {
        println!("name: {name}");
    }
    if let Some(author) = &manifest.metadata.author {
        println!("author: {author}");
    }

    println!("regions: {}", manifest.regions.len());
    println!(
        "bounds: min {:?}, max {:?}, size {:?}",
        manifest.bounds.min, manifest.bounds.max_exclusive, manifest.bounds.size
    );
    println!(
        "volume: {} (air {}, non-air {})",
        manifest.statistics.volume, manifest.statistics.air, manifest.statistics.non_air
    );
    println!("block entities: {}", manifest.statistics.block_entities);
    println!("entities: {}", manifest.statistics.entities);
    println!("diagnostics: {}", manifest.diagnostics.len());
    println!("input hash: {}", manifest.input_hash);
    println!("scene hash: {}", manifest.scene_hash);
}

#[derive(Debug)]
struct CliError {
    exit_code: u8,
    message: String,
}

impl CliError {
    fn input(message: impl Into<String>) -> Self {
        Self {
            exit_code: 2,
            message: message.into(),
        }
    }

    fn internal(error: impl fmt::Display) -> Self {
        Self {
            exit_code: 1,
            message: format!("INTERNAL_ERROR: {error}"),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<LitematicError> for CliError {
    fn from(error: LitematicError) -> Self {
        Self::input(error.to_string())
    }
}

impl From<MeshError> for CliError {
    fn from(error: MeshError) -> Self {
        Self::input(error.to_string())
    }
}

impl From<RenderError> for CliError {
    fn from(error: RenderError) -> Self {
        Self::input(error.to_string())
    }
}

impl From<litematic_material::MaterialError> for CliError {
    fn from(error: litematic_material::MaterialError) -> Self {
        Self::input(error.to_string())
    }
}

impl From<SceneError> for CliError {
    fn from(error: SceneError) -> Self {
        Self::input(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use litematic_scene::{BlockStateKey, PackedIndices, Region, SceneMetadata};

    fn one_block_scene() -> Scene {
        let states = PackedIndices::from_values(2, &[0]).unwrap();
        let region = Region {
            name: "main".to_string(),
            origin: [0, 0, 0],
            size: [1, 1, 1],
            data_version: 0,
            palette: vec![BlockStateKey::simple("minecraft:stone")],
            states,
            block_entities: Vec::new(),
            entity_count: 0,
            pending_block_tick_count: 0,
            pending_fluid_tick_count: 0,
        };
        Scene::new(SceneMetadata::default(), vec![region], Vec::new()).unwrap()
    }

    #[test]
    fn prepare_scene_rejects_oversized_thread_requests() {
        let error = match prepare_scene(
            one_block_scene(),
            None,
            LodMode::Fixed(1),
            &MeshLimits::default(),
            100_000,
        ) {
            Ok(_) => panic!("oversized thread request must be rejected"),
            Err(error) => error,
        };

        assert!(
            error.message.starts_with("INVALID_RENDER_OPTIONS: threads"),
            "{error}"
        );
    }

    #[test]
    fn forced_limits_raise_every_cap() {
        assert!(mesh_limits(true).max_quads > mesh_limits(false).max_quads);
        assert!(
            decode_limits(true, false).max_input_bytes
                > decode_limits(false, false).max_input_bytes
        );
        assert!(scene_limits(true).max_total_volume > scene_limits(false).max_total_volume);

        let mut options = RenderOptions::default();
        apply_render_limits(true, &mut options);
        assert_eq!(options.max_pixels, u64::MAX);
        assert_eq!(options.max_supersampled_pixels, u64::MAX);
    }
}
