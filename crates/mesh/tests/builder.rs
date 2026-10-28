mod support;

use litematic_mesh::{
    ColorBlockCatalog, MeshBuildOptions, MeshErrorCode, MeshLimits, build_mesh, count_mesh_quads,
    estimate_mesh_quads,
};
use support::{region as make_region, scene as make_scene};

fn build(scene: &litematic_scene::Scene, limits: &MeshLimits) -> litematic_mesh::Mesh {
    let catalog = ColorBlockCatalog::default();
    build_mesh(scene, &catalog, limits).unwrap()
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

#[test]
fn single_cube_emits_six_quads() {
    let region = make_region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1],
    );
    let mesh = build(&make_scene(vec![region]), &MeshLimits::default());

    assert_eq!(mesh.statistics.quads, 6);
    assert_eq!(mesh.statistics.pages, 1);
    assert_eq!(mesh.statistics.blocks_visited, 1);
    assert_eq!(mesh.statistics.blocks_rendered, 1);
    assert_eq!(mesh.statistics.opaque_quads, 6);
    assert_eq!(mesh.pages.len(), 1);
    assert_eq!(mesh.pages[0].origin, [0, 0, 0]);
    assert_eq!(mesh.pages[0].quads.len(), 6);

    for quad in &mesh.pages[0].quads {
        let p0 = quad.vertices[0].position;
        let p1 = quad.vertices[1].position;
        let p2 = quad.vertices[2].position;
        let u = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let v = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        assert!(
            dot(cross(u, v), quad.normal) > 0.0,
            "quad winding should match its normal"
        );
    }
}

#[test]
fn page_capacity_tracks_density() {
    let size = [32u32, 16, 16];
    let mut values = vec![0u32; (size[0] * size[1] * size[2]) as usize];
    let index = |x: u32, y: u32, z: u32| (x + size[0] * (z + size[2] * y)) as usize;
    for y in 0..16 {
        for z in 0..16 {
            for x in 0..16 {
                values[index(x, y, z)] = 1;
            }
        }
    }
    values[index(16, 0, 0)] = 1;

    let scene = make_scene(vec![make_region(
        "main",
        [0, 0, 0],
        size,
        &["minecraft:air", "minecraft:stone"],
        &values,
    )]);
    let mesh = build(&scene, &MeshLimits::default());

    assert_eq!(mesh.pages.len(), 2);
    for page in &mesh.pages {
        assert!(
            page.quads.capacity() <= page.quads.len() * 4 + 64,
            "{} quads reserved {} slots",
            page.quads.len(),
            page.quads.capacity()
        );
    }
}

#[test]
fn quad_count_matches_the_built_mesh() {
    let mut values = vec![0u32; 34 * 34];
    for (index, value) in values.iter_mut().enumerate() {
        if index % 3 == 0 {
            *value = 1;
        }
    }
    let scene = make_scene(vec![make_region(
        "main",
        [0, 0, 0],
        [34, 34, 1],
        &["minecraft:air", "minecraft:stone"],
        &values,
    )]);
    let catalog = ColorBlockCatalog::default();
    let snapshot = catalog.snapshot(
        scene
            .regions
            .iter()
            .flat_map(|region| region.palette.iter().cloned()),
    );

    for threads in [1, 4] {
        let count = count_mesh_quads(
            &scene,
            &snapshot,
            &MeshLimits::default(),
            threads,
            MeshBuildOptions::default(),
        )
        .unwrap();
        let mesh = build_mesh(&scene, &catalog, &MeshLimits::default()).unwrap();
        assert_eq!(count, Some(mesh.statistics.quads), "threads={threads}");

        // Sampling every page must reproduce the exact count.
        let estimate = estimate_mesh_quads(
            &scene,
            &snapshot,
            &MeshLimits::default(),
            usize::MAX,
            threads,
            MeshBuildOptions::default(),
        )
        .unwrap();
        assert_eq!(estimate, mesh.statistics.quads, "threads={threads}");
    }
}

