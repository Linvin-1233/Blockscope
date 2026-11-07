use crate::CliError;
use litematic_material::MaterialSnapshot;
use litematic_mesh::Mesh;
use litematic_render::{
    GpuStageTimings, RenderBackend, RenderOptions, RenderOutput, RenderStatistics, render_mesh,
    render_mesh_with_textures,
};
use litematic_scene::Scene;
use litematic_video::{Ffmpeg, FramePlan, VideoEncoder, VideoError, VideoSettings};
use std::path::Path;
use std::time::Instant;

pub(crate) struct VideoRender<'a> {
    pub scene: &'a Scene,
    pub mesh: &'a Mesh,
    pub snapshot: Option<&'a MaterialSnapshot>,
    pub options: RenderOptions,
    pub plan: &'a FramePlan,
    pub settings: &'a VideoSettings,
    pub ffmpeg: &'a Ffmpeg,
    pub output: &'a Path,
}

pub(crate) struct VideoRenderReport {
    pub backend: RenderBackend,
    pub encoder: &'static str,
    pub render_ms: f64,
    pub encode_ms: f64,
    pub bytes: u64,
    pub statistics: RenderStatistics,
    pub gpu: GpuStageTimings,
}

/// Render every frame in the plan, feed it to the encoder, and finish the
/// output. The caller owns progress display and result formatting.
pub(crate) fn render_video(
    job: VideoRender<'_>,
    mut on_frame: impl FnMut(usize, usize),
) -> Result<VideoRenderReport, CliError> {
    let resolved = job
        .ffmpeg
        .resolve_encoder(
            job.settings.encoder,
            job.settings.format,
            job.settings.width,
            job.settings.height,
        )
        .map_err(video_error)?;

    #[cfg(feature = "gpu")]
    let textures = job.snapshot.map(|snapshot| snapshot.textures());
    #[cfg(feature = "gpu")]
    let mut gpu_session: Option<litematic_render::GpuFrameSession<'_>> = None;
    #[cfg(feature = "gpu")]
    let mut actual_backend = RenderBackend::Cpu;
    #[cfg(not(feature = "gpu"))]
    let actual_backend = RenderBackend::Cpu;

    match job.options.backend {
        RenderBackend::Cpu => {}
        RenderBackend::Gpu | RenderBackend::Auto => {
            #[cfg(feature = "gpu")]
            {
                let first = frame_options(&job.options, job.plan, 0, RenderBackend::Gpu);
                match litematic_render::GpuFrameSession::new(job.scene, job.mesh, &first, textures)
                {
                    Ok(session) => {
                        gpu_session = Some(session);
                        actual_backend = RenderBackend::Gpu;
                    }
                    Err(error) if job.options.backend == RenderBackend::Auto => {
                        crate::ui::errln!(
                            "{}warning:{:#} GPU frame session unavailable, using CPU: {error}",
                            crate::ui::WARNING,
                            crate::ui::WARNING
                        );
                    }
                    Err(error) => return Err(CliError::from(error)),
                }
            }
            #[cfg(not(feature = "gpu"))]
            if job.options.backend == RenderBackend::Gpu {
                return Err(CliError::input(
                    "BACKEND_UNAVAILABLE: the GPU backend is not enabled in this build",
                ));
            }
        }
    }

    let mut encoder = VideoEncoder::start_resolved(job.ffmpeg, job.settings, resolved, job.output)
        .map_err(video_error)?;
    let mut render_ms = 0.0;
    let mut encode_ms = 0.0;
    let mut statistics = None;
    let mut gpu = GpuStageTimings::default();

    for index in 0..job.plan.len() {
        on_frame(index as usize, job.plan.len() as usize);
        let frame_backend = if actual_backend == RenderBackend::Gpu {
            RenderBackend::Gpu
        } else {
            RenderBackend::Cpu
        };
        let options = frame_options(&job.options, job.plan, index, frame_backend);
        let render_started = Instant::now();
        #[cfg(feature = "gpu")]
        let output = if let Some(session) = gpu_session.as_mut() {
            session.render_frame(&options).map_err(CliError::from)?
        } else {
            render_cpu(job.scene, job.mesh, job.snapshot, &options)?
        };
        #[cfg(not(feature = "gpu"))]
        let output = render_cpu(job.scene, job.mesh, job.snapshot, &options)?;
        render_ms += render_started.elapsed().as_secs_f64() * 1000.0;
        if statistics.is_none() {
            statistics = Some(output.statistics.clone());
        }
        let stages = &output.statistics.gpu;
        gpu.setup_us = gpu.setup_us.saturating_add(stages.setup_us);
        gpu.atlas_us = gpu.atlas_us.saturating_add(stages.atlas_us);
        gpu.encode_us = gpu.encode_us.saturating_add(stages.encode_us);
        gpu.upload_us = gpu.upload_us.saturating_add(stages.upload_us);
        gpu.project_us = gpu.project_us.saturating_add(stages.project_us);
        gpu.draw_us = gpu.draw_us.saturating_add(stages.draw_us);
        gpu.translucent_us = gpu.translucent_us.saturating_add(stages.translucent_us);
        gpu.readback_us = gpu.readback_us.saturating_add(stages.readback_us);
        gpu.resolve_us = gpu.resolve_us.saturating_add(stages.resolve_us);
        gpu.total_us = gpu.total_us.saturating_add(stages.total_us);
        gpu.atlas_cache_hit |= stages.atlas_cache_hit;
        gpu.wboit_used |= stages.wboit_used;

        let encode_started = Instant::now();
        encoder.write_frame(&output.rgba).map_err(video_error)?;
        encode_ms += encode_started.elapsed().as_secs_f64() * 1000.0;
    }

    let finish_started = Instant::now();
    let bytes = encoder.finish().map_err(video_error)?;
    encode_ms += finish_started.elapsed().as_secs_f64() * 1000.0;

    Ok(VideoRenderReport {
        backend: actual_backend,
        encoder: resolved.name(),
        render_ms,
        encode_ms,
        bytes,
        statistics: statistics.unwrap_or_default(),
        gpu,
    })
}

