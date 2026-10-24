use crate::RenderError;
use litematic_material::TextureFilter;
use litematic_mesh::Rgba;
use serde::{Deserialize, Serialize};

/// Lighting profile used by the CPU rasterizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LightingProfile {
    FlatV1,
    SimpleAoV1,
}

impl LightingProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FlatV1 => "flat-v1",
            Self::SimpleAoV1 => "simple-ao-v1",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "flat-v1" => Some(Self::FlatV1),
            "simple-ao-v1" => Some(Self::SimpleAoV1),
            _ => None,
        }
    }

    pub fn face_shade(self, normal: [f32; 3]) -> f32 {
        match self {
            Self::FlatV1 => 1.0,
            Self::SimpleAoV1 => {
                const LIGHT: [f32; 3] = [0.408_248_3, 0.816_496_6, 0.408_248_3];
                let diffuse =
                    (normal[0] * LIGHT[0] + normal[1] * LIGHT[1] + normal[2] * LIGHT[2]).max(0.0);
                0.55 + 0.45 * diffuse
            }
        }
    }
}

/// Background fill behind the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Background {
    Transparent,
    Solid(Rgba),
}

impl Background {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transparent => "transparent",
            Self::Solid(_) => "solid",
        }
    }
}

/// Renderer options. Defaults match the first QQBot preview profile.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOptions {
    pub width: u32,
    pub height: u32,
    /// Horizontal view rotation in degrees. 0 looks from +Z (south); 45 looks
    /// from the south-west. Matches the reference Litematic viewer.
    pub yaw: f32,
    pub elevation: f32,
    pub background: Background,
    pub lighting: LightingProfile,
    pub tile_size: u32,
    pub padding: f32,
    pub supersample: u32,
    /// Worker threads. 0 selects the available parallelism.
    pub threads: u32,
    pub texture_filter: TextureFilter,
    pub max_pixels: u64,
    pub max_supersampled_pixels: u64,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 1024,
            height: 1024,
            yaw: 45.0,
            elevation: 30.0,
            background: Background::Transparent,
            lighting: LightingProfile::SimpleAoV1,
            tile_size: 64,
            padding: 16.0,
            supersample: 2,
            threads: 0,
            texture_filter: TextureFilter::Nearest,
            max_pixels: 16_777_216,
            max_supersampled_pixels: 33_554_432,
        }
    }
}

impl RenderOptions {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.width == 0 || self.height == 0 {
            return Err(RenderError::invalid("width and height must be positive"));
        }

        let pixels = u64::from(self.width) * u64::from(self.height);
        if pixels > self.max_pixels {
            return Err(RenderError::too_large(format!(
                "{pixels} pixels exceed limit {}",
                self.max_pixels
            )));
        }

        if self.supersample == 0 || self.supersample > 4 {
            return Err(RenderError::invalid("supersample must be between 1 and 4"));
        }

        let internal_width = self
            .width
            .checked_mul(self.supersample)
            .ok_or_else(|| RenderError::too_large("internal width overflow"))?;
        let internal_height = self
            .height
            .checked_mul(self.supersample)
            .ok_or_else(|| RenderError::too_large("internal height overflow"))?;
        let internal_pixels = u64::from(internal_width) * u64::from(internal_height);
        if internal_pixels > self.max_supersampled_pixels {
            return Err(RenderError::too_large(format!(
                "{internal_pixels} supersampled pixels exceed limit {}",
                self.max_supersampled_pixels
            )));
        }

        if !self.yaw.is_finite() || !self.elevation.is_finite() {
            return Err(RenderError::invalid("yaw and elevation must be finite"));
        }
        if self.elevation.abs() >= 89.9 {
            return Err(RenderError::invalid(
                "elevation must be between -89.9 and 89.9 degrees",
            ));
        }
        if self.tile_size == 0 {
            return Err(RenderError::invalid("tile_size must be positive"));
        }
        if self.threads > 256 {
            return Err(RenderError::invalid("threads must be at most 256"));
        }
        if !self.padding.is_finite() || self.padding < 0.0 {
            return Err(RenderError::invalid(
                "padding must be a non-negative finite number",
            ));
        }

        let internal_padding = f64::from(self.padding) * f64::from(self.supersample);
        if f64::from(internal_width) <= 2.0 * internal_padding
            || f64::from(internal_height) <= 2.0 * internal_padding
        {
            return Err(RenderError::invalid("padding leaves no drawable area"));
        }

        Ok(())
    }

    pub fn internal_width(&self) -> u32 {
        self.width * self.supersample
    }

    pub fn internal_height(&self) -> u32 {
        self.height * self.supersample
    }
}
