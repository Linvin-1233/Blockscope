#![allow(dead_code)]

use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::{Command, Output};
use tempfile::NamedTempFile;

pub fn named(tag: u8, name: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(payload);
    out
}

pub fn root(children: &[u8]) -> Vec<u8> {
    let mut out = vec![10, 0, 0];
    out.extend_from_slice(children);
    out.push(0);
    out
}

pub fn int(name: &str, value: i32) -> Vec<u8> {
    named(3, name, &value.to_be_bytes())
}

pub fn long(name: &str, value: i64) -> Vec<u8> {
    named(4, name, &value.to_be_bytes())
}

pub fn string(name: &str, value: &str) -> Vec<u8> {
    let mut payload = (value.len() as u16).to_be_bytes().to_vec();
    payload.extend_from_slice(value.as_bytes());
    named(8, name, &payload)
}

pub fn compound_payload(children: &[u8]) -> Vec<u8> {
    let mut payload = children.to_vec();
    payload.push(0);
    payload
}

pub fn compound(name: &str, children: &[u8]) -> Vec<u8> {
    named(10, name, &compound_payload(children))
}

pub fn list_of_compounds(name: &str, entries: &[Vec<u8>]) -> Vec<u8> {
    let mut payload = vec![10];
    payload.extend_from_slice(&(entries.len() as i32).to_be_bytes());
    for entry in entries {
        payload.extend_from_slice(entry);
    }
    named(9, name, &payload)
}

pub fn long_array(name: &str, values: &[i64]) -> Vec<u8> {
    let mut payload = (values.len() as i32).to_be_bytes().to_vec();
    for value in values {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    named(12, name, &payload)
}

pub fn vec3i(name: &str, x: i32, y: i32, z: i32) -> Vec<u8> {
    let mut children = Vec::new();
    children.extend(int("x", x));
    children.extend(int("y", y));
    children.extend(int("z", z));
    compound(name, &children)
}

pub fn empty_compound_list(name: &str) -> Vec<u8> {
    named(9, name, &[10, 0, 0, 0, 0])
}

pub fn palette_entry(name: &str) -> Vec<u8> {
    compound_payload(&string("Name", name))
}

#[derive(Default)]
pub struct RegionLists {
    pub tile_entities: Vec<Vec<u8>>,
    pub entities: Vec<Vec<u8>>,
    pub pending_block_ticks: Vec<Vec<u8>>,
    pub pending_fluid_ticks: Vec<Vec<u8>>,
}

pub fn region(
    name: &str,
    position: [i32; 3],
    size: [i32; 3],
    data_version: Option<i32>,
    block_states: &[i64],
) -> Vec<u8> {
    region_with_palette(
        name,
        position,
        size,
        data_version,
        &[palette_entry("minecraft:air")],
        block_states,
    )
}

pub fn region_with_palette(
    name: &str,
    position: [i32; 3],
    size: [i32; 3],
    data_version: Option<i32>,
    palette: &[Vec<u8>],
    block_states: &[i64],
) -> Vec<u8> {
    region_with_palette_and_lists(
        name,
        position,
        size,
        data_version,
        palette,
        block_states,
        &RegionLists::default(),
    )
}

pub fn region_with_palette_and_lists(
    name: &str,
    position: [i32; 3],
    size: [i32; 3],
    data_version: Option<i32>,
    palette: &[Vec<u8>],
    block_states: &[i64],
    lists: &RegionLists,
) -> Vec<u8> {
    let mut children = Vec::new();
    children.extend(vec3i("Position", position[0], position[1], position[2]));
    children.extend(vec3i("Size", size[0], size[1], size[2]));
    children.extend(list_of_compounds("BlockStatePalette", palette));
    children.extend(long_array("BlockStates", block_states));
    children.extend(list_of_compounds("TileEntities", &lists.tile_entities));
    children.extend(list_of_compounds("Entities", &lists.entities));
    children.extend(list_of_compounds(
        "PendingBlockTicks",
        &lists.pending_block_ticks,
    ));
    children.extend(list_of_compounds(
        "PendingFluidTicks",
        &lists.pending_fluid_ticks,
    ));
    if let Some(version) = data_version {
        children.extend(int("DataVersion", version));
    }
    compound(name, &children)
}

pub fn metadata(
    name: &str,
    enclosing_size: [i32; 3],
    region_count: i32,
    total_volume: i64,
    total_blocks: i64,
) -> Vec<u8> {
    let mut children = Vec::new();
    children.extend(string("Name", name));
    children.extend(vec3i(
        "EnclosingSize",
        enclosing_size[0],
        enclosing_size[1],
        enclosing_size[2],
    ));
    children.extend(int("RegionCount", region_count));
    children.extend(long("TotalVolume", total_volume));
    children.extend(long("TotalBlocks", total_blocks));
    children
}

pub fn litematic(
    version: i32,
    sub_version: Option<i32>,
    data_version: i32,
    metadata_children: &[u8],
    regions: &[Vec<u8>],
) -> Vec<u8> {
    let mut children = Vec::new();
    children.extend(int("Version", version));
    if let Some(sub_version) = sub_version {
        children.extend(int("SubVersion", sub_version));
    }
    children.extend(int("MinecraftDataVersion", data_version));
    children.extend(compound("Metadata", metadata_children));
    children.extend(compound("Regions", &regions.concat()));
    root(&children)
}

pub fn basic_fixture() -> Vec<u8> {
    let region = region_with_palette(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        None,
        &[
            palette_entry("minecraft:air"),
            palette_entry("minecraft:stone"),
        ],
        &[4],
    );
    litematic(
        7,
        None,
        3700,
        &metadata("cli-fixture", [2, 1, 1], 1, 2, 1),
        &[region],
    )
}

pub fn v1_block_entity(id: &str, position: [i32; 3]) -> Vec<u8> {
    let mut tile_children = Vec::new();
    tile_children.extend(string("id", id));
    let tile_nbt = compound_payload(&tile_children);

    let mut wrapper = Vec::new();
    wrapper.extend(named(10, "TileNBT", &tile_nbt));
    wrapper.extend(int("x", position[0]));
    wrapper.extend(int("y", position[1]));
    wrapper.extend(int("z", position[2]));
    compound_payload(&wrapper)
}

pub fn write_fixture(bytes: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
    file
}

pub fn stone_pair_fixture() -> Vec<u8> {
    let region = region_with_palette(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        None,
        &[
            palette_entry("minecraft:air"),
            palette_entry("minecraft:stone"),
        ],
        &[5],
    );
    litematic(
        7,
        None,
        3700,
        &metadata("cli-greedy", [2, 1, 1], 1, 2, 2),
        &[region],
    )
}

pub fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_blockscope"))
        .args(args)
        .output()
        .unwrap()
}

pub fn write_material_pack(root: &Path) {
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
            "elements": [{
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
            }]
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

    let file = fs::File::create(assets.join("textures/block/test_texture.png")).unwrap();
    let mut encoder = png::Encoder::new(BufWriter::new(file), 16, 16);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[200, 40, 40, 255].repeat(16 * 16))
        .unwrap();
    writer.finish().unwrap();
}
