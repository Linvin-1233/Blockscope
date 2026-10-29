use crate::ui::progress::Progress;
use crate::ui::{self, ACCENT, HEADER, MUTED, WARNING};
use crate::{
    AssetArgs, CliError, GEOMETRY_IGNORES_BLOCK_ENTITY_NBT, LimitArgs, MeshOptionArgs,
    PreparedScene, apply_render_limits, load_materials, load_scene, mesh_limits, parse_background,
    parse_texture_filter, warn_forced,
};
use clap::Args;
use litematic_render::terminal::enable_windows_virtual_terminal;
use litematic_render::{
    Background, LightingProfile, RenderOptions, RenderOutput, RenderStatistics, StreamingRenderer,
    TerminalOptions, TerminalProtocol, detect_terminal_pixels, detect_terminal_protocol,
    detect_terminal_size, encode_terminal, fit_terminal_cells, write_png,
};
use std::env;
use std::fs::File;
use std::io::{BufWriter, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum StreamMeshMode {
    /// Stream the mesh, which is the default.
    Auto,
    /// Always stream the mesh.
    On,
    /// Retain the full mesh.
    Off,
}

/// Per-view metadata kept after the RGBA buffer has been written and dropped.
struct ViewResult {
    path: PathBuf,
    statistics: RenderStatistics,
    width: u32,
    height: u32,
    yaw: f32,
    elevation: f32,
    lighting: LightingProfile,
    background: Background,
    render_ms: f64,
    png_ms: f64,
    /// Encoded terminal graphics. Every view is encoded before the first image
    /// is written so all summaries stay together and the images remain
    /// contiguous at the end instead of scrolling each other away.
    terminal: Option<Vec<u8>>,
}

#[derive(Debug, Args)]
#[command(after_help = "\
Examples:
  blockscope render model.litematic --output out.png
  blockscope render model.litematic --width 2048 --height 2048 --supersample 2
  blockscope render model.litematic --views 45,225 --threads 8
  blockscope render model.litematic --terminal --no-file")]
pub(crate) struct RenderArgs {
    file: PathBuf,
    /// Output PNG path.
    #[arg(long, default_value = "render.png", value_name = "PATH")]
    output: PathBuf,
    /// Output width in pixels.
    #[arg(long, default_value_t = 1024, value_name = "PIXELS")]
    width: u32,
    /// Output height in pixels.
    #[arg(long, default_value_t = 1024, value_name = "PIXELS")]
    height: u32,
    /// Horizontal rotation in degrees; 0 looks from the south, 45 from the
    /// south-west.
    #[arg(long, default_value_t = 45.0, value_name = "DEGREES")]
    yaw: f32,
    /// Camera elevation in degrees.
    #[arg(long, default_value_t = 30.0, value_name = "DEGREES")]
    elevation: f32,
    /// "transparent" or #RRGGBB / #RRGGBBAA.
    #[arg(long, default_value = "transparent", value_name = "COLOR")]
    background: String,
    /// flat-v1 or simple-ao-v1.
    #[arg(long, default_value = "simple-ao-v1", value_name = "PROFILE")]
    lighting: String,
    /// Supersampling factor: 1, 2, 3, or 4.
    #[arg(long, default_value_t = 2, value_name = "N")]
    supersample: u32,
    /// Rasterizer worker threads; 0 selects available parallelism.
    #[arg(long, default_value_t = 0, value_name = "N")]
    threads: u32,
    /// Comma-separated yaw angles, for example 45,225.
    #[arg(long, value_name = "LIST")]
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
    /// Mesh streaming: auto and on stream the mesh, off keeps the full mesh.
    /// `--stream-mesh` without a value means on.
    #[arg(
        long,
        value_enum,
        num_args = 0..=1,
        default_missing_value = "on",
        default_value = "auto"
    )]
    stream_mesh: StreamMeshMode,
    #[command(flatten)]
    assets: AssetArgs,
    #[command(flatten)]
    mesh: MeshOptionArgs,
    #[command(flatten)]
    limits: LimitArgs,
}

