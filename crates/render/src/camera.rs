use crate::{RenderError, RenderOptions};
use blockscope_scene::Bounds;

/// Orthographic camera derived from scene bounds and a yaw/elevation preset.
#[derive(Debug, Clone)]
pub struct Camera {
    center: [f64; 3],
    right: [f64; 3],
    up: [f64; 3],
    forward: [f64; 3],
    scale: f64,
    width: f64,
    height: f64,
    #[cfg(feature = "gpu")]
    scene_span: [f64; 3],
}

/// Largest scene span for which f32 camera-relative coordinates keep at least
/// one-block resolution. Above this, the GPU path falls back to the CPU.
#[cfg(feature = "gpu")]
pub(crate) const GPU_SAFE_SPAN: f64 = 16_777_216.0;

/// Camera data for the GPU vertex shader. The frame origin is subtracted before
/// converting to f32, so large absolute world coordinates do not lose precision.
#[cfg(feature = "gpu")]
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[allow(dead_code)] // Consumed by the instanced shader path in G2.
pub(crate) struct GpuCamera {
    /// xyz = camera right, w = pixels per world unit.
    pub(crate) right: [f32; 4],
    /// xyz = camera up, w unused.
    pub(crate) up: [f32; 4],
    /// xyz = camera forward, w unused.
    pub(crate) forward: [f32; 4],
    /// Camera center relative to the frame origin.
    pub(crate) center: [f32; 4],
    /// width, height, depth_min, depth_span.
    pub(crate) size: [f32; 4],
}

#[cfg(feature = "gpu")]
impl GpuCamera {
    /// CPU-side mirror of the shader projection, used by precision tests.
    #[allow(dead_code)] // Consumed by the instanced shader path in G2.
    pub(crate) fn project(&self, page_offset: [f32; 3], local: [f32; 3]) -> [f32; 3] {
        let relative = [
            page_offset[0] + local[0] - self.center[0],
            page_offset[1] + local[1] - self.center[1],
            page_offset[2] + local[2] - self.center[2],
        ];
        let x =
            relative[0] * self.right[0] + relative[1] * self.right[1] + relative[2] * self.right[2];
        let y = relative[0] * self.up[0] + relative[1] * self.up[1] + relative[2] * self.up[2];
        let depth = relative[0] * self.forward[0]
            + relative[1] * self.forward[1]
            + relative[2] * self.forward[2];
        [
            self.size[0] * 0.5 + x * self.right[3],
            self.size[1] * 0.5 - y * self.right[3],
            (depth - self.size[2]) / self.size[3],
        ]
    }
}

impl Camera {
    pub fn new(bounds: &Bounds, options: &RenderOptions) -> Result<Self, RenderError> {
        let yaw = f64::from(options.yaw).to_radians();
        let elevation = f64::from(options.elevation).to_radians();

        // Yaw follows the same convention as the reference Litematic viewer and
        // Minecraft's compass: yaw 0 looks from +Z (south) toward north, yaw 45
        // looks from the south-west. The negated Z term is what makes a shared
        // yaw value produce the same side of the structure as that viewer.
        let forward = normalize([
            elevation.cos() * yaw.sin(),
            -elevation.sin(),
            -elevation.cos() * yaw.cos(),
        ])
        .ok_or_else(|| RenderError::invalid("camera forward vector is degenerate"))?;

        let right = normalize(cross(forward, [0.0, 1.0, 0.0]))
            .ok_or_else(|| RenderError::invalid("camera right vector is degenerate"))?;
        let up = cross(right, forward);

        let center = [
            (bounds.min[0] as f64 + bounds.max_exclusive[0] as f64) * 0.5,
            (bounds.min[1] as f64 + bounds.max_exclusive[1] as f64) * 0.5,
            (bounds.min[2] as f64 + bounds.max_exclusive[2] as f64) * 0.5,
        ];

        let (min_x, max_x, min_y, max_y) = projected_span(bounds, center, right, up);

        let width = f64::from(options.internal_width());
        let height = f64::from(options.internal_height());
        let padding = f64::from(options.padding) * f64::from(options.supersample);
        let available_width = width - 2.0 * padding;
        let available_height = height - 2.0 * padding;
        if available_width <= 0.0 || available_height <= 0.0 {
            return Err(RenderError::invalid("padding leaves no drawable area"));
        }

        // The normal fit uses the yaw-dependent projected silhouette. A sphere
        // fit uses the rotation-invariant bounding sphere instead, which keeps
        // turntable framing stable at the cost of a slightly looser still.
        let (span_x, span_y) = if options.sphere_fit {
            let diameter = 2.0 * bounding_sphere_radius(bounds, center);
            (diameter, diameter)
        } else {
            ((max_x - min_x).max(1e-6), (max_y - min_y).max(1e-6))
        };
        let scale = (available_width / span_x).min(available_height / span_y);
        if !scale.is_finite() || scale <= 0.0 {
            return Err(RenderError::invalid("camera scale is invalid"));
        }

        #[cfg(feature = "gpu")]
        let scene_span = [
            (bounds.max_exclusive[0] - bounds.min[0]) as f64,
            (bounds.max_exclusive[1] - bounds.min[1]) as f64,
            (bounds.max_exclusive[2] - bounds.min[2]) as f64,
        ];

        Ok(Self {
            center,
            right,
            up,
            forward,
            scale,
            width,
            height,
            #[cfg(feature = "gpu")]
            scene_span,
        })
    }

