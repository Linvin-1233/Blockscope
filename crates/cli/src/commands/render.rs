use crate::{
    AssetArgs, CliError, GEOMETRY_IGNORES_BLOCK_ENTITY_NBT, LimitArgs, LodMode, PreparedScene,
    apply_render_limits, load_materials, load_scene, mesh_limits, parse_background, parse_lod,
    parse_texture_filter, prepare_scene, warn_forced,
};
use clap::Args;
use litematic_render::{
    LightingProfile, RenderOptions, RenderStatistics, StreamingRenderer, TerminalOptions,
    TerminalProtocol, detect_terminal_protocol, render_mesh, render_mesh_with_textures,
    render_to_terminal, write_png,
};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Args)]
pub(crate) struct RenderArgs {
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
    /// Level of detail: 1 disables, 2-16 sets a factor, "auto" picks one.
    #[arg(long, default_value = "1")]
    lod: String,
    /// Comma-separated yaw angles, for example 45,225.
    #[arg(long)]
    views: Option<String>,
    /// Print render statistics as JSON.
    #[arg(long)]
    json: bool,
    /// Display the rendered image directly in the terminal.
    #[arg(long, short = 't')]
    terminal: bool,
    /// Terminal graphics protocol: auto, kitty, iterm2, sixel, or halfblocks.
    #[arg(long, value_name = "PROTOCOL")]
    terminal_protocol: Option<String>,
    /// Terminal display width in character columns.
    #[arg(long, value_name = "COLS")]
    terminal_width: Option<u32>,
    /// Terminal display height in character rows.
    #[arg(long, value_name = "ROWS")]
    terminal_height: Option<u32>,
    /// Skip writing output image file (useful with --terminal).
    #[arg(long)]
    no_file: bool,
    /// Build mesh in bounded page chunks instead of retaining the full mesh.
    #[arg(long)]
    stream_mesh: bool,
    #[command(flatten)]
    assets: AssetArgs,
    #[command(flatten)]
    limits: LimitArgs,
}

