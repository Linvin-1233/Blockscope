use litematic_material::{AssetSource, MaterialSet};
use litematic_mesh::{MeshLimits, build_mesh};
use litematic_render::{RenderOptions, render_mesh_with_textures, write_png};
use litematic_scene::{BlockStateKey, PackedIndices, Region, Scene, SceneMetadata};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

fn key(name: &str, properties: &[(&str, &str)]) -> BlockStateKey {
    let mut key = BlockStateKey::simple(name);
    key.properties = properties
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect::<BTreeMap<_, _>>();
    key
}

#[test]
#[ignore = "requires MINECRAFT_CLIENT_JAR"]
fn renders_a_textured_material_strip() {
    let Some(jar) = std::env::var_os("MINECRAFT_CLIENT_JAR") else {
        return;
    };

    let palette = vec![
        BlockStateKey::simple("minecraft:air"),
        BlockStateKey::simple("minecraft:stone_bricks"),
        key(
            "minecraft:oak_stairs",
            &[
                ("facing", "east"),
                ("half", "bottom"),
                ("shape", "straight"),
            ],
        ),
        key("minecraft:grass_block", &[("snowy", "false")]),
        BlockStateKey::simple("minecraft:white_stained_glass"),
        key("minecraft:water", &[("level", "0")]),
        key(
            "minecraft:chest",
            &[
                ("facing", "north"),
                ("type", "left"),
                ("waterlogged", "false"),
            ],
        ),
        key(
            "minecraft:chest",
            &[
                ("facing", "north"),
                ("type", "right"),
                ("waterlogged", "false"),
            ],
        ),
        key(
            "minecraft:observer",
            &[("facing", "north"), ("powered", "false")],
        ),
        key(
            "minecraft:redstone_wire",
            &[
                ("east", "side"),
                ("north", "none"),
                ("power", "15"),
                ("south", "side"),
                ("west", "none"),
            ],
        ),
        key(
            "minecraft:rail",
            &[("shape", "east_west"), ("waterlogged", "false")],
        ),
    ];

    let size = [11u32, 1, 3];
    let mut values = vec![0u32; (size[0] * size[1] * size[2]) as usize];
    for (index, state) in palette.iter().enumerate().skip(1) {
        values[size[0] as usize + index] = state_index(&palette, state);
    }

    let bits = PackedIndices::bits_for_palette(palette.len());
    let states = PackedIndices::from_values(bits, &values).unwrap();
    let region = Region {
        name: "materials".to_string(),
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
    let scene = Scene::new(SceneMetadata::default(), vec![region], Vec::new()).unwrap();

    let mut source = AssetSource::new();
    source.add_jar(Path::new(&jar)).unwrap();
    let materials = MaterialSet::open(source).unwrap();
    let mesh = build_mesh(&scene, &materials, &MeshLimits::default()).unwrap();
    let options = RenderOptions {
        width: 768,
        height: 384,
        yaw: 45.0,
        elevation: 25.0,
        ..Default::default()
    };
    let textures = materials.textures();
    let output = render_mesh_with_textures(&scene, &mesh, &options, &textures).unwrap();

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../renders/debug-materials.png");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = File::create(&path).unwrap();
    let mut writer = BufWriter::new(file);
    write_png(&output, &mut writer).unwrap();
    println!("wrote {}", path.display());
    println!("diagnostics: {:?}", materials.diagnostics());
}

fn state_index(palette: &[BlockStateKey], state: &BlockStateKey) -> u32 {
    palette.iter().position(|item| item == state).unwrap() as u32
}
