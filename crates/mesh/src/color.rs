use serde::{Deserialize, Serialize};

/// Straight-alpha RGBA color used by color-block meshes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    pub const WHITE: Self = Self::new(255, 255, 255, 255);
    pub const MAGENTA: Self = Self::new(255, 0, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    pub fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Parse `#RRGGBB` or `#RRGGBBAA`.
    pub fn from_hex(value: &str) -> Result<Self, String> {
        let hex = value.strip_prefix('#').unwrap_or(value);
        if hex.len() != 6 && hex.len() != 8 {
            return Err(format!("color must be #RRGGBB or #RRGGBBAA, got '{value}'"));
        }
        if !hex.is_ascii() {
            return Err(format!("color must be ASCII hex, got '{value}'"));
        }

        let parse = |range: std::ops::Range<usize>| -> Result<u8, String> {
            u8::from_str_radix(&hex[range], 16).map_err(|_| format!("invalid hex color '{value}'"))
        };

        Ok(Self::new(
            parse(0..2)?,
            parse(2..4)?,
            parse(4..6)?,
            if hex.len() == 8 { parse(6..8)? } else { 255 },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_hex_rejects_non_ascii_without_panicking() {
        assert!(Rgba::from_hex("#日aaa").is_err());
        assert!(Rgba::from_hex("#aéaaa").is_err());
    }

    #[test]
    fn from_hex_parses_rgb_and_rgba() {
        assert_eq!(Rgba::from_hex("#112233"), Ok(Rgba::rgb(0x11, 0x22, 0x33)));
        assert_eq!(
            Rgba::from_hex("#11223344"),
            Ok(Rgba::new(0x11, 0x22, 0x33, 0x44))
        );
    }
}

/// Material layer used by the future rasterizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderLayer {
    Opaque,
    Cutout,
    Translucent,
}

impl RenderLayer {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
            Self::Cutout => "cutout",
            Self::Translucent => "translucent",
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Opaque => 0,
            Self::Cutout => 1,
            Self::Translucent => 2,
        }
    }
}
