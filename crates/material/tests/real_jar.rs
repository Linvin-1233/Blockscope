use litematic_material::{AssetSource, MaterialKind, MaterialSet};
use litematic_mesh::{BlockCatalog, NO_TEXTURE};
use litematic_scene::BlockStateKey;
use std::path::Path;

fn materials() -> Option<MaterialSet> {
    let jar = std::env::var_os("MINECRAFT_CLIENT_JAR")?;
    let mut source = AssetSource::new();
    source.add_jar(Path::new(&jar)).unwrap();
    Some(MaterialSet::open(source).unwrap())
}

fn state(name: &str, properties: &[(&str, &str)]) -> BlockStateKey {
    let mut state = BlockStateKey::simple(name);
    for (key, value) in properties {
        state
            .properties
            .insert((*key).to_string(), (*value).to_string());
    }
    state
}

#[test]
#[ignore = "requires MINECRAFT_CLIENT_JAR"]
fn resolves_real_jar_models() {
    let Some(materials) = materials() else {
        return;
    };

    let cases = [
        state("minecraft:stone", &[]),
        state(
            "minecraft:oak_stairs",
            &[
                ("facing", "east"),
                ("half", "bottom"),
                ("shape", "straight"),
            ],
        ),
        state("minecraft:grass_block", &[("snowy", "false")]),
        state("minecraft:white_stained_glass", &[]),
        state("minecraft:water", &[("level", "0")]),
        state(
            "minecraft:observer",
            &[("facing", "north"), ("powered", "false")],
        ),
        state(
            "minecraft:redstone_wire",
            &[
                ("east", "side"),
                ("north", "none"),
                ("power", "15"),
                ("south", "side"),
                ("west", "none"),
            ],
        ),
        state(
            "minecraft:rail",
            &[("shape", "east_west"), ("waterlogged", "false")],
        ),
        state(
            "minecraft:oak_fence",
            &[
                ("east", "true"),
                ("north", "false"),
                ("south", "false"),
                ("waterlogged", "false"),
                ("west", "true"),
            ],
        ),
        state(
            "minecraft:chest",
            &[
                ("facing", "west"),
                ("type", "left"),
                ("waterlogged", "false"),
            ],
        ),
    ];

    for state in cases {
        let block = materials.resolve(&state);
        let textured = block
            .model
            .quads
            .iter()
            .filter(|quad| quad.texture != NO_TEXTURE)
            .count();
        let info = materials.info_for(&state);
        if state.name == "minecraft:chest" {
            assert_eq!(info.map(|info| info.kind), Some(MaterialKind::Chest));
        }
        println!(
            "{}: quads={} textured={} known={} info={info:?}",
            state.canonical(),
            block.model.quads.len(),
            textured,
            block.known
        );
        assert!(
            !block.model.quads.is_empty(),
            "{} resolved to no quads",
            state.canonical()
        );
        assert!(textured > 0, "{} resolved no textures", state.canonical());
    }

    println!(
        "textures={} diagnostics={:?}",
        materials.textures().len(),
        materials.diagnostics()
    );
}
