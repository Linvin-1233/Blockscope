mod support;

use litematic_core::{DecodeLimits, LitematicFile, LitematicLimits};
use litematic_scene::{Manifest, sha256};
use std::fs;
use std::path::PathBuf;
use support::*;

fn cases() -> Vec<(&'static str, Vec<u8>)> {
    let mut cases = Vec::new();

    cases.push(("minimal-v7", basic_fixture()));

    let west = region_with_palette(
        "west",
        [10, 10, 10],
        [-2, -3, -4],
        None,
        &[
            palette_entry("minecraft:air"),
            palette_entry("minecraft:stone"),
        ],
        &[4],
    );
    let east = region("east", [0, 0, 0], [1, 1, 1], None, &[0]);
    cases.push((
        "negative-multiregion-v6",
        litematic(
            6,
            None,
            3700,
            &metadata("negative", [11, 11, 11], 2, 25, 1),
            &[west, east],
        ),
    ));

    let mut lists = RegionLists::default();
    lists
        .tile_entities
        .push(v1_block_entity("minecraft:chest", [1, 0, 1]));
    let v1_region = region_with_palette_and_lists(
        "main",
        [0, 0, 0],
        [2, 2, 2],
        None,
        &[palette_entry("minecraft:air")],
        &[0],
        &lists,
    );
    cases.push((
        "v1-block-entity",
        litematic(
            1,
            None,
            3700,
            &metadata("v1-be", [2, 2, 2], 1, 8, 0),
            &[v1_region],
        ),
    ));

    let all_air = region_with_palette(
        "main",
        [0, 0, 0],
        [2, 3, 4],
        None,
        &[palette_entry("minecraft:air")],
        &[0],
    );
    cases.push((
        "all-air-v7",
        litematic(
            7,
            None,
            3700,
            &metadata("all-air", [2, 3, 4], 1, 24, 0),
            &[all_air],
        ),
    ));

    let palette: Vec<_> = (0..33)
        .map(|index| palette_entry(&format!("minecraft:block_{index}")))
        .collect();
    let bit_width = region_with_palette("main", [0, 0, 0], [1, 1, 1], None, &palette, &[32]);
    cases.push((
        "bit-width-33",
        litematic(
            7,
            None,
            3700,
            &metadata("bit-width-33", [1, 1, 1], 1, 1, 1),
            &[bit_width],
        ),
    ));

    cases
}

fn build_manifest(bytes: &[u8]) -> Manifest {
    let file =
        LitematicFile::parse_bytes(bytes, &DecodeLimits::default(), &LitematicLimits::default())
            .unwrap();
    let scene = file.into_scene().unwrap();
    Manifest::from_scene(&scene, sha256(bytes), env!("CARGO_PKG_VERSION"))
}

#[test]
fn golden_manifests_match() {
    let golden_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures")
        .join("golden");
    fs::create_dir_all(&golden_dir).unwrap();

    let update = std::env::var_os("UPDATE_GOLDEN").is_some();

    for (name, bytes) in cases() {
        let manifest = build_manifest(&bytes);
        let json = manifest.to_json_pretty().unwrap();
        let path = golden_dir.join(format!("{name}.json"));

        if update {
            fs::write(&path, &json).unwrap();
            continue;
        }

        let expected = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("missing golden file {}", path.display()));
        assert_eq!(json, expected, "golden mismatch for {name}");
    }
}