#[test]
fn sampled_quad_estimate_is_deterministic() {
    let mut values = vec![0u32; 34 * 34];
    for (index, value) in values.iter_mut().enumerate() {
        if index % 3 == 0 {
            *value = 1;
        }
    }
    let scene = make_scene(vec![make_region(
        "main",
        [0, 0, 0],
        [34, 34, 1],
        &["minecraft:air", "minecraft:stone"],
        &values,
    )]);
    let catalog = ColorBlockCatalog::default();
    let snapshot = catalog.snapshot(
        scene
            .regions
            .iter()
            .flat_map(|region| region.palette.iter().cloned()),
    );

    let first = estimate_mesh_quads(
        &scene,
        &snapshot,
        &MeshLimits::default(),
        3,
        4,
        MeshBuildOptions::default(),
    )
    .unwrap();
    let second = estimate_mesh_quads(
        &scene,
        &snapshot,
        &MeshLimits::default(),
        3,
        4,
        MeshBuildOptions::default(),
    )
    .unwrap();

    assert!(first > 0);
    assert_eq!(first, second);
}

#[test]
fn quad_count_stops_at_the_quad_limit() {
    let scene = make_scene(vec![make_region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1],
    )]);
    let catalog = ColorBlockCatalog::default();
    let snapshot = catalog.snapshot(
        scene
            .regions
            .iter()
            .flat_map(|region| region.palette.iter().cloned()),
    );
    let limits = MeshLimits {
        max_quads: 5,
        ..Default::default()
    };

    assert_eq!(
        count_mesh_quads(&scene, &snapshot, &limits, 2, MeshBuildOptions::default()).unwrap(),
        None
    );
}

#[test]
fn adjacent_opaque_cubes_cull_shared_faces() {
    let region = make_region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1, 1],
    );
    let mesh = build(&make_scene(vec![region]), &MeshLimits::default());

    assert_eq!(mesh.statistics.quads, 10);
    assert_eq!(mesh.statistics.blocks_rendered, 2);
}

#[test]
fn stone_and_glass_keep_the_visible_interface() {
    let region = make_region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:air", "minecraft:stone", "minecraft:glass"],
        &[1, 2],
    );
    let mesh = build(&make_scene(vec![region]), &MeshLimits::default());

    assert_eq!(mesh.statistics.quads, 11);
    assert_eq!(mesh.statistics.opaque_quads, 6);
    assert_eq!(mesh.statistics.translucent_quads, 5);
}

#[test]
fn adjacent_glass_culls_shared_faces() {
    let region = make_region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:air", "minecraft:glass"],
        &[1, 1],
    );
    let mesh = build(&make_scene(vec![region]), &MeshLimits::default());

    assert_eq!(mesh.statistics.quads, 10);
    assert_eq!(mesh.statistics.translucent_quads, 10);
}

#[test]
fn page_boundary_blocks_go_to_separate_pages() {
    let region = make_region(
        "main",
        [15, 0, 0],
        [2, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1, 1],
    );
    let mesh = build(&make_scene(vec![region]), &MeshLimits::default());

    assert_eq!(mesh.statistics.quads, 10);
    assert_eq!(mesh.pages.len(), 2);
    let origins: Vec<_> = mesh.pages.iter().map(|page| page.origin).collect();
    assert_eq!(origins, vec![[0, 0, 0], [16, 0, 0]]);

    for page in &mesh.pages {
        for quad in &page.quads {
            for vertex in &quad.vertices {
                for axis in 0..3 {
                    assert!(vertex.position[axis] >= 0.0);
                    assert!(vertex.position[axis] <= 16.0);
                }
            }
        }
    }
}

#[test]
fn solid_blocks_have_no_interior_faces() {
    let values = vec![1; 27];
    let region = make_region(
        "main",
        [0, 0, 0],
        [3, 3, 3],
        &["minecraft:air", "minecraft:stone"],
        &values,
    );
    let mesh = build(&make_scene(vec![region]), &MeshLimits::default());

    assert_eq!(mesh.statistics.quads, 54);
    assert_eq!(mesh.statistics.blocks_visited, 27);
    assert_eq!(mesh.statistics.blocks_rendered, 27);
}

#[test]
fn equal_states_cull_across_region_boundaries() {
    let west = make_region("west", [0, 0, 0], [1, 1, 1], &["minecraft:glass"], &[0]);
    let east = make_region("east", [1, 0, 0], [1, 1, 1], &["minecraft:glass"], &[0]);
    let mesh = build(&make_scene(vec![west, east]), &MeshLimits::default());

    assert_eq!(mesh.statistics.quads, 10);
    assert_eq!(mesh.statistics.translucent_quads, 10);
}

