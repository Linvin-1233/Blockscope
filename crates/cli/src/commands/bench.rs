use crate::{
    AssetArgs, CliError, GEOMETRY_IGNORES_BLOCK_ENTITY_NBT, LimitArgs, LodMode, PreparedScene,
    apply_render_limits, load_materials, load_scene, mesh_limits, parse_lod, parse_texture_filter,
    prepare_scene, warn_forced,
};
use clap::Args;
use litematic_render::{
    Background, LightingProfile, RenderOptions, render_mesh, render_mesh_with_textures, write_png,
};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Args)]
pub(crate) struct BenchArgs {
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
    /// Level of detail: 1 disables, 2-16 sets a factor, "auto" picks one.
    #[arg(long, default_value = "1")]
    lod: String,
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

#[derive(Debug, Clone, Copy)]
struct TimingSummary {
    min: f64,
    median: f64,
    mean: f64,
    max: f64,
}

pub(crate) fn run(args: BenchArgs) -> Result<(), CliError> {
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
    let (_, scene) = load_scene(
        &args.file,
        args.limits.force_limits,
        GEOMETRY_IGNORES_BLOCK_ENTITY_NBT,
    )?;
    let parse_ms = parse_started.elapsed().as_secs_f64() * 1000.0;
    let lod = parse_lod(&args.lod)?;

    let PreparedScene {
        scene,
        mesh,
        snapshot,
        factor,
        lod_ms,
        mesh_ms,
    } = prepare_scene(
        scene,
        materials.as_ref(),
        lod,
        &mesh_limits(args.limits.force_limits),
        args.threads,
    )?;

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
            "lod": factor,
            "parseMs": parse_ms,
            "lodMs": lod_ms,
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
        if factor > 1 || matches!(lod, LodMode::Auto) {
            println!("lod: {lod_ms:.2} ms (factor {factor})");
        }
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

/// Peak resident set size. Linux exposes it through `/proc`; macOS exposes it
/// through `getrusage`; other platforms report nothing rather than guessing.
fn peak_rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
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
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct TimeVal {
            tv_sec: usize,
            tv_usec: i32,
        }
        #[repr(C)]
        struct RUsage {
            ru_utime: TimeVal,
            ru_stime: TimeVal,
            ru_maxrss: i64,
            ru_ixrss: i64,
            ru_idrss: i64,
            ru_isrss: i64,
            ru_minflt: i64,
            ru_majflt: i64,
            ru_nswap: i64,
            ru_inblock: i64,
            ru_oublock: i64,
            ru_msgsnd: i64,
            ru_msgrcv: i64,
            ru_nsignals: i64,
            ru_nvcsw: i64,
            ru_nivcsw: i64,
        }
        const RUSAGE_SELF: i32 = 0;
        unsafe extern "C" {
            fn getrusage(who: i32, usage: *mut RUsage) -> i32;
        }
        let mut usage: RUsage = unsafe { std::mem::zeroed() };
        let ret = unsafe { getrusage(RUSAGE_SELF, &mut usage) };
        if ret == 0 && usage.ru_maxrss > 0 {
            Some(usage.ru_maxrss as u64)
        } else {
            None
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn peak_rss_on_macos_is_nonzero() {
        let rss = peak_rss_bytes();
        assert!(rss.is_some());
        assert!(rss.unwrap() > 0);
    }
}
