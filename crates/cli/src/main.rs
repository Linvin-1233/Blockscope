use clap::{Args, Parser, Subcommand};
use litematic_core::{DecodeLimits, LitematicError, LitematicFile, LitematicLimits};
use litematic_material::{AssetSource, MaterialSet, MaterialSnapshot, TextureFilter};
use litematic_mesh::{
    ColorBlockCatalog, Mesh, MeshError, MeshLimits, MeshStatistics, Rgba, build_mesh, write_obj,
    write_ply,
};
use litematic_render::{
    Background, LightingProfile, RenderError, RenderOptions, RenderStatistics, render_mesh,
    render_mesh_with_textures, write_png,
};
use litematic_scene::{Manifest, Scene, sha256};
use std::fmt;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

mod capacity;
mod serve;
mod supervise;

/// Litematic parser and render service.
#[derive(Debug, Parser)]
#[command(
    name = "renderd",
    version,
    about = "Litematic parser and render service",
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
    Mesh(MeshArgs),
    /// Render a color-block mesh to PNG.
    Render(RenderArgs),
    /// Measure parse, mesh, render, and PNG timings.
    Bench(BenchArgs),
    /// Report model and texture coverage from a resource pack.
    Materials(MaterialsArgs),
    /// Print detected CPU and memory capacity with suggested pool settings.
    Capacity(CapacityArgs),
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

#[derive(Debug, Args)]
struct MeshArgs {
    file: PathBuf,
    /// Print mesh statistics as JSON.
    #[arg(long)]
    json: bool,
    /// Write the mesh as OBJ.
    #[arg(long)]
    obj: Option<PathBuf>,
    /// Write the mesh as PLY with vertex colors.
    #[arg(long)]
    ply: Option<PathBuf>,
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

#[derive(Debug, Args)]
struct RenderArgs {
    file: PathBuf,
    /// Output PNG path.
    #[arg(long, default_value = "render.png")]
    output: PathBuf,
    /// Output width in pixels.
    #[arg(long, default_value_t = 1024)]
    width: u32,
    /// Output height in pixels.
    #[arg(long, default_value_t = 1024)]
    height: u32,
    /// Horizontal rotation in degrees; 0 looks from the south, 45 from the
    /// south-west.
    #[arg(long, default_value_t = 45.0)]
    yaw: f32,
    /// Camera elevation in degrees.
    #[arg(long, default_value_t = 30.0)]
    elevation: f32,
    /// "transparent" or #RRGGBB / #RRGGBBAA.
    #[arg(long, default_value = "transparent")]
    background: String,
    /// flat-v1 or simple-ao-v1.
    #[arg(long, default_value = "simple-ao-v1")]
    lighting: String,
    /// Supersampling factor: 1, 2, 3, or 4.
    #[arg(long, default_value_t = 2)]
    supersample: u32,
    /// Rasterizer worker threads; 0 selects available parallelism.
    #[arg(long, default_value_t = 0)]
    threads: u32,
    /// Comma-separated yaw angles, for example 45,225.
    #[arg(long)]
    views: Option<String>,
    /// Print render statistics as JSON.
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    assets: AssetArgs,
    #[command(flatten)]
    limits: LimitArgs,
}

#[derive(Debug, Args)]
struct BenchArgs {
    file: PathBuf,
    /// Output width in pixels.
    #[arg(long, default_value_t = 1024)]
    width: u32,
    /// Output height in pixels.
    #[arg(long, default_value_t = 1024)]
    height: u32,
    /// Camera elevation in degrees.
    #[arg(long, default_value_t = 30.0)]
    elevation: f32,
    /// Supersampling factor: 1, 2, 3, or 4.
    #[arg(long, default_value_t = 2)]
    supersample: u32,
    /// Rasterizer worker threads; 0 selects available parallelism.
    #[arg(long, default_value_t = 0)]
    threads: u32,
    /// Number of render iterations.
    #[arg(long, default_value_t = 3)]
    iterations: u32,
    /// flat-v1 or simple-ao-v1.
    #[arg(long, default_value = "simple-ao-v1")]
    lighting: String,
    /// Print the report as JSON.
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    assets: AssetArgs,
    #[command(flatten)]
    limits: LimitArgs,
}

#[derive(Debug, Args)]
struct MaterialsArgs {
    file: PathBuf,
    /// Print the coverage report as JSON.
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    assets: AssetArgs,
    #[command(flatten)]
    limits: LimitArgs,
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
        Command::Mesh(args) => mesh(args),
        Command::Render(args) => render(args),
        Command::Bench(args) => bench(args),
        Command::Materials(args) => materials(args),
        Command::Capacity(args) => capacity(args),
        Command::Serve(args) => serve::run(args),
        Command::Supervise(args) => supervise::run(args),
    }
}

