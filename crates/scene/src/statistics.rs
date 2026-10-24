use crate::BlockStateKey;
use crate::Region;
use crate::error::{SceneError, SceneErrorCode};
use std::collections::BTreeMap;

/// Material statistics computed directly from canonical block states.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockStatistics {
    pub volume: u64,
    pub air: u64,
    pub non_air: u64,
    pub blocks: BTreeMap<BlockStateKey, u64>,
    pub block_entities: u64,
    pub entities: u64,
    pub pending_block_ticks: u64,
    pub pending_fluid_ticks: u64,
}

impl BlockStatistics {
    pub fn from_regions(regions: &[Region]) -> Result<Self, SceneError> {
        let mut statistics = Self::default();

        for region in regions {
            let mut counts = vec![0u64; region.palette.len()];
            for value in region.states.iter() {
                let index = usize::try_from(value).map_err(|_| {
                    SceneError::new(
                        SceneErrorCode::StatisticsOverflow,
                        "palette index does not fit in usize",
                    )
                })?;
                let slot = counts.get_mut(index).ok_or_else(|| {
                    SceneError::new(
                        SceneErrorCode::StatisticsOverflow,
                        "palette index outside region palette",
                    )
                })?;
                *slot = slot.checked_add(1).ok_or_else(|| {
                    SceneError::new(SceneErrorCode::StatisticsOverflow, "block count overflow")
                })?;
            }

            statistics.volume = statistics
                .volume
                .checked_add(region.states.volume())
                .ok_or_else(|| {
                    SceneError::new(SceneErrorCode::StatisticsOverflow, "total volume overflow")
                })?;

            for (state, count) in region.palette.iter().zip(counts) {
                if count == 0 {
                    continue;
                }
                if state.is_air() {
                    statistics.air = statistics.air.checked_add(count).ok_or_else(|| {
                        SceneError::new(SceneErrorCode::StatisticsOverflow, "air count overflow")
                    })?;
                }

                let entry = statistics.blocks.entry(state.clone()).or_insert(0);
                *entry = entry.checked_add(count).ok_or_else(|| {
                    SceneError::new(
                        SceneErrorCode::StatisticsOverflow,
                        "block state count overflow",
                    )
                })?;
            }

            statistics.block_entities = statistics
                .block_entities
                .checked_add(region.block_entities.len() as u64)
                .ok_or_else(|| {
                    SceneError::new(
                        SceneErrorCode::StatisticsOverflow,
                        "block entity count overflow",
                    )
                })?;
            statistics.entities = statistics
                .entities
                .checked_add(region.entity_count as u64)
                .ok_or_else(|| {
                    SceneError::new(SceneErrorCode::StatisticsOverflow, "entity count overflow")
                })?;
            statistics.pending_block_ticks = statistics
                .pending_block_ticks
                .checked_add(region.pending_block_tick_count as u64)
                .ok_or_else(|| {
                    SceneError::new(
                        SceneErrorCode::StatisticsOverflow,
                        "block tick count overflow",
                    )
                })?;
            statistics.pending_fluid_ticks = statistics
                .pending_fluid_ticks
                .checked_add(region.pending_fluid_tick_count as u64)
                .ok_or_else(|| {
                    SceneError::new(
                        SceneErrorCode::StatisticsOverflow,
                        "fluid tick count overflow",
                    )
                })?;
        }

        statistics.non_air = statistics
            .volume
            .checked_sub(statistics.air)
            .ok_or_else(|| {
                SceneError::new(
                    SceneErrorCode::StatisticsOverflow,
                    "air count exceeds total volume",
                )
            })?;

        Ok(statistics)
    }
}