pub(crate) fn run(args: RenderArgs) -> Result<(), CliError> {
    let started = Instant::now();
    warn_forced(args.limits.force_limits);
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

    let show_terminal = args.terminal
        || args.terminal_protocol.is_some()
        || args.terminal_width.is_some()
        || args.terminal_height.is_some();

    let term_options = if show_terminal {
        // The CLI writes encoded graphics directly instead of going through
        // render_to_terminal, so enable Windows VT processing here.
        enable_windows_virtual_terminal();
        if !std::io::stdout().is_terminal() {
            ui::errln!(
                "{WARNING}warning:{WARNING:#} stdout is not a terminal; the image is emitted as raw escape sequences instead of being displayed"
            );
        }
        Some(terminal_options(&args)?)
    } else {
        None
    };
    let stream_mesh = !matches!(args.stream_mesh, StreamMeshMode::Off);

    let view_options: Vec<RenderOptions> = views
        .iter()
        .map(|&yaw| {
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
            options
        })
        .collect();
    for options in &view_options {
        options.validate().map_err(CliError::from)?;
    }

    let progress = Progress::new(args.json);

    let parse_started = Instant::now();
    let parse_stage = progress.stage("Parsing scene");
    let (_, scene) = load_scene(
        &args.file,
        args.limits.force_limits,
        GEOMETRY_IGNORES_BLOCK_ENTITY_NBT,
    )?;
    parse_stage.finish();
    let parse_ms = parse_started.elapsed().as_secs_f64() * 1000.0;

    let materials_stage = progress.stage("Loading materials");
    let materials = load_materials(&args.assets)?;
    materials_stage.finish();

    let limits = mesh_limits(args.limits.force_limits);
    let (scene, snapshot, mesh, mut mesh_ms_total) = if stream_mesh {
        let snapshot = materials
            .as_ref()
            .map(|materials| crate::material_snapshot(&scene, materials));
        (scene, snapshot, None, 0.0)
    } else {
        let bar = progress.bar("Building mesh");
        let prepared = {
            let update = |done, total| bar.update(done, total);
            crate::prepare_scene_with_progress(
                scene,
                materials.as_ref(),
                &limits,
                args.threads,
                args.mesh.options(),
                Some(&update),
            )
        };
        bar.finish();
        let prepared = prepared?;
        let PreparedScene {
            scene,
            mesh,
            snapshot,
            mesh_ms,
        } = prepared;
        (scene, snapshot, Some(mesh), mesh_ms)
    };

    let mut results = Vec::with_capacity(view_count);
    if let Some(mesh) = &mesh {
        let single_tile_progress =
            view_count == 1 && !litematic_render::will_stream(mesh, &view_options[0]);
        let tile_bar = single_tile_progress.then(|| progress.bar("Rendering"));
        let tile_update = |done, total| {
            if let Some(bar) = &tile_bar {
                bar.update(done, total);
            }
        };
        let tile_progress: Option<&(dyn Fn(u64, u64) + Sync)> =
            single_tile_progress.then_some(&tile_update);
        let render_stage = (!single_tile_progress).then(|| progress.stage("Rendering"));
        let result = crate::render_views_with(
            &scene,
            mesh,
            snapshot.as_ref(),
            &view_options,
            args.threads,
            tile_progress,
            |output, render_ms, options| {
                let write_stage = progress.stage(if args.no_file {
                    "Displaying image"
                } else {
                    "Writing output"
                });
                let path = view_output_path(&args.output, view_count, options.yaw);
                results.push(write_view_output(
                    output,
                    options,
                    path,
                    render_ms,
                    args.json,
                    args.no_file,
                    term_options.as_ref(),
                )?);
                write_stage.finish();
                if let Some(stage) = &render_stage {
                    stage.message(format!("Rendering view {}/{view_count}", results.len()));
                }
                Ok(())
            },
        );
        if let Some(bar) = tile_bar {
            bar.finish();
        }
        if let Some(stage) = render_stage {
            stage.finish();
        }
        result?;
    } else {
        for (view_index, options) in view_options.iter().enumerate() {
            let mut renderer = StreamingRenderer::new(
                &scene,
                options,
                snapshot.as_ref().map(|snapshot| snapshot.textures()),
            )
            .map_err(CliError::from)?;
            let mut render_chunk_ms = 0.0;
            let mut spill_error = None;
            let build_started = Instant::now();
            let build_bar = progress.bar("Building and rendering mesh");
            if view_count > 1 {
                build_bar.message(format!("View {}/{view_count}: meshing", view_index + 1));
            }
            let result = {
                let update = |done, total| build_bar.update(done, total);
                crate::build_scene_mesh_streaming(
                    &scene,
                    snapshot.as_ref(),
                    &limits,
                    args.threads,
                    args.mesh.options(),
                    Some(&update),
                    |pages| {
                        let chunk_started = Instant::now();
                        if spill_error.is_none()
                            && let Err(error) = renderer.render_pages(pages)
                        {
                            spill_error = Some(error);
                        }
                        render_chunk_ms += chunk_started.elapsed().as_secs_f64() * 1000.0;
                    },
                )
            };
            build_bar.finish();
            result?;
            if let Some(error) = spill_error {
                return Err(CliError::from(error));
            }
            let finish_started = Instant::now();
            let composite_stage = progress.stage("Compositing image");
            let output = renderer.finish().map_err(CliError::from)?;
            composite_stage.finish();
            let finish_ms = finish_started.elapsed().as_secs_f64() * 1000.0;
            let total_stream_ms = build_started.elapsed().as_secs_f64() * 1000.0;
            let build_ms = (total_stream_ms - render_chunk_ms - finish_ms).max(0.0);
            mesh_ms_total += build_ms;
            let path = view_output_path(&args.output, view_count, options.yaw);
            let write_stage = progress.stage(if args.no_file {
                "Displaying image"
            } else {
                "Writing output"
            });
            let written = write_view_output(
                output,
                options,
                path,
                total_stream_ms - build_ms,
                args.json,
                args.no_file,
                term_options.as_ref(),
            );
            write_stage.finish();
            results.push(written?);
        }
    }

    let total_ms = started.elapsed().as_secs_f64() * 1000.0;

    if args.json {
        if results.len() == 1 {
            let json =
                serde_json::to_string_pretty(&results[0].statistics).map_err(CliError::internal)?;
            println!("{json}");
        } else {
            let entries: Vec<_> = results
                .iter()
                .map(|view| {
                    serde_json::json!({
                        "output": view.path.display().to_string(),
                        "statistics": &view.statistics,
                    })
                })
                .collect();
            let json = serde_json::to_string_pretty(&entries).map_err(CliError::internal)?;
            println!("{json}");
        }
    } else {
        for (index, view) in results.iter().enumerate() {
            if results.len() > 1 {
                ui::outln!("\n{HEADER}view {}/{}{HEADER:#}", index + 1, results.len());
            }
            if args.no_file {
                if let Some(opts) = &term_options {
                    ui::outln!(
                        "{MUTED}output:{MUTED:#} terminal ({})",
                        opts.protocol.name()
                    );
                }
            } else {
                ui::outln!(
                    "{MUTED}output:{MUTED:#} {ACCENT}{}{ACCENT:#}",
                    view.path.display()
                );
            }
            ui::outln!("{MUTED}size:{MUTED:#} {}x{}", view.width, view.height);
            ui::outln!(
                "{MUTED}yaw:{MUTED:#} {}, {MUTED}elevation:{MUTED:#} {}",
                view.yaw,
                view.elevation
            );
            ui::outln!("{MUTED}lighting:{MUTED:#} {}", view.lighting.as_str());
            ui::outln!("{MUTED}background:{MUTED:#} {}", view.background.as_str());
            ui::outln!(
                "{MUTED}supersample:{MUTED:#} {} {MUTED}(internal {}x{}){MUTED:#}",
                view.statistics.supersample,
                view.statistics.internal_width,
                view.statistics.internal_height
            );
            print_render_stats(&view.statistics);
            if args.no_file {
                ui::outln!("{MUTED}render:{MUTED:#} {:.1} ms", view.render_ms);
            } else {
                ui::outln!(
                    "{MUTED}render:{MUTED:#} {:.1} ms, {MUTED}png:{MUTED:#} {:.1} ms",
                    view.render_ms,
                    view.png_ms
                );
            }
        }
        ui::outln!(
            "{MUTED}timings:{MUTED:#} parse {parse_ms:.1} ms, mesh {mesh_ms_total:.1} ms, total {total_ms:.1} ms"
        );
        if let Some(snapshot) = &snapshot {
            let diagnostics = snapshot.diagnostics();
            ui::outln!(
                "{MUTED}materials:{MUTED:#} pack {} states, chest {} states, fallback {} states, missing models {} missing textures {}",
                diagnostics.pack_models,
                diagnostics.chests,
                diagnostics.fallback_shapes,
                diagnostics.missing_models,
                diagnostics.missing_textures
            );
        }
        // Emit graphics last so no further text can scroll the image away.
        for view in &results {
            if let Some(bytes) = &view.terminal {
                let mut stdout = std::io::stdout().lock();
                stdout
                    .write_all(bytes)
                    .and_then(|()| stdout.flush())
                    .map_err(|error| CliError::input(format!("IO_ERROR: stdout: {error}")))?;
            }
        }
    }

    Ok(())
}