#[derive(Debug, Args)]
struct CapacityArgs {
    /// Print capacity as JSON.
    #[arg(long)]
    json: bool,
}

fn capacity(args: CapacityArgs) -> Result<(), CliError> {
    let capacity = capacity::Capacity::detect();

    if args.json {
        let json = serde_json::to_string_pretty(&capacity).map_err(CliError::internal)?;
        println!("{json}");
        return Ok(());
    }

    println!("platform: {}", capacity.platform);
    println!("cores: {}", capacity.cores);
    match capacity.cpu_quota_milli {
        Some(milli) => println!("cpu quota: {:.1} cores", f64::from(milli) / 1000.0),
        None => println!("cpu quota: unlimited"),
    }
    println!(
        "memory limit: {}",
        format_megabytes(capacity.memory_limit_bytes)
    );
    println!(
        "memory available: {}",
        format_megabytes(capacity.memory_available_bytes)
    );
    println!("suggested workers: {}", capacity.suggested_workers);
    println!("suggested threads: {}", capacity.suggested_threads);
    println!("default tier: {}", capacity.tier());

    Ok(())
}

fn format_megabytes(bytes: u64) -> String {
    if bytes == 0 {
        "unknown".to_string()
    } else {
        format!("{} MB", bytes / (1024 * 1024))
    }
}

