use clap::{Args, Parser, Subcommand};
use litematic_core::{DecodeLimits, LitematicError, LitematicFile, LitematicLimits};
use litematic_mesh::{
    ColorBlockCatalog, MeshError, MeshLimits, MeshStatistics, build_mesh, write_obj, write_ply,
};
use litematic_scene::{Manifest, Scene, sha256};
use std::fmt;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
}

#[derive(Debug, Args)]
struct ValidateArgs {
    file: PathBuf,
}

#[derive(Debug, Args)]
struct StatsArgs {
    file: PathBuf,
    /// Print statistics as JSON.
    #[arg(long)]
    json: bool,
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
    }
}

fn inspect(args: InspectArgs) -> Result<(), CliError> {
    let (input, scene) = load_scene(&args.file)?;
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
    let (_, scene) = load_scene(&args.file)?;

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
    let (input, scene) = load_scene(&args.file)?;

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
    let (_, scene) = load_scene(&args.file)?;
    let catalog = ColorBlockCatalog::new().map_err(CliError::internal)?;
    let mesh = build_mesh(&scene, &catalog, &MeshLimits::default()).map_err(CliError::from)?;

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

fn load_scene(path: &Path) -> Result<(Vec<u8>, Scene), CliError> {
    let input = fs::read(path)
        .map_err(|error| CliError::input(format!("IO_ERROR: {}: {error}", path.display())))?;

    let file = LitematicFile::parse_bytes(
        &input,
        &DecodeLimits::default(),
        &LitematicLimits::default(),
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
