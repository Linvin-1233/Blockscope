mod support;

use litematic_core::{LitematicFile, LitematicLimits};
use litematic_nbt::DecodeLimits;
use litematic_scene::DiagnosticCode;
use support::*;

fn metadata(enclosing_size: [i32; 3], total_blocks: i64) -> Vec<u8> {
    let mut children = Vec::new();
    children.extend(string("Name", "scene"));
    children.extend(vec3i(
        "EnclosingSize",
        enclosing_size[0],
        enclosing_size[1],
        enclosing_size[2],
    ));
    children.extend(int("RegionCount", 2));
    children.extend(long("TotalVolume", 3));
    children.extend(long("TotalBlocks", total_blocks));
    children
}

fn parse(bytes: &[u8]) -> litematic_scene::Scene {
    LitematicFile::parse_bytes(bytes, &DecodeLimits::default(), &LitematicLimits::default())
        .unwrap()
        .into_scene()
        .unwrap()
}

fn two_regions() -> Vec<Vec<u8>> {
    let first = region_with_palette(
        "a",
        [0, 0, 0],
        [2, 1, 1],
        None,
        &[
            palette_entry("minecraft:air"),
            palette_entry("minecraft:stone"),
        ],
        &[4],
    );
    let second = region("b", [10, 0, 0], [1, 1, 1], None, &[0]);
    vec![first, second]
}

#[test]
fn builds_scene_statistics_and_bounds() {
    let bytes = litematic(7, None, 3700, &metadata([11, 1, 1], 1), &two_regions());
    let scene = parse(&bytes);

    assert_eq!(scene.metadata.name.as_deref(), Some("scene"));
    assert_eq!(scene.metadata.litematic_version, 7);
    assert_eq!(scene.bounds.min, [0, 0, 0]);
    assert_eq!(scene.bounds.max_exclusive, [11, 1, 1]);
    assert_eq!(scene.bounds.size(), [11, 1, 1]);
    assert_eq!(scene.regions.len(), 2);
    assert_eq!(scene.statistics.volume, 3);
    assert_eq!(scene.statistics.air, 2);
    assert_eq!(scene.statistics.non_air, 1);
    assert_eq!(
        scene
            .statistics
            .blocks
            .get(&litematic_scene::BlockStateKey::simple("minecraft:stone")),
        Some(&1)
    );
    assert!(
        !scene
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::MetadataMismatch)
    );
}

#[test]
fn reports_metadata_mismatch() {
    let bytes = litematic(7, None, 3700, &metadata([11, 1, 1], 999), &two_regions());
    let scene = parse(&bytes);

    assert!(
        scene
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::MetadataMismatch)
    );
}

#[test]
fn reports_enclosing_size_mismatch() {
    let bytes = litematic(7, None, 3700, &metadata([99, 1, 1], 1), &two_regions());
    let scene = parse(&bytes);

    assert!(
        scene
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::MetadataMismatch)
    );
}

#[test]
fn reports_region_overlap() {
    let first = region("a", [0, 0, 0], [2, 1, 1], None, &[0]);
    let second = region("b", [0, 0, 0], [2, 1, 1], None, &[0]);
    let bytes = litematic(7, None, 3700, &metadata([2, 1, 1], 0), &[first, second]);
    let scene = parse(&bytes);

    assert!(
        scene
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::RegionOverlap)
    );
}

#[test]
fn reports_old_data_version() {
    let bytes = litematic(7, None, 1139, &metadata([11, 1, 1], 1), &two_regions());
    let scene = parse(&bytes);

    assert!(
        scene
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::UnsupportedDataVersion)
    );
}
