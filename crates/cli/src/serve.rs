use crate::capacity::Capacity;
use crate::{
    AssetArgs, CliError, MeshOptionArgs, load_materials, parse_background, parse_texture_filter,
};
use blockscope_core::LitematicFile;
use blockscope_material::{MaterialSet, MaterialSnapshot};
use blockscope_mesh::{Mesh, MeshLimits, MeshPage};
use blockscope_render::{LightingProfile, RenderBackend, RenderOptions, write_png};
use blockscope_scene::{Scene, sha256};
use blockscope_video::{Ffmpeg, VideoFormat};
use clap::Args;
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

mod video;

pub const PROTOCOL_VERSION: u32 = 4;
const DEFAULT_SIZE: u32 = 1024;
const DEFAULT_SUPERSAMPLE: u32 = 2;
const DEFAULT_ELEVATION: f32 = 30.0;
const DEFAULT_YAW: f32 = 45.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceStreamMode {
    Auto,
    On,
    Off,
}

impl ServiceStreamMode {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "on" => Some(Self::On),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    #[command(flatten)]
    assets: AssetArgs,
    /// Rasterizer threads for requests that omit `threads`.
    #[arg(long, default_value_t = 0)]
    threads: u32,
    /// Parsed scenes and meshes to keep for repeated inputs.
    #[arg(long, default_value_t = 1)]
    cache: u32,
    /// Estimated cache budget in bytes; 0 uses half of available memory.
    #[arg(long, default_value_t = 0)]
    cache_bytes: u64,
    /// External ffmpeg executable used for MP4 and GIF jobs.
    #[arg(long, value_name = "PATH")]
    ffmpeg: Option<PathBuf>,
    #[command(flatten)]
    mesh: MeshOptionArgs,
    #[command(flatten)]
    limits: crate::LimitArgs,
}

#[derive(Clone)]
struct CachedScene {
    hash: String,
    scene: Arc<Scene>,
    mesh: Arc<Mesh>,
    snapshot: Option<Arc<MaterialSnapshot>>,
    bytes: u64,
}

