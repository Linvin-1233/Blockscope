#![allow(dead_code)]

use litematic_scene::{BlockStateKey, PackedIndices, Region, Scene, SceneMetadata};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;

pub fn metadata() -> SceneMetadata {
    SceneMetadata {
        litematic_version: 7,
        minecraft_data_version: 3700,
        ..Default::default()
    }
}

pub fn region(
    name: &str,
    origin: [i64; 3],
    size: [u32; 3],
    palette: &[&str],
    values: &[u32],
) -> Region {
    let volume = u64::from(size[0]) * u64::from(size[1]) * u64::from(size[2]);
    assert_eq!(values.len() as u64, volume);

    let palette: Vec<BlockStateKey> = palette
        .iter()
        .map(|name| BlockStateKey::simple(*name))
        .collect();
    let bits = PackedIndices::bits_for_palette(palette.len());
    let states = PackedIndices::from_values(bits, values).unwrap();

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

pub fn scene(regions: Vec<Region>) -> Scene {
    Scene::new(metadata(), regions, Vec::new()).unwrap()
}

/// Write a tiny pack where `minecraft:stone` resolves to a textured cube.
pub fn pack(root: &Path) {
    let assets = root.join("assets/minecraft");
    fs::create_dir_all(assets.join("blockstates")).unwrap();
    fs::create_dir_all(assets.join("models/block")).unwrap();
    fs::create_dir_all(assets.join("textures/block")).unwrap();

    fs::write(
        assets.join("blockstates/stone.json"),
        r##"{ "variants": { "": { "model": "block/test_cube" } } }"##,
    )
    .unwrap();
    fs::write(
        assets.join("models/block/cube.json"),
        r##"{
            "elements": [
                {
                    "from": [0, 0, 0],
                    "to": [16, 16, 16],
                    "faces": {
                        "down":  { "texture": "#all", "cullface": "down" },
                        "up":    { "texture": "#all", "cullface": "up" },
                        "north": { "texture": "#all", "cullface": "north" },
                        "south": { "texture": "#all", "cullface": "south" },
                        "west":  { "texture": "#all", "cullface": "west" },
                        "east":  { "texture": "#all", "cullface": "east" }
                    }
                }
            ]
        }"##,
    )
    .unwrap();
    fs::write(
        assets.join("models/block/cube_all.json"),
        r##"{
            "parent": "block/cube",
            "textures": {
                "down": "#all", "up": "#all", "north": "#all",
                "south": "#all", "west": "#all", "east": "#all"
            }
        }"##,
    )
    .unwrap();
    fs::write(
        assets.join("models/block/test_cube.json"),
        r##"{
            "parent": "block/cube_all",
            "textures": { "all": "block/test_texture" }
        }"##,
    )
    .unwrap();

    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            let color = match (x < 8, y < 8) {
                (true, true) => [220, 40, 40, 255],
                (false, true) => [40, 200, 60, 255],
                (true, false) => [40, 80, 220, 255],
                (false, false) => [230, 200, 40, 255],
            };
            rgba.extend_from_slice(&color);
        }
    }

    let file = File::create(assets.join("textures/block/test_texture.png")).unwrap();
    let mut encoder = png::Encoder::new(BufWriter::new(file), 16, 16);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&rgba).unwrap();
    writer.finish().unwrap();
}
