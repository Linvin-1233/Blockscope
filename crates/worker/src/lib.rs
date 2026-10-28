//! Cloudflare Workers adapter for the Blockscope renderer.
//!
//! The worker accepts a `.litematic` body over POST and returns a PNG. A
//! resource pack is loaded from Workers Static Assets when configured; without
//! one it falls back to the built-in color catalog. The render endpoint, CORS
//! origin, and pack path are text bindings (`ENDPOINT`, `ALLOWED_ORIGIN`,
//! `RESOURCE_PACK_PATH`) so deployments can change them without a rebuild.

use litematic_core::{
    DecodeLimits, LitematicError, LitematicErrorCode, LitematicFile, LitematicLimits,
};
use litematic_material::{AssetSource, MaterialError, MaterialErrorCode, MaterialSet};
use litematic_mesh::{
    ColorBlockCatalog, MeshBuildOptions, MeshError, MeshLimits, Rgba, build_mesh_with_options,
};
use litematic_render::{
    Background, LightingProfile, RenderError, RenderOptions, render_mesh,
    render_mesh_with_textures, write_png,
};
use litematic_scene::{Scene, aggregate_with_threads};
use serde::{Deserialize, Serialize};
use worker::{Context, Env, Method, Request, Response, Result, event};

const DEFAULT_ENDPOINT: &str = "/render";
const DEFAULT_PACK_PATH: &str = "/pack.zip";
const HEALTH_ENDPOINT: &str = "/health";
const ASSETS_BINDING: &str = "ASSETS";
const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_PACK_BYTES: usize = 24 * 1024 * 1024;
const MAX_DECOMPRESSED_BYTES: usize = 32 * 1024 * 1024;
const MAX_NBT_BYTES: usize = 64 * 1024 * 1024;
const MAX_TOTAL_VOLUME: u64 = 64_000_000;
const MAX_PIXELS: u64 = 1_048_576;
const MAX_SUPERSAMPLED_PIXELS: u64 = 4_194_304;
const MAX_DIMENSION: u32 = 1024;
const MAX_MESH_QUADS: usize = 200_000;
const MAX_MESH_VERTICES: usize = 800_000;
const MAX_MESH_PAGES: usize = 8_000;
const MAX_LOD: u32 = 16;

#[derive(Debug, Default, Deserialize)]
struct QueryOptions {
    width: Option<u32>,
    height: Option<u32>,
    supersample: Option<u32>,
    yaw: Option<f32>,
    elevation: Option<f32>,
    lod: Option<u32>,
    background: Option<String>,
    lighting: Option<String>,
}

#[derive(Debug, Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<&'a str>,
}

#[derive(Debug)]
struct ApiError {
    status: u16,
    code: String,
    message: String,
    hint: Option<&'static str>,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            code: "INVALID_REQUEST".to_string(),
            message: message.into(),
            hint: None,
        }
    }

    fn payload_too_large(message: impl Into<String>) -> Self {
        Self {
            status: 413,
            code: "INPUT_TOO_LARGE".to_string(),
            message: message.into(),
            hint: Some("send a .litematic under 8 MiB"),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: 500,
            code: "INTERNAL_ERROR".to_string(),
            message: message.into(),
            hint: None,
        }
    }

    fn from_litematic(error: LitematicError) -> Self {
        let status = if error.code == LitematicErrorCode::ResourceLimit {
            413
        } else {
            422
        };
        Self {
            status,
            code: error.code.as_str().to_string(),
            message: error.to_string(),
            hint: None,
        }
    }

    fn from_mesh(error: MeshError) -> Self {
        Self {
            status: 422,
            code: error.code.as_str().to_string(),
            message: error.message,
            hint: Some("lower lod or request a smaller image"),
        }
    }

    fn from_render(error: RenderError) -> Self {
        let status = match error.code {
            litematic_render::RenderErrorCode::InvalidOptions => 400,
            litematic_render::RenderErrorCode::PngEncodeFailed => 500,
            _ => 422,
        };
        let hint = match error.code {
            litematic_render::RenderErrorCode::TempStorageFailed => {
                Some("lower lod or request a smaller image")
            }
            _ => None,
        };
        Self {
            status,
            code: error.code.as_str().to_string(),
            message: error.message,
            hint,
        }
    }

    fn from_scene(error: litematic_scene::SceneError) -> Self {
        Self {
            status: 422,
            code: error.code.as_str().to_string(),
            message: error.message,
            hint: None,
        }
    }

    fn from_material(error: MaterialError) -> Self {
        let status = if error.code == MaterialErrorCode::AssetLimit {
            413
        } else {
            500
        };
        Self {
            status,
            code: error.code.as_str().to_string(),
            message: error.to_string(),
            hint: Some("check the resource pack and its asset limits"),
        }
    }

    fn resource_pack(message: impl Into<String>) -> Self {
        Self {
            status: 503,
            code: "RESOURCE_PACK_UNAVAILABLE".to_string(),
            message: message.into(),
            hint: Some("check the ASSETS binding and RESOURCE_PACK_PATH"),
        }
    }
}

