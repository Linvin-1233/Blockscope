use litematic_nbt::{NbtCompound, NbtValue};
use litematic_scene::{
    BlockEntity, BlockStateKey, DiagnosticCode, PackedIndices, Region, Scene, SceneErrorCode,
    SceneMetadata,
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

#[test]
fn statistics_count_air_and_blocks() {
    let palette = vec![
        BlockStateKey::simple("minecraft:air"),
        BlockStateKey::simple("minecraft:stone"),
    ];
    let states = PackedIndices::new(2, vec![4], 2).unwrap();
    let scene = Scene::new(
        metadata(),
        vec![region("main", [0, 0, 0], [2, 1, 1], palette, states)],
        Vec::new(),
    )
    .unwrap();

    assert_eq!(scene.statistics.volume, 2);
    assert_eq!(scene.statistics.air, 1);
    assert_eq!(scene.statistics.non_air, 1);
    assert_eq!(
        scene
            .statistics
            .blocks
            .get(&BlockStateKey::simple("minecraft:air")),
        Some(&1)
    );
    assert_eq!(
        scene
            .statistics
            .blocks
            .get(&BlockStateKey::simple("minecraft:stone")),
        Some(&1)
    );
}

#[test]
fn bounds_union_covers_all_regions() {
    let first = region(
        "first",
        [0, 0, 0],
        [1, 1, 1],
        vec![BlockStateKey::simple("minecraft:air")],
        PackedIndices::new(2, vec![0], 1).unwrap(),
    );
    let second = region(
        "second",
        [10, 20, 30],
        [2, 2, 2],
        vec![BlockStateKey::simple("minecraft:air")],
        PackedIndices::new(2, vec![0], 8).unwrap(),
    );
    let scene = Scene::new(metadata(), vec![first, second], Vec::new()).unwrap();

    assert_eq!(scene.bounds.min, [0, 0, 0]);
    assert_eq!(scene.bounds.max_exclusive, [12, 22, 32]);
    assert_eq!(scene.bounds.size(), [12, 22, 32]);
}

#[test]
fn overlapping_regions_produce_a_diagnostic() {
    let first = region(
        "first",
        [0, 0, 0],
        [2, 1, 1],
        vec![BlockStateKey::simple("minecraft:air")],
        PackedIndices::new(2, vec![0], 2).unwrap(),
    );
    let second = region(
        "second",
        [0, 0, 0],
        [2, 1, 1],
        vec![BlockStateKey::simple("minecraft:air")],
        PackedIndices::new(2, vec![0], 2).unwrap(),
    );
    let scene = Scene::new(metadata(), vec![first, second], Vec::new()).unwrap();

    assert!(
        scene
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::RegionOverlap)
    );
}

#[test]
fn empty_scene_is_rejected() {
    let error = Scene::new(metadata(), Vec::new(), Vec::new()).unwrap_err();

    assert_eq!(error.code, SceneErrorCode::EmptyScene);
}

#[test]
fn rejects_region_size_and_volume_mismatch() {
    let region = region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        vec![BlockStateKey::simple("minecraft:air")],
        PackedIndices::new(2, vec![0], 1).unwrap(),
    );

    let error = Scene::new(metadata(), vec![region], Vec::new()).unwrap_err();
    assert_eq!(error.code, SceneErrorCode::RegionLayoutMismatch);
}

#[test]
fn rejects_empty_region_palette() {
    let region = region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        Vec::new(),
        PackedIndices::new(2, vec![0], 1).unwrap(),
    );

    let error = Scene::new(metadata(), vec![region], Vec::new()).unwrap_err();
    assert_eq!(error.code, SceneErrorCode::RegionLayoutMismatch);
}

#[test]
fn heap_bytes_include_block_entity_nbt() {
    let base = region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        vec![BlockStateKey::simple("minecraft:air")],
        PackedIndices::new(2, vec![0], 1).unwrap(),
    );
    let without = Scene::new(metadata(), vec![base.clone()], Vec::new())
        .unwrap()
        .heap_bytes();

    let mut with_entity = base;
    let mut data = NbtCompound::default();
    data.insert("Items".to_string(), NbtValue::ByteArray(vec![0u8; 4096]));
    with_entity.block_entities.push(BlockEntity {
        local_pos: [0, 0, 0],
        id: "minecraft:chest".to_string(),
        data,
    });
    let with = Scene::new(metadata(), vec![with_entity], Vec::new())
        .unwrap()
        .heap_bytes();

    assert!(
        with >= without + 4096,
        "entity NBT should be accounted: without={without}, with={with}"
    );
}

#[test]
fn block_entities_are_counted() {
    let mut region = region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        vec![BlockStateKey::simple("minecraft:air")],
        PackedIndices::new(2, vec![0], 1).unwrap(),
    );
    region.block_entities.push(BlockEntity {
        local_pos: [0, 0, 0],
        id: "minecraft:chest".to_string(),
        data: NbtCompound::default(),
    });

    let scene = Scene::new(metadata(), vec![region], Vec::new()).unwrap();
    assert_eq!(scene.statistics.block_entities, 1);
}