    pub fn forward(&self) -> [f64; 3] {
        self.forward
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Project a world position to screen x, screen y, and camera depth.
    /// Smaller depth values are closer to the camera.
    pub fn project(&self, world: [f64; 3]) -> [f64; 3] {
        let relative = [
            world[0] - self.center[0],
            world[1] - self.center[1],
            world[2] - self.center[2],
        ];

        let x = dot(relative, self.right);
        let y = dot(relative, self.up);
        let depth = dot(relative, self.forward);

        [
            self.width * 0.5 + x * self.scale,
            self.height * 0.5 - y * self.scale,
            depth,
        ]
    }

    /// Integer frame origin used to keep GPU coordinates small.
    #[cfg(feature = "gpu")]
    pub(crate) fn frame_origin(&self) -> Option<[i32; 3]> {
        let mut origin = [0i32; 3];
        for (axis, value) in origin.iter_mut().enumerate() {
            *value = i32::try_from(self.center[axis].round() as i64).ok()?;
        }
        Some(origin)
    }

    /// Camera-relative uniform for the GPU vertex shader.
    ///
    /// Returns `None` when the scene span exceeds the measured f32-safe limit;
    /// callers fall back to the CPU path rather than rendering a silently
    /// inaccurate image.
    #[cfg(feature = "gpu")]
    pub(crate) fn gpu_camera(
        &self,
        frame_origin: [i32; 3],
        depth_min: f64,
        depth_max: f64,
    ) -> Option<GpuCamera> {
        if self.scene_span.iter().any(|span| *span > GPU_SAFE_SPAN) {
            return None;
        }

        let center: [f32; 3] =
            std::array::from_fn(|axis| (self.center[axis] - f64::from(frame_origin[axis])) as f32);
        let size = [
            self.width as f32,
            self.height as f32,
            depth_min as f32,
            (depth_max - depth_min).max(1e-6) as f32,
        ];
        if !size.iter().all(|value| value.is_finite()) {
            return None;
        }

        Some(GpuCamera {
            right: [
                self.right[0] as f32,
                self.right[1] as f32,
                self.right[2] as f32,
                self.scale as f32,
            ],
            up: [self.up[0] as f32, self.up[1] as f32, self.up[2] as f32, 0.0],
            forward: [
                self.forward[0] as f32,
                self.forward[1] as f32,
                self.forward[2] as f32,
                0.0,
            ],
            center: [center[0], center[1], center[2], 0.0],
            size,
        })
    }
}

fn projected_span(
    bounds: &Bounds,
    center: [f64; 3],
    right: [f64; 3],
    up: [f64; 3],
) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for x in [bounds.min[0] as f64, bounds.max_exclusive[0] as f64] {
        for y in [bounds.min[1] as f64, bounds.max_exclusive[1] as f64] {
            for z in [bounds.min[2] as f64, bounds.max_exclusive[2] as f64] {
                let relative = [x - center[0], y - center[1], z - center[2]];
                let projected_x = dot(relative, right);
                let projected_y = dot(relative, up);
                min_x = min_x.min(projected_x);
                max_x = max_x.max(projected_x);
                min_y = min_y.min(projected_y);
                max_y = max_y.max(projected_y);
            }
        }
    }

