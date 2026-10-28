use litematic_mesh::{Rgba, redstone_color};
use litematic_scene::BlockStateKey;

const GRASS: Rgba = Rgba::rgb(0x91, 0xBD, 0x59);
const FOLIAGE: Rgba = Rgba::rgb(0x77, 0xAB, 0x2F);
const WATER: Rgba = Rgba::rgb(0x3F, 0x76, 0xE4);
const WHITE: Rgba = Rgba::WHITE;

/// Fixed tint for the first textured release. Biome colormaps are a later
/// additive feature, so renders stay deterministic without world data.
pub fn tint_for(state: &BlockStateKey, tintindex: Option<i32>) -> Rgba {
    if tintindex.is_none() {
        return WHITE;
    }

    let path = state
        .name
        .split_once(':')
        .map(|(_, path)| path)
        .unwrap_or(&state.name);

    if path == "water" || path == "flowing_water" || path == "bubble_column" {
        return WATER;
    }
    if path == "redstone_wire" {
        return redstone_tint(state);
    }
    if path.contains("leaves") || path == "vine" {
        return FOLIAGE;
    }
    if path.contains("grass") || path.contains("fern") {
        return GRASS;
    }

    WHITE
}

fn redstone_tint(state: &BlockStateKey) -> Rgba {
    let power = state
        .properties
        .get("power")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0)
        .min(15);

    redstone_color(power)
}
