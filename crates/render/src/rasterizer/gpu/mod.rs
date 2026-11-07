use crate::{RenderError, RenderOptions, RenderOutput};
use litematic_material::TextureSet;
use litematic_mesh::Mesh;
use litematic_scene::Scene;

#[cfg(feature = "gpu")]
mod atlas;
#[cfg(feature = "gpu")]
mod backend;
#[cfg(feature = "gpu")]
mod budget;
#[cfg(feature = "gpu")]
mod draw;
#[cfg(feature = "gpu")]
mod frame;
#[cfg(feature = "gpu")]
mod instance;
#[cfg(feature = "gpu")]
mod resolve;
#[cfg(feature = "gpu")]
mod session;
#[cfg(feature = "gpu")]
mod targets;
#[cfg(feature = "gpu")]
mod translucent;
#[cfg(feature = "gpu")]
mod vertex;

#[cfg(feature = "gpu")]
pub use frame::GpuFrameSession;
#[cfg(feature = "gpu")]
pub use session::GpuRenderSession;

/// Machine-level GPU facts used by the backend policy before a session exists.
#[cfg(feature = "gpu")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AdapterProfile {
    pub(super) software: bool,
    pub(super) working_set_bytes: u64,
    pub(super) wboit_supported: bool,
}

#[cfg(feature = "gpu")]
pub(super) fn adapter_profile() -> Option<AdapterProfile> {
    backend::adapter_profile()
}

#[cfg(feature = "gpu")]
pub(super) fn context_initialized() -> bool {
    backend::context_initialized()
}

#[cfg(feature = "gpu")]
pub(super) fn estimate_session_bytes(
    width: u32,
    height: u32,
    views: usize,
    wboit: bool,
    atlas_bytes: u64,
) -> u64 {
    budget::estimate_session_bytes(
        width,
        height,
        views,
        wboit,
        backend::MAX_INSTANCE_BATCH_BYTES,
        session::STAGING_SLOTS_U64,
        atlas_bytes,
    )
}

/// 60 percent of adapter VRAM when the `gpu` feature is compiled and an
/// adapter exists. Used by the CLI to batch multi-view targets.
#[cfg(feature = "gpu")]
pub fn working_set_bytes() -> Option<u64> {
    adapter_profile().map(|profile| profile.working_set_bytes)
}

/// Distinguishes a capability or initialization failure (Auto may fall back to
/// the CPU) from a failure after rendering started (propagate the error).
#[allow(dead_code)]
pub(super) enum GpuFailure {
    Unavailable(RenderError),
    Failed(RenderError),
}

impl GpuFailure {
    pub(super) fn into_error(self) -> RenderError {
        match self {
            Self::Unavailable(error) | Self::Failed(error) => error,
        }
    }
}

#[cfg(feature = "gpu")]
pub(super) fn depth_range(scene: &Scene, camera: &crate::Camera) -> (f64, f64) {
    depth_range_bounds(&scene.bounds, camera)
}

#[cfg(feature = "gpu")]
pub(super) fn depth_range_bounds(
    bounds: &litematic_scene::Bounds,
    camera: &crate::Camera,
) -> (f64, f64) {
    let min = bounds.min;
    let max = bounds.max_exclusive;
    let mut near = f64::INFINITY;
    let mut far = f64::NEG_INFINITY;
    for x in [min[0], max[0]] {
        for y in [min[1], max[1]] {
            for z in [min[2], max[2]] {
                let depth = camera.project([x as f64, y as f64, z as f64])[2];
                near = near.min(depth);
                far = far.max(depth);
            }
        }
    }
    (near, far)
}

#[cfg(feature = "gpu")]
pub(super) fn camera_uniform(
    scene: &Scene,
    camera: &crate::Camera,
) -> Result<(crate::camera::GpuCamera, [i32; 3]), RenderError> {
    camera_uniform_for_bounds(&scene.bounds, camera)
}

#[cfg(feature = "gpu")]
pub(super) fn camera_uniform_for_bounds(
    bounds: &litematic_scene::Bounds,
    camera: &crate::Camera,
) -> Result<(crate::camera::GpuCamera, [i32; 3]), RenderError> {
    let (near, far) = depth_range_bounds(bounds, camera);
    let origin = camera.frame_origin().ok_or_else(|| {
        RenderError::unavailable("scene center does not fit the GPU frame origin")
    })?;
    let uniform = camera.gpu_camera(origin, near, far).ok_or_else(|| {
        RenderError::unavailable(format!(
            "scene span exceeds the f32-safe GPU limit of {} blocks",
            crate::camera::GPU_SAFE_SPAN
        ))
    })?;
    Ok((uniform, origin))
}

#[cfg(feature = "gpu")]
pub(super) fn try_render(
    scene: &Scene,
    mesh: &Mesh,
    camera: &crate::Camera,
    options: &RenderOptions,
    textures: Option<&TextureSet>,
) -> Result<RenderOutput, GpuFailure> {
    backend::render(scene, mesh, camera, options, textures)
}

#[cfg(not(feature = "gpu"))]
pub(super) fn try_render(
    _scene: &Scene,
    _mesh: &Mesh,
    _camera: &crate::Camera,
    _options: &RenderOptions,
    _textures: Option<&TextureSet>,
) -> Result<RenderOutput, GpuFailure> {
    Err(GpuFailure::Unavailable(RenderError::unavailable(
        "GPU backend is not enabled in this build",
    )))
}
