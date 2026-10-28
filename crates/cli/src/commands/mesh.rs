use crate::ui::progress::Progress;
use crate::ui::{self, ACCENT, MUTED};
use crate::{
    CliError, GEOMETRY_IGNORES_BLOCK_ENTITY_NBT, LimitArgs, MeshOptionArgs, PreparedScene,
    load_scene, mesh_limits, parse_lod, warn_forced, warn_lod,
};
use clap::Args;
use litematic_mesh::{MeshStatistics, write_obj, write_ply};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Args)]
#[command(after_help = "\
Examples:
  blockscope mesh model.litematic
  blockscope mesh model.litematic --obj out.obj
  blockscope mesh model.litematic --ply out.ply --greedy-mesh")]
pub(crate) struct MeshArgs {
    file: PathBuf,
    /// Print mesh statistics as JSON.
    #[arg(long)]
    json: bool,
    /// Write the mesh as OBJ.
    #[arg(long, value_name = "PATH")]
    obj: Option<PathBuf>,
    /// Write the mesh as PLY with vertex colors.
    #[arg(long, value_name = "PATH")]
    ply: Option<PathBuf>,
    /// Mesh worker threads; 0 selects available parallelism.
    #[arg(long, default_value_t = 0, value_name = "N")]
    threads: u32,
    /// Level of detail: 1 disables it, 2-16 aggregates N-sized cells.
    #[arg(long, default_value = "1", value_name = "FACTOR")]
    lod: String,
    #[command(flatten)]
    mesh: MeshOptionArgs,
    #[command(flatten)]
    limits: LimitArgs,
}

pub(crate) fn run(args: MeshArgs) -> Result<(), CliError> {
    warn_forced(args.limits.force_limits);
    let progress = Progress::new(args.json);
    let parse_stage = progress.stage("Parsing scene");
    let (_, scene) = load_scene(
        &args.file,
        args.limits.force_limits,
        GEOMETRY_IGNORES_BLOCK_ENTITY_NBT,
    )?;
    parse_stage.finish();
    let lod = parse_lod(&args.lod)?;
    warn_lod(lod);
    let bar = progress.bar("Building mesh");
    let prepared = {
        let update = |done, total| bar.update(done, total);
        crate::prepare_scene_with_progress(
            scene,
            None,
            lod,
            &mesh_limits(args.limits.force_limits),
            args.threads,
            args.mesh.options(),
            Some(&update),
        )
    };
    bar.finish();
    let prepared = prepared?;
    let PreparedScene { mesh, .. } = prepared;

    if let Some(path) = &args.obj {
        let stage = progress.stage("Writing OBJ");
        write_export(path, |writer| write_obj(&mesh, writer))?;
        stage.finish();
        ui::outln!("{ACCENT}wrote OBJ:{ACCENT:#} {}", path.display());
    }
    if let Some(path) = &args.ply {
        let stage = progress.stage("Writing PLY");
        write_export(path, |writer| write_ply(&mesh, writer))?;
        stage.finish();
        ui::outln!("{ACCENT}wrote PLY:{ACCENT:#} {}", path.display());
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
    ui::outln!("{MUTED}pages:{MUTED:#} {}", statistics.pages);
    ui::outln!("{MUTED}quads:{MUTED:#} {}", statistics.quads);
    ui::outln!("{MUTED}vertices:{MUTED:#} {}", statistics.vertices);
    ui::outln!(
        "{MUTED}blocks visited:{MUTED:#} {}",
        statistics.blocks_visited
    );
    ui::outln!(
        "{MUTED}blocks rendered:{MUTED:#} {}",
        statistics.blocks_rendered
    );
    ui::outln!(
        "{MUTED}unknown blocks:{MUTED:#} {}",
        statistics.unknown_blocks
    );
    ui::outln!(
        "{MUTED}layers:{MUTED:#} opaque {} cutout {} translucent {}",
        statistics.opaque_quads,
        statistics.cutout_quads,
        statistics.translucent_quads
    );
}
