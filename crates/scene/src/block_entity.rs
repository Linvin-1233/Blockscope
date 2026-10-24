use litematic_nbt::NbtCompound;

/// A block entity stored in region-local coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockEntity {
    pub local_pos: [u32; 3],
    pub id: String,
    pub data: NbtCompound,
}

impl BlockEntity {
    pub fn world_pos(&self, region_origin: [i64; 3]) -> [i64; 3] {
        [
            region_origin[0] + i64::from(self.local_pos[0]),
            region_origin[1] + i64::from(self.local_pos[1]),
            region_origin[2] + i64::from(self.local_pos[2]),
        ]
    }
}
