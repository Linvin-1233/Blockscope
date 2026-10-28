use crate::model::{box_quads, rotate_block, rotate_normal};
use crate::source::AssetSource;
use crate::texture::TextureSet;
use crate::util::strip_namespace;
use litematic_mesh::{BlockModel, ModelQuad};
use litematic_scene::BlockStateKey;

/// Build the closed chest body/lid/lock model from entity textures.
pub fn build_chest_model(
    state: &BlockStateKey,
    source: &AssetSource,
    textures: &mut TextureSet,
) -> Option<BlockModel> {
    let path = strip_namespace(&state.name);
    let base = chest_texture_base(path)?;
    let kind = state
        .properties
        .get("type")
        .map(String::as_str)
        .unwrap_or("single");
    let texture_name = match kind {
        "left" => format!("entity/chest/{base}_left"),
        "right" => format!("entity/chest/{base}_right"),
        _ => format!("entity/chest/{base}"),
    };
    let texture = textures.load(source, &texture_name)?;

    let facing = state
        .properties
        .get("facing")
        .map(String::as_str)
        .unwrap_or("north");
    let rotation = facing_rotation(facing);

    let mut quads = Vec::new();
    if kind == "single" {
        add_box(
            &mut quads,
            [1.0, 0.0, 1.0],
            [15.0, 10.0, 15.0],
            [0.0, 19.0],
            texture,
        );
        add_box(
            &mut quads,
            [1.0, 9.0, 1.0],
            [15.0, 14.0, 15.0],
            [0.0, 0.0],
            texture,
        );
        add_box(
            &mut quads,
            [7.0, 7.0, 15.0],
            [9.0, 11.0, 16.0],
            [0.0, 0.0],
            texture,
        );
    } else {
        let (x1, x2) = if kind == "left" {
            (0.0, 15.0)
        } else {
            (1.0, 16.0)
        };
        add_box(
            &mut quads,
            [x1, 0.0, 1.0],
            [x2, 10.0, 15.0],
            [0.0, 19.0],
            texture,
        );
        add_box(
            &mut quads,
            [x1, 9.0, 1.0],
            [x2, 14.0, 15.0],
            [0.0, 0.0],
            texture,
        );
        let lock_x = if kind == "left" { 0.0 } else { 15.0 };
        add_box(
            &mut quads,
            [lock_x, 7.0, 15.0],
            [lock_x + 1.0, 11.0, 16.0],
            [0.0, 0.0],
            texture,
        );
    }

    for quad in &mut quads {
        quad.vertices = quad
            .vertices
            .map(|vertex| rotate_block(vertex, 0, rotation));
        quad.normal = rotate_normal(quad.normal, 0, rotation);
    }

    Some(BlockModel {
        quads,
        is_full_cube: false,
        ambient_occlusion: true,
    })
}

fn add_box(
    quads: &mut Vec<ModelQuad>,
    from: [f32; 3],
    to: [f32; 3],
    texture_origin: [f32; 2],
    texture: u16,
) {
    quads.extend(box_quads(from, to, texture_origin, 64.0, texture));
}

fn facing_rotation(facing: &str) -> i32 {
    // The model's front is +Z (south); rotate it to the requested facing.
    match facing {
        "south" => 0,
        "east" => 90,
        "north" => 180,
        "west" => 270,
        _ => 180,
    }
}

fn chest_texture_base(path: &str) -> Option<&'static str> {
    match path {
        "chest" => Some("normal"),
        "trapped_chest" => Some("trapped"),
        "ender_chest" => Some("ender"),
        "copper_chest" | "waxed_copper_chest" => Some("copper"),
        "exposed_copper_chest" | "waxed_exposed_copper_chest" => Some("copper_exposed"),
        "weathered_copper_chest" | "waxed_weathered_copper_chest" => Some("copper_weathered"),
        "oxidized_copper_chest" | "waxed_oxidized_copper_chest" => Some("copper_oxidized"),
        _ => None,
    }
}
