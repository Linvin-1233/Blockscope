use litematic_material::{AssetSource, MaterialKind, MaterialSet, TextureFilter};
use litematic_mesh::{BlockCatalog, FaceDirection, NO_TEXTURE};
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

fn chest_material_set() -> (tempfile::TempDir, MaterialSet) {
    let dir = tempfile::tempdir().unwrap();
    write_pack(dir.path());
    let chest = dir.path().join("assets/minecraft/textures/entity/chest");
    write_solid_texture(&chest.join("normal_left.png"), [180, 110, 40, 255]);
    write_solid_texture(&chest.join("normal_right.png"), [180, 110, 40, 255]);
    let mut source = AssetSource::new();
    source.add_dir(dir.path()).unwrap();
    let materials = MaterialSet::open(source).unwrap();
    (dir, materials)
}

fn chest_state(kind: &str) -> BlockStateKey {
    let mut state = BlockStateKey::simple("minecraft:chest");
    state
        .properties
        .insert("facing".to_string(), "south".to_string());
    state
        .properties
        .insert("type".to_string(), kind.to_string());
    state
        .properties
        .insert("waterlogged".to_string(), "false".to_string());
    state
}

fn lid_quad(model: &litematic_mesh::BlockModel, face: FaceDirection) -> &litematic_mesh::ModelQuad {
    model
        .quads
        .iter()
        .find(|quad| {
            quad.face == face
                && quad.vertices.iter().all(|vertex| vertex[1] >= 9.0)
                && quad.vertices.iter().any(|vertex| vertex[1] >= 14.0)
        })
        .unwrap()
}

fn uv_at_x(quad: &litematic_mesh::ModelQuad, x: f32) -> [f32; 2] {
    quad.vertices
        .iter()
        .zip(quad.uv.iter())
        .find(|(vertex, _)| (vertex[0] - x).abs() < 1e-4)
        .map(|(_, uv)| *uv)
        .unwrap()
}

fn uv_at_z(quad: &litematic_mesh::ModelQuad, z: f32) -> [f32; 2] {
    quad.vertices
        .iter()
        .zip(quad.uv.iter())
        .find(|(vertex, _)| (vertex[2] - z).abs() < 1e-4)
        .map(|(_, uv)| *uv)
        .unwrap()
}

#[test]
fn double_chest_front_texture_keeps_outer_border() {
    let (_dir, materials) = chest_material_set();

    // Left is the east block and right is the west block. Their textures are
    // mirrored, so vanilla maps u=58 to the west edge and u=43 to the east
    // edge of both front faces, keeping the dark border on the outside.
    for (kind, west_x, east_x) in [("left", 0.0, 15.0), ("right", 1.0, 16.0)] {
        let resolved = materials.resolve(&chest_state(kind));
        let front = lid_quad(&resolved.model, FaceDirection::South);

        let west = uv_at_x(front, west_x);
        let east = uv_at_x(front, east_x);

        assert!(
            (west[0] - 58.0 / 64.0).abs() < 1e-4,
            "{kind} chest west edge has unexpected u={}",
            west[0]
        );
        assert!(
            (east[0] - 43.0 / 64.0).abs() < 1e-4,
            "{kind} chest east edge has unexpected u={}",
            east[0]
        );
    }
}

#[test]
fn double_chest_lid_top_texture_keeps_outer_border() {
    let (_dir, materials) = chest_material_set();

    for (kind, from_x, to_x) in [("left", 0.0, 15.0), ("right", 1.0, 16.0)] {
        let resolved = materials.resolve(&chest_state(kind));
        let top = lid_quad(&resolved.model, FaceDirection::Up);

        let west = uv_at_x(top, from_x);
        let east = uv_at_x(top, to_x);
        let north = uv_at_z(top, 1.0);
        let south = uv_at_z(top, 15.0);

        assert!(
            (west[0] - 29.0 / 64.0).abs() < 1e-4,
            "{kind} chest top west edge has unexpected u={}",
            west[0]
        );
        assert!(
            (east[0] - 44.0 / 64.0).abs() < 1e-4,
            "{kind} chest top east edge has unexpected u={}",
            east[0]
        );
        assert!(
            (north[1] - 14.0 / 64.0).abs() < 1e-4,
            "{kind} chest top north edge has unexpected v={}",
            north[1]
        );
        assert!(
            (south[1] - 0.0).abs() < 1e-4,
            "{kind} chest top south edge has unexpected v={}",
            south[1]
        );
    }
}

#[test]
fn double_chest_renders_center_lock() {
    let (_dir, materials) = chest_material_set();

    for (kind, expected_x) in [("left", 0.0), ("right", 15.0)] {
        let resolved = materials.resolve(&chest_state(kind));
        let lock = resolved
            .model
            .quads
            .iter()
            .find(|quad| {
                quad.vertices
                    .iter()
                    .all(|vertex| (7.0..=11.0).contains(&vertex[1]) && vertex[2] >= 15.0)
            })
            .unwrap();

        let min_x = lock
            .vertices
            .iter()
            .map(|vertex| vertex[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = lock
            .vertices
            .iter()
            .map(|vertex| vertex[0])
            .fold(f32::NEG_INFINITY, f32::max);

        assert!((min_x - expected_x).abs() < 1e-4);
        assert!((max_x - expected_x - 1.0).abs() < 1e-4);
    }
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