#[test]
fn cross_page_glass_culls_across_region_boundaries() {
    let west = make_region("west", [15, 0, 0], [1, 1, 1], &["minecraft:glass"], &[0]);
    let east = make_region("east", [16, 0, 0], [1, 1, 1], &["minecraft:glass"], &[0]);
    let mesh = build(&make_scene(vec![west, east]), &MeshLimits::default());

    assert_eq!(mesh.statistics.quads, 10);
    assert_eq!(mesh.statistics.translucent_quads, 10);
    assert_eq!(mesh.pages.len(), 2);
}

#[test]
fn sparse_pages_are_not_counted_against_the_page_limit() {
    let mut values = vec![0u32; 64];
    values[63] = 1;
    let region = make_region(
        "sparse",
        [0, 0, 0],
        [64, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &values,
    );
    let limits = MeshLimits {
        max_pages: 1,
        max_quads: 100,
        max_vertices: 400,
    };
    let mesh = build(&make_scene(vec![region]), &limits);

    assert_eq!(mesh.statistics.pages, 1);
    assert_eq!(mesh.statistics.quads, 6);
    assert_eq!(mesh.statistics.blocks_visited, 16);
}

#[test]
fn negative_page_glass_culls_across_region_boundaries() {
    let west = make_region("west", [-1, 0, 0], [1, 1, 1], &["minecraft:glass"], &[0]);
    let east = make_region("east", [0, 0, 0], [1, 1, 1], &["minecraft:glass"], &[0]);
    let mesh = build(&make_scene(vec![west, east]), &MeshLimits::default());

    assert_eq!(mesh.statistics.quads, 10);
    assert_eq!(mesh.statistics.translucent_quads, 10);
    assert_eq!(mesh.pages.len(), 2);
}

#[test]
fn unknown_blocks_are_counted_and_visible() {
    let region = make_region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        &["minecraft:air", "modded:unknown"],
        &[1],
    );
    let mesh = build(&make_scene(vec![region]), &MeshLimits::default());

    assert_eq!(mesh.statistics.quads, 6);
    assert_eq!(mesh.statistics.unknown_blocks, 1);
    assert_eq!(mesh.pages[0].quads[0].color, [255, 0, 255, 255]);
}

#[test]
fn adjacent_partial_blocks_cull_shared_boundaries() {
    let region = make_region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:air", "minecraft:oak_slab"],
        &[1, 1],
    );
    let scene = make_scene(vec![region]);
    let mesh = build(&scene, &MeshLimits::default());

    assert_eq!(mesh.statistics.quads, 10);
    assert_eq!(mesh.statistics.blocks_rendered, 2);
}

#[test]
fn parallel_mesh_matches_serial_across_pages_and_regions() {
    use litematic_mesh::{ColorSnapshot, build_mesh_parallel};

    let west_values: Vec<u32> = (0..40 * 10 * 40).map(|index| (index % 3) as u32).collect();
    let west = make_region(
        "west",
        [-20, -5, -20],
        [40, 10, 40],
        &["minecraft:air", "minecraft:stone", "minecraft:oak_stairs"],
        &west_values,
    );
    let east_values: Vec<u32> = (0..20 * 10 * 20).map(|index| (index % 2) as u32).collect();
    let east = make_region(
        "east",
        [10, 0, 10],
        [20, 10, 20],
        &["minecraft:glass", "minecraft:stone"],
        &east_values,
    );
    let scene = make_scene(vec![west, east]);

    let catalog = ColorBlockCatalog::default();
    let states = scene
        .regions
        .iter()
        .flat_map(|region| region.palette.iter().cloned());
    let snapshot: ColorSnapshot = catalog.snapshot(states);

    let serial = build_mesh(&scene, &snapshot, &MeshLimits::default()).unwrap();
    for threads in [2, 4, 8] {
        let parallel =
            build_mesh_parallel(&scene, &snapshot, &MeshLimits::default(), threads).unwrap();
        assert_eq!(serial, parallel, "threads={threads}");
    }

    let serial_error = build_mesh(&scene, &snapshot, &MeshLimits::small()).unwrap_err();
    let parallel_error =
        build_mesh_parallel(&scene, &snapshot, &MeshLimits::small(), 4).unwrap_err();
    assert_eq!(serial_error.code, parallel_error.code);
}

