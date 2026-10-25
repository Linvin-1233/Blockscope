use crate::CliError;
use crate::capacity::Capacity;
use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct CapacityArgs {
    /// Print capacity as JSON.
    #[arg(long)]
    json: bool,
}

pub(crate) fn run(args: CapacityArgs) -> Result<(), CliError> {
    let capacity = Capacity::detect();

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
    println!("suggested max quads: {}", capacity.suggested_max_quads);
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
