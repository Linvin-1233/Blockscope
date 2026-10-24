mod support;

use litematic_core::{
    LitematicError, LitematicErrorCode, LitematicFile, LitematicLimits, RegionData,
};
use litematic_nbt::{DecodeLimits, NbtValue};
use litematic_scene::DiagnosticCode;
use support::*;

fn parse_region(
    size: [i32; 3],
    palette: &[Vec<u8>],
    block_states: &[i64],
) -> Result<RegionData, LitematicError> {
    let region = region_with_palette("main", [0, 0, 0], size, None, palette, block_states);
    let bytes = litematic(7, None, 3700, &string("Name", "data"), &[region]);
    let file = LitematicFile::parse_bytes(
        &bytes,
        &DecodeLimits::default(),
        &LitematicLimits::default(),
    )?;
    file.regions.into_iter().next().unwrap().into_data()
}

fn parse_region_with_lists(
    format_version: i32,
    size: [i32; 3],
    lists: &RegionLists,
) -> Result<RegionData, LitematicError> {
    parse_region_with_lists_and_limits(format_version, size, lists, &LitematicLimits::default())
}

fn parse_region_with_lists_and_limits(
    format_version: i32,
    size: [i32; 3],
    lists: &RegionLists,
    limits: &LitematicLimits,
) -> Result<RegionData, LitematicError> {
    let region = region_with_palette_and_lists(
        "main",
        [0, 0, 0],
        size,
        None,
        &[palette_entry("minecraft:air")],
        &[0],
        lists,
    );
    let bytes = litematic(
        format_version,
        None,
        3700,
        &string("Name", "data"),
        &[region],
    );
    let file = LitematicFile::parse_bytes(&bytes, &DecodeLimits::default(), limits)?;
    file.regions.into_iter().next().unwrap().into_data()
}

fn block_entity(id: &str, position: [i32; 3]) -> Vec<u8> {
    let mut children = Vec::new();
    children.extend(string("id", id));
    children.extend(int("x", position[0]));
    children.extend(int("y", position[1]));
    children.extend(int("z", position[2]));
    children.extend(string("CustomName", "test"));
    compound_payload(&children)
}

fn block_entity_without_id(position: [i32; 3]) -> Vec<u8> {
    let mut children = Vec::new();
    children.extend(int("x", position[0]));
    children.extend(int("y", position[1]));
    children.extend(int("z", position[2]));
    compound_payload(&children)
}

fn v1_block_entity(id: &str, position: [i32; 3]) -> Vec<u8> {
    let mut tile_children = Vec::new();
    tile_children.extend(string("id", id));
    let tile_nbt = compound_payload(&tile_children);

    let mut wrapper = Vec::new();
    wrapper.extend(named(10, "TileNBT", &tile_nbt));
    wrapper.extend(int("x", position[0]));
    wrapper.extend(int("y", position[1]));
    wrapper.extend(int("z", position[2]));
    compound_payload(&wrapper)
}

#[test]
fn decodes_palette_and_packed_indices() {
    let palette = [
        palette_entry("minecraft:air"),
        palette_entry("minecraft:stone"),
    ];
    let data = parse_region([2, 1, 1], &palette, &[4]).unwrap();

    assert_eq!(data.palette.len(), 2);
    assert_eq!(data.palette[0].canonical(), "minecraft:air");
    assert_eq!(data.palette[1].canonical(), "minecraft:stone");
    assert_eq!(data.states.bits(), 2);
    assert_eq!(data.states.get(0), 0);
    assert_eq!(data.states.get(1), 1);
}

#[test]
fn normalizes_namespace_and_sorts_properties() {
    let palette = [palette_entry_with_properties(
        "oak_stairs",
        &[("half", "bottom"), ("facing", "east")],
    )];
    let data = parse_region([1, 1, 1], &palette, &[0]).unwrap();

    assert_eq!(data.palette[0].name, "minecraft:oak_stairs");
    assert_eq!(
        data.palette[0].canonical(),
        "minecraft:oak_stairs[facing=east,half=bottom]"
    );
}

#[test]
fn all_air_region_decodes() {
    let data = parse_region([2, 3, 4], &[palette_entry("minecraft:air")], &[0]).unwrap();

    assert_eq!(data.states.volume(), 24);
    assert!(data.states.iter().all(|value| value == 0));
}

#[test]
fn rejects_invalid_palette_index() {
    let palette = [
        palette_entry("minecraft:air"),
        palette_entry("minecraft:stone"),
    ];
    let error = parse_region([2, 1, 1], &palette, &[12]).unwrap_err();

    assert_eq!(error.code, LitematicErrorCode::InvalidPaletteIndex);
}

#[test]
fn rejects_block_state_length_mismatch() {
    let palette = [
        palette_entry("minecraft:air"),
        palette_entry("minecraft:stone"),
    ];
    let error = parse_region([2, 1, 1], &palette, &[0, 0]).unwrap_err();

    assert_eq!(error.code, LitematicErrorCode::BlockStateDataLengthMismatch);
}

#[test]
fn rejects_missing_block_name() {
    let mut children = Vec::new();
    children.extend(compound("Properties", &string("facing", "east")));
    let entry = compound_payload(&children);

    let error = parse_region([1, 1, 1], &[entry], &[0]).unwrap_err();
    assert_eq!(error.code, LitematicErrorCode::UnsupportedPaletteFormat);
}

