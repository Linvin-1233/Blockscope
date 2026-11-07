#[cfg(feature = "gpu")]
use crate::ui::WARNING;
use crate::ui::progress::Progress;
use crate::ui::{self, ACCENT, BOLD, MUTED};
use crate::{
    AssetArgs, CliError, GEOMETRY_IGNORES_BLOCK_ENTITY_NBT, LimitArgs, MeshOptionArgs,
    PreparedScene, apply_render_limits, load_materials, load_scene, mesh_limits, parse_background,
    parse_texture_filter, prepare_scene, warn_forced,
};
use clap::Args;
use litematic_render::{
    Background, LightingProfile, RenderBackend, RenderOptions, RenderOutput, render_mesh,
    render_mesh_with_textures,
};
use litematic_video::{
    Animation, Easing, Encoder, Ffmpeg, FramePlan, Keyframe, Quality, VideoEncoder, VideoError,
    VideoFormat, VideoSettings,
};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Args)]
#[command(after_help = "\
Examples:
  blockscope animate build.litematic --output build.mp4 --keyframe 0:45:30 --keyframe 6:405:30
  blockscope animate build.litematic --output build.gif --keyframe 0:45:30 --keyframe 4:405:30 --background transparent
  blockscope animate build.litematic --format mp4 --keyframe 0:45:30 --duration 5 --fps 60")]
pub(crate) struct AnimateArgs {
    file: PathBuf,
    /// Output video path; the extension selects MP4 or GIF when --format is absent.
    #[arg(long, default_value = "animate.mp4", value_name = "PATH")]
    output: PathBuf,
    /// Output format: mp4 or gif.
    #[arg(long, value_name = "FORMAT")]
    format: Option<String>,
    /// Keyframe as TIME:YAW:ELEVATION; repeat for each keyframe.
    #[arg(long, value_name = "TIME:YAW:ELEV", action = clap::ArgAction::Append)]
    keyframe: Vec<String>,
    /// Clip duration in seconds; defaults to the last keyframe time.
    #[arg(long, value_name = "SECONDS")]
    duration: Option<f64>,
    /// Frames per second; defaults to 30 for MP4 and 15 for GIF.
    #[arg(long, value_name = "N")]
    fps: Option<u32>,
    /// Exclude the duplicate endpoint from a looping clip.
    #[arg(long)]
    no_loop: bool,
    /// Interpolation: linear or smooth.
    #[arg(long, default_value = "linear", value_name = "EASING")]
    easing: String,
    /// Encoder: auto, software, nvenc, qsv, amf, videotoolbox, or vaapi.
    #[arg(long, default_value = "auto", value_name = "ENCODER")]
    encoder: String,
    /// Encoder quality: low, medium, or high.
    #[arg(long, default_value = "medium", value_name = "QUALITY")]
    quality: String,
    /// External ffmpeg executable.
    #[arg(long, value_name = "PATH")]
    ffmpeg: Option<PathBuf>,
    /// Output width in pixels.
    #[arg(long, default_value_t = 1024, value_name = "PIXELS")]
    width: u32,
    /// Output height in pixels.
    #[arg(long, default_value_t = 1024, value_name = "PIXELS")]
    height: u32,
    /// Background: transparent or #RRGGBB / #RRGGBBAA. MP4 requires an opaque color.
    #[arg(long, default_value = "#000000", value_name = "COLOR")]
    background: String,
    /// flat-v1 or simple-ao-v1.
    #[arg(long, default_value = "simple-ao-v1", value_name = "PROFILE")]
    lighting: String,
    /// Rasterization backend: cpu, gpu, or auto.
    #[arg(long, default_value = "auto", value_name = "BACKEND")]
    backend: String,
    /// Supersampling factor: 1, 2, 3, or 4.
    #[arg(long, default_value_t = 2, value_name = "N")]
    supersample: u32,
    /// Rasterizer worker threads; 0 selects available parallelism.
    #[arg(long, default_value_t = 0, value_name = "N")]
    threads: u32,
    /// Fit a rotation-invariant bounding sphere for stable turntable framing.
    #[arg(long)]
    sphere_fit: bool,
    /// Print the video report as JSON.
    #[arg(long)]
    json: bool,
    #[arg(long, hide = true, value_name = "MODE")]
    stream_mesh: Option<String>,
    #[arg(long, hide = true, value_name = "LIST")]
    views: Option<String>,
    #[arg(long, hide = true, value_name = "DEGREES")]
    yaw: Option<f32>,
    #[arg(long, hide = true, value_name = "DEGREES")]
    elevation: Option<f32>,
    #[command(flatten)]
    assets: AssetArgs,
    #[command(flatten)]
    mesh: MeshOptionArgs,
    #[command(flatten)]
    limits: LimitArgs,
}

