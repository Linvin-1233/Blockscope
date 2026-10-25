use litematic_scene::{
    BlockStateKey, PackedIndices, Region, Scene, SceneErrorCode, SceneMetadata, aggregate,
};

fn metadata() -> SceneMetadata {
    SceneMetadata {
        litematic_version: 7,
        minecraft_data_version: 3700,
        ..Default::default()
    }
}

fn region(
    name: &str,
    origin: [i64; 3],
    size: [u32; 3],
    palette: &[&str],
    values: &[u32],
) -> Region {
    let volume = u64::from(size[0]) * u64::from(size[1]) * u64::from(size[2]);
    assert_eq!(values.len() as u64, volume);

    let palette: Vec<BlockStateKey> = palette
        .iter()
        .map(|name| BlockStateKey::simple(*name))
        .collect();
    let bits = PackedIndices::bits_for_palette(palette.len());
    let states = PackedIndices::from_values(bits, values).unwrap();

    Region {
        name: name.to_string(),
        origin,
        size,
        data_version: 3700,
        palette,
        states,
        block_entities: Vec::new(),
        entity_count: 0,
        pending_block_tick_count: 0,
        pending_fluid_tick_count: 0,
    }
}

fn scene(regions: Vec<Region>) -> Scene {
    Scene::new(metadata(), regions, Vec::new()).unwrap()
}

fn only_region(scene: &Scene) -> &Region {
    assert_eq!(scene.regions.len(), 1);
    &scene.regions[0]
}

fn state_at(region: &Region, index: u64) -> &BlockStateKey {
    let palette_id = region.states.get(index) as usize;
    &region.palette[palette_id]
}

#[test]
fn rejects_factors_outside_the_supported_range() {
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        &["minecraft:stone"],
        &[0],
    )]);

    assert_eq!(
        aggregate(&scene, 1).unwrap_err().code,
        SceneErrorCode::InvalidLodFactor
    );
    assert_eq!(
        aggregate(&scene, 17).unwrap_err().code,
        SceneErrorCode::InvalidLodFactor
    );
}

#[test]
fn majority_state_wins() {
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:stone", "minecraft:dirt"],
        &[0, 0],
    )]);

    let lod = aggregate(&scene, 2).unwrap();
    let region = only_region(&lod);
    assert_eq!(region.size, [1, 1, 1]);
    assert_eq!(state_at(region, 0).name, "minecraft:stone");
}

#[test]
fn ties_use_canonical_order() {
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:stone", "minecraft:dirt"],
        &[0, 1],
    )]);

    let lod = aggregate(&scene, 2).unwrap();
    assert_eq!(state_at(only_region(&lod), 0).name, "minecraft:dirt");
}

#[test]
fn air_loses_to_any_non_air() {
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[0, 1],
    )]);

    let lod = aggregate(&scene, 2).unwrap();
    assert_eq!(state_at(only_region(&lod), 0).name, "minecraft:stone");
}

#[test]
fn all_air_stays_air() {
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:air"],
        &[0, 0],
    )]);

    let lod = aggregate(&scene, 2).unwrap();
    assert_eq!(only_region(&lod).palette.len(), 1);
    assert_eq!(lod.statistics.non_air, 0);
    assert_eq!(lod.statistics.air, 1);
}

#[test]
fn sizes_round_up_at_the_edges() {
    let values = vec![0u32; 5];
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [5, 1, 1],
        &["minecraft:stone"],
        &values,
    )]);

    let lod = aggregate(&scene, 2).unwrap();
    assert_eq!(lod.bounds.min, [0, 0, 0]);
    assert_eq!(only_region(&lod).size, [3, 1, 1]);
}

#[test]
fn negative_origins_align_to_cells() {
    let values = vec![0u32; 4];
    let scene = scene(vec![region(
        "main",
        [-3, 0, 0],
        [4, 1, 1],
        &["minecraft:stone"],
        &values,
    )]);

    let lod = aggregate(&scene, 2).unwrap();
    assert_eq!(lod.bounds.min, [-2, 0, 0]);
    assert_eq!(only_region(&lod).size, [3, 1, 1]);
}

#[test]
fn later_regions_win_over_overlapping_ones() {
    let values = vec![0u32; 8];
    let lower = region(
        "lower",
        [0, 0, 0],
        [2, 2, 2],
        &["minecraft:black_wool"],
        &values,
    );
    let upper = region(
        "upper",
        [0, 0, 0],
        [2, 2, 2],
        &["minecraft:white_wool"],
        &values,
    );

    let lod = aggregate(&scene(vec![lower, upper]), 2).unwrap();
    assert_eq!(state_at(only_region(&lod), 0).name, "minecraft:white_wool");
}

#[test]
fn aggregation_is_deterministic() {
    let values: Vec<u32> = (0..64).map(|index| (index % 3) as u32).collect();
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [4, 4, 4],
        &["minecraft:stone", "minecraft:glass", "minecraft:air"],
        &values,
    )]);

    let first = aggregate(&scene, 2).unwrap();
    let second = aggregate(&scene, 2).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.scene_hash(), second.scene_hash());
}