/// Rows kept free below the image so the shell prompt cannot push it off-screen.
const TERMINAL_PROMPT_ROWS: u16 = 2;

/// Smallest cell size assumed when the terminal reports no pixel dimensions
/// (common for ptys and some wrappers). It is a lower bound, so the resulting
/// cell footprint is an upper bound and the image is never left unbounded.
const MIN_CELL_PX: (u32, u32) = (8, 16);

/// Build terminal options, fitting the image to the viewport unless the user
/// pinned the cell size with `--terminal-width` / `--terminal-height`.
fn terminal_options(args: &RenderArgs) -> Result<TerminalOptions, CliError> {
    let protocol = match args.terminal_protocol.as_deref() {
        Some(p) if p.eq_ignore_ascii_case("auto") => detect_display_protocol(),
        Some(p) => TerminalProtocol::parse(p).ok_or_else(|| {
            CliError::input(format!(
                "INVALID_TERMINAL_PROTOCOL: unknown protocol '{p}', expected one of: auto, kitty, iterm2, sixel, halfblocks"
            ))
        })?,
        None => detect_display_protocol(),
    };
    let mut options = TerminalOptions {
        protocol,
        width: args.terminal_width,
        height: args.terminal_height,
        ..Default::default()
    };
    if options.width.is_none() && options.height.is_none() {
        fit_to_viewport(&mut options, args.width, args.height);
    }
    Ok(options)
}

