use litematic_material::{AssetSource, MaterialSet};
use litematic_mesh::{MeshLimits, build_mesh};
use litematic_render::{
    Background, LightingProfile, RenderOptions, render_mesh_with_textures, write_png,
};
use litematic_scene::{BlockStateKey, PackedIndices, Region, Scene, SceneMetadata};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

fn key(name: &str, properties: &[(&str, &str)]) -> BlockStateKey {
    let mut key = BlockStateKey::simple(name);
    key.properties = properties
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect::<BTreeMap<_, _>>();
    key
}

fn write_texture(path: &Path, width: u32, height: u32, rgba: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = File::create(path).unwrap();
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(rgba).unwrap();
    writer.finish().unwrap();
}

fn quadrant_texture() -> Vec<u8> {
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
    rgba
}

fn cutout_texture() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for _y in 0..16 {
        for x in 0..16 {
            if x < 8 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                rgba.extend_from_slice(&[60, 190, 70, 255]);
            }
        }
    }
    rgba
}

fn glass_texture() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            let edge = x == 0 || y == 0 || x == 15 || y == 15;
            if edge {
                rgba.extend_from_slice(&[120, 200, 230, 220]);
            } else {
                rgba.extend_from_slice(&[120, 200, 230, 90]);
            }
        }
    }
    rgba
}

fn chest_texture() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64 {
        for x in 0..64 {
            let dark = ((x / 4) + (y / 4)) % 2 == 0;
            let color = if dark {
                [120, 78, 30, 255]
            } else {
                [170, 116, 48, 255]
            };
            rgba.extend_from_slice(&color);
        }
    }
    rgba
}

fn write_pack(root: &Path) {
    let assets = root.join("assets/minecraft");
    fs::create_dir_all(assets.join("blockstates")).unwrap();
    fs::create_dir_all(assets.join("models/block")).unwrap();
    fs::create_dir_all(assets.join("textures/block")).unwrap();
    fs::create_dir_all(assets.join("textures/entity/chest")).unwrap();

    fs::write(
        assets.join("blockstates/stone.json"),
        r##"{ "variants": { "": { "model": "block/test_cube" } } }"##,
    )
    .unwrap();
    fs::write(
        assets.join("blockstates/oak_stairs.json"),
        r##"{
            "variants": {
                "facing=east,half=bottom,shape=straight": { "model": "block/test_stairs" }
            }
        }"##,
    )
    .unwrap();
    fs::write(
        assets.join("blockstates/oak_leaves.json"),
        r##"{ "variants": { "": { "model": "block/test_cutout_cube" } } }"##,
    )
    .unwrap();
    fs::write(
        assets.join("blockstates/white_stained_glass.json"),
        r##"{ "variants": { "": { "model": "block/test_glass_cube" } } }"##,
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
    fs::write(
        assets.join("models/block/test_cutout_cube.json"),
        r##"{
            "parent": "block/cube_all",
            "textures": { "all": "block/test_cutout" }
        }"##,
    )
    .unwrap();
    fs::write(
        assets.join("models/block/test_glass_cube.json"),
        r##"{
            "parent": "block/cube_all",
            "textures": { "all": "block/test_glass" }
        }"##,
    )
    .unwrap();
    fs::write(
        assets.join("models/block/test_stairs.json"),
        r##"{
            "textures": { "all": "block/test_texture" },
            "elements": [
                {
                    "from": [0, 0, 0],
                    "to": [16, 8, 16],
                    "faces": {
                        "down":  { "uv": [0, 0, 16, 16], "texture": "#all", "cullface": "down" },
                        "up":    { "uv": [0, 0, 16, 16], "texture": "#all" },
                        "north": { "uv": [0, 8, 16, 16], "texture": "#all", "cullface": "north" },
                        "south": { "uv": [0, 8, 16, 16], "texture": "#all", "cullface": "south" },
                        "west":  { "uv": [0, 8, 16, 16], "texture": "#all", "cullface": "west" },
                        "east":  { "uv": [0, 8, 16, 16], "texture": "#all", "cullface": "east" }
                    }
                },
                {
                    "from": [8, 8, 0],
                    "to": [16, 16, 16],
                    "faces": {
                        "up":    { "uv": [8, 0, 16, 16], "texture": "#all", "cullface": "up" },
                        "north": { "uv": [0, 0, 8, 8], "texture": "#all", "cullface": "north" },
                        "south": { "uv": [8, 0, 16, 8], "texture": "#all", "cullface": "south" },
                        "west":  { "uv": [0, 0, 16, 8], "texture": "#all" },
                        "east":  { "uv": [0, 0, 16, 8], "texture": "#all", "cullface": "east" }
                    }
                }
            ]
        }"##,
    )
    .unwrap();

    write_texture(
        &assets.join("textures/block/test_texture.png"),
        16,
        16,
        &quadrant_texture(),
    );
    write_texture(
        &assets.join("textures/block/test_cutout.png"),
        16,
        16,
        &cutout_texture(),
    );
    write_texture(
        &assets.join("textures/block/test_glass.png"),
        16,
        16,
        &glass_texture(),
    );
    write_texture(
        &assets.join("textures/entity/chest/normal.png"),
        64,
        64,
        &chest_texture(),
    );
}

