use crate::capacity::Capacity;
use crate::{AssetArgs, CliError, load_materials, parse_background, parse_texture_filter};
use clap::Args;
use litematic_core::LitematicFile;
use litematic_material::{MaterialSet, MaterialSnapshot};
use litematic_mesh::Mesh;
use litematic_render::{
    LightingProfile, RenderOptions, render_mesh, render_mesh_with_textures, write_png,
};
use litematic_scene::{Scene, sha256};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::{BufRead, BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

pub const PROTOCOL_VERSION: u32 = 1;
const DEFAULT_SIZE: u32 = 1024;
const DEFAULT_SUPERSAMPLE: u32 = 2;
const DEFAULT_ELEVATION: f32 = 30.0;
const DEFAULT_YAW: f32 = 45.0;

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
    #[command(flatten)]
    limits: crate::LimitArgs,
}

#[derive(Clone)]
struct CachedScene {
    hash: String,
    lod_key: String,
    lod: u32,
    scene: Arc<Scene>,
    mesh: Arc<Mesh>,
    snapshot: Option<Arc<MaterialSnapshot>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Message {
    Render(Box<RenderRequest>),
    Shutdown,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LodRequest {
    Factor(u32),
    Text(String),
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
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    threads: Option<u32>,
    #[serde(default)]
    lod: Option<LodRequest>,
    output_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
struct InputRef {
    path: PathBuf,
    #[serde(default)]
    hash: Option<String>,
}

pub fn run(args: ServeArgs) -> Result<(), CliError> {
    let capacity = Capacity::detect();
    crate::warn_forced(args.limits.force_limits);
    let materials = load_materials(&args.assets)?;
    let mut cache: Vec<CachedScene> = Vec::new();
    let cache_limit = args.cache as usize;
    let force_limits = args.limits.force_limits;

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    write_line(&mut out, &hello(&capacity))?;

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| CliError::input(format!("IO_ERROR: stdin: {error}")))?;
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
                let response = handle(
                    *request,
                    materials.as_ref(),
                    args.threads,
                    force_limits,
                    &mut cache,
                    cache_limit,
                );
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

fn hello(capacity: &Capacity) -> Value {
    json!({
        "type": "hello",
        "protocolVersion": PROTOCOL_VERSION,
        "engineVersion": env!("CARGO_PKG_VERSION"),
        "platform": &capacity.platform,
        "cores": capacity.cores,
        "cpuQuota": capacity.cpu_quota_milli.map(|milli| f64::from(milli) / 1000.0),
        "memoryLimitBytes": capacity.memory_limit_bytes,
        "memoryAvailableBytes": capacity.memory_available_bytes,
        "suggestedWorkers": capacity.suggested_workers,
        "suggestedThreads": capacity.suggested_threads,
        "defaultLod": 1,
        "defaultTier": {
            "width": capacity.default_width,
            "height": capacity.default_height,
            "supersample": capacity.default_supersample,
        },
    })
}

fn handle(
    request: RenderRequest,
    materials: Option<&MaterialSet>,
    default_threads: u32,
    force_limits: bool,
    cache: &mut Vec<CachedScene>,
    cache_limit: usize,
) -> Value {
    let id = request.id.clone();
    match process(
        &request,
        materials,
        default_threads,
        force_limits,
        cache,
        cache_limit,
    ) {
        Ok(result) => result,
        Err(error) => {
            let code = error_code(&error.message);
            let retryable = matches!(
                code.as_str(),
                "PNG_ENCODE_FAILED" | "IO_ERROR" | "INTERNAL_ERROR"
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
    materials: Option<&MaterialSet>,
    default_threads: u32,
    force_limits: bool,
    cache: &mut Vec<CachedScene>,
    cache_limit: usize,
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
    if let Some(format) = &request.format
        && format != "png"
    {
        return Err(CliError::input(
            "INVALID_REQUEST: only png output is supported",
        ));
    }

    let input = fs::read(&request.input.path).map_err(|error| {
        CliError::input(format!(
            "IO_ERROR: {}: {error}",
            request.input.path.display()
        ))
    })?;
    let input_hash = sha256(&input).to_hex();

    if let Some(expected) = &request.input.hash
        && !input_hash.eq_ignore_ascii_case(expected)
    {
        return Err(CliError::input(format!(
            "INPUT_HASH_MISMATCH: expected {expected}, got {input_hash}"
        )));
    }

    let lod_mode = match request.lod.as_ref() {
        None => crate::LodMode::Fixed(1),
        Some(LodRequest::Factor(factor)) => {
            if !(1..=16).contains(factor) {
                return Err(CliError::input(
                    "INVALID_RENDER_OPTIONS: lod must be between 1 and 16, or auto",
                ));
            }
            crate::LodMode::Fixed(*factor)
        }
        Some(LodRequest::Text(text)) if text.eq_ignore_ascii_case("auto") => crate::LodMode::Auto,
        Some(LodRequest::Text(text)) => {
            return Err(CliError::input(format!(
                "INVALID_RENDER_OPTIONS: invalid lod '{text}', expected 1-16 or auto"
            )));
        }
    };
    let lod_key = match lod_mode {
        crate::LodMode::Fixed(factor) => factor.to_string(),
        crate::LodMode::Auto => "auto".to_string(),
    };

    let cached = cache
        .iter()
        .position(|entry| entry.hash == input_hash && entry.lod_key == lod_key)
        .map(|index| cache.remove(index));

    let (scene, mesh, snapshot, factor, parse_ms, lod_ms, mesh_ms, cache_hit) = match cached {
        Some(entry) => {
            let scene = Arc::clone(&entry.scene);
            let mesh = Arc::clone(&entry.mesh);
            let snapshot = entry.snapshot.clone();
            let factor = entry.lod;
            cache.insert(0, entry);
            (scene, mesh, snapshot, factor, 0.0, 0.0, 0.0, true)
        }
        None => {
            let parse_started = Instant::now();
            let file = LitematicFile::parse_bytes(
                &input,
                &crate::decode_limits(force_limits),
                &crate::scene_limits(force_limits),
            )
            .map_err(CliError::from)?;
            let scene = file.into_scene().map_err(CliError::from)?;
            let parse_ms = parse_started.elapsed().as_secs_f64() * 1000.0;

            let prepared = crate::prepare_scene(
                scene,
                materials,
                lod_mode,
                &crate::mesh_limits(force_limits),
                request.threads.unwrap_or(default_threads),
            )?;
            let crate::PreparedScene {
                scene,
                mesh,
                snapshot,
                factor,
                lod_ms,
                mesh_ms,
            } = prepared;

            let scene = Arc::new(scene);
            let mesh = Arc::new(mesh);
            let snapshot = snapshot.map(Arc::new);
            let entry = CachedScene {
                hash: input_hash.clone(),
                lod_key,
                lod: factor,
                scene: Arc::clone(&scene),
                mesh: Arc::clone(&mesh),
                snapshot: snapshot.clone(),
            };
            if cache_limit > 0 {
                cache.insert(0, entry);
                cache.truncate(cache_limit);
            }

            (
                scene, mesh, snapshot, factor, parse_ms, lod_ms, mesh_ms, false,
            )
        }
    };

    let views = if request.views.is_empty() {
        vec![DEFAULT_YAW]
    } else {
        request.views.clone()
    };
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

    fs::create_dir_all(&request.output_dir).map_err(|error| {
        CliError::input(format!(
            "IO_ERROR: {}: {error}",
            request.output_dir.display()
        ))
    })?;

    let view_count = views.len();
    let mut outputs = Vec::with_capacity(view_count);
    let mut render_total = 0.0;
    let mut png_total = 0.0;
    let mut last_statistics = None;

    for yaw in views {
        let mut options = RenderOptions {
            width: request.width.unwrap_or(DEFAULT_SIZE),
            height: request.height.unwrap_or(DEFAULT_SIZE),
            yaw,
            elevation: request.elevation.unwrap_or(DEFAULT_ELEVATION),
            background,
            lighting,
            supersample: request.supersample.unwrap_or(DEFAULT_SUPERSAMPLE),
            threads: request.threads.unwrap_or(default_threads),
            texture_filter,
            ..Default::default()
        };
        crate::apply_render_limits(force_limits, &mut options);

        let render_started = Instant::now();
        let output = match &snapshot {
            Some(snapshot) => {
                render_mesh_with_textures(&scene, &mesh, &options, snapshot.textures())
                    .map_err(CliError::from)?
            }
            None => render_mesh(&scene, &mesh, &options).map_err(CliError::from)?,
        };
        let render_ms = render_started.elapsed().as_secs_f64() * 1000.0;
        render_total += render_ms;

        let name = if view_count == 1 {
            format!("{}.png", request.id)
        } else {
            format!("{}-{yaw}.png", request.id)
        };
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
        png_total += png_ms;

        let bytes = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        last_statistics = Some(output.statistics);
        outputs.push(json!({
            "path": path.display().to_string(),
            "yaw": yaw,
            "width": output.width,
            "height": output.height,
            "bytes": bytes,
            "renderMs": render_ms,
            "pngMs": png_ms,
        }));
    }

    let statistics = last_statistics.expect("views are never empty");
    Ok(json!({
        "type": "result",
        "id": request.id,
        "ok": true,
        "inputHash": input_hash,
        "sceneHash": scene.scene_hash().to_hex(),
        "lod": factor,
        "cacheHit": cache_hit,
        "outputs": outputs,
        "timings": {
            "parseMs": parse_ms,
            "lodMs": lod_ms,
            "meshMs": mesh_ms,
            "renderMs": render_total,
            "pngMs": png_total,
            "totalMs": started.elapsed().as_secs_f64() * 1000.0,
        },
        "statistics": {
            "triangles": statistics.triangles,
            "culledBackfaces": statistics.culled_backfaces,
            "textures": snapshot.as_ref().map(|value| value.textures().len()).unwrap_or(0),
        },
    }))
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