struct ServeContext<'a> {
    materials: Option<&'a MaterialSet>,
    default_threads: u32,
    force_limits: bool,
    ignore_block_entity_nbt: bool,
    cache_limit: usize,
    cache_bytes: u64,
    mesh_options: blockscope_mesh::MeshBuildOptions,
    mesh_limits: MeshLimits,
    ffmpeg: Option<Ffmpeg>,
    ffmpeg_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Message {
    Render(Box<RenderRequest>),
    Shutdown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenderRequest {
    id: String,
    input: InputRef,
    #[serde(default)]
    views: Vec<f32>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    supersample: Option<u32>,
    #[serde(default)]
    elevation: Option<f32>,
    #[serde(default)]
    lighting: Option<String>,
    #[serde(default)]
    background: Option<String>,
    #[serde(default)]
    texture_filter: Option<String>,
    /// Rasterization backend: cpu, gpu, or auto (default).
    #[serde(default)]
    backend: Option<String>,
    /// Mesh streaming: off (default), on, or auto.
    #[serde(default)]
    stream_mesh: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    animation: Option<AnimationRequest>,
    #[serde(default)]
    encoder: Option<String>,
    #[serde(default)]
    quality: Option<String>,
    #[serde(default)]
    threads: Option<u32>,
    output_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnimationRequest {
    keyframes: Vec<KeyframeRequest>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    fps: Option<u32>,
    #[serde(default = "default_true", rename = "loop")]
    looped: bool,
    #[serde(default)]
    easing: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KeyframeRequest {
    time: f64,
    yaw: f32,
    elevation: f32,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct InputRef {
    path: PathBuf,
    #[serde(default)]
    hash: Option<String>,
}

pub fn run(args: ServeArgs) -> Result<(), CliError> {
    if args.threads > crate::MAX_THREADS {
        return Err(CliError::input(format!(
            "INVALID_RENDER_OPTIONS: threads must be at most {}",
            crate::MAX_THREADS
        )));
    }
    let capacity = Capacity::detect();
    crate::warn_forced(args.limits.force_limits);
    let materials = load_materials(&args.assets)?;
    let (ffmpeg, ffmpeg_error) = match Ffmpeg::discover(args.ffmpeg.as_deref()) {
        Ok(ffmpeg) => (Some(ffmpeg), None),
        Err(error) => (None, Some(error.to_string())),
    };
    let mut cache: Vec<CachedScene> = Vec::new();
    let cache_limit = args.cache as usize;
    let cache_bytes = cache_budget(args.cache_bytes, &capacity);
    let context = ServeContext {
        materials: materials.as_ref(),
        default_threads: args.threads,
        force_limits: args.limits.force_limits,
        ignore_block_entity_nbt: args.limits.ignore_block_entity_nbt,
        cache_limit,
        cache_bytes,
        mesh_options: args.mesh.options(),
        // Capacity is fixed for the process lifetime, so detect the mesh cap
        // once instead of once per uncached job.
        mesh_limits: crate::mesh_limits(args.limits.force_limits),
        ffmpeg,
        ffmpeg_error,
    };

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    write_line(
        &mut out,
        &hello(
            &capacity,
            cache_bytes,
            context.ffmpeg.as_ref(),
            context.ffmpeg_error.as_deref(),
        ),
    )?;

    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    loop {
        let line = match crate::file_io::read_line_limited(
            &mut input,
            crate::file_io::MAX_REQUEST_LINE_BYTES,
        ) {
            Ok(crate::file_io::LineRead::Line(line)) => line,
            Ok(crate::file_io::LineRead::Eof) => break,
            Ok(crate::file_io::LineRead::TooLong) => {
                write_line(
                    &mut out,
                    &error_result(
                        "",
                        "INVALID_REQUEST",
                        &format!(
                            "request line exceeds {} bytes",
                            crate::file_io::MAX_REQUEST_LINE_BYTES
                        ),
                    ),
                )?;
                continue;
            }
            Ok(crate::file_io::LineRead::InvalidUtf8) => {
                write_line(
                    &mut out,
                    &error_result("", "INVALID_REQUEST", "request line is not valid UTF-8"),
                )?;
                continue;
            }
            Err(error) => {
                return Err(CliError::input(format!("IO_ERROR: stdin: {error}")));
            }
        };

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<Message>(line) {
            Ok(Message::Shutdown) => {
                write_line(&mut out, &json!({ "type": "bye" }))?;
                return Ok(());
            }
            Ok(Message::Render(request)) => {
                let response = handle(*request, &mut cache, &context);
                write_line(&mut out, &response)?;
            }
            Err(error) => {
                write_line(
                    &mut out,
                    &error_result("", "INVALID_REQUEST", &error.to_string()),
                )?;
            }
        }
    }

    write_line(&mut out, &json!({ "type": "bye" }))?;
    Ok(())
}

fn hello(
    capacity: &Capacity,
    cache_bytes: u64,
    ffmpeg: Option<&Ffmpeg>,
    ffmpeg_error: Option<&str>,
) -> Value {
    let mut backends = vec!["cpu", "auto"];
    if cfg!(feature = "gpu") {
        backends.push("gpu");
    }
    json!({
        "type": "hello",
        "protocolVersion": PROTOCOL_VERSION,
        "engineVersion": env!("CARGO_PKG_VERSION"),
        "platform": &capacity.platform,
        "cores": capacity.cores,
        "cpuQuota": capacity.cpu_quota_milli.map(|milli| f64::from(milli) / 1000.0),
        "memoryLimitBytes": capacity.memory_limit_bytes,
        "memoryAvailableBytes": capacity.memory_available_bytes,
        "cacheBytes": cache_bytes,
        "suggestedWorkers": capacity.suggested_workers,
        "suggestedThreads": capacity.suggested_threads,
        "suggestedMaxQuads": capacity.suggested_max_quads,
        "defaultLod": 1,
        "backends": backends,
        "defaultTier": {
            "width": capacity.default_width,
            "height": capacity.default_height,
            "supersample": capacity.default_supersample,
        },
        "video": crate::video::hello_object(ffmpeg, ffmpeg_error),
    })
}

fn handle(
    request: RenderRequest,
    cache: &mut Vec<CachedScene>,
    context: &ServeContext<'_>,
) -> Value {
    let id = request.id.clone();
    match process(&request, cache, context) {
        Ok(result) => result,
        Err(error) => {
            let code = error_code(&error.message);
            let retryable = matches!(
                code.as_str(),
                "PNG_ENCODE_FAILED"
                    | "TEMP_STORAGE_FAILED"
                    | "FFMPEG_FAILED"
                    | "VIDEO_TEMP_FAILED"
                    | "IO_ERROR"
                    | "INTERNAL_ERROR"
            );
            json!({
                "type": "result",
                "id": id,
                "ok": false,
                "error": { "code": code, "message": error.message, "retryable": retryable },
            })
        }
    }
}

fn process(
    request: &RenderRequest,
    cache: &mut Vec<CachedScene>,
    context: &ServeContext<'_>,
) -> Result<Value, CliError> {
    let started = Instant::now();
    validate_id(&request.id)?;

    if !request.input.path.is_absolute() {
        return Err(CliError::input(
            "INVALID_REQUEST: input.path must be absolute",
        ));
    }
    if !request.output_dir.is_absolute() {
        return Err(CliError::input(
            "INVALID_REQUEST: outputDir must be absolute",
        ));
    }
    let format = request
        .format
        .as_deref()
        .unwrap_or("png")
        .to_ascii_lowercase();
    match format.as_str() {
        "png" => {
            if request.animation.is_some() {
                return Err(CliError::input(
                    "ANIMATION_INVALID: animation requires mp4 or gif output",
                ));
            }
            if request.encoder.is_some() || request.quality.is_some() {
                return Err(CliError::input(
                    "INVALID_RENDER_OPTIONS: encoder and quality only apply to video output",
                ));
            }
        }
        "mp4" | "gif" => {
            let format = VideoFormat::parse(&format).expect("matched video formats");
            return video::process_video(request, cache, context, format, started);
        }
        other => {
            return Err(CliError::input(format!(
                "INVALID_REQUEST: format must be png, mp4, or gif, got '{other}'"
            )));
        }
    }

    let views = if request.views.is_empty() {
        vec![DEFAULT_YAW]
    } else {
        request.views.clone()
    };
    if views.len() > crate::MAX_VIEWS {
        return Err(CliError::input(format!(
            "INVALID_RENDER_OPTIONS: views must contain at most {} angles",
            crate::MAX_VIEWS
        )));
    }
    for yaw in &views {
        if !yaw.is_finite() || !(0.0..=360.0).contains(yaw) {
            return Err(CliError::input(format!(
                "INVALID_RENDER_OPTIONS: view angle '{yaw}' must be within 0..=360"
            )));
        }
    }

    let lighting = LightingProfile::parse(request.lighting.as_deref().unwrap_or("simple-ao-v1"))
        .ok_or_else(|| {
            CliError::input(format!(
                "INVALID_RENDER_OPTIONS: unknown lighting profile '{}'",
                request.lighting.as_deref().unwrap_or_default()
            ))
        })?;
    let background = parse_background(request.background.as_deref().unwrap_or("transparent"))?;
    let texture_filter =
        parse_texture_filter(request.texture_filter.as_deref().unwrap_or("nearest"))?;
    let backend =
        RenderBackend::parse(request.backend.as_deref().unwrap_or("auto")).ok_or_else(|| {
            CliError::input(format!(
                "INVALID_RENDER_OPTIONS: unknown backend '{}'",
                request.backend.as_deref().unwrap_or_default()
            ))
        })?;

    fs::create_dir_all(&request.output_dir).map_err(|error| {
        CliError::input(format!(
            "IO_ERROR: {}: {error}",
            request.output_dir.display()
        ))
    })?;

    let view_options: Vec<RenderOptions> = views
        .iter()
        .map(|&yaw| {
            let mut options = RenderOptions {
                width: request.width.unwrap_or(DEFAULT_SIZE),
                height: request.height.unwrap_or(DEFAULT_SIZE),
                yaw,
                elevation: request.elevation.unwrap_or(DEFAULT_ELEVATION),
                background,
                lighting,
                supersample: request.supersample.unwrap_or(DEFAULT_SUPERSAMPLE),
                threads: request.threads.unwrap_or(context.default_threads),
                texture_filter,
                backend,
                ..Default::default()
            };
            crate::apply_render_limits(context.force_limits, &mut options);
            options
        })
        .collect();
    // Validate before parsing or meshing so a bad size, elevation, or thread
    // count cannot force a full expensive job.
    for options in &view_options {
        options.validate().map_err(CliError::from)?;
    }

    let stream_mode = ServiceStreamMode::parse(request.stream_mesh.as_deref().unwrap_or("off"))
        .ok_or_else(|| {
            CliError::input(format!(
                "INVALID_RENDER_OPTIONS: unknown streamMesh '{}'",
                request.stream_mesh.as_deref().unwrap_or_default()
            ))
        })?;
    let stream_mesh = match stream_mode {
        ServiceStreamMode::Off => false,
        ServiceStreamMode::On if view_options.len() > 1 => {
            return Err(CliError::input(
                "INVALID_RENDER_OPTIONS: streamMesh supports a single view",
            ));
        }
        ServiceStreamMode::On => true,
        // Auto keeps multi-view jobs on the shared full-mesh session.
        ServiceStreamMode::Auto => view_options.len() == 1,
    };
    if stream_mesh {
        return process_streaming(request, context, &view_options, started);
    }

    let LoadedScene {
        scene,
        mesh,
        snapshot,
        parse_ms,
        mesh_ms,
        cache_hit,
        input_hash,
    } = load_scene_for_request(
        request,
        cache,
        context,
        request.threads.unwrap_or(context.default_threads),
    )?;

    let view_count = view_options.len();
    let mut outputs = Vec::with_capacity(view_count);
    let mut png_total = 0.0;
    let mut last_statistics = None;

    // Write each view as soon as it is rendered instead of retaining every RGBA
    // buffer until the whole request finishes.
    let (_, render_total) = crate::render_views_with(
        &scene,
        &mesh,
        snapshot.as_deref(),
        &view_options,
        request.threads.unwrap_or(context.default_threads),
        None,
        |output, render_ms, options| {
            let name = if view_count == 1 {
                format!("{}.png", request.id)
            } else {
                format!("{}-{}.png", request.id, options.yaw)
            };
            let path = request.output_dir.join(name);

            let png_started = Instant::now();
            let file = File::create(&path).map_err(|error| {
                CliError::input(format!("IO_ERROR: {}: {error}", path.display()))
            })?;
            let mut writer = BufWriter::new(file);
            write_png(&output, &mut writer).map_err(CliError::from)?;
            writer.flush().map_err(|error| {
                CliError::input(format!("IO_ERROR: {}: {error}", path.display()))
            })?;
            let png_ms = png_started.elapsed().as_secs_f64() * 1000.0;
            png_total += png_ms;

            let bytes = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            let width = output.width;
            let height = output.height;
            let statistics = output.statistics;
            last_statistics = Some(statistics);
            outputs.push(json!({
                "kind": "image",
                "path": path.display().to_string(),
                "yaw": options.yaw,
                "width": width,
                "height": height,
                "bytes": bytes,
                "renderMs": render_ms,
                "pngMs": png_ms,
            }));
            Ok(())
        },
    )?;

    let statistics = last_statistics.expect("views are never empty");
    Ok(json!({
        "type": "result",
        "id": request.id,
        "ok": true,
        "inputHash": input_hash,
        "sceneHash": scene.scene_hash().to_hex(),
        "cacheHit": cache_hit,
        "outputs": outputs,
        "timings": {
            "parseMs": parse_ms,
            "meshMs": mesh_ms,
            "renderMs": render_total,
            "pngMs": png_total,
            "totalMs": started.elapsed().as_secs_f64() * 1000.0,
        },
        "statistics": {
            "triangles": statistics.triangles,
            "culledBackfaces": statistics.culled_backfaces,
            "textures": snapshot.as_ref().map(|value| value.textures().len()).unwrap_or(0),
            "backendUsed": statistics.backend_used.as_str(),
        },
    }))
}

pub(super) struct LoadedScene {
    pub scene: Arc<Scene>,
    pub mesh: Arc<Mesh>,
    pub snapshot: Option<Arc<MaterialSnapshot>>,
    pub parse_ms: f64,
    pub mesh_ms: f64,
    pub cache_hit: bool,
    pub input_hash: String,
}

fn load_scene_for_request(
    request: &RenderRequest,
    cache: &mut Vec<CachedScene>,
    context: &ServeContext<'_>,
    threads: u32,
) -> Result<LoadedScene, CliError> {
    let limits = crate::decode_limits(context.force_limits, context.ignore_block_entity_nbt);

    // A caller-supplied hash is an identity, so a cache hit must not re-read the
    // file. The supervisor already hashes inputs for routing and forwards it.
    let requested_hash = request
        .input
        .hash
        .as_ref()
        .map(|hash| hash.to_ascii_lowercase());
    if let Some(entry) = requested_hash
        .as_deref()
        .and_then(|hash| take_cached(cache, hash))
    {
        let input_hash = entry.hash.clone();
        let scene = Arc::clone(&entry.scene);
        let mesh = Arc::clone(&entry.mesh);
        let snapshot = entry.snapshot.clone();
        cache.insert(0, entry);
        return Ok(LoadedScene {
            scene,
            mesh,
            snapshot,
            parse_ms: 0.0,
            mesh_ms: 0.0,
            cache_hit: true,
            input_hash,
        });
    }

    let input = crate::read_file_limited(&request.input.path, limits.max_input_bytes)?;
    let input_hash = sha256(&input).to_hex();

    if let Some(expected) = &request.input.hash
        && !input_hash.eq_ignore_ascii_case(expected)
    {
        return Err(CliError::input(format!(
            "INPUT_HASH_MISMATCH: expected {expected}, got {input_hash}"
        )));
    }

    if let Some(entry) = take_cached(cache, &input_hash) {
        let scene = Arc::clone(&entry.scene);
        let mesh = Arc::clone(&entry.mesh);
        let snapshot = entry.snapshot.clone();
        cache.insert(0, entry);
        return Ok(LoadedScene {
            scene,
            mesh,
            snapshot,
            parse_ms: 0.0,
            mesh_ms: 0.0,
            cache_hit: true,
            input_hash,
        });
    }

    let parse_started = Instant::now();
    let file =
        LitematicFile::parse_bytes(&input, &limits, &crate::scene_limits(context.force_limits))
            .map_err(CliError::from)?;
    let scene = file.into_scene().map_err(CliError::from)?;
    let parse_ms = parse_started.elapsed().as_secs_f64() * 1000.0;

    let prepared = crate::prepare_scene(
        scene,
        context.materials,
        &context.mesh_limits,
        threads,
        context.mesh_options,
    )?;
    let crate::PreparedScene {
        scene,
        mesh,
        snapshot,
        mesh_ms,
    } = prepared;

    let scene = Arc::new(scene);
    let mesh = Arc::new(mesh);
    let snapshot = snapshot.map(Arc::new);
    let entry = CachedScene {
        hash: input_hash.clone(),
        scene: Arc::clone(&scene),
        mesh: Arc::clone(&mesh),
        snapshot: snapshot.clone(),
        bytes: estimated_cache_bytes(&scene, &mesh, snapshot.as_deref()),
    };
    cache_insert(cache, entry, context.cache_limit, context.cache_bytes);

    Ok(LoadedScene {
        scene,
        mesh,
        snapshot,
        parse_ms,
        mesh_ms,
        cache_hit: false,
        input_hash,
    })
}

#[allow(clippy::too_many_arguments)]
fn process_streaming(
    request: &RenderRequest,
    context: &ServeContext<'_>,
    view_options: &[RenderOptions],
    started: Instant,
) -> Result<Value, CliError> {
    let options = view_options
        .first()
        .ok_or_else(|| CliError::input("INVALID_RENDER_OPTIONS: views must not be empty"))?;
    if view_options.len() != 1 {
        return Err(CliError::input(
            "INVALID_RENDER_OPTIONS: streamMesh supports a single view",
        ));
    }

    let limits = crate::decode_limits(context.force_limits, context.ignore_block_entity_nbt);
    let input = crate::read_file_limited(&request.input.path, limits.max_input_bytes)?;
    let input_hash = sha256(&input).to_hex();
    if let Some(expected) = &request.input.hash
        && !input_hash.eq_ignore_ascii_case(expected)
    {
        return Err(CliError::input(format!(
            "INPUT_HASH_MISMATCH: expected {expected}, got {input_hash}"
        )));
    }

    let parse_started = Instant::now();
    let file =
        LitematicFile::parse_bytes(&input, &limits, &crate::scene_limits(context.force_limits))
            .map_err(CliError::from)?;
    let scene = file.into_scene().map_err(CliError::from)?;
    let parse_ms = parse_started.elapsed().as_secs_f64() * 1000.0;

    let snapshot = context
        .materials
        .map(|materials| crate::material_snapshot(&scene, materials));
    let threads = request.threads.unwrap_or(context.default_threads);
    let mut options = options.clone();
    let auto_backend = options.backend == RenderBackend::Auto;
    if auto_backend {
        let estimate = crate::estimate_scene_quads(
            &scene,
            snapshot.as_ref(),
            &context.mesh_limits,
            threads,
            context.mesh_options,
        )?;
        options.backend =
            crate::backend_decision(&scene, &options, estimate, true, snapshot.as_ref(), 1).backend;
    }

    let mut renderer =
        match crate::stream_target(options.backend, &scene, &options, snapshot.as_ref()) {
            Ok(renderer) => renderer,
            Err(error) if auto_backend && error.is_backend_unavailable() => {
                crate::ui::errln!(
                    "{}warning:{:#} GPU setup failed, falling back to CPU streaming: {}",
                    crate::ui::WARNING,
                    crate::ui::WARNING,
                    error.message
                );
                options.backend = RenderBackend::Cpu;
                crate::stream_target(RenderBackend::Cpu, &scene, &options, snapshot.as_ref())?
            }
            Err(error) => return Err(error),
        };
    let batch_stream = renderer.is_cpu();
    let mut render_chunk_ms = 0.0;
    let mut render_error = None;
    let mut pending: Vec<MeshPage> = Vec::new();
    let mut pending_quads = 0usize;
    let mesh_started = Instant::now();
    crate::build_scene_mesh_streaming_owned(
        &scene,
        snapshot.as_ref(),
        &context.mesh_limits,
        threads,
        context.mesh_options,
        None,
        |pages| {
            if !batch_stream {
                let chunk_started = Instant::now();
                if render_error.is_none()
                    && let Err(error) = renderer.render_pages(&pages)
                {
                    render_error = Some(error);
                }
                render_chunk_ms += chunk_started.elapsed().as_secs_f64() * 1000.0;
                return;
            }

            for page in pages {
                pending_quads += page.quads.len();
                pending.push(page);
            }
            if pending_quads < crate::STREAM_RENDER_QUADS {
                return;
            }
            let chunk_started = Instant::now();
            if render_error.is_none()
                && let Err(error) = renderer.render_pages(&pending)
            {
                render_error = Some(error);
            }
            render_chunk_ms += chunk_started.elapsed().as_secs_f64() * 1000.0;
            pending.clear();
            pending_quads = 0;
        },
    )?;
    let mesh_total_ms = mesh_started.elapsed().as_secs_f64() * 1000.0;
    if batch_stream && render_error.is_none() && !pending.is_empty() {
        let chunk_started = Instant::now();
        if let Err(error) = renderer.render_pages(&pending) {
            render_error = Some(error);
        }
        render_chunk_ms += chunk_started.elapsed().as_secs_f64() * 1000.0;
    }
    pending.clear();
    if let Some(error) = render_error {
        return Err(CliError::from(error));
    }
    let finish_started = Instant::now();
    let output = renderer.finish().map_err(CliError::from)?;
    let render_ms = render_chunk_ms + finish_started.elapsed().as_secs_f64() * 1000.0;

    let name = format!("{}.png", request.id);
    let path = request.output_dir.join(name);
    let png_started = Instant::now();
    let file = File::create(&path)
        .map_err(|error| CliError::input(format!("IO_ERROR: {}: {error}", path.display())))?;
    let mut writer = BufWriter::new(file);
    write_png(&output, &mut writer).map_err(CliError::from)?;
    writer
        .flush()
        .map_err(|error| CliError::input(format!("IO_ERROR: {}: {error}", path.display())))?;
    let png_ms = png_started.elapsed().as_secs_f64() * 1000.0;
    let bytes = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);

    let statistics = output.statistics;
    Ok(json!({
        "type": "result",
        "id": request.id,
        "ok": true,
        "inputHash": input_hash,
        "sceneHash": scene.scene_hash().to_hex(),
        "cacheHit": false,
        "outputs": [{
            "kind": "image",
            "path": path.display().to_string(),
            "yaw": options.yaw,
            "width": output.width,
            "height": output.height,
            "bytes": bytes,
            "renderMs": render_ms,
            "pngMs": png_ms,
        }],
        "timings": {
            "parseMs": parse_ms,
            "meshMs": (mesh_total_ms - render_chunk_ms).max(0.0),
            "renderMs": render_ms,
            "pngMs": png_ms,
            "totalMs": started.elapsed().as_secs_f64() * 1000.0,
        },
        "statistics": {
            "triangles": statistics.triangles,
            "culledBackfaces": statistics.culled_backfaces,
            "textures": snapshot.as_ref().map(|value| value.textures().len()).unwrap_or(0),
            "backendUsed": statistics.backend_used.as_str(),
        },
    }))
}

fn estimated_cache_bytes(scene: &Scene, mesh: &Mesh, snapshot: Option<&MaterialSnapshot>) -> u64 {
    scene
        .heap_bytes()
        .saturating_add(mesh.heap_bytes())
        .saturating_add(
            snapshot
                .map(|snapshot| snapshot.textures().bytes())
                .unwrap_or(0),
        )
        .saturating_add(std::mem::size_of::<CachedScene>() as u64)
}

fn cache_insert(
    cache: &mut Vec<CachedScene>,
    entry: CachedScene,
    cache_limit: usize,
    cache_bytes: u64,
) {
    if cache_limit == 0 {
        return;
    }

    let mut total = cache.iter().map(|entry| entry.bytes).sum::<u64>();
    while !cache.is_empty() {
        let count_over = cache.len() + 1 > cache_limit;
        let bytes_over = cache_bytes != u64::MAX && total.saturating_add(entry.bytes) > cache_bytes;
        if !count_over && !bytes_over {
            break;
        }
        if let Some(removed) = cache.pop() {
            total = total.saturating_sub(removed.bytes);
        }
    }

    cache.insert(0, entry);
    cache.truncate(cache_limit);
}

fn cache_budget(requested: u64, capacity: &Capacity) -> u64 {
    if requested > 0 {
        requested
    } else if capacity.memory_available_bytes > 0 {
        capacity.memory_available_bytes / 2
    } else {
        u64::MAX
    }
}

fn take_cached(cache: &mut Vec<CachedScene>, hash: &str) -> Option<CachedScene> {
    let index = cache
        .iter()
        .position(|entry| entry.hash.eq_ignore_ascii_case(hash))?;
    Some(cache.remove(index))
}

fn validate_id(id: &str) -> Result<(), CliError> {
    let valid = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || value == '-' || value == '_');
    if valid {
        Ok(())
    } else {
        Err(CliError::input(
            "INVALID_REQUEST: id must be 1-64 characters of [A-Za-z0-9_-]",
        ))
    }
}

fn error_result(id: &str, code: &str, message: &str) -> Value {
    json!({
        "type": "result",
        "id": id,
        "ok": false,
        "error": { "code": code, "message": message, "retryable": false },
    })
}

fn error_code(message: &str) -> String {
    let prefix = message.split(':').next().unwrap_or_default();
    let known = !prefix.is_empty()
        && prefix
            .chars()
            .all(|value| value.is_ascii_uppercase() || value == '_');
    if known {
        prefix.to_string()
    } else {
        "INTERNAL_ERROR".to_string()
    }
}

fn write_line(out: &mut impl Write, value: &Value) -> Result<(), CliError> {
    serde_json::to_writer(&mut *out, value).map_err(CliError::internal)?;
    out.write_all(b"\n")
        .map_err(|error| CliError::input(format!("IO_ERROR: stdout: {error}")))?;
    out.flush()
        .map_err(|error| CliError::input(format!("IO_ERROR: stdout: {error}")))
}
