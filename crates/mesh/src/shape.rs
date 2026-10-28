use crate::color::Rgba;
use crate::model::{BlockModel, FaceDirection, ModelBox, ModelQuad, NO_TEXTURE, face_uv};
use crate::util::strip_namespace;
use crate::voxel::build_model;
use litematic_scene::BlockStateKey;

/// The two fluids the mesh builder shapes from neighbouring blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FluidKind {
    Water,
    Lava,
}

/// Fluid level in ninths of a block. `9` means a full block and is only
/// produced when the cell above holds the same fluid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FluidState {
    pub kind: FluidKind,
    pub height_ninths: u8,
}

/// Resolve a pure fluid block to its kind and resting height.
///
/// Water and lava levels 0-7 sit at `(8 - level) / 9` blocks. Falling fluid
/// (level 8 and above) keeps the source height until the cell above fills the
/// column; the mesh builder promotes it to a full block in that case.
pub(crate) fn fluid_state(state: &BlockStateKey) -> Option<FluidState> {
    let path = strip_namespace(&state.name);
    let kind = match path {
        "water" | "flowing_water" | "bubble_column" => FluidKind::Water,
        "lava" | "flowing_lava" => FluidKind::Lava,
        _ => return None,
    };

    let level = prop(state, "level")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0)
        .min(15);
    let height_ninths = if level == 0 || level >= 8 {
        8
    } else {
        8 - level
    };

    Some(FluidState {
        kind,
        height_ninths,
    })
}

/// True when a non-fluid block contains a water source.
///
/// Waterlogged partial blocks keep their own model and gain a shaped water
/// layer. Kelp and seagrass are always water-filled even without the property.
/// Bubble columns are pure fluid and are handled by [`fluid_state`].
pub(crate) fn waterlogged(state: &BlockStateKey) -> bool {
    if fluid_state(state).is_some() {
        return false;
    }

    if matches!(
        strip_namespace(&state.name),
        "kelp" | "kelp_plant" | "seagrass" | "tall_seagrass"
    ) {
        return true;
    }

    prop(state, "waterlogged") == Some("true")
}

/// Water source state for a waterlogged block.
pub(crate) fn waterlogged_fluid(state: &BlockStateKey) -> Option<FluidState> {
    waterlogged(state).then_some(FluidState {
        kind: FluidKind::Water,
        height_ninths: 8,
    })
}

/// Power level of a redstone dust state, if this is redstone dust.
pub(crate) fn redstone_power(state: &BlockStateKey) -> Option<u8> {
    if strip_namespace(&state.name) != "redstone_wire" {
        return None;
    }

    Some(
        prop(state, "power")
            .and_then(|value| value.parse::<u8>().ok())
            .unwrap_or(0)
            .min(15),
    )
}

/// Build a partial model for a known shape family. `None` means full cube.
pub(crate) fn model_for(state: &BlockStateKey) -> Option<BlockModel> {
    let path = strip_namespace(&state.name);

    if path == "water"
        || path == "lava"
        || path == "flowing_water"
        || path == "flowing_lava"
        || path == "bubble_column"
    {
        return Some(fluid());
    }
    if path == "redstone_wire" {
        return Some(redstone_wire());
    }
    if path == "chest" || path == "trapped_chest" || path == "ender_chest" {
        return Some(single_box([1, 0, 1], [15, 14, 15]));
    }
    if path == "barrel" || path.ends_with("_shulker_box") {
        return Some(single_box([1, 0, 1], [15, 16, 15]));
    }
    if path.ends_with("_sign") {
        return Some(single_box([7, 0, 0], [9, 16, 16]));
    }
    if path.ends_with("_banner") {
        return Some(single_box([7, 0, 0], [9, 16, 16]));
    }
    if path.ends_with("skull") || path.ends_with("_head") {
        return Some(single_box([4, 0, 4], [12, 8, 12]));
    }
    if path == "lectern" {
        return Some(lectern());
    }
    if path == "conduit" {
        return Some(single_box([4, 4, 4], [12, 12, 12]));
    }
    if path.ends_with("_slab") {
        return Some(slab(state));
    }
    if path.ends_with("_stairs") {
        return Some(stairs(state));
    }
    if path.ends_with("_carpet") || path == "moss_carpet" {
        return Some(single_box([0, 0, 0], [16, 1, 16]));
    }
    if path == "snow" {
        return Some(snow_layer(state));
    }
    if path.ends_with("_trapdoor") {
        return Some(trapdoor(state));
    }
    if path.ends_with("_door") {
        return Some(door(state));
    }
    if path.ends_with("_fence_gate") {
        return Some(fence_gate(state));
    }
    if path.ends_with("_fence") {
        return Some(fence(state));
    }
    if path.ends_with("_wall") {
        return Some(wall(state));
    }
    if path.ends_with("_pane") || path == "iron_bars" {
        return Some(pane(state));
    }
    if path.ends_with("_button") {
        return Some(button(state));
    }
    if path.ends_with("_pressure_plate") {
        return Some(single_box([1, 0, 1], [15, 1, 15]));
    }
    if path == "ladder" {
        return Some(ladder(state));
    }
    if path.ends_with("rail") {
        return Some(single_box([0, 0, 0], [16, 1, 16]));
    }
    if path == "lily_pad" {
        return Some(single_box([0, 0, 0], [16, 1, 16]));
    }
    if path == "cactus" {
        return Some(single_box([1, 0, 1], [15, 16, 15]));
    }
    if is_cross_plant(path) {
        return Some(build_model(&[], cross_quads()));
    }
    if path.contains("torch") {
        return Some(torch(state));
    }

    None
}

