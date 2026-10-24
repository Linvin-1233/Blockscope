use crate::{RenderError, RenderOptions};
use litematic_scene::Bounds;

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

        let span_x = (max_x - min_x).max(1e-6);
        let span_y = (max_y - min_y).max(1e-6);
        let scale = (available_width / span_x).min(available_height / span_y);
        if !scale.is_finite() || scale <= 0.0 {
            return Err(RenderError::invalid("camera scale is invalid"));
        }

        Ok(Self {
            center,
            right,
            up,
            forward,
            scale,
            width,
            height,
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
}
