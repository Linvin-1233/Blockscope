use crate::{BlockStateKey, PackedIndices, Region, Scene, SceneError, SceneErrorCode};
use std::collections::HashMap;

const MIN_FACTOR: u32 = 2;
const MAX_FACTOR: u32 = 16;
const AIR: u32 = u32::MAX;

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
    aggregate_with_threads(scene, factor, 0)
}

/// Aggregate with an explicit worker count. `threads == 0` uses the available
/// parallelism. The result is identical for every worker count.
pub fn aggregate_with_threads(
    scene: &Scene,
    factor: u32,
    threads: u32,
) -> Result<Scene, SceneError> {
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

    let table = build_state_table(scene);
    let air = BlockStateKey::simple("minecraft:air");
    // Chosen global state id per output cell, in y/z/x order. Filled in
    // parallel over y, then rewritten with palette indices in one serial pass
    // so the palette order stays identical to the single-threaded path.
    let mut values = vec![AIR; capacity];

    let fallback = if threads == 0 {
        std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
    } else {
        threads as usize
    };
    let workers = fallback.clamp(1, size[1].max(1) as usize);
    let plane = size[0] as usize * size[2] as usize;

    if workers <= 1 || capacity < 8192 || plane == 0 {
        fill_rows(
            scene,
            &table,
            factor,
            min_cell,
            size,
            0,
            size[1],
            &mut values,
        );
    } else {
        let rows_per_chunk = (size[1] as usize).div_ceil(workers);
        let chunk_cells = rows_per_chunk * plane;
        std::thread::scope(|scope| {
            let handles: Vec<_> = values
                .chunks_mut(chunk_cells)
                .enumerate()
                .map(|(chunk_index, chunk)| {
                    let start_y =
                        u32::try_from(chunk_index * rows_per_chunk).expect("chunk row fits u32");
                    let rows = u32::try_from(chunk.len() / plane).expect("chunk rows fit u32");
                    let table = &table;
                    scope.spawn(move || {
                        fill_rows(scene, table, factor, min_cell, size, start_y, rows, chunk);
                    })
                })
                .collect();
            for handle in handles {
                handle.join().expect("lod aggregation worker panicked");
            }
        });
    }

    let mut palette: Vec<BlockStateKey> = Vec::new();
    let mut palette_ids: HashMap<u32, u32> = HashMap::new();
    let mut air_index: Option<u32> = None;
    for id in &mut values {
        if *id == AIR {
            *id = match air_index {
                Some(index) => index,
                None => {
                    let index = u32::try_from(palette.len()).map_err(size_overflow)?;
                    palette.push(air.clone());
                    air_index = Some(index);
                    index
                }
            };
            continue;
        }

        *id = match palette_ids.get(id) {
            Some(index) => *index,
            None => {
                let index = u32::try_from(palette.len()).map_err(size_overflow)?;
                palette.push(table.states[*id as usize].clone());
                palette_ids.insert(*id, index);
                index
            }
        };
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

/// Fill `rows` output rows starting at `start_y` with chosen state ids.
#[allow(clippy::too_many_arguments)]
fn fill_rows(
    scene: &Scene,
    table: &StateTable<'_>,
    factor: i64,
    min_cell: [i64; 3],
    size: [u32; 3],
    start_y: u32,
    rows: u32,
    out: &mut [u32],
) {
    let size_x = size[0] as usize;
    let size_z = size[2] as usize;
    let plane = size_x * size_z;
    debug_assert_eq!(out.len(), rows as usize * plane);

    let mut counts: HashMap<u32, u32> = HashMap::new();
    for local_y in 0..rows {
        let y = start_y + local_y;
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
                            let Some(id) = resolve_id(scene, table, world) else {
                                continue;
                            };
                            *counts.entry(id).or_insert(0) += 1;
                        }
                    }
                }

                let chosen = counts
                    .iter()
                    .max_by(|left, right| {
                        left.1.cmp(right.1).then_with(|| {
                            table.states[*right.0 as usize].cmp(table.states[*left.0 as usize])
                        })
                    })
                    .map(|(id, _)| *id)
                    .unwrap_or(AIR);

                let index = local_y as usize * plane + z as usize * size_x + x as usize;
                out[index] = chosen;
            }
        }
    }
}

/// Unique non-air states and a per-region mapping from palette id to state id.
struct StateTable<'a> {
    states: Vec<&'a BlockStateKey>,
    region_ids: Vec<Vec<u32>>,
}

fn build_state_table(scene: &Scene) -> StateTable<'_> {
    let mut states: Vec<&BlockStateKey> = Vec::new();
    let mut ids: HashMap<&BlockStateKey, u32> = HashMap::new();
    let mut region_ids = Vec::with_capacity(scene.regions.len());

    for region in &scene.regions {
        let mut local = Vec::with_capacity(region.palette.len());
        for state in &region.palette {
            if state.is_air() {
                local.push(AIR);
                continue;
            }

            let id = match ids.get(state) {
                Some(&id) => id,
                None => {
                    let id = u32::try_from(states.len()).unwrap_or(AIR);
                    if id != AIR {
                        states.push(state);
                        ids.insert(state, id);
                    }
                    id
                }
            };
            local.push(id);
        }
        region_ids.push(local);
    }

    StateTable { states, region_ids }
}

fn resolve_id(scene: &Scene, table: &StateTable<'_>, world: [i64; 3]) -> Option<u32> {
    for (region_index, region) in scene.regions.iter().enumerate().rev() {
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

        let size_x = region.size[0] as u64;
        let size_z = region.size[2] as u64;
        let index = local[1] as u64 * size_x * size_z + local[2] as u64 * size_x + local[0] as u64;
        let palette_id = usize::try_from(region.states.get(index)).ok()?;
        let id = *table.region_ids.get(region_index)?.get(palette_id)?;
        if id == AIR {
            return None;
        }
        return Some(id);
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