pub(crate) fn frame_options(
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
    scene: &Scene,
    mesh: &Mesh,
    snapshot: Option<&MaterialSnapshot>,
    options: &RenderOptions,
) -> Result<RenderOutput, CliError> {
    match snapshot {
        Some(snapshot) => render_mesh_with_textures(scene, mesh, options, snapshot.textures())
            .map_err(CliError::from),
        None => render_mesh(scene, mesh, options).map_err(CliError::from),
    }
}

pub(crate) fn video_error(error: VideoError) -> CliError {
    CliError::input(error.to_string())
}

/// `hello.video` shared by `serve` and `supervise`.
pub(crate) fn hello_object(ffmpeg: Option<&Ffmpeg>, error: Option<&str>) -> serde_json::Value {
    match ffmpeg {
        Some(ffmpeg) => serde_json::json!({
            "available": true,
            "formats": ["mp4", "gif"],
            "encoders": ffmpeg.video_encoders(),
            "ffmpegVersion": ffmpeg.version_string(),
            "maxFrames": litematic_video::MAX_FRAMES,
            "maxDurationSeconds": litematic_video::MAX_DURATION_SECONDS,
            "hardwareProbe": "lazy",
        }),
        None => serde_json::json!({
            "available": false,
            "reason": error.unwrap_or("ffmpeg is not available"),
            "formats": [],
            "encoders": [],
            "ffmpegVersion": null,
            "maxFrames": litematic_video::MAX_FRAMES,
            "maxDurationSeconds": litematic_video::MAX_DURATION_SECONDS,
            "hardwareProbe": "lazy",
        }),
    }
}