pub(crate) fn run(args: RenderArgs) -> Result<(), CliError> {
    let started = Instant::now();
    warn_forced(args.limits.force_limits);
    let (_, scene) = load_scene(
        &args.file,
        args.limits.force_limits,
        GEOMETRY_IGNORES_BLOCK_ENTITY_NBT,
    )?;
    let parse_ms = started.elapsed().as_secs_f64() * 1000.0;
    let lod = parse_lod(&args.lod)?;

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

    let show_terminal = args.terminal
        || args.terminal_protocol.is_some()
        || args.terminal_width.is_some()
        || args.terminal_height.is_some();

    let term_options = if show_terminal {
        let protocol = match args.terminal_protocol.as_deref() {
            Some(p) if p.eq_ignore_ascii_case("auto") => detect_terminal_protocol(),
            Some(p) => TerminalProtocol::parse(p).ok_or_else(|| {
                CliError::input(format!(
                    "INVALID_TERMINAL_PROTOCOL: unknown protocol '{p}', expected one of: auto, kitty, iterm2, sixel, halfblocks"
                ))
            })?,
            None => detect_terminal_protocol(),
        };
        Some(TerminalOptions {
            protocol,
            width: args.terminal_width,
            height: args.terminal_height,
            ..Default::default()
        })
    } else {
        None
    };
    if args.stream_mesh && matches!(lod, LodMode::Auto) {
        return Err(CliError::input(
            "INVALID_RENDER_OPTIONS: --stream-mesh cannot be combined with --lod auto",
        ));
    }

    let limits = mesh_limits(args.limits.force_limits);
    let (scene, snapshot, mesh, factor, lod_ms, mut mesh_ms_total) = if args.stream_mesh {
        let factor = match lod {
            LodMode::Fixed(factor) => factor,
            LodMode::Auto => unreachable!("auto LOD was rejected above"),
        };
        let lod_started = Instant::now();
        let scene = crate::apply_lod(scene, factor, args.threads)?;
        let lod_ms = lod_started.elapsed().as_secs_f64() * 1000.0;
        let snapshot = materials
            .as_ref()
            .map(|materials| crate::material_snapshot(&scene, materials));
        (scene, snapshot, None, factor, lod_ms, 0.0)
    } else {
        let PreparedScene {
            scene,
            mesh,
            snapshot,
            factor,
            lod_ms,
            mesh_ms,
        } = prepare_scene(scene, materials.as_ref(), lod, &limits, args.threads)?;
        (scene, snapshot, Some(mesh), factor, lod_ms, mesh_ms)
    };

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

        let (output, render_ms) = if let Some(mesh) = &mesh {
            let render_started = Instant::now();
            let output = match &snapshot {
                Some(snapshot) => {
                    render_mesh_with_textures(&scene, mesh, &options, snapshot.textures())
                        .map_err(CliError::from)?
                }
                None => render_mesh(&scene, mesh, &options).map_err(CliError::from)?,
            };
            (output, render_started.elapsed().as_secs_f64() * 1000.0)
        } else {
            let mut renderer = StreamingRenderer::new(
                &scene,
                &options,
                snapshot.as_ref().map(|snapshot| snapshot.textures()),
            )
            .map_err(CliError::from)?;
            let mut render_chunk_ms = 0.0;
            let build_started = Instant::now();
            crate::build_scene_mesh_streaming(
                &scene,
                snapshot.as_ref(),
                &limits,
                args.threads,
                |pages| {
                    let chunk_started = Instant::now();
                    renderer.render_pages(pages);
                    render_chunk_ms += chunk_started.elapsed().as_secs_f64() * 1000.0;
                },
            )?;
            let finish_started = Instant::now();
            let output = renderer.finish();
            let finish_ms = finish_started.elapsed().as_secs_f64() * 1000.0;
            let total_stream_ms = build_started.elapsed().as_secs_f64() * 1000.0;
            let build_ms = (total_stream_ms - render_chunk_ms - finish_ms).max(0.0);
            mesh_ms_total += build_ms;
            (output, total_stream_ms - build_ms)
        };

        if let Some(term_options) = &term_options {
            if args.json {
                render_to_terminal(&output, term_options, &mut std::io::stderr())
                    .map_err(CliError::from)?;
            } else {
                render_to_terminal(&output, term_options, &mut std::io::stdout())
                    .map_err(CliError::from)?;
            }
        }

        let output_path = if view_count == 1 {
            args.output.clone()
        } else {
            output_path_for_yaw(&args.output, yaw)
        };
        let mut png_ms = 0.0;
        if !args.no_file {
            let png_started = Instant::now();
            let file = File::create(&output_path).map_err(|error| {
                CliError::input(format!("IO_ERROR: {}: {error}", output_path.display()))
            })?;
            let mut writer = BufWriter::new(file);
            write_png(&output, &mut writer).map_err(CliError::from)?;
            writer.flush().map_err(|error| {
                CliError::input(format!("IO_ERROR: {}: {error}", output_path.display()))
            })?;
            png_ms = png_started.elapsed().as_secs_f64() * 1000.0;
        }

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
            if args.no_file {
                if let Some(opts) = &term_options {
                    println!("output: terminal ({})", opts.protocol.name());
                }
            } else {
                println!("output: {}", path.display());
            }
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
            if args.no_file {
                println!("render: {render_ms:.1} ms");
            } else {
                println!("render: {render_ms:.1} ms, png: {png_ms:.1} ms");
            }
        }
        println!(
            "timings: parse {parse_ms:.1} ms, mesh {mesh_ms_total:.1} ms, total {total_ms:.1} ms"
        );
        if factor > 1 || matches!(lod, LodMode::Auto) {
            println!("lod: {lod_ms:.1} ms (factor {factor})");
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
