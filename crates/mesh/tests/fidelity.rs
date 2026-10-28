mod support;

use litematic_mesh::{
    BlockCatalog, ColorBlockCatalog, Mesh, RenderLayer, Rgba, build_mesh, build_mesh_parallel,
    build_mesh_streaming, count_mesh_quads, redstone_color,
};
use litematic_scene::{BlockStateKey, Scene};
use std::collections::BTreeMap;
use support::{region_states, scene as make_scene};

fn key(name: &str, properties: &[(&str, &str)]) -> BlockStateKey {
    let mut key = BlockStateKey::simple(name);
    key.properties = properties
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect::<BTreeMap<_, _>>();
    key
}

fn water_row(levels: &[u8]) -> Scene {
    let mut palette = vec![BlockStateKey::simple("minecraft:air")];
    palette.extend(
        levels
            .iter()
            .map(|level| key("minecraft:water", &[("level", &level.to_string())])),
    );
    let values: Vec<u32> = (1..=levels.len() as u32).collect();
    make_scene(vec![region_states(
        "main",
        [0, 0, 0],
        [levels.len() as u32, 1, 1],
        &palette,
        &values,
    )])
}

fn quads(mesh: &Mesh) -> Vec<&litematic_mesh::Quad> {
    mesh.pages.iter().flat_map(|page| &page.quads).collect()
}

fn top_quad_at_x(mesh: &Mesh, block_x: f32) -> &litematic_mesh::Quad {
    quads(mesh)
        .into_iter()
        .filter(|quad| quad.normal == [0.0, 1.0, 0.0])
        .find(|quad| {
            let center = quad.vertices.iter().map(|v| v.position[0]).sum::<f32>() / 4.0;
            (center - (block_x + 0.5)).abs() < 0.01
        })
        .unwrap_or_else(|| panic!("missing top quad for water block x={block_x}"))
}

#[test]
fn redstone_wire_without_textures_tracks_power() {
    let catalog = ColorBlockCatalog::default();
    let low = key(
        "minecraft:redstone_wire",
        &[
            ("east", "side"),
            ("north", "none"),
            ("power", "0"),
            ("south", "none"),
            ("west", "none"),
        ],
    );
    let high = key(
        "minecraft:redstone_wire",
        &[
            ("east", "side"),
            ("north", "none"),
            ("power", "15"),
            ("south", "none"),
            ("west", "none"),
        ],
    );

    let low = catalog.resolve(&low);
    let high = catalog.resolve(&high);

    assert!(low.known && high.known);
    assert_eq!(low.layer, RenderLayer::Cutout);
    assert!(!low.model.is_full_cube);
    assert_eq!(low.model.quads.len(), 2);
    assert_eq!(low.color, redstone_color(0));
    assert_eq!(high.color, redstone_color(15));
    assert_ne!(low.color, Rgba::MAGENTA);
    assert!(high.color.r > low.color.r);
}

