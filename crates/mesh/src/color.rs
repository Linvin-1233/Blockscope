use serde::{Deserialize, Serialize};

const REDSTONE_LOW: Rgba = Rgba::rgb(0x38, 0x00, 0x00);
const REDSTONE_HIGH: Rgba = Rgba::rgb(0xFF, 0x20, 0x20);

/// Redstone dust tint for a power level in `0..=15`.
///
/// The square-root curve spreads the dark low-power steps apart. A linear ramp
/// leaves power 0 and 1 only about twelve red levels apart, which disappears on
/// thin dust lines; this keeps every power visibly distinct.
pub fn redstone_color(power: u8) -> Rgba {
    let factor = (f32::from(power.min(15)) / 15.0).sqrt();
    let channel = |low: u8, high: u8| {
        (f32::from(low) + (f32::from(high) - f32::from(low)) * factor).round() as u8
    };

    Rgba::rgb(
        channel(REDSTONE_LOW.r, REDSTONE_HIGH.r),
        channel(REDSTONE_LOW.g, REDSTONE_HIGH.g),
        channel(REDSTONE_LOW.b, REDSTONE_HIGH.b),
    )
}

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

    #[test]
    fn redstone_power_spans_a_visible_gradient() {
        let low = super::redstone_color(0);
        let one = super::redstone_color(1);
        let high = super::redstone_color(15);

        assert_eq!(low, Rgba::rgb(0x38, 0x00, 0x00));
        assert_eq!(high, Rgba::rgb(0xFF, 0x20, 0x20));
        assert!(high.r > low.r);
        assert!(
            u16::from(one.r) - u16::from(low.r) >= 40,
            "power 0 and 1 must be clearly different: {low:?} vs {one:?}"
        );
        assert!(super::redstone_color(7).r > one.r);
        assert!(super::redstone_color(7).r < high.r);
        assert_eq!(super::redstone_color(99), high);
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