/// Multiplexers drop graphics sequences unless passthrough is enabled, so draw
/// with half blocks there instead of emitting an invisible image.
fn detect_display_protocol() -> TerminalProtocol {
    if env::var_os("TMUX").is_some() || env::var_os("STY").is_some() {
        TerminalProtocol::Halfblocks
    } else {
        detect_terminal_protocol()
    }
}

/// Constrain the encoded image to the visible grid. Only one axis is limited so
/// the terminal keeps the aspect ratio from the unconstrained side.
fn fit_to_viewport(options: &mut TerminalOptions, image_w: u32, image_h: u32) {
    if options.protocol == TerminalProtocol::Halfblocks {
        return; // The half-block encoder already fits itself to the detected grid.
    }
    let Some((cols, rows)) = detect_terminal_size() else {
        return;
    };
    let avail_cols = u32::from(cols.saturating_sub(1).max(1));
    let avail_rows = u32::from(rows.saturating_sub(TERMINAL_PROMPT_ROWS).max(1));
    // Without pixel metrics the exact cell footprint is unknown; the minimum
    // assumed cell keeps the bound conservative instead of sending native pixels.
    let (cell_w, cell_h) = cell_pixels(cols, rows).unwrap_or(MIN_CELL_PX);

    if options.protocol == TerminalProtocol::Sixel {
        options.pixel_width = Some(avail_cols * cell_w);
        options.pixel_height = Some(avail_rows * cell_h);
        return;
    }
    let (width, height) =
        fit_terminal_cells(image_w, image_h, avail_cols, avail_rows, cell_w, cell_h);
    options.width = width;
    options.height = height;
}