fn inspect(args: InspectArgs) -> Result<(), CliError> {
    warn_forced(args.limits.force_limits);
    let (input, scene) = load_scene(&args.file, args.limits.force_limits)?;
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
    let (_, scene) = load_scene(&args.file, args.limits.force_limits)?;

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
    let (input, scene) = load_scene(&args.file, args.limits.force_limits)?;

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

fn mesh(args: MeshArgs) -> Result<(), CliError> {
    warn_forced(args.limits.force_limits);
    let (_, scene) = load_scene(&args.file, args.limits.force_limits)?;
    let catalog = ColorBlockCatalog::new().map_err(CliError::internal)?;
    let mesh = build_mesh(&scene, &catalog, &mesh_limits(args.limits.force_limits))
        .map_err(CliError::from)?;

    if let Some(path) = &args.obj {
        write_export(path, |writer| write_obj(&mesh, writer))?;
        println!("wrote OBJ: {}", path.display());
    }
    if let Some(path) = &args.ply {
        write_export(path, |writer| write_ply(&mesh, writer))?;
        println!("wrote PLY: {}", path.display());
    }

    if args.json {
        let json = serde_json::to_string_pretty(&mesh.statistics).map_err(CliError::internal)?;
        println!("{json}");
    } else {
        print_mesh_stats(&mesh.statistics);
    }

    Ok(())
}

fn render(args: RenderArgs) -> Result<(), CliError> {
    let started = Instant::now();
    warn_forced(args.limits.force_limits);
    let (_, scene) = load_scene(&args.file, args.limits.force_limits)?;
    let parse_ms = started.elapsed().as_secs_f64() * 1000.0;

    let background = parse_background(&args.background)?;

    let lighting = LightingProfile::parse(&args.lighting).ok_or_else(|| {
        CliError::input(format!(
            "INVALID_RENDER_OPTIONS: unknown lighting profile '{}'",
            args.lighting
        ))
    })?;

    let views = parse_views(&args)?;
    let view_count = views.len();
    let texture_filter = parse_texture_filter(&args.assets.texture_filter)?;
    let materials = load_materials(&args.assets)?;

    let mesh_started = Instant::now();
    let snapshot = materials.map(|materials| material_snapshot(&scene, &materials));
    let mesh = build_scene_mesh(
        &scene,
        snapshot.as_ref(),
        &mesh_limits(args.limits.force_limits),
    )?;
    let mesh_ms = mesh_started.elapsed().as_secs_f64() * 1000.0;

    let mut results = Vec::with_capacity(view_count);
    for yaw in views {
        let mut options = RenderOptions {
            width: args.width,
            height: args.height,
            yaw,
            elevation: args.elevation,
            background,
            lighting,
            supersample: args.supersample,
            threads: args.threads,
            texture_filter,
            ..Default::default()
        };
        apply_render_limits(args.limits.force_limits, &mut options);

        let render_started = Instant::now();
        let output = match &snapshot {
            Some(snapshot) => {
                render_mesh_with_textures(&scene, &mesh, &options, snapshot.textures())
                    .map_err(CliError::from)?
            }
            None => render_mesh(&scene, &mesh, &options).map_err(CliError::from)?,
        };
        let render_ms = render_started.elapsed().as_secs_f64() * 1000.0;

        let output_path = if view_count == 1 {
            args.output.clone()
        } else {
            output_path_for_yaw(&args.output, yaw)
        };
        let png_started = Instant::now();
        let file = File::create(&output_path).map_err(|error| {
            CliError::input(format!("IO_ERROR: {}: {error}", output_path.display()))
        })?;
        let mut writer = BufWriter::new(file);
        write_png(&output, &mut writer).map_err(CliError::from)?;
        writer.flush().map_err(|error| {
            CliError::input(format!("IO_ERROR: {}: {error}", output_path.display()))
        })?;
        let png_ms = png_started.elapsed().as_secs_f64() * 1000.0;

        results.push((output_path, output, options, render_ms, png_ms));
    }

    let total_ms = started.elapsed().as_secs_f64() * 1000.0;

    if args.json {
        if results.len() == 1 {
            let statistics = &results[0].1.statistics;
            let json = serde_json::to_string_pretty(statistics).map_err(CliError::internal)?;
            println!("{json}");
        } else {
            let entries: Vec<_> = results
                .iter()
                .map(|(path, output, _, _, _)| {
                    serde_json::json!({
                        "output": path.display().to_string(),
                        "statistics": output.statistics,
                    })
                })
                .collect();
            let json = serde_json::to_string_pretty(&entries).map_err(CliError::internal)?;
            println!("{json}");
        }
    } else {
        for (path, output, options, render_ms, png_ms) in &results {
            println!("output: {}", path.display());
            println!("size: {}x{}", output.width, output.height);
            println!("yaw: {}, elevation: {}", options.yaw, options.elevation);
            println!("lighting: {}", options.lighting.as_str());
            println!("background: {}", options.background.as_str());
            println!(
                "supersample: {} (internal {}x{})",
                output.statistics.supersample,
                output.statistics.internal_width,
                output.statistics.internal_height
            );
            print_render_stats(&output.statistics);
            println!("render: {render_ms:.1} ms, png: {png_ms:.1} ms");
        }
        println!("timings: parse {parse_ms:.1} ms, mesh {mesh_ms:.1} ms, total {total_ms:.1} ms");
        if let Some(snapshot) = &snapshot {
            let diagnostics = snapshot.diagnostics();
            println!(
                "materials: pack {} states, chest {} states, fallback {} states, missing models {} missing textures {}",
                diagnostics.pack_models,
                diagnostics.chests,
                diagnostics.fallback_shapes,
                diagnostics.missing_models,
                diagnostics.missing_textures
            );
        }
    }

    Ok(())
}

fn build_scene_mesh(
    scene: &Scene,
    snapshot: Option<&MaterialSnapshot>,
    limits: &MeshLimits,
) -> Result<Mesh, CliError> {
    match snapshot {
        Some(snapshot) => build_mesh(scene, snapshot, limits).map_err(CliError::from),
        None => {
            let catalog = ColorBlockCatalog::new().map_err(CliError::internal)?;
            build_mesh(scene, &catalog, limits).map_err(CliError::from)
        }
    }
}

/// Limits used for parsing and meshing. `--force-limits` removes the caps but
/// leaves a warning so an operator can see the risk in the logs.
fn decode_limits(force: bool) -> DecodeLimits {
    if force {
        DecodeLimits {
            max_input_bytes: 1 << 40,
            max_decompressed_bytes: 1 << 40,
            max_total_bytes: 1 << 40,
            ..DecodeLimits::default()
        }
    } else {
        DecodeLimits::default()
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
        MeshLimits::default()
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

fn materials(args: MaterialsArgs) -> Result<(), CliError> {
    warn_forced(args.limits.force_limits);
    let (_, scene) = load_scene(&args.file, args.limits.force_limits)?;
    let materials = load_materials(&args.assets)?.ok_or_else(|| {
        CliError::input("ASSET_SOURCE_REQUIRED: provide --client-jar or --assets")
    })?;
    let report = materials.report(&scene);

    if args.json {
        let json = serde_json::to_string_pretty(&report).map_err(CliError::internal)?;
        println!("{json}");
        return Ok(());
    }

    println!("file: {}", args.file.display());
    println!("total blocks: {}", report.total_blocks);
    println!("pack models: {}", report.pack_models);
    println!("chests: {}", report.chests);
    println!("fallback: {}", report.fallback);
    println!("unknown: {}", report.unknown);
    println!("textured: {}", report.textured);
    println!("missing textures: {}", report.missing_textures);
    println!("entries:");

    for entry in &report.entries {
        println!(
            "{:>10}  {:<10} {}  {}",
            entry.count,
            entry.kind.as_str(),
            if entry.textured { "textured" } else { "color" },
            entry.state
        );
    }

    Ok(())
}

fn parse_views(args: &RenderArgs) -> Result<Vec<f32>, CliError> {
    let Some(value) = &args.views else {
        return Ok(vec![args.yaw]);
    };

    let views: Vec<f32> = value
        .split(',')
        .map(|part| {
            part.trim().parse::<f32>().map_err(|_| {
                CliError::input(format!(
                    "INVALID_RENDER_OPTIONS: invalid view angle '{}'",
                    part.trim()
                ))
            })
        })
        .collect::<Result<_, _>>()?;

    if views.is_empty() {
        return Err(CliError::input(
            "INVALID_RENDER_OPTIONS: views must not be empty",
        ));
    }
    if views.iter().any(|view| !view.is_finite()) {
        return Err(CliError::input(
            "INVALID_RENDER_OPTIONS: view angles must be finite",
        ));
    }

    Ok(views)
}

fn output_path_for_yaw(path: &Path, yaw: f32) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("render");
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("png");
    path.with_file_name(format!("{stem}-{yaw}.{extension}"))
}

#[derive(Debug, Clone, Copy)]
struct TimingSummary {
    min: f64,
    median: f64,
    mean: f64,
    max: f64,
}

fn summarize(samples: &[f64]) -> TimingSummary {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);

    let min = sorted.first().copied().unwrap_or(0.0);
    let max = sorted.last().copied().unwrap_or(0.0);
    let mean = sorted.iter().sum::<f64>() / sorted.len().max(1) as f64;
    let median = if sorted.is_empty() {
        0.0
    } else if sorted.len() % 2 == 1 {
        sorted[sorted.len() / 2]
    } else {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) * 0.5
    };

    TimingSummary {
        min,
        median,
        mean,
        max,
    }
}

