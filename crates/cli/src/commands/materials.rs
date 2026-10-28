use crate::ui::progress::Progress;
use crate::ui::{self, ACCENT, MUTED};
use crate::{AssetArgs, CliError, LimitArgs, load_materials, load_scene, warn_forced};
use clap::Args;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub(crate) struct MaterialsArgs {
    file: PathBuf,
    /// Print the coverage report as JSON.
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    assets: AssetArgs,
    #[command(flatten)]
    limits: LimitArgs,
}

pub(crate) fn run(args: MaterialsArgs) -> Result<(), CliError> {
    warn_forced(args.limits.force_limits);
    let progress = Progress::new(args.json);
    let parse_stage = progress.stage("Parsing scene");
    let (_, scene) = load_scene(
        &args.file,
        args.limits.force_limits,
        args.limits.ignore_block_entity_nbt,
    )?;
    parse_stage.finish();
    let assets_stage = progress.stage("Loading materials");
    let materials = load_materials(&args.assets)?.ok_or_else(|| {
        CliError::input("ASSET_SOURCE_REQUIRED: provide --client-jar or --assets")
    })?;
    assets_stage.finish();
    let report_stage = progress.stage("Analyzing coverage");
    let report = materials.report(&scene);
    report_stage.finish();

    if args.json {
        let json = serde_json::to_string_pretty(&report).map_err(CliError::internal)?;
        println!("{json}");
        return Ok(());
    }

    ui::outln!(
        "{MUTED}file:{MUTED:#} {ACCENT}{}{ACCENT:#}",
        args.file.display()
    );
    ui::outln!("{MUTED}total blocks:{MUTED:#} {}", report.total_blocks);
    ui::outln!("{MUTED}pack models:{MUTED:#} {}", report.pack_models);
    ui::outln!("{MUTED}chests:{MUTED:#} {}", report.chests);
    ui::outln!("{MUTED}fallback:{MUTED:#} {}", report.fallback);
    ui::outln!("{MUTED}unknown:{MUTED:#} {}", report.unknown);
    ui::outln!("{MUTED}textured:{MUTED:#} {}", report.textured);
    ui::outln!(
        "{MUTED}missing textures:{MUTED:#} {}",
        report.missing_textures
    );
    ui::outln!("{MUTED}entries:{MUTED:#}");
    ui::outln!(
        "{MUTED}{:>10}  {:<10}  {:<9}  STATE{MUTED:#}",
        "COUNT",
        "KIND",
        "TEXTURE"
    );

    for entry in &report.entries {
        let style = match entry.kind.as_str() {
            "pack-model" => ui::SUCCESS,
            "chest" => ui::ACCENT,
            "fallback" => ui::WARNING,
            "unknown" => ui::ERROR,
            _ => ui::MUTED,
        };
        let kind = format!("{:<10}", entry.kind.as_str());
        let texture = format!("{:<9}", if entry.textured { "textured" } else { "color" });
        let texture_style = if entry.textured {
            ui::SUCCESS
        } else {
            ui::MUTED
        };
        ui::outln!(
            "{MUTED}{:>10}{MUTED:#}  {style}{kind}{style:#}  {texture_style}{texture}{texture_style:#}  {}",
            entry.count,
            entry.state
        );
    }

    Ok(())
}