#[test]
fn water_surface_slopes_toward_a_lower_neighbor() {
    let mesh = build_mesh(
        &water_row(&[0, 0, 2, 5, 7]),
        &ColorBlockCatalog::default(),
        &Default::default(),
    )
    .unwrap();

    let quad = top_quad_at_x(&mesh, 1.0);
    let min_x = quad
        .vertices
        .iter()
        .map(|vertex| vertex.position[0])
        .fold(f32::INFINITY, f32::min);
    let max_x = quad
        .vertices
        .iter()
        .map(|vertex| vertex.position[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let edge_height = |x: f32| {
        let values: Vec<f32> = quad
            .vertices
            .iter()
            .filter(|vertex| (vertex.position[0] - x).abs() < 1e-4)
            .map(|vertex| vertex.position[1])
            .collect();
        assert_eq!(values.len(), 2);
        values.iter().sum::<f32>() / 2.0
    };

    let west = edge_height(min_x);
    let east = edge_height(max_x);
    assert!(
        west - east > 0.02,
        "surface should fall toward the lower neighbour: west={west} east={east}"
    );
}

#[test]
fn water_hides_the_shared_side_between_different_levels() {
    let mesh = build_mesh(
        &water_row(&[0, 3]),
        &ColorBlockCatalog::default(),
        &Default::default(),
    )
    .unwrap();

    for quad in quads(&mesh) {
        let all_at_shared_plane = quad
            .vertices
            .iter()
            .all(|vertex| (vertex.position[0] - 1.0).abs() < 1e-4);
        let side_facing_x = quad.normal[0].abs() > 0.5;
        assert!(
            !(all_at_shared_plane && side_facing_x),
            "shared water side at x=1 should be culled: {quad:?}"
        );
    }
}

#[test]
fn waterlogged_partial_block_keeps_its_model_and_adds_water() {
    let palette = vec![
        BlockStateKey::simple("minecraft:air"),
        key(
            "minecraft:oak_slab",
            &[("type", "bottom"), ("waterlogged", "true")],
        ),
    ];
    let mesh = build_mesh(
        &make_scene(vec![region_states(
            "main",
            [0, 0, 0],
            [1, 1, 1],
            &palette,
            &[1],
        )]),
        &ColorBlockCatalog::default(),
        &Default::default(),
    )
    .unwrap();

    let mut slab_top = false;
    let mut water_top = None;
    for quad in quads(&mesh) {
        let is_up = quad.normal == [0.0, 1.0, 0.0];
        let min_y = quad
            .vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::INFINITY, f32::min);
        let max_y = quad
            .vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        if is_up && quad.layer == RenderLayer::Opaque && (max_y - 0.5).abs() < 1e-4 {
            slab_top = true;
        }
        if is_up && quad.layer == RenderLayer::Translucent {
            water_top = Some((min_y, max_y));
        }
    }

    assert!(slab_top, "waterlogged slab must keep its own top face");
    let (min_y, max_y) = water_top.expect("waterlogged slab must emit a water surface");
    assert!(
        min_y > 0.6 && max_y < 1.0,
        "water layer height: {min_y}..{max_y}"
    );
}

#[test]
fn fluid_mesh_matches_across_builder_paths() {
    let levels: Vec<u8> = (0..33u8).map(|index| index % 8).collect();
    let scene = water_row(&levels);
    let catalog = ColorBlockCatalog::default();
    let snapshot = catalog.snapshot(
        scene
            .regions
            .iter()
            .flat_map(|region| region.palette.iter().cloned()),
    );

    let serial = build_mesh(&scene, &snapshot, &Default::default()).unwrap();
    let parallel = build_mesh_parallel(&scene, &snapshot, &Default::default(), 4).unwrap();
    assert_eq!(serial, parallel);

    let mut streamed_pages = Vec::new();
    let streamed = build_mesh_streaming(&scene, &snapshot, &Default::default(), 4, |pages| {
        streamed_pages.extend_from_slice(pages)
    })
    .unwrap();
    assert_eq!(serial.statistics, streamed);
    assert_eq!(serial.pages, streamed_pages);

    let counted = count_mesh_quads(
        &scene,
        &snapshot,
        &Default::default(),
        4,
        Default::default(),
    )
    .unwrap();
    assert_eq!(counted, Some(serial.statistics.quads));
}

#[test]
fn waterlogged_mesh_matches_across_builder_paths() {
    let palette = vec![
        BlockStateKey::simple("minecraft:air"),
        key(
            "minecraft:oak_slab",
            &[("type", "bottom"), ("waterlogged", "true")],
        ),
        key("minecraft:water", &[("level", "0")]),
        key(
            "minecraft:oak_stairs",
            &[
                ("facing", "east"),
                ("half", "bottom"),
                ("shape", "straight"),
                ("waterlogged", "true"),
            ],
        ),
    ];
    let scene = make_scene(vec![region_states(
        "main",
        [0, 0, 0],
        [3, 1, 1],
        &palette,
        &[1, 2, 3],
    )]);
    let catalog = ColorBlockCatalog::default();
    let snapshot = catalog.snapshot(
        scene
            .regions
            .iter()
            .flat_map(|region| region.palette.iter().cloned()),
    );

    let serial = build_mesh(&scene, &snapshot, &Default::default()).unwrap();
    let parallel = build_mesh_parallel(&scene, &snapshot, &Default::default(), 4).unwrap();
    assert_eq!(serial, parallel);

    let mut streamed_pages = Vec::new();
    let streamed = build_mesh_streaming(&scene, &snapshot, &Default::default(), 4, |pages| {
        streamed_pages.extend_from_slice(pages)
    })
    .unwrap();
    assert_eq!(serial.statistics, streamed);
    assert_eq!(serial.pages, streamed_pages);

    let counted = count_mesh_quads(
        &scene,
        &snapshot,
        &Default::default(),
        4,
        Default::default(),
    )
    .unwrap();
    assert_eq!(counted, Some(serial.statistics.quads));
}

#[test]
fn water_under_water_fills_the_column_and_hides_internal_faces() {
    let palette = vec![
        BlockStateKey::simple("minecraft:air"),
        key("minecraft:water", &[("level", "8")]),
        key("minecraft:water", &[("level", "0")]),
    ];
    let mesh = build_mesh(
        &make_scene(vec![region_states(
            "main",
            [0, 0, 0],
            [1, 2, 1],
            &palette,
            &[1, 2],
        )]),
        &ColorBlockCatalog::default(),
        &Default::default(),
    )
    .unwrap();

    let mut full_side = false;
    for quad in quads(&mesh) {
        let is_side = quad.normal[1].abs() < 0.5;
        let min_y = quad
            .vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::INFINITY, f32::min);
        let max_y = quad
            .vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        if is_side && min_y.abs() < 1e-4 && (max_y - 1.0).abs() < 1e-4 {
            full_side = true;
        }
        let horizontal_at_seam = quad.normal[1].abs() > 0.5
            && quad
                .vertices
                .iter()
                .all(|vertex| (vertex.position[1] - 1.0).abs() < 1e-4);
        assert!(
            !horizontal_at_seam,
            "water column should not emit an internal horizontal face: {quad:?}"
        );
    }
    assert!(
        full_side,
        "lower water block should render a full-height side"
    );
}
