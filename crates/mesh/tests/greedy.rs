mod support;

use litematic_mesh::{
    ColorBlockCatalog, ColorSnapshot, MeshBuildOptions, MeshLimits, build_mesh,
    build_mesh_parallel_with_options, build_mesh_streaming_with_options, build_mesh_with_options,
};
use support::{region as make_region, scene as make_scene};

fn flat_scene() -> litematic_scene::Scene {
    let values = vec![1u32; 16 * 16];
    let region = make_region(
        "main",
        [0, 0, 0],
        [16, 1, 16],
        &["minecraft:air", "minecraft:stone"],
        &values,
    );
    make_scene(vec![region])
}

#[test]
fn greedy_merges_a_flat_stone_plane() {
    let scene = flat_scene();
    let catalog = ColorBlockCatalog::default();
    let limits = MeshLimits::default();

    let plain = build_mesh(&scene, &catalog, &limits).unwrap();
    assert_eq!(plain.statistics.quads, 576);

    let greedy =
        build_mesh_with_options(&scene, &catalog, &limits, MeshBuildOptions::GREEDY).unwrap();
    // The color catalog is untextured, so each page face can merge completely:
    // top, bottom, and four sides.
    assert_eq!(greedy.statistics.quads, 6);
    assert_eq!(greedy.statistics.vertices, 24);
    assert_eq!(greedy.statistics.opaque_quads, 6);
    assert_eq!(greedy.pages.len(), 1);
    assert_eq!(greedy.pages[0].quads.len(), 6);
}

#[test]
fn parallel_and_streaming_greedy_match_the_serial_builder() {
    let scene = flat_scene();
    let catalog = ColorBlockCatalog::default();
    let states = scene
        .regions
        .iter()
        .flat_map(|region| region.palette.iter().cloned());
    let snapshot: ColorSnapshot = catalog.snapshot(states);
    let limits = MeshLimits::default();
    let expected =
        build_mesh_with_options(&scene, &snapshot, &limits, MeshBuildOptions::GREEDY).unwrap();

    let parallel =
        build_mesh_parallel_with_options(&scene, &snapshot, &limits, 4, MeshBuildOptions::GREEDY)
            .unwrap();
    assert_eq!(parallel.statistics, expected.statistics);
    assert_eq!(parallel.pages, expected.pages);

    let mut streamed = Vec::new();
    let statistics = build_mesh_streaming_with_options(
        &scene,
        &snapshot,
        &limits,
        4,
        MeshBuildOptions::GREEDY,
        |pages| streamed.extend_from_slice(pages),
    )
    .unwrap();
    assert_eq!(statistics, expected.statistics);
    assert_eq!(streamed, expected.pages);
}
