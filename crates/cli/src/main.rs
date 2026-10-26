use clap::{Args, Parser, Subcommand};
use litematic_core::LitematicError;
use litematic_mesh::{MeshBuildOptions, MeshError};
use litematic_render::RenderError;
use litematic_scene::{Manifest, SceneError, sha256};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod capacity;
mod commands;
mod file_io;
mod runtime;
mod serve;
mod supervise;

pub(crate) use file_io::read_file_limited;
pub(crate) use runtime::*;

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
    /// Render a color-block mesh to PNG or directly to terminal graphics.
    #[command(alias = "preview", alias = "term")]
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
pub(crate) struct MeshOptionArgs {
    /// Merge coplanar faces with identical material and lighting into larger
    /// quads. Most useful for large flat or repetitive surfaces.
    #[arg(long)]
    greedy_mesh: bool,
}

impl MeshOptionArgs {
    pub(crate) const fn options(self) -> MeshBuildOptions {
        MeshBuildOptions {
            greedy: self.greedy_mesh,
        }
    }
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
    use litematic_mesh::MeshLimits;
    use litematic_render::RenderOptions;
    use litematic_scene::{BlockStateKey, PackedIndices, Region, Scene, SceneMetadata};

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
            MeshBuildOptions::default(),
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