fn timing_json(summary: &TimingSummary) -> serde_json::Value {
    serde_json::json!({
        "min": summary.min,
        "median": summary.median,
        "mean": summary.mean,
        "max": summary.max,
    })
}

fn print_timing(name: &str, summary: &TimingSummary) {
    println!(
        "{name}: min {:.2} ms, median {:.2} ms, mean {:.2} ms, max {:.2} ms",
        summary.min, summary.median, summary.mean, summary.max
    );
}

fn bench(args: BenchArgs) -> Result<(), CliError> {
    if args.iterations == 0 {
        return Err(CliError::input(
            "INVALID_RENDER_OPTIONS: iterations must be positive",
        ));
    }
    warn_forced(args.limits.force_limits);

    let lighting = LightingProfile::parse(&args.lighting).ok_or_else(|| {
        CliError::input(format!(
            "INVALID_RENDER_OPTIONS: unknown lighting profile '{}'",
            args.lighting
        ))
    })?;

    let texture_filter = parse_texture_filter(&args.assets.texture_filter)?;
    let materials = load_materials(&args.assets)?;

    let parse_started = Instant::now();
    let (_, scene) = load_scene(&args.file, args.limits.force_limits)?;
    let parse_ms = parse_started.elapsed().as_secs_f64() * 1000.0;

    let mesh_started = Instant::now();
    let snapshot = materials.map(|materials| material_snapshot(&scene, &materials));
    let mesh = build_scene_mesh(
        &scene,
        snapshot.as_ref(),
        &mesh_limits(args.limits.force_limits),
    )?;
    let mesh_ms = mesh_started.elapsed().as_secs_f64() * 1000.0;

    let mut options = RenderOptions {
        width: args.width,
        height: args.height,
        yaw: 45.0,
        elevation: args.elevation,
        background: Background::Transparent,
        lighting,
        supersample: args.supersample,
        threads: args.threads,
        texture_filter,
        ..Default::default()
    };
    apply_render_limits(args.limits.force_limits, &mut options);
    options.validate().map_err(CliError::from)?;

    let mut render_samples = Vec::with_capacity(args.iterations as usize);
    let mut png_samples = Vec::with_capacity(args.iterations as usize);
    let mut png_bytes = 0u64;
    let mut last_output = None;

    for _ in 0..args.iterations {
        let render_started = Instant::now();
        let output = match &snapshot {
            Some(snapshot) => {
                render_mesh_with_textures(&scene, &mesh, &options, snapshot.textures())
                    .map_err(CliError::from)?
            }
            None => render_mesh(&scene, &mesh, &options).map_err(CliError::from)?,
        };
        render_samples.push(render_started.elapsed().as_secs_f64() * 1000.0);

        let png_started = Instant::now();
        let mut bytes = Vec::new();
        write_png(&output, &mut bytes).map_err(CliError::from)?;
        png_samples.push(png_started.elapsed().as_secs_f64() * 1000.0);

        png_bytes = bytes.len() as u64;
        last_output = Some(output);
    }

    let output = last_output.expect("iterations is positive");
    let totals: Vec<f64> = render_samples
        .iter()
        .zip(&png_samples)
        .map(|(render, png)| render + png)
        .collect();
    let render = summarize(&render_samples);
    let png = summarize(&png_samples);
    let total = summarize(&totals);
    let peak_rss = peak_rss_bytes();

    if args.json {
        let report = serde_json::json!({
            "file": args.file.display().to_string(),
            "iterations": args.iterations,
            "width": options.width,
            "height": options.height,
            "supersample": options.supersample,
            "threads": options.threads,
            "parseMs": parse_ms,
            "meshMs": mesh_ms,
            "renderMs": timing_json(&render),
            "pngMs": timing_json(&png),
            "totalMs": timing_json(&total),
            "mesh": mesh.statistics,
            "renderStatistics": output.statistics,
            "pngBytes": png_bytes,
            "rgbaBytes": output.rgba.len() as u64,
            "peakRssBytes": peak_rss,
            "materials": snapshot.as_ref().map(|snapshot| snapshot.diagnostics().clone()),
        });
        let json = serde_json::to_string_pretty(&report).map_err(CliError::internal)?;
        println!("{json}");
    } else {
        println!("file: {}", args.file.display());
        println!(
            "size: {}x{} (supersample {})",
            options.width, options.height, options.supersample
        );
        println!(
            "threads: {}",
            if options.threads == 0 {
                "auto".to_string()
            } else {
                options.threads.to_string()
            }
        );
        println!("parse: {parse_ms:.2} ms");
        println!("mesh: {mesh_ms:.2} ms");
        print_timing("render", &render);
        print_timing("png", &png);
        print_timing("total", &total);
        println!(
            "mesh: {} pages, {} quads, {} vertices",
            mesh.statistics.pages, mesh.statistics.quads, mesh.statistics.vertices
        );
        println!(
            "buffers: {} rgba bytes, {} png bytes",
            output.rgba.len(),
            png_bytes
        );
        if let Some(peak_rss) = peak_rss {
            println!("peak rss: {} bytes", peak_rss);
        }
        if let Some(snapshot) = &snapshot {
            let diagnostics = snapshot.diagnostics();
            println!(
                "materials: pack {} states, chest {} states, fallback {} states, missing models {} missing textures {}",
                diagnostics.pack_models,
                diagnostics.chests,
                diagnostics.fallback_shapes,
                diagnostics.missing_models,
                diagnostics.missing_textures
            );
        }
    }

    Ok(())
}

