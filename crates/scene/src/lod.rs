use crate::{BlockStateKey, PackedIndices, Region, Scene, SceneError, SceneErrorCode};
use std::collections::HashMap;

const MIN_FACTOR: u32 = 2;
const MAX_FACTOR: u32 = 16;

/// Aggregate a scene into `factor`-sized cells for level-of-detail rendering.
///
/// Every output cell covers a `factor` cube of input blocks. Air is ignored
/// unless the whole cell is air; among non-air states the most frequent wins,
/// and ties are broken by the canonical state order. Overlapping regions use
/// the same priority as the mesh builder: later regions win.
///
/// The result is an ordinary [`Scene`] in LOD block units, so the mesh builder
/// and renderer need no special cases. Block entities, entities, and pending
/// ticks are dropped, and the palette is rebuilt in first-seen order.
pub fn aggregate(scene: &Scene, factor: u32) -> Result<Scene, SceneError> {
    if !(MIN_FACTOR..=MAX_FACTOR).contains(&factor) {
        return Err(SceneError::new(
            SceneErrorCode::InvalidLodFactor,
            format!("lod factor must be between {MIN_FACTOR} and {MAX_FACTOR}, got {factor}"),
        ));
    }

    let factor = i64::from(factor);
    let min_cell = [
        scene.bounds.min[0].div_euclid(factor),
        scene.bounds.min[1].div_euclid(factor),
        scene.bounds.min[2].div_euclid(factor),
    ];
    let max_cell = [
        ceil_div(scene.bounds.max_exclusive[0], factor),
        ceil_div(scene.bounds.max_exclusive[1], factor),
        ceil_div(scene.bounds.max_exclusive[2], factor),
    ];
    let size = [
        u32::try_from(max_cell[0] - min_cell[0]).map_err(size_overflow)?,
        u32::try_from(max_cell[1] - min_cell[1]).map_err(size_overflow)?,
        u32::try_from(max_cell[2] - min_cell[2]).map_err(size_overflow)?,
    ];
    let volume = u64::from(size[0]) * u64::from(size[1]) * u64::from(size[2]);
    let capacity = usize::try_from(volume).map_err(size_overflow)?;

    let air = BlockStateKey::simple("minecraft:air");
    let mut values = Vec::with_capacity(capacity);
    let mut palette: Vec<BlockStateKey> = Vec::new();
    let mut palette_index: HashMap<BlockStateKey, u32> = HashMap::new();
    let mut counts: Vec<(&BlockStateKey, u32)> = Vec::new();

    for y in 0..size[1] {
        for z in 0..size[2] {
            for x in 0..size[0] {
                let base = [
                    (min_cell[0] + i64::from(x)) * factor,
                    (min_cell[1] + i64::from(y)) * factor,
                    (min_cell[2] + i64::from(z)) * factor,
                ];

                counts.clear();
                for dy in 0..factor {
                    for dz in 0..factor {
                        for dx in 0..factor {
                            let world = [base[0] + dx, base[1] + dy, base[2] + dz];
                            let Some(state) = resolve(scene, world) else {
                                continue;
                            };
                            if state.is_air() {
                                continue;
                            }

                            match counts.iter_mut().find(|(key, _)| **key == *state) {
                                Some((_, count)) => *count += 1,
                                None => counts.push((state, 1)),
                            }
                        }
                    }
                }

                let chosen = counts
                    .iter()
                    .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(left.0)))
                    .map(|(state, _)| (*state).clone())
                    .unwrap_or_else(|| air.clone());

                let index = match palette_index.get(&chosen) {
                    Some(index) => *index,
                    None => {
                        let index = u32::try_from(palette.len()).map_err(size_overflow)?;
                        palette.push(chosen.clone());
                        palette_index.insert(chosen, index);
                        index
                    }
                };
                values.push(index);
            }
        }
    }

    let bits = PackedIndices::bits_for_palette(palette.len());
    let states = PackedIndices::from_values(bits, &values).ok_or_else(|| {
        SceneError::new(
            SceneErrorCode::LodAggregationFailed,
            "aggregated palette did not fit its bit width",
        )
    })?;

    let region = Region {
        name: "lod".to_string(),
        origin: min_cell,
        size,
        data_version: scene.metadata.minecraft_data_version,
        palette,
        states,
        block_entities: Vec::new(),
        entity_count: 0,
        pending_block_tick_count: 0,
        pending_fluid_tick_count: 0,
    };

    Scene::new(scene.metadata.clone(), vec![region], Vec::new())
}

fn resolve(scene: &Scene, world: [i64; 3]) -> Option<&BlockStateKey> {
    for region in scene.regions.iter().rev() {
        let local = [
            world[0] - region.origin[0],
            world[1] - region.origin[1],
            world[2] - region.origin[2],
        ];
        if local
            .iter()
            .enumerate()
            .any(|(axis, value)| *value < 0 || *value >= i64::from(region.size[axis]))
        {
            continue;
        }

        let size_x = u64::from(region.size[0]);
        let size_z = u64::from(region.size[2]);
        let index = u64::try_from(local[1]).ok()? * size_x * size_z
            + u64::try_from(local[2]).ok()? * size_x
            + u64::try_from(local[0]).ok()?;
        let palette_id = usize::try_from(region.states.get(index)).ok()?;
        return region.palette.get(palette_id);
    }

    None
}

fn ceil_div(value: i64, divisor: i64) -> i64 {
    let quotient = value.div_euclid(divisor);
    if value.rem_euclid(divisor) == 0 {
        quotient
    } else {
        quotient + 1
    }
}

fn size_overflow<E: std::fmt::Display>(error: E) -> SceneError {
    SceneError::new(
        SceneErrorCode::BoundsOverflow,
        format!("aggregated scene dimensions do not fit u32: {error}"),
    )
}
