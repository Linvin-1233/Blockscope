use litematic_material::{AssetSource, MaterialKind, MaterialSet, TextureFilter};
use litematic_mesh::{BlockCatalog, NO_TEXTURE};
use litematic_scene::BlockStateKey;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;

fn write_texture(path: &Path) {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            let color = match (x < 8, y < 8) {
                (true, true) => [255, 0, 0, 255],
                (false, true) => [0, 255, 0, 255],
                (true, false) => [0, 0, 255, 255],
                (false, false) => [255, 255, 0, 255],
            };
            rgba.extend_from_slice(&color);
        }
    }

    let file = File::create(path).unwrap();
    let mut encoder = png::Encoder::new(BufWriter::new(file), 16, 16);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&rgba).unwrap();
    writer.finish().unwrap();
}

fn write_pack(root: &Path) {
    let assets = root.join("assets/minecraft");
    fs::create_dir_all(assets.join("blockstates")).unwrap();
    fs::create_dir_all(assets.join("models/block")).unwrap();
    fs::create_dir_all(assets.join("textures/block")).unwrap();

    fs::write(
        assets.join("blockstates/test_cube.json"),
        r##"{
            "variants": {
                "": { "model": "minecraft:block/test_cube" }
            }
        }"##,
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
                "particle": "#all",
                "down": "#all",
                "up": "#all",
                "north": "#all",
                "south": "#all",
                "west": "#all",
                "east": "#all"
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
        assets.join("blockstates/test_multipart.json"),
        r##"{
            "multipart": [
                { "apply": { "model": "block/test_post" } },
                {
                    "apply": { "model": "block/test_side", "y": 90 },
                    "when": { "north": "true" }
                }
            ]
        }"##,
    )
    .unwrap();
    fs::write(
        assets.join("models/block/test_post.json"),
        r##"{
            "textures": { "all": "block/test_texture" },
            "elements": [
                {
                    "from": [4, 0, 4],
                    "to": [12, 16, 12],
                    "faces": {
                        "down":  { "texture": "#all" },
                        "up":    { "texture": "#all" },
                        "north": { "texture": "#all" },
                        "south": { "texture": "#all" },
                        "west":  { "texture": "#all" },
                        "east":  { "texture": "#all" }
                    }
                }
            ]
        }"##,
    )
    .unwrap();
    fs::write(
        assets.join("models/block/test_side.json"),
        r##"{
            "textures": { "all": "block/test_texture" },
            "elements": [
                {
                    "from": [4, 4, 0],
                    "to": [12, 12, 4],
                    "faces": {
                        "up":    { "texture": "#all", "uv": [0, 0, 16, 16] },
                        "down":  { "texture": "#all", "uv": [0, 16, 16, 0] }
                    }
                }
            ]
        }"##,
    )
    .unwrap();

    write_texture(&assets.join("textures/block/test_texture.png"));
}

fn write_solid_texture(path: &Path, color: [u8; 4]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for _ in 0..16 * 16 {
        rgba.extend_from_slice(&color);
    }

    let file = File::create(path).unwrap();
    let mut encoder = png::Encoder::new(BufWriter::new(file), 16, 16);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&rgba).unwrap();
    writer.finish().unwrap();
}

fn material_set() -> (tempfile::TempDir, MaterialSet) {
    let dir = tempfile::tempdir().unwrap();
    write_pack(dir.path());
    let mut source = AssetSource::new();
    source.add_dir(dir.path()).unwrap();
    let materials = MaterialSet::open(source).unwrap();
    (dir, materials)
}

#[test]
fn resolves_parent_chain_and_texture_variables() {
    let (_dir, materials) = material_set();
    let state = BlockStateKey::simple("minecraft:test_cube");
    let resolved = materials.resolve(&state);

    assert_eq!(resolved.model.quads.len(), 6);
    assert!(resolved.model.is_full_cube);
    assert!(resolved.model.ambient_occlusion);
    assert!(
        resolved
            .model
            .quads
            .iter()
            .all(|quad| quad.texture != NO_TEXTURE)
    );

    let textures = materials.textures();
    let id = resolved.model.quads[0].texture;
    let red = textures
        .sample(id, [0.25, 0.25], TextureFilter::Nearest)
        .unwrap();
    let green = textures
        .sample(id, [0.75, 0.25], TextureFilter::Nearest)
        .unwrap();
    let blue = textures
        .sample(id, [0.25, 0.75], TextureFilter::Nearest)
        .unwrap();
    let yellow = textures
        .sample(id, [0.75, 0.75], TextureFilter::Nearest)
        .unwrap();

    assert!(red[0] > 0.99 && red[1] < 0.01);
    assert!(green[1] > 0.99 && green[0] < 0.01);
    assert!(blue[2] > 0.99 && blue[0] < 0.01);
    assert!(yellow[0] > 0.99 && yellow[1] > 0.99);
}

#[test]
fn evaluates_multipart_and_applies_blockstate_rotation() {
    let (_dir, materials) = material_set();
    let mut state = BlockStateKey::simple("minecraft:test_multipart");
    state
        .properties
        .insert("north".to_string(), "true".to_string());

    let resolved = materials.resolve(&state);
    assert_eq!(resolved.model.quads.len(), 8);

    let info = materials.info_for(&state).unwrap();
    assert_eq!(info.kind, MaterialKind::PackModel);
    assert!(info.textured);
}

#[test]
fn overlay_wins_over_base_textures() {
    let base = tempfile::tempdir().unwrap();
    write_pack(base.path());

    let overlay = tempfile::tempdir().unwrap();
    let texture = overlay
        .path()
        .join("assets/minecraft/textures/block/test_texture.png");
    write_solid_texture(&texture, [0, 255, 0, 255]);

    let mut source = AssetSource::new();
    source.add_dir(base.path()).unwrap();
    source.add_overlay_dir(overlay.path()).unwrap();
    let materials = MaterialSet::open(source).unwrap();

    let state = BlockStateKey::simple("minecraft:test_cube");
    let resolved = materials.resolve(&state);
    let textures = materials.textures();
    let sample = textures
        .sample(
            resolved.model.quads[0].texture,
            [0.5, 0.5],
            TextureFilter::Nearest,
        )
        .unwrap();
    assert!(sample[1] > 0.99 && sample[0] < 0.01 && sample[2] < 0.01);
}

#[test]
fn falls_back_to_color_blocks_for_missing_models() {
    let (_dir, materials) = material_set();
    let state = BlockStateKey::simple("minecraft:not_in_pack");
    let resolved = materials.resolve(&state);

    assert_eq!(resolved.model.quads.len(), 6);
    assert!(
        resolved
            .model
            .quads
            .iter()
            .all(|quad| quad.texture == NO_TEXTURE)
    );

    let info = materials.info_for(&state).unwrap();
    assert_eq!(info.kind, MaterialKind::Unknown);
    assert!(!info.textured);
}