/// Peak resident set size. Linux exposes it through `/proc`; other platforms
/// report nothing rather than guessing.
fn peak_rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            let Some(value) = line.strip_prefix("VmHWM:") else {
                continue;
            };
            let kilobytes = value
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<u64>()
                .ok()?;
            return Some(kilobytes * 1024);
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn print_render_stats(statistics: &RenderStatistics) {
    println!("tiles: {}", statistics.tiles);
    println!("triangles: {}", statistics.triangles);
    println!("culled backfaces: {}", statistics.culled_backfaces);
    println!(
        "layer triangles: opaque {} cutout {} translucent {}",
        statistics.opaque_triangles, statistics.cutout_triangles, statistics.translucent_triangles
    );
    println!("pixels tested: {}", statistics.pixels_tested);
    println!("pixels written: {}", statistics.pixels_written);
}

fn write_export<F>(path: &Path, write: F) -> Result<(), CliError>
where
    F: FnOnce(&mut BufWriter<File>) -> std::io::Result<()>,
{
    let file = File::create(path)
        .map_err(|error| CliError::input(format!("IO_ERROR: {}: {error}", path.display())))?;
    let mut writer = BufWriter::new(file);
    write(&mut writer)
        .map_err(|error| CliError::input(format!("IO_ERROR: {}: {error}", path.display())))?;
    writer
        .flush()
        .map_err(|error| CliError::input(format!("IO_ERROR: {}: {error}", path.display())))?;
    Ok(())
}

