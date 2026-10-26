use crate::{
    CliError, GEOMETRY_IGNORES_BLOCK_ENTITY_NBT, LimitArgs, MeshOptionArgs, PreparedScene,
    load_scene, mesh_limits, parse_lod, prepare_scene, warn_forced,
};
use clap::Args;
use litematic_mesh::{MeshStatistics, write_obj, write_ply};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Args)]
pub(crate) struct MeshArgs {
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
    /// Mesh worker threads; 0 selects available parallelism.
    #[arg(long, default_value_t = 0)]
    threads: u32,
    /// Level of detail: 1 disables, 2-16 sets a factor, "auto" picks one.
    #[arg(long, default_value = "1")]
    lod: String,
    #[command(flatten)]
    mesh: MeshOptionArgs,
    #[command(flatten)]
    limits: LimitArgs,
}

pub(crate) fn run(args: MeshArgs) -> Result<(), CliError> {
    warn_forced(args.limits.force_limits);
    let (_, scene) = load_scene(
        &args.file,
        args.limits.force_limits,
        GEOMETRY_IGNORES_BLOCK_ENTITY_NBT,
    )?;
    let lod = parse_lod(&args.lod)?;
    let PreparedScene { mesh, .. } = prepare_scene(
        scene,
        None,
        lod,
        &mesh_limits(args.limits.force_limits),
        args.threads,
        args.mesh.options(),
    )?;

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