/// Cell size in pixels, which cell-addressed protocols need to count cells.
fn cell_pixels(cols: u16, rows: u16) -> Option<(u32, u32)> {
    let (px_w, px_h) = detect_terminal_pixels()?;
    Some((
        (px_w / u32::from(cols)).max(1),
        (px_h / u32::from(rows)).max(1),
    ))
}

fn view_output_path(output: &Path, view_count: usize, yaw: f32) -> PathBuf {
    if view_count == 1 {
        output.to_path_buf()
    } else {
        output_path_for_yaw(output, yaw)
    }
}

/// Emit one view to the terminal and/or PNG and drop its RGBA buffer.
#[allow(clippy::too_many_arguments)]
fn write_view_output(
    output: RenderOutput,
    options: &RenderOptions,
    output_path: PathBuf,
    render_ms: f64,
    json: bool,
    no_file: bool,
    term_options: Option<&TerminalOptions>,
) -> Result<ViewResult, CliError> {
    let terminal = match term_options {
        // JSON keeps stdout machine-readable, so graphics go to stderr immediately.
        Some(term_options) if json => {
            let bytes = encode_terminal(&output, term_options).map_err(CliError::from)?;
            let mut stderr = std::io::stderr();
            stderr
                .write_all(&bytes)
                .and_then(|()| stderr.flush())
                .map_err(|error| CliError::input(format!("IO_ERROR: stderr: {error}")))?;
            None
        }
        Some(term_options) => Some(encode_terminal(&output, term_options).map_err(CliError::from)?),
        None => None,
    };

    let mut png_ms = 0.0;
    if !no_file {
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

    Ok(ViewResult {
        path: output_path,
        statistics: output.statistics,
        width: output.width,
        height: output.height,
        yaw: options.yaw,
        elevation: options.elevation,
        lighting: options.lighting,
        background: options.background,
        render_ms,
        png_ms,
        terminal,
    })
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
    if views.len() > crate::MAX_VIEWS {
        return Err(CliError::input(format!(
            "INVALID_RENDER_OPTIONS: views must contain at most {} angles",
            crate::MAX_VIEWS
        )));
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
    ui::outln!("{MUTED}tiles:{MUTED:#} {}", statistics.tiles);
    ui::outln!("{MUTED}triangles:{MUTED:#} {}", statistics.triangles);
    ui::outln!(
        "{MUTED}culled backfaces:{MUTED:#} {}",
        statistics.culled_backfaces
    );
    ui::outln!(
        "{MUTED}layer triangles:{MUTED:#} opaque {} cutout {} translucent {}",
        statistics.opaque_triangles,
        statistics.cutout_triangles,
        statistics.translucent_triangles
    );
    ui::outln!(
        "{MUTED}pixels tested:{MUTED:#} {}",
        statistics.pixels_tested
    );
    ui::outln!(
        "{MUTED}pixels written:{MUTED:#} {}",
        statistics.pixels_written
    );
}