fn print_mesh_stats(statistics: &MeshStatistics) {
    println!("pages: {}", statistics.pages);
    println!("quads: {}", statistics.quads);
    println!("vertices: {}", statistics.vertices);
    println!("blocks visited: {}", statistics.blocks_visited);
    println!("blocks rendered: {}", statistics.blocks_rendered);
    println!("unknown blocks: {}", statistics.unknown_blocks);
    println!(
        "layers: opaque {} cutout {} translucent {}",
        statistics.opaque_quads, statistics.cutout_quads, statistics.translucent_quads
    );
}

fn load_scene(path: &Path, force_limits: bool) -> Result<(Vec<u8>, Scene), CliError> {
    let input = fs::read(path)
        .map_err(|error| CliError::input(format!("IO_ERROR: {}: {error}", path.display())))?;

    let file = LitematicFile::parse_bytes(
        &input,
        &decode_limits(force_limits),
        &scene_limits(force_limits),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_limits_raise_every_cap() {
        assert!(mesh_limits(true).max_quads > mesh_limits(false).max_quads);
        assert!(decode_limits(true).max_input_bytes > decode_limits(false).max_input_bytes);
        assert!(scene_limits(true).max_total_volume > scene_limits(false).max_total_volume);

        let mut options = RenderOptions::default();
        apply_render_limits(true, &mut options);
        assert_eq!(options.max_pixels, u64::MAX);
        assert_eq!(options.max_supersampled_pixels, u64::MAX);
    }
}
