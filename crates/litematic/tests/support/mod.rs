#![allow(dead_code)]

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

pub fn empty_list(name: &str, element_type: u8) -> Vec<u8> {
    named(9, name, &[element_type, 0, 0, 0, 0])
}

pub fn list_of_compounds(name: &str, entries: &[Vec<u8>]) -> Vec<u8> {
    let mut payload = vec![10];
    payload.extend_from_slice(&(entries.len() as i32).to_be_bytes());
    for entry in entries {
        payload.extend_from_slice(entry);
    }
    named(9, name, &payload)
}

pub fn int_list(name: &str, values: &[i32]) -> Vec<u8> {
    let mut payload = vec![3];
    payload.extend_from_slice(&(values.len() as i32).to_be_bytes());
    for value in values {
        payload.extend_from_slice(&value.to_be_bytes());
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

pub fn palette_entry(name: &str) -> Vec<u8> {
    compound_payload(&string("Name", name))
}

pub fn palette_entry_with_properties(name: &str, properties: &[(&str, &str)]) -> Vec<u8> {
    let mut children = Vec::new();
    children.extend(string("Name", name));

    if !properties.is_empty() {
        let mut property_children = Vec::new();
        for (key, value) in properties {
            property_children.extend(string(key, value));
        }
        children.extend(compound("Properties", &property_children));
    }

    compound_payload(&children)
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
