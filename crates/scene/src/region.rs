use crate::{BlockEntity, BlockStateKey, PackedIndices};

/// A canonical scene region in its own local coordinate system.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub name: String,
    pub origin: [i64; 3],
    pub size: [u32; 3],
    pub data_version: i32,
    pub palette: Vec<BlockStateKey>,
    pub states: PackedIndices,
    pub block_entities: Vec<BlockEntity>,
    pub entity_count: usize,
    pub pending_block_tick_count: usize,
    pub pending_fluid_tick_count: usize,
}

impl Region {
    pub fn volume(&self) -> u64 {
        self.states.volume()
    }

    pub fn max_exclusive(&self) -> Option<[i64; 3]> {
        let mut result = [0i64; 3];
        for (axis, value) in result.iter_mut().enumerate() {
            *value = self.origin[axis].checked_add(i64::from(self.size[axis]))?;
        }
        Some(result)
    }
}
