#![allow(dead_code)]

use litematic_scene::{BlockStateKey, PackedIndices, Region, Scene, SceneMetadata};

pub fn metadata() -> SceneMetadata {
    SceneMetadata {
        litematic_version: 7,
        minecraft_data_version: 3700,
        ..Default::default()
    }
}

pub fn region(
    name: &str,
    origin: [i64; 3],
    size: [u32; 3],
    palette: &[&str],
    values: &[u32],
) -> Region {
    let palette: Vec<BlockStateKey> = palette
        .iter()
        .map(|name| BlockStateKey::simple(*name))
        .collect();
    region_states(name, origin, size, &palette, values)
}

pub fn region_states(
    name: &str,
    origin: [i64; 3],
    size: [u32; 3],
    palette: &[BlockStateKey],
    values: &[u32],
) -> Region {
    let volume = u64::from(size[0]) * u64::from(size[1]) * u64::from(size[2]);
    assert_eq!(
        values.len() as u64,
        volume,
        "test region values must cover the volume"
    );

    let bits = PackedIndices::bits_for_palette(palette.len());
    let states = PackedIndices::from_values(bits, values).unwrap();

    Region {
        name: name.to_string(),
        origin,
        size,
        data_version: 3700,
        palette: palette.to_vec(),
        states,
        block_entities: Vec::new(),
        entity_count: 0,
        pending_block_tick_count: 0,
        pending_fluid_tick_count: 0,
    }
}

pub fn scene(regions: Vec<Region>) -> Scene {
    Scene::new(metadata(), regions, Vec::new()).unwrap()
}
