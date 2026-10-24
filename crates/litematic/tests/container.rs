mod support;

use litematic_core::{LitematicErrorCode, LitematicFile, LitematicLimits};
use litematic_nbt::DecodeLimits;
use support::*;

fn parse(bytes: &[u8]) -> Result<LitematicFile, litematic_core::LitematicError> {
    LitematicFile::parse_bytes(bytes, &DecodeLimits::default(), &LitematicLimits::default())
}

fn metadata(name: &str) -> Vec<u8> {
    let mut children = Vec::new();
    children.extend(string("Name", name));
    children.extend(string("Author", "tester"));
    children.extend(string("Description", "fixture"));
    children.extend(vec3i("EnclosingSize", 2, 3, 4));
    children.extend(long("TimeCreated", 123));
    children.extend(long("TimeModified", 123));
    children
}

fn minimal() -> Vec<u8> {
    let region = region("main", [0, 0, 0], [2, 3, 4], None, &[0]);
    litematic(7, Some(1), 3700, &metadata("demo"), &[region])
}

#[test]
fn parses_minimal_litematic() {
    let parsed = parse(&minimal()).unwrap();

    assert_eq!(parsed.version, 7);
    assert_eq!(parsed.sub_version, Some(1));
    assert_eq!(parsed.minecraft_data_version, 3700);
    assert_eq!(parsed.metadata.name.as_deref(), Some("demo"));
    assert_eq!(parsed.metadata.author.as_deref(), Some("tester"));
    assert_eq!(parsed.metadata.enclosing_size, Some([2, 3, 4]));
    assert_eq!(parsed.metadata.time_created, Some(123));
    assert_eq!(parsed.regions.len(), 1);

    let region = &parsed.regions[0];
    assert_eq!(region.name, "main");
    assert_eq!(region.position, [0, 0, 0]);
    assert_eq!(region.size, [2, 3, 4]);
    assert_eq!(region.origin, [0, 0, 0]);
    assert_eq!(region.size_abs, [2, 3, 4]);
    assert_eq!(region.volume, 24);
    assert_eq!(region.data_version, 3700);
    assert_eq!(region.block_states, vec![0]);
}

#[test]
fn normalizes_negative_sizes() {
    let region = region("negative", [10, 20, 30], [-2, -3, -4], None, &[0]);
    let parsed = parse(&litematic(7, None, 3700, &metadata("negative"), &[region])).unwrap();

    let region = &parsed.regions[0];
    assert_eq!(region.position, [10, 20, 30]);
    assert_eq!(region.size, [-2, -3, -4]);
    assert_eq!(region.origin, [9, 18, 27]);
    assert_eq!(region.size_abs, [2, 3, 4]);
    assert_eq!(region.volume, 24);
}

#[test]
fn sorts_regions_by_name() {
    let a = region("b", [0, 0, 0], [1, 1, 1], None, &[0]);
    let b = region("a", [0, 0, 0], [1, 1, 1], None, &[0]);
    let parsed = parse(&litematic(7, None, 3700, &metadata("order"), &[a, b])).unwrap();

    let names: Vec<_> = parsed
        .regions
        .iter()
        .map(|region| region.name.as_str())
        .collect();
    assert_eq!(names, vec!["a", "b"]);
}

#[test]
fn region_data_version_overrides_top_level() {
    let region = region("main", [0, 0, 0], [1, 1, 1], Some(1234), &[0]);
    let parsed = parse(&litematic(7, None, 3700, &metadata("version"), &[region])).unwrap();

    assert_eq!(parsed.regions[0].data_version, 1234);
}

#[test]
fn metadata_values_are_optional() {
    let region = region("main", [0, 0, 0], [1, 1, 1], None, &[0]);
    let parsed = parse(&litematic(
        7,
        None,
        3700,
        &string("Name", "only-name"),
        &[region],
    ))
    .unwrap();

    assert_eq!(parsed.metadata.name.as_deref(), Some("only-name"));
    assert_eq!(parsed.metadata.author, None);
    assert_eq!(parsed.metadata.description, None);
    assert_eq!(parsed.metadata.enclosing_size, None);
}

#[test]
fn rejects_unsupported_version() {
    let region = region("main", [0, 0, 0], [1, 1, 1], None, &[0]);
    let error = parse(&litematic(8, None, 3700, &metadata("bad"), &[region])).unwrap_err();

    assert_eq!(error.code, LitematicErrorCode::UnsupportedVersion);
}

#[test]
fn rejects_missing_metadata() {
    let region = region("main", [0, 0, 0], [1, 1, 1], None, &[0]);
    let mut children = Vec::new();
    children.extend(int("Version", 7));
    children.extend(int("MinecraftDataVersion", 3700));
    children.extend(compound("Regions", &region));
    let error = parse(&root(&children)).unwrap_err();

    assert_eq!(error.code, LitematicErrorCode::InvalidLitematic);
}

#[test]
fn rejects_zero_size() {
    let region = region("main", [0, 0, 0], [0, 1, 1], None, &[0]);
    let error = parse(&litematic(7, None, 3700, &metadata("zero"), &[region])).unwrap_err();

    assert_eq!(error.code, LitematicErrorCode::InvalidRegionBounds);
}

#[test]
fn rejects_region_volume_limit() {
    let region = region("big", [0, 0, 0], [5, 5, 5], None, &[0]);
    let bytes = litematic(7, None, 3700, &metadata("big"), &[region]);
    let error =
        LitematicFile::parse_bytes(&bytes, &DecodeLimits::default(), &LitematicLimits::small())
            .unwrap_err();

    assert_eq!(error.code, LitematicErrorCode::ResourceLimit);
}

#[test]
fn rejects_region_count_limit() {
    let regions: Vec<_> = (0..5)
        .map(|index| region(&format!("r{index}"), [0, 0, 0], [1, 1, 1], None, &[0]))
        .collect();
    let bytes = litematic(7, None, 3700, &metadata("many"), &regions);
    let error =
        LitematicFile::parse_bytes(&bytes, &DecodeLimits::default(), &LitematicLimits::small())
            .unwrap_err();

    assert_eq!(error.code, LitematicErrorCode::ResourceLimit);
}

#[test]
fn rejects_empty_block_states() {
    let region = region("main", [0, 0, 0], [1, 1, 1], None, &[]);
    let error = parse(&litematic(7, None, 3700, &metadata("empty"), &[region])).unwrap_err();

    assert_eq!(error.code, LitematicErrorCode::BlockStateDataLengthMismatch);
}

#[test]
fn rejects_empty_palette() {
    let mut children = Vec::new();
    children.extend(vec3i("Position", 0, 0, 0));
    children.extend(vec3i("Size", 1, 1, 1));
    children.extend(list_of_compounds("BlockStatePalette", &[]));
    children.extend(long_array("BlockStates", &[0]));
    let region = compound("main", &children);
    let error = parse(&litematic(7, None, 3700, &metadata("palette"), &[region])).unwrap_err();

    assert_eq!(error.code, LitematicErrorCode::InvalidPalette);
}

#[test]
fn rejects_bad_position_type() {
    let mut children = Vec::new();
    children.extend(string("Position", "not-a-vector"));
    children.extend(vec3i("Size", 1, 1, 1));
    children.extend(list_of_compounds(
        "BlockStatePalette",
        &[palette_entry("minecraft:air")],
    ));
    children.extend(long_array("BlockStates", &[0]));
    let region = compound("main", &children);
    let error = parse(&litematic(7, None, 3700, &metadata("position"), &[region])).unwrap_err();

    assert_eq!(error.code, LitematicErrorCode::InvalidRegionBounds);
}