#[event(fetch)]
pub async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let origin = binding(&env, "ALLOWED_ORIGIN", "*");
    if req.method() == Method::Options {
        return preflight(&origin);
    }

    let endpoint = route(&env, "ENDPOINT", DEFAULT_ENDPOINT);
    let path = req.path();

    if path == HEALTH_ENDPOINT {
        if req.method() != Method::Get {
            return method_not_allowed(&origin, "GET");
        }
        return json_response(
            200,
            &serde_json::json!({
                "status": "ok",
                "engine": "blockscope",
                "version": env!("CARGO_PKG_VERSION"),
                "renderEndpoint": endpoint,
                "textures": !binding(&env, "RESOURCE_PACK_PATH", DEFAULT_PACK_PATH).is_empty(),
            }),
            &origin,
        );
    }

    if path != endpoint {
        return json_response(
            404,
            &ErrorBody {
                code: "NOT_FOUND",
                message: format!("no route for {path}"),
                hint: Some("POST a .litematic to the configured render endpoint"),
            },
            &origin,
        );
    }

    if req.method() != Method::Post {
        return method_not_allowed(&origin, "POST");
    }

    if let Some(length) = req.headers().get("content-length")?
        && length
            .parse::<usize>()
            .is_ok_and(|length| length > MAX_INPUT_BYTES)
    {
        return error_response(
            ApiError::payload_too_large("request body is too large"),
            &origin,
        );
    }

    let input = match req.bytes().await {
        Ok(input) => input,
        Err(error) => {
            return error_response(
                ApiError::bad_request(format!("could not read request body: {error}")),
                &origin,
            );
        }
    };
    if input.len() > MAX_INPUT_BYTES {
        return error_response(
            ApiError::payload_too_large("request body is too large"),
            &origin,
        );
    }
    if input.is_empty() {
        return error_response(ApiError::bad_request("request body is empty"), &origin);
    }

    let query = match req.query::<QueryOptions>() {
        Ok(query) => query,
        Err(error) => {
            return error_response(
                ApiError::bad_request(format!("invalid query string: {error}")),
                &origin,
            );
        }
    };

    let pack = match load_resource_pack(&env).await {
        Ok(pack) => pack,
        Err(error) => return error_response(error, &origin),
    };

    match render_png(&input, &query, pack) {
        Ok(png) => png_response(png, &origin),
        Err(error) => error_response(error, &origin),
    }
}