fn slab(state: &BlockStateKey) -> BlockModel {
    let kind = prop(state, "type").unwrap_or("bottom");
    if kind == "double" {
        return build_model(&[ModelBox::new([0, 0, 0], [16, 16, 16])], Vec::new());
    }

    let model_box = if kind == "top" {
        ModelBox::new([0, 8, 0], [16, 16, 16])
    } else {
        ModelBox::new([0, 0, 0], [16, 8, 16])
    };
    build_model(&[model_box], Vec::new())
}

fn stairs(state: &BlockStateKey) -> BlockModel {
    let facing = prop(state, "facing").unwrap_or("north");
    let top = prop(state, "half") == Some("top");
    let mut builder = ShapeBuilder::new();

    if top {
        builder.add([0, 8, 0], [16, 16, 16]);
        match facing {
            "east" => builder.add([8, 0, 0], [16, 8, 16]),
            "west" => builder.add([0, 0, 0], [8, 8, 16]),
            "south" => builder.add([0, 0, 8], [16, 8, 16]),
            _ => builder.add([0, 0, 0], [16, 8, 8]),
        };
    } else {
        builder.add([0, 0, 0], [16, 8, 16]);
        match facing {
            "east" => builder.add([8, 8, 0], [16, 16, 16]),
            "west" => builder.add([0, 8, 0], [8, 16, 16]),
            "south" => builder.add([0, 8, 8], [16, 16, 16]),
            _ => builder.add([0, 8, 0], [16, 16, 8]),
        };
    }

    builder.build()
}

fn snow_layer(state: &BlockStateKey) -> BlockModel {
    let layers = prop(state, "layers")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(1)
        .clamp(1, 8);
    let height = layers * 2;
    build_model(&[ModelBox::new([0, 0, 0], [16, height, 16])], Vec::new())
}

fn trapdoor(state: &BlockStateKey) -> BlockModel {
    let open = bool_prop(state, "open");
    let top = prop(state, "half") == Some("top");
    let facing = prop(state, "facing").unwrap_or("north");

    let model_box = if open {
        match facing {
            "south" => ModelBox::new([0, 0, 13], [16, 16, 16]),
            "west" => ModelBox::new([0, 0, 0], [3, 16, 16]),
            "east" => ModelBox::new([13, 0, 0], [16, 16, 16]),
            _ => ModelBox::new([0, 0, 0], [16, 16, 3]),
        }
    } else if top {
        ModelBox::new([0, 13, 0], [16, 16, 16])
    } else {
        ModelBox::new([0, 0, 0], [16, 3, 16])
    };

    build_model(&[model_box], Vec::new())
}

fn door(state: &BlockStateKey) -> BlockModel {
    let facing = prop(state, "facing").unwrap_or("north");
    let open = bool_prop(state, "open");

    let model_box = if open {
        match facing {
            "south" => ModelBox::new([13, 0, 0], [16, 16, 16]),
            "west" => ModelBox::new([0, 0, 13], [16, 16, 16]),
            "east" => ModelBox::new([0, 0, 0], [16, 16, 3]),
            _ => ModelBox::new([0, 0, 0], [3, 16, 16]),
        }
    } else {
        match facing {
            "south" => ModelBox::new([0, 0, 13], [16, 16, 16]),
            "west" => ModelBox::new([0, 0, 0], [3, 16, 16]),
            "east" => ModelBox::new([13, 0, 0], [16, 16, 16]),
            _ => ModelBox::new([0, 0, 0], [16, 16, 3]),
        }
    };

    build_model(&[model_box], Vec::new())
}

