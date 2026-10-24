use crate::container::{RawRegion, number_to_i64};
use crate::error::{LitematicError, LitematicErrorCode};
use litematic_nbt::{NbtCompound, NbtList, NbtValue};
use litematic_scene::{
    BlockEntity, BlockStateKey, Diagnostic, DiagnosticCode, DiagnosticSeverity, PackedIndices,
};
use std::collections::BTreeMap;

/// A region with a canonical palette, validated indices, and auxiliary data.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionData {
    pub palette: Vec<BlockStateKey>,
    pub states: PackedIndices,
    pub block_entities: Vec<BlockEntity>,
    pub entity_count: usize,
    pub pending_block_tick_count: usize,
    pub pending_fluid_tick_count: usize,
    pub diagnostics: Vec<Diagnostic>,
}

impl RawRegion {
    /// Consume this region and decode its palette, states, and block entities.
    pub fn into_data(self) -> Result<RegionData, LitematicError> {
        let RawRegion {
            name,
            format_version,
            size_abs,
            volume,
            palette,
            block_states,
            block_entities,
            entities,
            pending_block_ticks,
            pending_fluid_ticks,
            ..
        } = self;

        let palette = parse_palette(&palette, &name)?;
        let bits = PackedIndices::bits_for_palette(palette.len());
        let expected = PackedIndices::expected_long_count(volume, bits).ok_or_else(|| {
            LitematicError::new(
                LitematicErrorCode::ResourceLimit,
                format!("region '{name}' packed state size overflow"),
            )
        })?;

        let actual = u64::try_from(block_states.len()).map_err(|_| {
            LitematicError::new(
                LitematicErrorCode::BlockStateDataLengthMismatch,
                format!("region '{name}' block state array length does not fit in u64"),
            )
        })?;

        if actual != expected {
            return Err(LitematicError::new(
                LitematicErrorCode::BlockStateDataLengthMismatch,
                format!(
                    "region '{name}' BlockStates has {actual} longs, expected {expected} for volume {volume} and {bits} bits"
                ),
            ));
        }

        let states = PackedIndices::new(bits, block_states, volume).ok_or_else(|| {
            LitematicError::new(
                LitematicErrorCode::BlockStateDataLengthMismatch,
                format!("region '{name}' packed state array is inconsistent"),
            )
        })?;

        validate_indices(&states, palette.len(), size_abs, &name)?;

        let mut diagnostics = Vec::new();
        let block_entities = parse_block_entities(
            format_version,
            block_entities,
            size_abs,
            &name,
            &mut diagnostics,
        )?;

        let entity_count = entities.values.len();
        let pending_block_tick_count = pending_block_ticks.values.len();
        let pending_fluid_tick_count = pending_fluid_ticks.values.len();

        if entity_count > 0 {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::EntitiesIgnored,
                    DiagnosticSeverity::Info,
                    format!("{entity_count} entities are counted but not rendered"),
                )
                .with_region(&name),
            );
        }

        if pending_block_tick_count > 0 || pending_fluid_tick_count > 0 {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::PendingTicksIgnored,
                    DiagnosticSeverity::Info,
                    format!(
                        "{} block ticks and {} fluid ticks are counted but not executed",
                        pending_block_tick_count, pending_fluid_tick_count
                    ),
                )
                .with_region(&name),
            );
        }

        Ok(RegionData {
            palette,
            states,
            block_entities,
            entity_count,
            pending_block_tick_count,
            pending_fluid_tick_count,
            diagnostics,
        })
    }
}