async fn load_resource_pack(env: &Env) -> std::result::Result<Option<Vec<u8>>, ApiError> {
    let path = binding(env, "RESOURCE_PACK_PATH", DEFAULT_PACK_PATH);
    if path.is_empty() {
        return Ok(None);
    }

    let assets = env.assets(ASSETS_BINDING).map_err(|error| {
        ApiError::resource_pack(format!("ASSETS binding is unavailable: {error}"))
    })?;
    // The assets binding ignores the host but still requires an absolute URL.
    let asset_path = if path.starts_with('/') {
        path.clone()
    } else {
        format!("/{path}")
    };
    let mut response = assets
        .fetch(format!("https://assets.local{asset_path}"), None)
        .await
        .map_err(|error| ApiError::resource_pack(format!("cannot read {path}: {error}")))?;
    if !(200..300).contains(&response.status_code()) {
        return Err(ApiError::resource_pack(format!(
            "{path} returned HTTP {}",
            response.status_code()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|error| ApiError::resource_pack(format!("cannot read {path}: {error}")))?;
    if bytes.len() > MAX_PACK_BYTES {
        return Err(ApiError {
            status: 413,
            code: "RESOURCE_PACK_TOO_LARGE".to_string(),
            message: format!(
                "resource pack is {} bytes; the limit is {MAX_PACK_BYTES}",
                bytes.len()
            ),
            hint: Some("use a resource pack under 24 MiB"),
        });
    }
    Ok(Some(bytes))
}

fn render_png(
    input: &[u8],
    query: &QueryOptions,
    pack: Option<Vec<u8>>,
) -> std::result::Result<Vec<u8>, ApiError> {
    let width = query.width.unwrap_or(512);
    let height = query.height.unwrap_or(512);
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ApiError::bad_request(format!(
            "width and height must be between 1 and {MAX_DIMENSION}"
        )));
    }
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(ApiError::bad_request(format!(
            "width * height must not exceed {MAX_PIXELS}"
        )));
    }

    let supersample = query.supersample.unwrap_or(1);
    if !(1..=2).contains(&supersample) {
        return Err(ApiError::bad_request("supersample must be 1 or 2"));
    }

    let lod = query.lod.unwrap_or(1);
    if !(1..=MAX_LOD).contains(&lod) {
        return Err(ApiError::bad_request(format!(
            "lod must be between 1 and {MAX_LOD}"
        )));
    }

    let background = match query.background.as_deref() {
        None | Some("transparent") => Background::Transparent,
        Some(value) => Background::Solid(Rgba::from_hex(value).map_err(ApiError::bad_request)?),
    };
    let lighting = match query.lighting.as_deref() {
        None => LightingProfile::SimpleAoV1,
        Some(value) => LightingProfile::parse(value)
            .ok_or_else(|| ApiError::bad_request(format!("unknown lighting profile '{value}'")))?,
    };

    let decode_limits = DecodeLimits {
        max_input_bytes: MAX_INPUT_BYTES,
        max_decompressed_bytes: MAX_DECOMPRESSED_BYTES,
        max_total_bytes: MAX_NBT_BYTES,
        ignore_block_entity_nbt: true,
        ..DecodeLimits::default()
    };
    let scene_limits = LitematicLimits {
        max_volume_per_region: MAX_TOTAL_VOLUME,
        max_total_volume: MAX_TOTAL_VOLUME,
        ..LitematicLimits::default()
    };

    let file = LitematicFile::parse_bytes(input, &decode_limits, &scene_limits)
        .map_err(ApiError::from_litematic)?;
    let scene = file.into_scene().map_err(ApiError::from_litematic)?;
    let aggregated = if lod > 1 {
        Some(aggregate_with_threads(&scene, lod, 1).map_err(ApiError::from_scene)?)
    } else {
        None
    };
    let scene: &Scene = aggregated.as_ref().unwrap_or(&scene);

    let mesh_limits = MeshLimits {
        max_pages: MAX_MESH_PAGES,
        max_quads: MAX_MESH_QUADS,
        max_vertices: MAX_MESH_VERTICES,
    };

    let options = RenderOptions {
        width,
        height,
        yaw: query.yaw.unwrap_or(45.0),
        elevation: query.elevation.unwrap_or(30.0),
        background,
        lighting,
        supersample,
        threads: 1,
        max_pixels: MAX_PIXELS,
        max_supersampled_pixels: MAX_SUPERSAMPLED_PIXELS,
        // Streaming keeps peak memory bounded on the 128 MiB isolate.
        force_streaming: true,
        ..RenderOptions::default()
    };
    options.validate().map_err(ApiError::from_render)?;

    let output = if let Some(pack) = pack {
        let mut source = AssetSource::new();
        source
            .add_zip_bytes(pack)
            .map_err(ApiError::from_material)?;
        let materials = MaterialSet::open(source).map_err(ApiError::from_material)?;
        let snapshot = materials.snapshot(scene.statistics.blocks.keys().cloned());
        let mesh =
            build_mesh_with_options(scene, &snapshot, &mesh_limits, MeshBuildOptions::default())
                .map_err(ApiError::from_mesh)?;
        render_mesh_with_textures(scene, &mesh, &options, snapshot.textures())
            .map_err(ApiError::from_render)?
    } else {
        let catalog = ColorBlockCatalog::new().map_err(|error| {
            ApiError::internal(format!("color catalog is unavailable: {error}"))
        })?;
        let mesh =
            build_mesh_with_options(scene, &catalog, &mesh_limits, MeshBuildOptions::default())
                .map_err(ApiError::from_mesh)?;
        render_mesh(scene, &mesh, &options).map_err(ApiError::from_render)?
    };

    let mut png = Vec::new();
    write_png(&output, &mut png).map_err(ApiError::from_render)?;
    Ok(png)
}

fn binding(env: &Env, name: &str, default: &str) -> String {
    env.var(name)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| default.to_string())
}

fn route(env: &Env, name: &str, default: &str) -> String {
    let value = binding(env, name, default);
    if value.starts_with('/') && value.len() > 1 && !value.ends_with('/') {
        value
    } else {
        default.to_string()
    }
}

fn preflight(origin: &str) -> Result<Response> {
    let mut response = Response::empty()?.with_status(204);
    let headers = response.headers_mut();
    headers.set("access-control-allow-origin", origin)?;
    headers.set("access-control-allow-methods", "GET, POST, OPTIONS")?;
    headers.set("access-control-allow-headers", "content-type")?;
    headers.set("access-control-max-age", "86400")?;
    Ok(response)
}

fn method_not_allowed(origin: &str, allow: &str) -> Result<Response> {
    let mut response = json_response(
        405,
        &ErrorBody {
            code: "METHOD_NOT_ALLOWED",
            message: format!("method not allowed; use {allow}"),
            hint: None,
        },
        origin,
    )?;
    response.headers_mut().set("allow", allow)?;
    Ok(response)
}

fn json_response(status: u16, body: &impl Serialize, origin: &str) -> Result<Response> {
    let mut response = Response::from_json(body)?.with_status(status);
    response
        .headers_mut()
        .set("access-control-allow-origin", origin)?;
    response.headers_mut().set("cache-control", "no-store")?;
    Ok(response)
}

fn error_response(error: ApiError, origin: &str) -> Result<Response> {
    json_response(
        error.status,
        &ErrorBody {
            code: &error.code,
            message: error.message,
            hint: error.hint,
        },
        origin,
    )
}

fn png_response(png: Vec<u8>, origin: &str) -> Result<Response> {
    let mut response = Response::from_bytes(png)?;
    let headers = response.headers_mut();
    headers.set("content-type", "image/png")?;
    headers.set("access-control-allow-origin", origin)?;
    headers.set("cache-control", "no-store")?;
    Ok(response)
}
