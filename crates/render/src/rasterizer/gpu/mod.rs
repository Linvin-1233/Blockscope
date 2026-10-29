use crate::{RenderError, RenderOptions, RenderOutput};
use litematic_material::TextureSet;
use litematic_mesh::Mesh;
use litematic_scene::Scene;

#[cfg(feature = "gpu")]
mod atlas;
#[cfg(feature = "gpu")]
mod backend;
#[cfg(feature = "gpu")]
mod resolve;
#[cfg(feature = "gpu")]
mod vertex;

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
