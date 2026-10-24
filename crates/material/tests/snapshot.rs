use litematic_material::{AssetSource, MaterialSet, MaterialSnapshot};
use litematic_mesh::{BlockCatalog, MeshLimits, NO_TEXTURE, build_mesh};
use litematic_scene::{BlockStateKey, PackedIndices, Region, Scene, SceneMetadata};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;
use std::sync::Arc;

fn write_texture(path: &Path) {
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
    write_texture(&assets.join("textures/block/test_texture.png"));
}

fn materials(root: &Path) -> MaterialSet {
    let mut source = AssetSource::new();
    source.add_dir(root).unwrap();
    MaterialSet::open(source).unwrap()
}

fn state(name: &str) -> BlockStateKey {
    BlockStateKey::simple(name)
}

fn fixture() -> (tempfile::TempDir, MaterialSet) {
    let pack = tempfile::tempdir().unwrap();
    write_pack(pack.path());
    let materials = materials(pack.path());
    (pack, materials)
}

#[test]
fn snapshot_matches_live_resolution() {
    let (_pack, materials) = fixture();
    let states = [
        state("minecraft:air"),
        state("minecraft:test_cube"),
        state("minecraft:stone"),
        state("minecraft:water"),
        state("minecraft:definitely_not_a_block"),
    ];

    let snapshot = materials.snapshot(states.iter().cloned());

    assert_eq!(snapshot.len(), 4, "air must not occupy a snapshot entry");
    for state in &states {
        let live = materials.resolve(state);
        let shared = snapshot.resolve(state);
        assert_eq!(live.known, shared.known, "known for {}", state.canonical());
        assert_eq!(live.layer, shared.layer, "layer for {}", state.canonical());
        assert_eq!(live.color, shared.color, "color for {}", state.canonical());
        assert_eq!(
            live.model.quads.len(),
            shared.model.quads.len(),
            "quads for {}",
            state.canonical()
        );
        if !state.is_air() {
            assert!(
                Arc::ptr_eq(&live.model, &shared.model),
                "snapshot must reuse the resolved model for {}",
                state.canonical()
            );
        }
        assert_eq!(
            snapshot.info_for(state),
            materials.info_for(state),
            "info for {}",
            state.canonical()
        );
    }

    assert_eq!(snapshot.textures().len(), materials.textures().len());
}

#[test]
fn snapshot_keeps_texture_ids_valid() {
    let (_pack, materials) = fixture();
    let cube = state("minecraft:test_cube");
    let snapshot = materials.snapshot([cube.clone()]);

    let resolved = snapshot.resolve(&cube);
    assert!(!resolved.model.quads.is_empty());
    assert!(
        resolved
            .model
            .quads
            .iter()
            .any(|quad| quad.texture != NO_TEXTURE),
        "pack cube should be textured"
    );
    for quad in &resolved.model.quads {
        if quad.texture != NO_TEXTURE {
            assert!(
                snapshot.textures().get(quad.texture).is_some(),
                "texture id {} must exist in the snapshot",
                quad.texture
            );
        }
    }
}

#[test]
fn snapshot_handles_states_outside_the_set() {
    let (_pack, materials) = fixture();
    let snapshot = materials.snapshot([state("minecraft:stone")]);

    let missing = snapshot.resolve(&state("minecraft:oak_stairs"));
    assert!(!missing.known, "unresolved states report unknown");
    assert!(
        !missing.model.quads.is_empty(),
        "unknown states stay visible"
    );

    let air = snapshot.resolve(&state("minecraft:cave_air"));
    assert!(air.model.quads.is_empty(), "air resolves to no geometry");
}

#[test]
fn snapshot_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MaterialSnapshot>();
}

fn scene(states: &[&str]) -> Scene {
    let mut palette = vec![BlockStateKey::simple("minecraft:air")];
    palette.extend(states.iter().map(|name| BlockStateKey::simple(*name)));

    let size = [palette.len() as u32 - 1, 1, 1];
    let values: Vec<u32> = (1..palette.len() as u32).collect();
    let bits = PackedIndices::bits_for_palette(palette.len());
    let states = PackedIndices::from_values(bits, &values).unwrap();
    let region = Region {
        name: "snapshot".to_string(),
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

#[test]
fn snapshot_builds_the_same_mesh_as_the_live_set() {
    let (_pack, materials) = fixture();
    let scene = scene(&["minecraft:test_cube", "minecraft:stone", "minecraft:water"]);
    let palette = scene
        .regions
        .iter()
        .flat_map(|region| region.palette.iter().cloned());
    let snapshot = materials.snapshot(palette);

    let live = build_mesh(&scene, &materials, &MeshLimits::default()).unwrap();
    let shared = build_mesh(&scene, &snapshot, &MeshLimits::default()).unwrap();

    assert_eq!(live, shared);
}

#[test]
fn snapshot_serves_concurrent_meshing() {
    let (_pack, materials) = fixture();
    let scene = scene(&["minecraft:test_cube", "minecraft:stone", "minecraft:water"]);
    let palette = scene
        .regions
        .iter()
        .flat_map(|region| region.palette.iter().cloned());
    let snapshot = Arc::new(materials.snapshot(palette));

    let expected = build_mesh(&scene, snapshot.as_ref(), &MeshLimits::default()).unwrap();
    let workers = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4)
        .min(8);
    let results: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    build_mesh(&scene, snapshot.as_ref(), &MeshLimits::default()).unwrap()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("mesh worker panicked"))
            .collect()
    });

    for mesh in results {
        assert_eq!(mesh, expected, "concurrent meshing must be deterministic");
    }
}