#[test]
fn streaming_builder_matches_the_full_mesh() {
    use litematic_mesh::{ColorSnapshot, build_mesh_streaming};

    let values: Vec<u32> = (0..128 * 10 * 128)
        .map(|index| (index % 3) as u32)
        .collect();
    let region = make_region(
        "main",
        [0, 0, 0],
        [128, 10, 128],
        &["minecraft:air", "minecraft:stone", "minecraft:glass"],
        &values,
    );
    let scene = make_scene(vec![region]);
    let catalog = ColorBlockCatalog::default();
    let states = scene
        .regions
        .iter()
        .flat_map(|region| region.palette.iter().cloned());
    let snapshot: ColorSnapshot = catalog.snapshot(states);

    let full = build_mesh(&scene, &snapshot, &MeshLimits::default()).unwrap();
    let mut pages = Vec::new();
    let statistics = build_mesh_streaming(&scene, &snapshot, &MeshLimits::default(), 4, |chunk| {
        pages.extend_from_slice(chunk)
    })
    .unwrap();

    assert_eq!(statistics, full.statistics);
    assert_eq!(pages, full.pages);
}

#[test]
fn streaming_builder_enforces_limits() {
    use litematic_mesh::{ColorSnapshot, build_mesh_streaming};

    let region = make_region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1, 1],
    );
    let scene = make_scene(vec![region]);
    let catalog = ColorBlockCatalog::default();
    let states = scene
        .regions
        .iter()
        .flat_map(|region| region.palette.iter().cloned());
    let snapshot: ColorSnapshot = catalog.snapshot(states);
    let limits = MeshLimits {
        max_pages: 10,
        max_quads: 5,
        max_vertices: 100,
    };

    let error = build_mesh_streaming(&scene, &snapshot, &limits, 1, |_| {}).unwrap_err();
    assert_eq!(error.code, MeshErrorCode::MeshLimitExceeded);
}

#[test]
fn all_air_scene_produces_an_empty_mesh() {
    let region = make_region("main", [0, 0, 0], [2, 1, 1], &["minecraft:air"], &[0, 0]);
    let mesh = build(&make_scene(vec![region]), &MeshLimits::default());

    assert_eq!(mesh.statistics.quads, 0);
    assert_eq!(mesh.statistics.pages, 0);
    assert_eq!(mesh.statistics.blocks_visited, 0);
    assert_eq!(mesh.statistics.blocks_rendered, 0);
    assert!(mesh.pages.is_empty());
}

#[test]
fn mesh_limits_are_enforced() {
    let region = make_region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1],
    );
    let scene = make_scene(vec![region]);
    let limits = MeshLimits {
        max_pages: 10,
        max_quads: 5,
        max_vertices: 100,
    };

    let error = build_mesh(&scene, &ColorBlockCatalog::default(), &limits).unwrap_err();
    assert_eq!(error.code, MeshErrorCode::MeshLimitExceeded);
}

#[test]
fn ambient_occlusion_darkens_contact_vertices() {
    let region = make_region(
        "main",
        [0, 0, 0],
        [2, 2, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1, 0, 0, 1],
    );
    let scene = make_scene(vec![region]);
    let mesh = build(&scene, &MeshLimits::default());

    assert!(
        mesh.pages[0]
            .quads
            .iter()
            .any(|quad| (0..4).any(|index| quad.ambient_occlusion(index) < 0.99))
    );
    assert!(
        mesh.pages[0]
            .quads
            .iter()
            .all(|quad| (0..4).all(|index| quad.ambient_occlusion(index) > 0.0))
    );
}

#[test]
fn mesh_is_deterministic() {
    let region = make_region(
        "main",
        [0, 0, 0],
        [3, 2, 2],
        &["minecraft:air", "minecraft:stone", "minecraft:glass"],
        &[1, 1, 0, 0, 2, 1, 0, 0, 1, 0, 0, 0],
    );
    let scene = make_scene(vec![region]);
    let first = build(&scene, &MeshLimits::default());
    let second = build(&scene, &MeshLimits::default());

    assert_eq!(first, second);
}