pub(crate) fn run(args: AnimateArgs) -> Result<(), CliError> {
    if args.stream_mesh.is_some()
        || args.views.is_some()
        || args.yaw.is_some()
        || args.elevation.is_some()
    {
        return Err(CliError::input(
            "INVALID_RENDER_OPTIONS: animate does not accept --stream-mesh, --views, --yaw, or --elevation; use --keyframe",
        ));
    }
    warn_forced(args.limits.force_limits);
    let started = Instant::now();
    let progress = Progress::new(args.json);

    let format = resolve_format(&args)?;
    let keyframes = parse_keyframes(&args.keyframe)?;
    let fps = args
        .fps
        .unwrap_or(if format == VideoFormat::Mp4 { 30 } else { 15 });
    let easing = Easing::parse(&args.easing)
        .ok_or_else(|| CliError::input("ANIMATION_INVALID: easing must be linear or smooth"))?;
    let animation = Animation::new(keyframes, args.duration, fps, !args.no_loop, easing)
        .map_err(video_error)?;
    let plan = animation.frame_plan();
    if !args.limits.force_limits {
        if animation.duration() > 60.0 {
            return Err(CliError::input(
                "ANIMATION_INVALID: duration must be at most 60 seconds; use --force-limits",
            ));
        }
        if plan.len() > 1800 {
            return Err(CliError::input(format!(
                "ANIMATION_INVALID: {} frames exceed the 1800-frame limit; use --force-limits",
                plan.len()
            )));
        }
    }

    let background = parse_background(&args.background)?;
    let lighting = LightingProfile::parse(&args.lighting).ok_or_else(|| {
        CliError::input(format!(
            "INVALID_RENDER_OPTIONS: unknown lighting profile '{}'",
            args.lighting
        ))
    })?;
    let backend = RenderBackend::parse(&args.backend).ok_or_else(|| {
        CliError::input(format!(
            "INVALID_RENDER_OPTIONS: unknown backend '{}'",
            args.backend
        ))
    })?;
    let texture_filter = parse_texture_filter(&args.assets.texture_filter)?;
    let encoder_choice = Encoder::parse(&args.encoder).ok_or_else(|| {
        CliError::input(format!(
            "INVALID_RENDER_OPTIONS: unknown encoder '{}'",
            args.encoder
        ))
    })?;
    let quality = Quality::parse(&args.quality).ok_or_else(|| {
        CliError::input(format!(
            "INVALID_RENDER_OPTIONS: unknown quality '{}'",
            args.quality
        ))
    })?;
    let video_settings = VideoSettings {
        width: args.width,
        height: args.height,
        fps,
        format,
        quality,
        encoder: encoder_choice,
        looped: !args.no_loop,
        transparent: matches!(background, Background::Transparent),
        threads: args.threads,
    };
    video_settings.validate().map_err(video_error)?;
    let ffmpeg = Ffmpeg::discover(args.ffmpeg.as_deref()).map_err(video_error)?;
    let resolved_encoder = ffmpeg
        .resolve_encoder(encoder_choice, format, args.width, args.height)
        .map_err(video_error)?;

    let mut base_options = RenderOptions {
        width: args.width,
        height: args.height,
        yaw: 45.0,
        elevation: 30.0,
        sphere_fit: args.sphere_fit,
        background,
        lighting,
        supersample: args.supersample,
        threads: args.threads,
        texture_filter,
        backend,
        ..Default::default()
    };
    apply_render_limits(args.limits.force_limits, &mut base_options);
    base_options.validate().map_err(CliError::from)?;

    let materials_stage = progress.stage("Loading materials");
    let materials = load_materials(&args.assets)?;
    materials_stage.finish();

    let parse_stage = progress.stage("Parsing scene");
    let parse_started = Instant::now();
    let (_, scene) = load_scene(
        &args.file,
        args.limits.force_limits,
        GEOMETRY_IGNORES_BLOCK_ENTITY_NBT,
    )?;
    parse_stage.finish();
    let parse_ms = parse_started.elapsed().as_secs_f64() * 1000.0;

    let mesh_stage = progress.stage("Building mesh");
    let prepared = prepare_scene(
        scene,
        materials.as_ref(),
        &mesh_limits(args.limits.force_limits),
        args.threads,
        args.mesh.options(),
    );
    mesh_stage.finish();
    let PreparedScene {
        scene,
        mesh,
        snapshot,
        mesh_ms,
    } = prepared?;

    #[cfg(feature = "gpu")]
    let textures = snapshot.as_ref().map(|snapshot| snapshot.textures());
    #[cfg(feature = "gpu")]
    let mut gpu_session: Option<litematic_render::GpuFrameSession<'_>> = None;
    #[cfg(feature = "gpu")]
    let mut actual_backend = RenderBackend::Cpu;
    #[cfg(not(feature = "gpu"))]
    let actual_backend = RenderBackend::Cpu;
    match backend {
        RenderBackend::Cpu => {}
        RenderBackend::Gpu | RenderBackend::Auto => {
            #[cfg(feature = "gpu")]
            {
                let first = frame_options(&base_options, &plan, 0, RenderBackend::Gpu);
                match litematic_render::GpuFrameSession::new(&scene, &mesh, &first, textures) {
                    Ok(session) => {
                        gpu_session = Some(session);
                        actual_backend = RenderBackend::Gpu;
                    }
                    Err(error) if backend == RenderBackend::Auto => {
                        ui::errln!(
                            "{WARNING}warning:{WARNING:#} GPU frame session unavailable, using CPU: {error}"
                        );
                    }
                    Err(error) => return Err(CliError::from(error)),
                }
            }
            #[cfg(not(feature = "gpu"))]
            if backend == RenderBackend::Gpu {
                return Err(CliError::input(
                    "BACKEND_UNAVAILABLE: the GPU backend is not enabled in this build",
                ));
            }
        }
    }

    let encoder_stage = progress.stage("Encoding video");
    let mut encoder =
        VideoEncoder::start(&ffmpeg, &video_settings, &args.output).map_err(video_error)?;
    let render_stage = progress.stage("Rendering frames");
    let mut render_ms = 0.0;
    let mut encode_ms = 0.0;
    for index in 0..plan.len() {
        let frame_backend = if actual_backend == RenderBackend::Gpu {
            RenderBackend::Gpu
        } else {
            RenderBackend::Cpu
        };
        let options = frame_options(&base_options, &plan, index, frame_backend);
        render_stage.message(format!("Frame {}/{}", index + 1, plan.len()));
        let render_started = Instant::now();
        #[cfg(feature = "gpu")]
        let output = if let Some(session) = gpu_session.as_mut() {
            session.render_frame(&options).map_err(CliError::from)?
        } else {
            render_cpu(&scene, &mesh, snapshot.as_ref(), &options)?
        };
        #[cfg(not(feature = "gpu"))]
        let output = render_cpu(&scene, &mesh, snapshot.as_ref(), &options)?;
        render_ms += render_started.elapsed().as_secs_f64() * 1000.0;
        let encode_started = Instant::now();
        encoder.write_frame(&output.rgba).map_err(video_error)?;
        encode_ms += encode_started.elapsed().as_secs_f64() * 1000.0;
    }
    render_stage.finish();
    let finish_started = Instant::now();
    let bytes = encoder.finish().map_err(video_error)?;
    encode_ms += finish_started.elapsed().as_secs_f64() * 1000.0;
    encoder_stage.finish();

    let total_ms = started.elapsed().as_secs_f64() * 1000.0;
    if args.json {
        let report = serde_json::json!({
            "output": args.output.display().to_string(),
            "format": format.as_str(),
            "width": args.width,
            "height": args.height,
            "frames": plan.len(),
            "fps": plan.fps(),
            "durationSeconds": plan.duration(),
            "looped": plan.looped(),
            "easing": args.easing,
            "backend": actual_backend.as_str(),
            "encoder": resolved_encoder.name(),
            "bytes": bytes,
            "parseMs": parse_ms,
            "meshMs": mesh_ms,
            "renderMs": render_ms,
            "encodeMs": encode_ms,
            "totalMs": total_ms,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(CliError::internal)?
        );
    } else {
        ui::outln!(
            "{MUTED}output:{MUTED:#} {ACCENT}{}{ACCENT:#}",
            args.output.display()
        );
        ui::outln!("{MUTED}format:{MUTED:#} {}", format.as_str());
        ui::outln!("{MUTED}size:{MUTED:#} {}x{}", args.width, args.height);
        ui::outln!(
            "{BOLD}frames:{BOLD:#} {} at {} fps ({:.3} s)",
            plan.len(),
            plan.fps(),
            plan.duration()
        );
        ui::outln!("{MUTED}backend:{MUTED:#} {}", actual_backend.as_str());
        ui::outln!(
            "{MUTED}encoder:{MUTED:#} {} ({bytes} bytes)",
            resolved_encoder.name()
        );
        ui::outln!(
            "{MUTED}timings:{MUTED:#} parse {parse_ms:.1} ms, mesh {mesh_ms:.1} ms, render {render_ms:.1} ms, encode {encode_ms:.1} ms, total {total_ms:.1} ms"
        );
    }
    Ok(())
}

fn resolve_format(args: &AnimateArgs) -> Result<VideoFormat, CliError> {
    let requested = args
        .format
        .as_deref()
        .map(|value| {
            VideoFormat::parse(value).ok_or_else(|| {
                CliError::input(format!(
                    "ANIMATION_INVALID: format must be mp4 or gif, got '{value}'"
                ))
            })
        })
        .transpose()?;
    let extension = args.output.extension().and_then(|value| value.to_str());
    let by_path = extension
        .map(|value| {
            VideoFormat::parse(value).ok_or_else(|| {
                CliError::input(format!(
                    "ANIMATION_INVALID: output extension must be .mp4 or .gif, got .{value}"
                ))
            })
        })
        .transpose()?;
    match (requested, by_path) {
        (Some(requested), Some(inferred)) if requested != inferred => Err(CliError::input(
            "ANIMATION_INVALID: --format does not match the output extension",
        )),
        (Some(requested), _) => Ok(requested),
        (None, Some(inferred)) => Ok(inferred),
        (None, None) if extension.is_none() => Ok(VideoFormat::Mp4),
        (None, None) => Err(CliError::input(
            "ANIMATION_INVALID: pass --format mp4 or gif when the output path has no known extension",
        )),
    }
}

fn parse_keyframes(values: &[String]) -> Result<Vec<Keyframe>, CliError> {
    if values.is_empty() {
        return Err(CliError::input(
            "ANIMATION_INVALID: at least one --keyframe is required",
        ));
    }
    let mut keyframes = Vec::with_capacity(values.len());
    for value in values {
        let parts: Vec<&str> = value.split(':').collect();
        if parts.len() != 3 {
            return Err(CliError::input(format!(
                "ANIMATION_INVALID: keyframe '{value}' must be TIME:YAW:ELEVATION"
            )));
        }
        let time = parts[0].trim().parse::<f64>().map_err(|_| {
            CliError::input(format!(
                "ANIMATION_INVALID: invalid keyframe time in '{value}'"
            ))
        })?;
        let yaw = parts[1].trim().parse::<f32>().map_err(|_| {
            CliError::input(format!(
                "ANIMATION_INVALID: invalid keyframe yaw in '{value}'"
            ))
        })?;
        let elevation = parts[2].trim().parse::<f32>().map_err(|_| {
            CliError::input(format!(
                "ANIMATION_INVALID: invalid keyframe elevation in '{value}'"
            ))
        })?;
        keyframes.push(Keyframe::new(time, yaw, elevation));
    }
    Ok(keyframes)
}

fn frame_options(
    base: &RenderOptions,
    plan: &FramePlan,
    index: u32,
    backend: RenderBackend,
) -> RenderOptions {
    let (yaw, elevation) = plan.camera_at(index).expect("frame index is in the plan");
    let mut options = base.clone();
    options.yaw = yaw;
    options.elevation = elevation;
    options.backend = backend;
    options
}

fn render_cpu(
    scene: &litematic_scene::Scene,
    mesh: &litematic_mesh::Mesh,
    snapshot: Option<&litematic_material::MaterialSnapshot>,
    options: &RenderOptions,
) -> Result<RenderOutput, CliError> {
    match snapshot {
        Some(snapshot) => render_mesh_with_textures(scene, mesh, options, snapshot.textures())
            .map_err(CliError::from),
        None => render_mesh(scene, mesh, options).map_err(CliError::from),
    }
}

fn video_error(error: VideoError) -> CliError {
    CliError::input(error.to_string())
}
