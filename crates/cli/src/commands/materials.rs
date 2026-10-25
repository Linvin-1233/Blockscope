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
    let (_, scene) = load_scene(
        &args.file,
        args.limits.force_limits,
        args.limits.ignore_block_entity_nbt,
    )?;
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
