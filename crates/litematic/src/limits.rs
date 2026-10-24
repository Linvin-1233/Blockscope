/// Limits applied while validating a litematic container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LitematicLimits {
    pub max_regions: usize,
    pub max_volume_per_region: u64,
    pub max_total_volume: u64,
    pub max_palette_entries: usize,
    pub max_block_entities: usize,
    pub max_entities: usize,
    pub max_pending_ticks: usize,
    pub max_coordinate: i64,
}

impl Default for LitematicLimits {
    fn default() -> Self {
        Self {
            max_regions: 1024,
            max_volume_per_region: 100_000_000,
            max_total_volume: 200_000_000,
            max_palette_entries: 1_000_000,
            max_block_entities: 1_000_000,
            max_entities: 1_000_000,
            max_pending_ticks: 1_000_000,
            max_coordinate: 30_000_000,
        }
    }
}

impl LitematicLimits {
    pub fn small() -> Self {
        Self {
            max_regions: 4,
            max_volume_per_region: 64,
            max_total_volume: 128,
            max_palette_entries: 8,
            max_block_entities: 8,
            max_entities: 8,
            max_pending_ticks: 8,
            max_coordinate: 1000,
        }
    }
}