fn fence(state: &BlockStateKey) -> BlockModel {
    let mut builder = ShapeBuilder::new();
    builder.add([6, 0, 6], [10, 16, 10]);

    for (direction, arm) in [
        ("north", ([6, 6, 0], [10, 9, 6])),
        ("south", ([6, 6, 10], [10, 9, 16])),
        ("west", ([0, 6, 6], [6, 9, 10])),
        ("east", ([10, 6, 6], [16, 9, 10])),
    ] {
        if bool_prop(state, direction) {
            builder.add(arm.0, arm.1);
            builder.add(
                [arm.0[0], arm.0[1] + 6, arm.0[2]],
                [arm.1[0], arm.1[1] + 6, arm.1[2]],
            );
        }
    }

    builder.build()
}

fn fence_gate(state: &BlockStateKey) -> BlockModel {
    let open = bool_prop(state, "open");
    let facing = prop(state, "facing").unwrap_or("north");
    let mut builder = ShapeBuilder::new();

    builder.add([6, 0, 6], [10, 16, 10]);
    if open {
        if facing == "north" || facing == "south" {
            builder.add([6, 6, 0], [10, 15, 16]);
        } else {
            builder.add([0, 6, 6], [16, 15, 10]);
        }
    } else if facing == "north" || facing == "south" {
        builder.add([0, 6, 6], [16, 15, 10]);
    } else {
        builder.add([6, 6, 0], [10, 15, 16]);
    }

    builder.build()
}

fn wall(state: &BlockStateKey) -> BlockModel {
    let up = bool_prop(state, "up");
    let mut builder = ShapeBuilder::new();
    let post_height = if up { 16 } else { 14 };
    builder.add([4, 0, 4], [12, post_height, 12]);

    for (direction, arm) in [
        ("north", ([4, 0, 0], [12, 14, 4])),
        ("south", ([4, 0, 12], [12, 14, 16])),
        ("west", ([0, 0, 4], [4, 14, 12])),
        ("east", ([12, 0, 4], [16, 14, 12])),
    ] {
        if prop(state, direction).is_some_and(|value| value != "none") {
            builder.add(arm.0, arm.1);
        }
    }

    builder.build()
}

fn pane(state: &BlockStateKey) -> BlockModel {
    let mut builder = ShapeBuilder::new();
    builder.add([7, 0, 7], [9, 16, 9]);

    let mut connected = false;
    for (direction, arm) in [
        ("north", ([7, 0, 0], [9, 16, 7])),
        ("south", ([7, 0, 9], [9, 16, 16])),
        ("west", ([0, 0, 7], [7, 16, 9])),
        ("east", ([9, 0, 7], [16, 16, 9])),
    ] {
        if bool_prop(state, direction) {
            builder.add(arm.0, arm.1);
            connected = true;
        }
    }

    if !connected {
        builder.add([7, 0, 0], [9, 16, 16]);
    }

    builder.build()
}

fn button(state: &BlockStateKey) -> BlockModel {
    let face = prop(state, "face").unwrap_or("floor");
    let facing = prop(state, "facing").unwrap_or("north");

    let model_box = match face {
        "ceiling" => ModelBox::new([5, 14, 5], [11, 16, 11]),
        "wall" => match facing {
            "south" => ModelBox::new([5, 5, 14], [11, 11, 16]),
            "west" => ModelBox::new([0, 5, 5], [2, 11, 11]),
            "east" => ModelBox::new([14, 5, 5], [16, 11, 11]),
            _ => ModelBox::new([5, 5, 0], [11, 11, 2]),
        },
        _ => ModelBox::new([5, 0, 5], [11, 2, 11]),
    };

    build_model(&[model_box], Vec::new())
}

fn torch(state: &BlockStateKey) -> BlockModel {
    let path = strip_namespace(&state.name);
    let facing = prop(state, "facing").unwrap_or("north");

    let model_box = if path.contains("wall_torch") {
        match facing {
            "south" => ModelBox::new([7, 3, 14], [9, 13, 16]),
            "west" => ModelBox::new([0, 3, 7], [2, 13, 9]),
            "east" => ModelBox::new([14, 3, 7], [16, 13, 9]),
            _ => ModelBox::new([7, 3, 0], [9, 13, 2]),
        }
    } else {
        ModelBox::new([7, 0, 7], [9, 10, 9])
    };

    build_model(&[model_box], Vec::new())
}

fn ladder(state: &BlockStateKey) -> BlockModel {
    let facing = prop(state, "facing").unwrap_or("north");
    let model_box = match facing {
        "south" => ModelBox::new([0, 0, 14], [16, 16, 16]),
        "west" => ModelBox::new([0, 0, 0], [2, 16, 16]),
        "east" => ModelBox::new([14, 0, 0], [16, 16, 16]),
        _ => ModelBox::new([0, 0, 0], [16, 16, 2]),
    };
    build_model(&[model_box], Vec::new())
}

fn single_box(min: [u8; 3], max: [u8; 3]) -> BlockModel {
    build_model(&[ModelBox::new(min, max)], Vec::new())
}

