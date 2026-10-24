use crate::{BlockStateKey, Scene};
use litematic_nbt::{NbtCompound, NbtTagType, NbtValue};
use sha2::{Digest, Sha256};

/// A SHA-256 digest with stable hexadecimal formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            use std::fmt::Write;
            write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
        }
        output
    }
}

pub fn sha256(bytes: &[u8]) -> Sha256Digest {
    let digest = Sha256::digest(bytes);
    let mut output = [0u8; 32];
    output.copy_from_slice(&digest);
    Sha256Digest(output)
}

impl Scene {
    /// Hash the render-relevant scene contents. Metadata text is excluded.
    pub fn scene_hash(&self) -> Sha256Digest {
        let mut hasher = Sha256::new();
        hasher.update(b"litematic-scene-v1");
        hash_i32(&mut hasher, self.metadata.litematic_version);
        hash_i32(&mut hasher, self.metadata.minecraft_data_version);

        for axis in 0..3 {
            hash_i64(&mut hasher, self.bounds.min[axis]);
            hash_i64(&mut hasher, self.bounds.max_exclusive[axis]);
        }

        hash_u64(&mut hasher, self.regions.len() as u64);
        for region in &self.regions {
            hash_string(&mut hasher, &region.name);
            for axis in 0..3 {
                hash_i64(&mut hasher, region.origin[axis]);
                hash_u32(&mut hasher, region.size[axis]);
            }
            hash_i32(&mut hasher, region.data_version);

            hash_u64(&mut hasher, region.palette.len() as u64);
            for state in &region.palette {
                hash_block_state(&mut hasher, state);
            }

            hash_u8(&mut hasher, region.states.bits());
            hash_u64(&mut hasher, region.states.volume());
            hash_u64(&mut hasher, region.states.longs().len() as u64);
            for value in region.states.longs() {
                hash_u64(&mut hasher, *value as u64);
            }

            hash_u64(&mut hasher, region.block_entities.len() as u64);
            for entity in &region.block_entities {
                for axis in 0..3 {
                    hash_u32(&mut hasher, entity.local_pos[axis]);
                }
                hash_string(&mut hasher, &entity.id);
                hash_nbt_compound(&mut hasher, &entity.data);
            }

            hash_u64(&mut hasher, region.entity_count as u64);
            hash_u64(&mut hasher, region.pending_block_tick_count as u64);
            hash_u64(&mut hasher, region.pending_fluid_tick_count as u64);
        }

        let digest = hasher.finalize();
        let mut output = [0u8; 32];
        output.copy_from_slice(&digest);
        Sha256Digest(output)
    }
}

fn hash_block_state(hasher: &mut Sha256, state: &BlockStateKey) {
    hash_string(hasher, &state.name);
    hash_u64(hasher, state.properties.len() as u64);
    for (key, value) in &state.properties {
        hash_string(hasher, key);
        hash_string(hasher, value);
    }
}

fn hash_nbt(hasher: &mut Sha256, value: &NbtValue) {
    hash_u8(hasher, value.tag_type().as_u8());

    match value {
        NbtValue::Byte(value) => hasher.update([*value as u8]),
        NbtValue::Short(value) => hasher.update(value.to_be_bytes()),
        NbtValue::Int(value) => hasher.update(value.to_be_bytes()),
        NbtValue::Long(value) => hasher.update(value.to_be_bytes()),
        NbtValue::Float(value) => hasher.update(value.to_bits().to_be_bytes()),
        NbtValue::Double(value) => hasher.update(value.to_bits().to_be_bytes()),
        NbtValue::ByteArray(values) => {
            hash_u64(hasher, values.len() as u64);
            hasher.update(values);
        }
        NbtValue::String(value) => hash_string(hasher, value),
        NbtValue::List(list) => {
            hash_u8(hasher, list.element_type.as_u8());
            hash_u64(hasher, list.values.len() as u64);
            for value in &list.values {
                hash_nbt(hasher, value);
            }
        }
        NbtValue::Compound(compound) => hash_compound_entries(hasher, compound),
        NbtValue::IntArray(values) => {
            hash_u64(hasher, values.len() as u64);
            for value in values {
                hasher.update(value.to_be_bytes());
            }
        }
        NbtValue::LongArray(values) => {
            hash_u64(hasher, values.len() as u64);
            for value in values {
                hasher.update(value.to_be_bytes());
            }
        }
    }
}

fn hash_nbt_compound(hasher: &mut Sha256, compound: &NbtCompound) {
    hash_u8(hasher, NbtTagType::Compound.as_u8());
    hash_compound_entries(hasher, compound);
}

fn hash_compound_entries(hasher: &mut Sha256, compound: &NbtCompound) {
    hash_u64(hasher, compound.entries.len() as u64);
    for (key, value) in &compound.entries {
        hash_string(hasher, key);
        hash_nbt(hasher, value);
    }
}

fn hash_string(hasher: &mut Sha256, value: &str) {
    hash_u64(hasher, value.len() as u64);
    hasher.update(value.as_bytes());
}

fn hash_u8(hasher: &mut Sha256, value: u8) {
    hasher.update([value]);
}

fn hash_u32(hasher: &mut Sha256, value: u32) {
    hasher.update(value.to_be_bytes());
}

fn hash_u64(hasher: &mut Sha256, value: u64) {
    hasher.update(value.to_be_bytes());
}

fn hash_i32(hasher: &mut Sha256, value: i32) {
    hasher.update(value.to_be_bytes());
}

fn hash_i64(hasher: &mut Sha256, value: i64) {
    hasher.update(value.to_be_bytes());
}
