use crate::CliError;
use crate::capacity::Capacity;
use crate::ui::{self, ACCENT, BOLD, MUTED};
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

    ui::outln!("{MUTED}platform:{MUTED:#} {}", capacity.platform);
    ui::outln!("{MUTED}cores:{MUTED:#} {}", capacity.cores);
    match capacity.cpu_quota_milli {
        Some(milli) => ui::outln!(
            "{MUTED}cpu quota:{MUTED:#} {:.1} cores",
            f64::from(milli) / 1000.0
        ),
        None => ui::outln!("{MUTED}cpu quota:{MUTED:#} unlimited"),
    }
    ui::outln!(
        "{MUTED}memory limit:{MUTED:#} {}",
        format_megabytes(capacity.memory_limit_bytes)
    );
    ui::outln!(
        "{MUTED}memory available:{MUTED:#} {}",
        format_megabytes(capacity.memory_available_bytes)
    );
    ui::outln!(
        "{MUTED}suggested workers:{MUTED:#} {BOLD}{}{BOLD:#}",
        capacity.suggested_workers
    );
    ui::outln!(
        "{MUTED}suggested threads:{MUTED:#} {BOLD}{}{BOLD:#}",
        capacity.suggested_threads
    );
    ui::outln!(
        "{MUTED}suggested max quads:{MUTED:#} {BOLD}{}{BOLD:#}",
        capacity.suggested_max_quads
    );
    ui::outln!(
        "{MUTED}default tier:{MUTED:#} {ACCENT}{}{ACCENT:#}",
        capacity.tier()
    );

    Ok(())
}

fn format_megabytes(bytes: u64) -> String {
    if bytes == 0 {
        "unknown".to_string()
    } else {
        format!("{} MB", bytes / (1024 * 1024))
    }
}