fn fluid() -> BlockModel {
    // Template only: the mesh builder replaces this box with a surface whose
    // four corners follow the neighbouring fluid columns.
    single_box([0, 0, 0], [16, 14, 16])
}

fn redstone_wire() -> BlockModel {
    let up_vertices = [
        [0.0, 0.25, 0.0],
        [0.0, 0.25, 16.0],
        [16.0, 0.25, 16.0],
        [16.0, 0.25, 0.0],
    ];
    let down_vertices = [
        [0.0, 0.25, 16.0],
        [0.0, 0.25, 0.0],
        [16.0, 0.25, 0.0],
        [16.0, 0.25, 16.0],
    ];

    let quad = |vertices: [[f32; 3]; 4], face: FaceDirection| ModelQuad {
        vertices,
        normal: face.normal(),
        face,
        cullface: None,
        uv: face_uv(face, vertices),
        texture: NO_TEXTURE,
        tint: Rgba::WHITE,
        layer: None,
        shade: false,
    };

    build_model(
        &[],
        vec![
            quad(up_vertices, FaceDirection::Up),
            quad(down_vertices, FaceDirection::Down),
        ],
    )
}

fn lectern() -> BlockModel {
    let mut builder = ShapeBuilder::new();
    builder.add([0, 0, 0], [16, 4, 16]);
    builder.add([4, 4, 4], [12, 12, 12]);
    builder.build()
}

fn is_cross_plant(path: &str) -> bool {
    matches!(
        path,
        "short_grass"
            | "tall_grass"
            | "fern"
            | "large_fern"
            | "dandelion"
            | "poppy"
            | "blue_orchid"
            | "allium"
            | "azure_bluet"
            | "oxeye_daisy"
            | "cornflower"
            | "lily_of_the_valley"
            | "red_tulip"
            | "orange_tulip"
            | "white_tulip"
            | "pink_tulip"
            | "sunflower"
            | "lilac"
            | "rose_bush"
            | "peony"
            | "vine"
            | "cobweb"
            | "bamboo"
            | "dead_bush"
            | "seagrass"
            | "kelp"
            | "kelp_plant"
            | "sugar_cane"
            | "wheat"
            | "carrots"
            | "potatoes"
            | "beetroots"
            | "torchflower"
            | "pitcher_plant"
            | "bush"
    )
}

fn cross_quads() -> Vec<ModelQuad> {
    let mut quads = Vec::with_capacity(4);

    let plane_a = [
        [0.0, 0.0, 0.0],
        [16.0, 0.0, 16.0],
        [16.0, 16.0, 16.0],
        [0.0, 16.0, 0.0],
    ];
    let normal_a = normalized([-1.0, 0.0, 1.0]);
    quads.push(cross_quad(plane_a, normal_a));
    quads.push(cross_quad(reverse(plane_a), negate(normal_a)));

    let plane_b = [
        [0.0, 0.0, 16.0],
        [16.0, 0.0, 0.0],
        [16.0, 16.0, 0.0],
        [0.0, 16.0, 16.0],
    ];
    let normal_b = normalized([1.0, 0.0, 1.0]);
    quads.push(cross_quad(plane_b, normal_b));
    quads.push(cross_quad(reverse(plane_b), negate(normal_b)));

    quads
}

fn cross_quad(vertices: [[f32; 3]; 4], normal: [f32; 3]) -> ModelQuad {
    ModelQuad {
        vertices,
        normal,
        face: FaceDirection::Up,
        cullface: None,
        uv: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        texture: NO_TEXTURE,
        tint: Rgba::WHITE,
        layer: None,
        shade: true,
    }
}

fn reverse(vertices: [[f32; 3]; 4]) -> [[f32; 3]; 4] {
    [vertices[3], vertices[2], vertices[1], vertices[0]]
}

fn negate(value: [f32; 3]) -> [f32; 3] {
    [-value[0], -value[1], -value[2]]
}

fn normalized(value: [f32; 3]) -> [f32; 3] {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    [value[0] / length, value[1] / length, value[2] / length]
}

fn prop<'a>(state: &'a BlockStateKey, key: &str) -> Option<&'a str> {
    state.properties.get(key).map(String::as_str)
}

fn bool_prop(state: &BlockStateKey, key: &str) -> bool {
    prop(state, key) == Some("true")
}

struct ShapeBuilder {
    boxes: Vec<ModelBox>,
}

impl ShapeBuilder {
    fn new() -> Self {
        Self { boxes: Vec::new() }
    }

    fn add(&mut self, min: [u8; 3], max: [u8; 3]) {
        self.boxes.push(ModelBox::new(min, max));
    }

    fn build(self) -> BlockModel {
        build_model(&self.boxes, Vec::new())
    }
}