fn parse_palette(
    palette: &NbtList,
    region_name: &str,
) -> Result<Vec<BlockStateKey>, LitematicError> {
    let mut result = Vec::with_capacity(palette.values.len());

    for (index, value) in palette.values.iter().enumerate() {
        let compound = match value {
            NbtValue::Compound(compound) => compound,
            other => {
                return Err(LitematicError::new(
                    LitematicErrorCode::InvalidPalette,
                    format!(
                        "region '{region_name}' palette entry {index} must be a compound, got {}",
                        other.tag_type().as_str()
                    ),
                ));
            }
        };

        let name = match compound.get("Name") {
            Some(NbtValue::String(name)) => normalize_name(name, region_name, index)?,
            Some(other) => {
                return Err(LitematicError::new(
                    LitematicErrorCode::UnsupportedPaletteFormat,
                    format!(
                        "region '{region_name}' palette entry {index} Name must be a string, got {}",
                        other.tag_type().as_str()
                    ),
                ));
            }
            None => {
                return Err(LitematicError::new(
                    LitematicErrorCode::UnsupportedPaletteFormat,
                    format!("region '{region_name}' palette entry {index} is missing Name"),
                ));
            }
        };

        let properties = match compound.get("Properties") {
            None => BTreeMap::new(),
            Some(NbtValue::Compound(properties)) => {
                parse_properties(properties, region_name, index)?
            }
            Some(other) => {
                return Err(LitematicError::new(
                    LitematicErrorCode::InvalidPalette,
                    format!(
                        "region '{region_name}' palette entry {index} Properties must be a compound, got {}",
                        other.tag_type().as_str()
                    ),
                ));
            }
        };

        result.push(BlockStateKey::new(name, properties));
    }

    Ok(result)
}

fn parse_properties(
    properties: &NbtCompound,
    region_name: &str,
    entry_index: usize,
) -> Result<BTreeMap<String, String>, LitematicError> {
    let mut result = BTreeMap::new();

    for (key, value) in &properties.entries {
        let value = match value {
            NbtValue::String(value) => value.clone(),
            other => {
                return Err(LitematicError::new(
                    LitematicErrorCode::InvalidPalette,
                    format!(
                        "region '{region_name}' palette entry {entry_index} property '{key}' must be a string, got {}",
                        other.tag_type().as_str()
                    ),
                ));
            }
        };
        result.insert(key.clone(), value);
    }

    Ok(result)
}

fn normalize_name(
    name: &str,
    region_name: &str,
    entry_index: usize,
) -> Result<String, LitematicError> {
    if name.is_empty() || name.chars().any(char::is_whitespace) {
        return Err(LitematicError::new(
            LitematicErrorCode::UnsupportedPaletteFormat,
            format!("region '{region_name}' palette entry {entry_index} has an invalid block name"),
        ));
    }

    let mut parts = name.split(':');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(namespace), Some(path), None) if !namespace.is_empty() && !path.is_empty() => {
            Ok(name.to_string())
        }
        (Some(path), None, None) if !path.is_empty() => Ok(format!("minecraft:{path}")),
        _ => Err(LitematicError::new(
            LitematicErrorCode::UnsupportedPaletteFormat,
            format!(
                "region '{region_name}' palette entry {entry_index} has an invalid resource location '{name}'"
            ),
        )),
    }
}

fn validate_indices(
    states: &PackedIndices,
    palette_len: usize,
    size_abs: [u32; 3],
    region_name: &str,
) -> Result<(), LitematicError> {
    let size_x = u64::from(size_abs[0]);
    let size_z = u64::from(size_abs[2]);
    let layer = size_x
        .checked_mul(size_z)
        .ok_or_else(|| invalid_index(region_name, "region layer size overflow"))?;
    let palette_len = u64::try_from(palette_len)
        .map_err(|_| invalid_index(region_name, "palette length does not fit in u64"))?;

    for index in 0..states.volume() {
        let value = u64::from(states.get(index));
        if value >= palette_len {
            let x = index % size_x;
            let z = (index / size_x) % size_z;
            let y = index / layer;
            return Err(LitematicError::new(
                LitematicErrorCode::InvalidPaletteIndex,
                format!(
                    "region '{region_name}' block [{x},{y},{z}] index {index} palette id {value} exceeds palette size {palette_len}"
                ),
            ));
        }
    }

    Ok(())
}

