use litematic_nbt::{NbtCompound, NbtValue};
use litematic_scene::{
    BlockEntity, BlockStateKey, PackedIndices, Region, Scene, SceneMetadata, sha256,
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
    palette: Vec<BlockStateKey>,
    states: PackedIndices,
) -> Region {
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

fn air_stone_region(value: i64) -> Region {
    region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        vec![
            BlockStateKey::simple("minecraft:air"),
            BlockStateKey::simple("minecraft:stone"),
        ],
        PackedIndices::new(2, vec![value], 1).unwrap(),
    )
}

#[test]
fn sha256_matches_known_vector() {
    assert_eq!(
        sha256(b"").to_hex(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn identical_scenes_share_a_hash() {
    let first = scene(vec![air_stone_region(1)]);
    let second = scene(vec![air_stone_region(1)]);

    assert_eq!(first.scene_hash(), second.scene_hash());
}

#[test]
fn changing_a_block_state_changes_the_hash() {
    let air = scene(vec![air_stone_region(0)]);
    let stone = scene(vec![air_stone_region(1)]);

    assert_ne!(air.scene_hash(), stone.scene_hash());
}

#[test]
fn changing_block_entity_data_changes_the_hash() {
    let mut left = air_stone_region(0);
    left.block_entities.push(BlockEntity {
        local_pos: [0, 0, 0],
        id: "minecraft:chest".to_string(),
        data: NbtCompound::default(),
    });

    let mut right = air_stone_region(0);
    let mut data = NbtCompound::default();
    data.insert(
        "CustomName".to_string(),
        NbtValue::String("treasure".to_string()),
    );
    right.block_entities.push(BlockEntity {
        local_pos: [0, 0, 0],
        id: "minecraft:chest".to_string(),
        data,
    });

    assert_ne!(
        scene(vec![left]).scene_hash(),
        scene(vec![right]).scene_hash()
    );
}

#[test]
fn changing_region_order_changes_the_hash() {
    let first = region(
        "a",
        [0, 0, 0],
        [1, 1, 1],
        vec![BlockStateKey::simple("minecraft:air")],
        PackedIndices::new(2, vec![0], 1).unwrap(),
    );
    let second = region(
        "b",
        [2, 0, 0],
        [1, 1, 1],
        vec![BlockStateKey::simple("minecraft:air")],
        PackedIndices::new(2, vec![0], 1).unwrap(),
    );

    let left = scene(vec![first.clone(), second.clone()]);
    let right = scene(vec![second, first]);

    assert_ne!(left.scene_hash(), right.scene_hash());
}
