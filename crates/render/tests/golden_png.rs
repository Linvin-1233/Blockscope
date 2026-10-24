mod support;

use litematic_mesh::{ColorBlockCatalog, MeshLimits, build_mesh};
use litematic_render::{RenderOptions, render_mesh, write_png};
use litematic_scene::Scene;
use std::fs;
use std::path::PathBuf;
use support::{region as make_region, scene as make_scene};

fn options() -> RenderOptions {
    RenderOptions {
        width: 64,
        height: 64,
        supersample: 2,
        ..Default::default()
    }
}

fn render_png(scene: &Scene) -> Vec<u8> {
    let mesh = build_mesh(scene, &ColorBlockCatalog::default(), &MeshLimits::default()).unwrap();
    let output = render_mesh(scene, &mesh, &options()).unwrap();
    let mut bytes = Vec::new();
    write_png(&output, &mut bytes).unwrap();
    bytes
}

fn cases() -> Vec<(&'static str, Scene)> {
    vec![
        (
            "stone-cube",
            make_scene(vec![make_region(
                "main",
                [0, 0, 0],
                [1, 1, 1],
                &["minecraft:air", "minecraft:stone"],
                &[1],
            )]),
        ),
        (
            "partial-shapes",
            make_scene(vec![make_region(
                "main",
                [0, 0, 0],
                [3, 1, 1],
                &[
                    "minecraft:air",
                    "minecraft:oak_slab",
                    "minecraft:oak_stairs",
                    "minecraft:oak_fence",
                ],
                &[1, 2, 3],
            )]),
        ),
        (
            "water",
            make_scene(vec![make_region(
                "main",
                [0, 0, 0],
                [1, 1, 1],
                &["minecraft:air", "minecraft:water"],
                &[1],
            )]),
        ),
        (
            "negative-coords",
            make_scene(vec![make_region(
                "main",
                [-17, 0, 0],
                [1, 1, 1],
                &["minecraft:air", "minecraft:stone"],
                &[1],
            )]),
        ),
        (
            "ambient-occlusion",
            make_scene(vec![make_region(
                "main",
                [0, 0, 0],
                [2, 2, 1],
                &["minecraft:air", "minecraft:stone"],
                &[1, 0, 0, 1],
            )]),
        ),
    ]
}

#[test]
fn golden_pngs_match() {
    let golden_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures")
        .join("golden")
        .join("render");
    fs::create_dir_all(&golden_dir).unwrap();
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();

    for (name, scene) in cases() {
        let bytes = render_png(&scene);
        let path = golden_dir.join(format!("{name}.png"));
        if update {
            fs::write(&path, &bytes).unwrap();
            continue;
        }
        let expected =
            fs::read(&path).unwrap_or_else(|_| panic!("missing golden PNG {}", path.display()));
        assert_eq!(bytes, expected, "golden PNG mismatch for {name}");
    }
}