fn make_scene(states: Vec<BlockStateKey>) -> Scene {
    let mut palette = vec![BlockStateKey::simple("minecraft:air")];
    palette.extend(states);
    let size = [palette.len() as u32 - 1, 1, 1];
    let values: Vec<u32> = (1..palette.len() as u32).collect();
    let bits = PackedIndices::bits_for_palette(palette.len());
    let states = PackedIndices::from_values(bits, &values).unwrap();
    let region = Region {
        name: "golden".to_string(),
        origin: [0, 0, 0],
        size,
        data_version: 3953,
        palette,
        states,
        block_entities: Vec::new(),
        entity_count: 0,
        pending_block_tick_count: 0,
        pending_fluid_tick_count: 0,
    };
    Scene::new(SceneMetadata::default(), vec![region], Vec::new()).unwrap()
}

fn render(scene: &Scene, pack: &Path, width: u32, height: u32) -> Vec<u8> {
    let mut source = AssetSource::new();
    source.add_dir(pack).unwrap();
    let materials = MaterialSet::open(source).unwrap();
    let mesh = build_mesh(scene, &materials, &MeshLimits::default()).unwrap();
    let options = RenderOptions {
        width,
        height,
        yaw: 0.0,
        elevation: 20.0,
        background: Background::Transparent,
        lighting: LightingProfile::FlatV1,
        supersample: 1,
        padding: 12.0,
        ..Default::default()
    };
    let textures = materials.textures();
    let output = render_mesh_with_textures(scene, &mesh, &options, &textures).unwrap();
    let mut bytes = Vec::new();
    write_png(&output, &mut bytes).unwrap();
    bytes
}

fn check_golden(name: &str, bytes: &[u8]) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/golden/render")
        .join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
    } else {
        let expected =
            fs::read(&path).unwrap_or_else(|_| panic!("missing golden PNG {}", path.display()));
        assert_eq!(bytes, expected, "golden PNG mismatch for {name}");
    }
}

#[test]
fn textured_cube_matches_golden() {
    let pack = tempfile::tempdir().unwrap();
    write_pack(pack.path());
    let scene = make_scene(vec![BlockStateKey::simple("minecraft:stone")]);
    let bytes = render(&scene, pack.path(), 256, 256);
    check_golden("textured-cube.png", &bytes);
}

#[test]
fn textured_materials_match_golden() {
    let pack = tempfile::tempdir().unwrap();
    write_pack(pack.path());
    let scene = make_scene(vec![
        BlockStateKey::simple("minecraft:stone"),
        key(
            "minecraft:oak_stairs",
            &[
                ("facing", "east"),
                ("half", "bottom"),
                ("shape", "straight"),
            ],
        ),
        BlockStateKey::simple("minecraft:oak_leaves"),
        BlockStateKey::simple("minecraft:white_stained_glass"),
        BlockStateKey::simple("minecraft:chest"),
    ]);
    let bytes = render(&scene, pack.path(), 768, 320);
    check_golden("textured-materials.png", &bytes);
}
