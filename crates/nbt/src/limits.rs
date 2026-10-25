/// Hard limits applied before and during NBT decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeLimits {
    pub max_input_bytes: usize,
    pub max_decompressed_bytes: usize,
    pub max_depth: usize,
    pub max_tags: usize,
    pub max_string_bytes: usize,
    pub max_list_len: usize,
    pub max_array_len: usize,
    pub max_total_bytes: usize,
    /// Skip `TileEntities` payloads while decoding. Rendering does not use
    /// container contents. If the stream ends inside the skipped payload, the
    /// block data parsed before it is kept and the missing block entities are
    /// treated as an empty list.
    pub ignore_block_entity_nbt: bool,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024,
            max_decompressed_bytes: 256 * 1024 * 1024,
            max_depth: 64,
            max_tags: 4_000_000,
            max_string_bytes: 1024 * 1024,
            max_list_len: 4_000_000,
            max_array_len: 64_000_000,
            max_total_bytes: 512 * 1024 * 1024,
            ignore_block_entity_nbt: false,
        }
    }
}

impl DecodeLimits {
    /// Limits used by tests that need to exercise a specific guard cheaply.
    pub fn small() -> Self {
        Self {
            max_input_bytes: 4096,
            max_decompressed_bytes: 8192,
            max_depth: 8,
            max_tags: 128,
            max_string_bytes: 64,
            max_list_len: 32,
            max_array_len: 64,
            max_total_bytes: 16_384,
            ignore_block_entity_nbt: false,
        }
    }
}