#[test]
fn rejects_bad_property_type() {
    let mut property_children = Vec::new();
    property_children.extend(int("facing", 1));
    let mut children = Vec::new();
    children.extend(string("Name", "minecraft:oak_stairs"));
    children.extend(compound("Properties", &property_children));
    let entry = compound_payload(&children);

    let error = parse_region([1, 1, 1], &[entry], &[0]).unwrap_err();
    assert_eq!(error.code, LitematicErrorCode::InvalidPalette);
}

#[test]
fn rejects_invalid_resource_location() {
    let error = parse_region([1, 1, 1], &[palette_entry("bad name")], &[0]).unwrap_err();
    assert_eq!(error.code, LitematicErrorCode::UnsupportedPaletteFormat);

    let error = parse_region([1, 1, 1], &[palette_entry("a:b:c")], &[0]).unwrap_err();
    assert_eq!(error.code, LitematicErrorCode::UnsupportedPaletteFormat);
}

#[test]
fn parses_modern_block_entity() {
    let mut lists = RegionLists::default();
    lists
        .tile_entities
        .push(block_entity("minecraft:chest", [1, 0, 1]));

    let data = parse_region_with_lists(7, [2, 2, 2], &lists).unwrap();

    assert_eq!(data.block_entities.len(), 1);
    let entity = &data.block_entities[0];
    assert_eq!(entity.local_pos, [1, 0, 1]);
    assert_eq!(entity.id, "minecraft:chest");
    assert_eq!(
        entity.data.get("CustomName"),
        Some(&NbtValue::String("test".into()))
    );
    assert!(data.diagnostics.is_empty());
}

#[test]
fn parses_v1_block_entity() {
    let mut lists = RegionLists::default();
    lists
        .tile_entities
        .push(v1_block_entity("minecraft:chest", [0, 1, 0]));

    let data = parse_region_with_lists(1, [2, 2, 2], &lists).unwrap();

    assert_eq!(data.block_entities.len(), 1);
    assert_eq!(data.block_entities[0].local_pos, [0, 1, 0]);
    assert_eq!(data.block_entities[0].id, "minecraft:chest");
}

#[test]
fn parses_block_entity_pos_list_fallback() {
    let mut children = Vec::new();
    children.extend(string("id", "minecraft:chest"));
    children.extend(int_list("Pos", &[1, 0, 1]));
    let entity = compound_payload(&children);

    let mut lists = RegionLists::default();
    lists.tile_entities.push(entity);

    let data = parse_region_with_lists(7, [2, 2, 2], &lists).unwrap();
    assert_eq!(data.block_entities[0].local_pos, [1, 0, 1]);
}

#[test]
fn drops_out_of_bounds_block_entity() {
    let mut lists = RegionLists::default();
    lists
        .tile_entities
        .push(block_entity("minecraft:chest", [5, 0, 0]));

    let data = parse_region_with_lists(7, [2, 2, 2], &lists).unwrap();

    assert!(data.block_entities.is_empty());
    assert!(
        data.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::OutOfBoundsBlockEntity)
    );
}

#[test]
fn duplicate_block_entity_last_wins() {
    let mut lists = RegionLists::default();
    lists
        .tile_entities
        .push(block_entity("minecraft:chest", [0, 0, 0]));
    lists
        .tile_entities
        .push(block_entity("minecraft:barrel", [0, 0, 0]));

    let data = parse_region_with_lists(7, [2, 2, 2], &lists).unwrap();

    assert_eq!(data.block_entities.len(), 1);
    assert_eq!(data.block_entities[0].id, "minecraft:barrel");
    assert!(
        data.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::DuplicateBlockEntity)
    );
}

#[test]
fn missing_block_entity_id_is_diagnostic() {
    let mut lists = RegionLists::default();
    lists.tile_entities.push(block_entity_without_id([0, 0, 0]));

    let data = parse_region_with_lists(7, [2, 2, 2], &lists).unwrap();

    assert_eq!(data.block_entities.len(), 1);
    assert_eq!(data.block_entities[0].id, "unknown");
    assert!(
        data.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::MissingBlockEntityId)
    );
}

#[test]
fn rejects_malformed_block_entity() {
    let mut lists = RegionLists::default();
    lists
        .tile_entities
        .push(compound_payload(&string("id", "minecraft:chest")));

    let error = parse_region_with_lists(7, [2, 2, 2], &lists).unwrap_err();
    assert_eq!(error.code, LitematicErrorCode::InvalidBlockEntity);
}

#[test]
fn rejects_block_entity_count_limit() {
    let mut lists = RegionLists::default();
    for _ in 0..9 {
        lists
            .tile_entities
            .push(block_entity("minecraft:chest", [0, 0, 0]));
    }

    let error = parse_region_with_lists_and_limits(7, [2, 2, 2], &lists, &LitematicLimits::small())
        .unwrap_err();

    assert_eq!(error.code, LitematicErrorCode::BlockEntityLimitExceeded);
}

#[test]
fn counts_entities_and_pending_ticks() {
    let mut lists = RegionLists::default();
    lists.entities.push(compound_payload(&[]));
    lists.pending_block_ticks.push(compound_payload(&[]));
    lists.pending_fluid_ticks.push(compound_payload(&[]));

    let data = parse_region_with_lists(7, [1, 1, 1], &lists).unwrap();

    assert_eq!(data.entity_count, 1);
    assert_eq!(data.pending_block_tick_count, 1);
    assert_eq!(data.pending_fluid_tick_count, 1);
    assert!(
        data.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::EntitiesIgnored)
    );
    assert!(
        data.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::PendingTicksIgnored)
    );
}