fn parse_block_entities(
    format_version: i32,
    block_entities: NbtList,
    size_abs: [u32; 3],
    region_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<BlockEntity>, LitematicError> {
    let mut by_position: BTreeMap<[u32; 3], BlockEntity> = BTreeMap::new();

    for (index, value) in block_entities.values.into_iter().enumerate() {
        let compound = match value {
            NbtValue::Compound(compound) => compound,
            other => {
                return Err(invalid_block_entity(
                    region_name,
                    format!(
                        "block entity {index} must be a compound, got {}",
                        other.tag_type().as_str()
                    ),
                ));
            }
        };

        let (raw_position, data, fallback_id) = if format_version == 1 {
            let wrapper_position = read_local_position_raw(&compound);
            let wrapper_id = extract_id(&compound);
            let mut wrapper = compound;
            let tile_nbt = match wrapper.entries.remove("TileNBT") {
                Some(NbtValue::Compound(tile_nbt)) => tile_nbt,
                Some(other) => {
                    return Err(invalid_block_entity(
                        region_name,
                        format!(
                            "block entity {index} TileNBT must be a compound, got {}",
                            other.tag_type().as_str()
                        ),
                    ));
                }
                None => {
                    return Err(invalid_block_entity(
                        region_name,
                        format!("block entity {index} is missing TileNBT"),
                    ));
                }
            };
            let raw_position = wrapper_position.or_else(|| read_local_position_raw(&tile_nbt));
            (raw_position, tile_nbt, wrapper_id)
        } else {
            let raw_position = read_local_position_raw(&compound);
            (raw_position, compound, None)
        };

        let raw_position = raw_position.ok_or_else(|| {
            invalid_block_entity(
                region_name,
                format!("block entity {index} is missing a valid x/y/z position"),
            )
        })?;

        let Some(position) = to_local_position(raw_position, size_abs) else {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::OutOfBoundsBlockEntity,
                    DiagnosticSeverity::Warning,
                    format!(
                        "block entity {index} at {:?} is outside region size {:?} and was dropped",
                        raw_position, size_abs
                    ),
                )
                .with_region(region_name),
            );
            continue;
        };

        let id = extract_id(&data).or(fallback_id).unwrap_or_else(|| {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::MissingBlockEntityId,
                    DiagnosticSeverity::Warning,
                    format!(
                        "block entity {index} at {position:?} has no id and was labelled unknown"
                    ),
                )
                .with_region(region_name),
            );
            "unknown".to_string()
        });

        let entity = BlockEntity {
            local_pos: position,
            id,
            data,
        };

        if by_position.insert(position, entity).is_some() {
            diagnostics.push(
                Diagnostic::new(
                    DiagnosticCode::DuplicateBlockEntity,
                    DiagnosticSeverity::Warning,
                    format!(
                        "block entity at {position:?} appears more than once; the last entry wins"
                    ),
                )
                .with_region(region_name),
            );
        }
    }

    Ok(by_position.into_values().collect())
}

fn read_local_position_raw(compound: &NbtCompound) -> Option<[i64; 3]> {
    if let (Some(x), Some(y), Some(z)) = (compound.get("x"), compound.get("y"), compound.get("z")) {
        return Some([number_to_i64(x)?, number_to_i64(y)?, number_to_i64(z)?]);
    }

    if let Some(NbtValue::List(list)) = compound.get("Pos")
        && list.values.len() == 3
    {
        let mut result = [0i64; 3];
        for (index, value) in list.values.iter().enumerate() {
            result[index] = number_to_i64(value)?;
        }
        return Some(result);
    }

    None
}

fn to_local_position(raw: [i64; 3], size_abs: [u32; 3]) -> Option<[u32; 3]> {
    let mut result = [0u32; 3];
    for axis in 0..3 {
        let value = raw[axis];
        if value < 0 || value >= i64::from(size_abs[axis]) {
            return None;
        }
        result[axis] = u32::try_from(value).ok()?;
    }
    Some(result)
}

fn extract_id(compound: &NbtCompound) -> Option<String> {
    match compound.get("id") {
        Some(NbtValue::String(id)) => Some(id.clone()),
        _ => None,
    }
}

fn invalid_index(region_name: &str, message: impl Into<String>) -> LitematicError {
    LitematicError::new(
        LitematicErrorCode::InvalidPaletteIndex,
        format!("region '{region_name}' {}", message.into()),
    )
}

fn invalid_block_entity(region_name: &str, message: impl Into<String>) -> LitematicError {
    LitematicError::new(
        LitematicErrorCode::InvalidBlockEntity,
        format!("region '{region_name}' {}", message.into()),
    )
}