    (min_x, max_x, min_y, max_y)
}

fn bounding_sphere_radius(bounds: &Bounds, center: [f64; 3]) -> f64 {
    let mut radius = 0.0f64;
    for x in [bounds.min[0] as f64, bounds.max_exclusive[0] as f64] {
        for y in [bounds.min[1] as f64, bounds.max_exclusive[1] as f64] {
            for z in [bounds.min[2] as f64, bounds.max_exclusive[2] as f64] {
                let dx = x - center[0];
                let dy = y - center[1];
                let dz = z - center[2];
                radius = radius.max((dx * dx + dy * dy + dz * dz).sqrt());
            }
        }
    }
    radius.max(1e-6)
}

fn normalize(value: [f64; 3]) -> Option<[f64; 3]> {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if length <= 1e-12 {
        return None;
    }
    Some([value[0] / length, value[1] / length, value[2] / length])
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF_SQRT_2: f64 = std::f64::consts::FRAC_1_SQRT_2;

    fn camera(yaw: f64) -> Camera {
        let bounds = Bounds {
            min: [0, 0, 0],
            max_exclusive: [1, 1, 1],
        };
        let options = RenderOptions {
            width: 64,
            height: 64,
            yaw: yaw as f32,
            elevation: 0.0,
            ..Default::default()
        };
        Camera::new(&bounds, &options).unwrap()
    }

    fn assert_close(actual: [f64; 3], expected: [f64; 3]) {
        for axis in 0..3 {
            assert!(
                (actual[axis] - expected[axis]).abs() < 1e-9,
                "axis {axis}: {actual:?} != {expected:?}"
            );
        }
    }

    #[test]
    fn yaw_zero_looks_north_from_the_south() {
        let camera = camera(0.0);
        assert_close(camera.forward, [0.0, 0.0, -1.0]);
        assert_close(camera.right, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn default_yaw_matches_the_reference_viewer_orientation() {
        // The reference viewer's 45 degree preset looks from the south-west and
        // puts the south face on the right of the image.
        let camera = camera(45.0);
        assert_close(camera.forward, [HALF_SQRT_2, 0.0, -HALF_SQRT_2]);
        assert_close(camera.right, [HALF_SQRT_2, 0.0, HALF_SQRT_2]);
    }

    #[cfg(feature = "gpu")]
    #[test]
    fn gpu_camera_matches_cpu_projection() {
        let bounds = Bounds {
            min: [0, 0, 0],
            max_exclusive: [8, 8, 8],
        };
        let options = RenderOptions {
            width: 256,
            height: 256,
            yaw: 45.0,
            elevation: 30.0,
            supersample: 1,
            ..Default::default()
        };
        let camera = Camera::new(&bounds, &options).unwrap();
        let frame_origin = camera.frame_origin().unwrap();
        let mut near = f64::INFINITY;
        let mut far = f64::NEG_INFINITY;
        for x in [bounds.min[0] as f64, bounds.max_exclusive[0] as f64] {
            for y in [bounds.min[1] as f64, bounds.max_exclusive[1] as f64] {
                for z in [bounds.min[2] as f64, bounds.max_exclusive[2] as f64] {
                    let depth = camera.project([x, y, z])[2];
                    near = near.min(depth);
                    far = far.max(depth);
                }
            }
        }
        let gpu = camera.gpu_camera(frame_origin, near, far).unwrap();

        for (page_origin, local) in [
            ([0, 0, 0], [0.25, 0.5, 0.75]),
            ([2, 3, 1], [0.5, 0.5, 0.5]),
            ([7, 7, 7], [1.0, 1.0, 1.0]),
        ] {
            let page_offset =
                std::array::from_fn(|axis| (page_origin[axis] - frame_origin[axis]) as f32);
            let world =
                std::array::from_fn(|axis| f64::from(page_origin[axis]) + f64::from(local[axis]));
            let cpu = camera.project(world);
            let projected = gpu.project(page_offset, local);
            assert!(
                (projected[0] - cpu[0] as f32).abs() < 0.01,
                "x error for {world:?}: {projected:?} vs {cpu:?}"
            );
            assert!(
                (projected[1] - cpu[1] as f32).abs() < 0.01,
                "y error for {world:?}: {projected:?} vs {cpu:?}"
            );
            let cpu_depth = ((cpu[2] - near) / (far - near)) as f32;
            assert!(
                (projected[2] - cpu_depth).abs() < 1e-5,
                "depth error for {world:?}: {projected:?} vs {cpu:?}"
            );
        }
    }

    #[cfg(feature = "gpu")]
    #[test]
    fn gpu_camera_precision_at_the_safe_limit() {
        let span = GPU_SAFE_SPAN as i64;
        let bounds = Bounds {
            min: [0, 0, 0],
            max_exclusive: [span, span, span],
        };
        let options = RenderOptions {
            width: 1024,
            height: 1024,
            yaw: 45.0,
            elevation: 30.0,
            supersample: 1,
            ..Default::default()
        };
        let camera = Camera::new(&bounds, &options).unwrap();
        let frame_origin = camera.frame_origin().unwrap();
        let mut near = f64::INFINITY;
        let mut far = f64::NEG_INFINITY;
        for x in [0.0, span as f64] {
            for y in [0.0, span as f64] {
                for z in [0.0, span as f64] {
                    let depth = camera.project([x, y, z])[2];
                    near = near.min(depth);
                    far = far.max(depth);
                }
            }
        }
        let gpu = camera.gpu_camera(frame_origin, near, far).unwrap();
        let page_origin = [(span / 2) as i32, (span / 3) as i32, (span / 5) as i32];
        let local = [0.25, 0.5, 0.75];
        let page_offset =
            std::array::from_fn(|axis| (page_origin[axis] - frame_origin[axis]) as f32);
        let world = std::array::from_fn(|axis| page_origin[axis] as f64 + local[axis] as f64);
        let cpu = camera.project(world);
        let projected = gpu.project(page_offset, local);

        assert!((projected[0] - cpu[0] as f32).abs() < 0.05);
        assert!((projected[1] - cpu[1] as f32).abs() < 0.05);
    }

    #[cfg(feature = "gpu")]
    #[test]
    fn gpu_camera_rejects_spans_above_the_safe_limit() {
        let safe = Bounds {
            min: [0, 0, 0],
            max_exclusive: [GPU_SAFE_SPAN as i64, 1, 1],
        };
        let unsafe_span = Bounds {
            min: [0, 0, 0],
            max_exclusive: [GPU_SAFE_SPAN as i64 + 1, 1, 1],
        };
        let options = RenderOptions {
            width: 64,
            height: 64,
            ..Default::default()
        };
        let safe_camera = Camera::new(&safe, &options).unwrap();
        let unsafe_camera = Camera::new(&unsafe_span, &options).unwrap();
        let origin = safe_camera.frame_origin().unwrap();

        assert!(safe_camera.gpu_camera(origin, -1.0, 1.0).is_some());
        assert!(unsafe_camera.gpu_camera(origin, -1.0, 1.0).is_none());
    }

    #[test]
    fn sphere_fit_keeps_scale_constant_across_yaw() {
        let bounds = Bounds {
            min: [0, 0, 0],
            max_exclusive: [10, 1, 1],
        };
        let base = RenderOptions {
            width: 64,
            height: 64,
            elevation: 0.0,
            sphere_fit: true,
            ..Default::default()
        };

        let first = Camera::new(
            &bounds,
            &RenderOptions {
                yaw: 0.0,
                ..base.clone()
            },
        )
        .unwrap();
        let second = Camera::new(&bounds, &RenderOptions { yaw: 90.0, ..base }).unwrap();

        assert!((first.scale() - second.scale()).abs() < 1e-9);
    }

    #[test]
    fn silhouette_fit_tracks_the_yaw_dependent_span() {
        let bounds = Bounds {
            min: [0, 0, 0],
            max_exclusive: [10, 1, 1],
        };
        let base = RenderOptions {
            width: 64,
            height: 64,
            elevation: 0.0,
            ..Default::default()
        };

        let first = Camera::new(
            &bounds,
            &RenderOptions {
                yaw: 0.0,
                ..base.clone()
            },
        )
        .unwrap();
        let second = Camera::new(&bounds, &RenderOptions { yaw: 90.0, ..base }).unwrap();

        assert!((first.scale() - second.scale()).abs() > 1.0);
    }
}
