use super::{
    CachedScene, DEFAULT_ELEVATION, DEFAULT_SIZE, DEFAULT_SUPERSAMPLE, DEFAULT_YAW, LoadedScene,
    RenderRequest, ServeContext, load_scene_for_request,
};
use crate::video::{VideoRender, render_video, video_error};
use blockscope_render::{Background, LightingProfile, RenderBackend, RenderOptions};
use blockscope_video::{
    Animation, Easing, Encoder, Keyframe, MAX_DURATION_SECONDS, MAX_FRAMES, Quality, VideoFormat,
    VideoSettings,
};
use serde_json::{Value, json};
use std::fs;
use std::time::Instant;

pub(super) fn process_video(
    request: &RenderRequest,
    cache: &mut Vec<CachedScene>,
    context: &ServeContext<'_>,
    format: VideoFormat,
    started: Instant,
) -> Result<Value, crate::CliError> {
    if !request.views.is_empty() {
        return Err(invalid(
            "views and animation are mutually exclusive for video output",
        ));
    }
    if request.elevation.is_some() {
        return Err(invalid(
            "elevation comes from animation keyframes, not the render options",
        ));
    }
    if let Some(mode) = &request.stream_mesh
        && mode != "off"
    {
        return Err(invalid("streamMesh is not supported for video output"));
    }

    let Some(animation) = request.animation.as_ref() else {
        return Err(crate::CliError::input(
            "ANIMATION_INVALID: animation is required for mp4 and gif output",
        ));
    };
    if animation.keyframes.is_empty() {
        return Err(crate::CliError::input(
            "ANIMATION_INVALID: at least one keyframe is required",
        ));
    }

    let keyframes = animation
        .keyframes
        .iter()
        .map(|keyframe| Keyframe::new(keyframe.time, keyframe.yaw, keyframe.elevation))
        .collect::<Vec<_>>();
    let fps = animation
        .fps
        .unwrap_or(if format == VideoFormat::Mp4 { 30 } else { 15 });
    let easing =
        Easing::parse(animation.easing.as_deref().unwrap_or("linear")).ok_or_else(|| {
            crate::CliError::input("ANIMATION_INVALID: easing must be linear or smooth")
        })?;
    let animation = Animation::new(keyframes, animation.duration, fps, animation.looped, easing)
        .map_err(video_error)?;
    let plan = animation.frame_plan();
    if !context.force_limits {
        if animation.duration() > MAX_DURATION_SECONDS {
            return Err(crate::CliError::input(format!(
                "ANIMATION_INVALID: duration must be at most {MAX_DURATION_SECONDS} seconds"
            )));
        }
        if plan.len() > MAX_FRAMES {
            return Err(crate::CliError::input(format!(
                "ANIMATION_INVALID: {} frames exceed the {MAX_FRAMES}-frame limit",
                plan.len()
            )));
        }
    }

    let background = crate::parse_background(request.background.as_deref().unwrap_or("#000000"))?;
    let lighting = LightingProfile::parse(request.lighting.as_deref().unwrap_or("simple-ao-v1"))
        .ok_or_else(|| {
            crate::CliError::input(format!(
                "INVALID_RENDER_OPTIONS: unknown lighting profile '{}'",
                request.lighting.as_deref().unwrap_or_default()
            ))
        })?;
    let texture_filter =
        crate::parse_texture_filter(request.texture_filter.as_deref().unwrap_or("nearest"))?;
    let backend =
        RenderBackend::parse(request.backend.as_deref().unwrap_or("auto")).ok_or_else(|| {
            crate::CliError::input(format!(
                "INVALID_RENDER_OPTIONS: unknown backend '{}'",
                request.backend.as_deref().unwrap_or_default()
            ))
        })?;
    let encoder =
        Encoder::parse(request.encoder.as_deref().unwrap_or("auto")).ok_or_else(|| {
            crate::CliError::input(format!(
                "INVALID_RENDER_OPTIONS: unknown encoder '{}'",
                request.encoder.as_deref().unwrap_or_default()
            ))
        })?;
    let quality =
        Quality::parse(request.quality.as_deref().unwrap_or("medium")).ok_or_else(|| {
            crate::CliError::input(format!(
                "INVALID_RENDER_OPTIONS: unknown quality '{}'",
                request.quality.as_deref().unwrap_or_default()
            ))
        })?;

    let width = request.width.unwrap_or(DEFAULT_SIZE);
    let height = request.height.unwrap_or(DEFAULT_SIZE);
    let threads = request.threads.unwrap_or(context.default_threads);
    let settings = VideoSettings {
        width,
        height,
        fps: plan.fps(),
        format,
        quality,
        encoder,
        looped: plan.looped(),
        transparent: matches!(background, Background::Transparent),
        threads,
    };
    settings.validate().map_err(video_error)?;

    let Some(ffmpeg) = context.ffmpeg.as_ref() else {
        return Err(crate::CliError::input(
            context
                .ffmpeg_error
                .clone()
                .unwrap_or_else(|| "FFMPEG_NOT_FOUND: ffmpeg is not available".to_string()),
        ));
    };
    ffmpeg
        .resolve_encoder(encoder, format, width, height)
        .map_err(video_error)?;

    fs::create_dir_all(&request.output_dir).map_err(|error| {
        crate::CliError::input(format!(
            "IO_ERROR: {}: {error}",
            request.output_dir.display()
        ))
    })?;

    let mut options = RenderOptions {
        width,
        height,
        yaw: DEFAULT_YAW,
        elevation: DEFAULT_ELEVATION,
        background,
        lighting,
        supersample: request.supersample.unwrap_or(DEFAULT_SUPERSAMPLE),
        threads,
        texture_filter,
        backend,
        ..Default::default()
    };
    crate::apply_render_limits(context.force_limits, &mut options);
    options.validate().map_err(crate::CliError::from)?;

    let LoadedScene {
        scene,
        mesh,
        snapshot,
        parse_ms,
        mesh_ms,
        cache_hit,
        input_hash,
    } = load_scene_for_request(request, cache, context, threads)?;

    let path = request
        .output_dir
        .join(format!("{}.{}", request.id, format.as_str()));
    let report = render_video(
        VideoRender {
            scene: &scene,
            mesh: &mesh,
            snapshot: snapshot.as_deref(),
            options,
            plan: &plan,
            settings: &settings,
            ffmpeg,
            output: &path,
        },
        |_, _| {},
    )?;

    Ok(json!({
        "type": "result",
        "id": request.id,
        "ok": true,
        "inputHash": input_hash,
        "sceneHash": scene.scene_hash().to_hex(),
        "cacheHit": cache_hit,
        "outputs": [{
            "kind": "video",
            "path": path.display().to_string(),
            "format": format.as_str(),
            "width": settings.width,
            "height": settings.height,
            "frames": plan.len(),
            "fps": plan.fps(),
            "durationSeconds": plan.duration(),
            "looped": plan.looped(),
            "easing": animation.easing().as_str(),
            "bytes": report.bytes,
            "encoder": report.encoder,
            "renderMs": report.render_ms,
            "encodeMs": report.encode_ms,
        }],
        "timings": {
            "parseMs": parse_ms,
            "meshMs": mesh_ms,
            "renderMs": report.render_ms,
            "encodeMs": report.encode_ms,
            "totalMs": started.elapsed().as_secs_f64() * 1000.0,
        },
        "statistics": {
            "triangles": report.statistics.triangles,
            "culledBackfaces": report.statistics.culled_backfaces,
            "textures": snapshot.as_ref().map(|snapshot| snapshot.textures().len()).unwrap_or(0),
            "backendUsed": report.statistics.backend_used.as_str(),
        },
    }))
}

fn invalid(message: &str) -> crate::CliError {
    crate::CliError::input(format!("INVALID_RENDER_OPTIONS: {message}"))
}
