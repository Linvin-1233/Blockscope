use crate::error::{LitematicError, LitematicErrorCode};
use crate::limits::LitematicLimits;
use litematic_nbt::{DecodeLimits, NbtCompound, NbtList, NbtRoot, NbtTagType, NbtValue, decode};

/// Metadata declared by the litematic file. Values are untrusted hints.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LitematicMetadata {
    pub name: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub enclosing_size: Option<[i32; 3]>,
    pub original_origin: Option<[i32; 3]>,
    pub time_created: Option<i64>,
    pub time_modified: Option<i64>,
    pub declared_region_count: Option<i64>,
    pub declared_total_volume: Option<i64>,
    pub declared_total_blocks: Option<i64>,
    pub declared_block_entity_count: Option<i64>,
    pub declared_block_tick_count: Option<i64>,
    pub declared_mc_data_version: Option<i64>,
    pub declared_mc_version: Option<String>,
    pub preview_image_pixels: Option<usize>,
}

/// A region with its raw palette, packed states, and auxiliary NBT lists.
#[derive(Debug, Clone, PartialEq)]
pub struct RawRegion {
    pub name: String,
    pub format_version: i32,
    pub position: [i32; 3],
    pub size: [i32; 3],
    pub origin: [i64; 3],
    pub size_abs: [u32; 3],
    pub volume: u64,
    pub data_version: i32,
    pub palette: NbtList,
    pub block_states: Vec<i64>,
    pub block_entities: NbtList,
    pub entities: NbtList,
    pub pending_block_ticks: NbtList,
    pub pending_fluid_ticks: NbtList,
}

/// A validated litematic container before palette and scene construction.
#[derive(Debug, Clone, PartialEq)]
pub struct LitematicFile {
    pub version: i32,
    pub sub_version: Option<i32>,
    pub minecraft_data_version: i32,
    pub metadata: LitematicMetadata,
    pub regions: Vec<RawRegion>,
}

impl LitematicFile {
    /// Decode and validate a litematic file from raw bytes.
    pub fn parse_bytes(
        input: &[u8],
        nbt_limits: &DecodeLimits,
        limits: &LitematicLimits,
    ) -> Result<Self, LitematicError> {
        let root = decode(input, nbt_limits)?;
        Self::parse_root(root, limits)
    }

    /// Validate a decoded NBT root as a litematic container.
    pub fn parse_root(root: NbtRoot, limits: &LitematicLimits) -> Result<Self, LitematicError> {
        let mut root = match root.value {
            NbtValue::Compound(compound) => compound,
            other => {
                return Err(invalid_litematic(format!(
                    "root must be a compound, got {}",
                    other.tag_type().as_str()
                )));
            }
        };

        let version = take_i32(&mut root, "Version", LitematicErrorCode::InvalidLitematic)?
            .ok_or_else(|| invalid_litematic("missing Version"))?;
        if !(1..=7).contains(&version) {
            return Err(LitematicError::new(
                LitematicErrorCode::UnsupportedVersion,
                format!("unsupported litematic version {version}; supported range is 1..=7"),
            ));
        }

        let sub_version = take_i32(
            &mut root,
            "SubVersion",
            LitematicErrorCode::InvalidLitematic,
        )?;
        let minecraft_data_version = take_i32(
            &mut root,
            "MinecraftDataVersion",
            LitematicErrorCode::InvalidLitematic,
        )?
        .unwrap_or(-1);

        let metadata = take_compound(&mut root, "Metadata", LitematicErrorCode::InvalidLitematic)?
            .ok_or_else(|| invalid_litematic("missing Metadata"))?;
        let metadata = parse_metadata(metadata)?;

        let regions = take_compound(&mut root, "Regions", LitematicErrorCode::InvalidLitematic)?
            .ok_or_else(|| invalid_litematic("missing Regions"))?;

        if regions.is_empty() {
            return Err(invalid_litematic("Regions is empty"));
        }
        if regions.len() > limits.max_regions {
            return Err(resource_limit(format!(
                "region count {} exceeds limit {}",
                regions.len(),
                limits.max_regions
            )));
        }

        let mut parsed_regions = Vec::with_capacity(regions.len());
        let mut total_volume = 0u64;

        for (name, value) in regions.entries {
            let region = match value {
                NbtValue::Compound(compound) => compound,
                other => {
                    return Err(invalid_litematic(format!(
                        "region '{name}' must be a compound, got {}",
                        other.tag_type().as_str()
                    )));
                }
            };

            let region = parse_region(name, region, version, minecraft_data_version, limits)?;
            total_volume = total_volume
                .checked_add(region.volume)
                .ok_or_else(|| resource_limit("total volume overflow"))?;
            if total_volume > limits.max_total_volume {
                return Err(resource_limit(format!(
                    "total volume {total_volume} exceeds limit {}",
                    limits.max_total_volume
                )));
            }
            parsed_regions.push(region);
        }

        Ok(Self {
            version,
            sub_version,
            minecraft_data_version,
            metadata,
            regions: parsed_regions,
        })
    }
}

fn parse_metadata(metadata: NbtCompound) -> Result<LitematicMetadata, LitematicError> {
    let mut metadata = metadata;
    let code = LitematicErrorCode::InvalidMetadata;

    Ok(LitematicMetadata {
        name: take_string(&mut metadata, "Name", code)?,
        author: take_string(&mut metadata, "Author", code)?,
        description: take_string(&mut metadata, "Description", code)?,
        enclosing_size: take_vec3i(&mut metadata, "EnclosingSize", code)?,
        original_origin: take_vec3i(&mut metadata, "Origin", code)?,
        time_created: take_i64(&mut metadata, "TimeCreated", code)?,
        time_modified: take_i64(&mut metadata, "TimeModified", code)?,
        declared_region_count: take_i64(&mut metadata, "RegionCount", code)?,
        declared_total_volume: take_i64(&mut metadata, "TotalVolume", code)?,
        declared_total_blocks: take_i64(&mut metadata, "TotalBlocks", code)?,
        declared_block_entity_count: take_i64(&mut metadata, "BlockEntityCount", code)?,
        declared_block_tick_count: take_i64(&mut metadata, "BlockTickCount", code)?,
        declared_mc_data_version: take_i64(&mut metadata, "McDataVersion", code)?,
        declared_mc_version: take_string(&mut metadata, "McVersion", code)?,
        preview_image_pixels: take_int_array_len(&mut metadata, "PreviewImageData", code)?,
    })
}

fn parse_region(
    name: String,
    mut region: NbtCompound,
    format_version: i32,
    top_data_version: i32,
    limits: &LitematicLimits,
) -> Result<RawRegion, LitematicError> {
    let bounds_code = LitematicErrorCode::InvalidRegionBounds;
    let position = take_vec3i(&mut region, "Position", bounds_code)?.ok_or_else(|| {
        LitematicError::new(bounds_code, format!("region '{name}' missing Position"))
    })?;
    let size = take_vec3i(&mut region, "Size", bounds_code)?
        .ok_or_else(|| LitematicError::new(bounds_code, format!("region '{name}' missing Size")))?;

    let (origin, size_abs, volume) =
        normalize_bounds(position, size, limits.max_coordinate, &name)?;
    if volume > limits.max_volume_per_region {
        return Err(resource_limit(format!(
            "region '{name}' volume {volume} exceeds limit {}",
            limits.max_volume_per_region
        )));
    }

    let data_version = take_i32(
        &mut region,
        "DataVersion",
        LitematicErrorCode::InvalidLitematic,
    )?
    .unwrap_or(top_data_version);

    let palette = take_list(
        &mut region,
        "BlockStatePalette",
        LitematicErrorCode::InvalidPalette,
    )?
    .ok_or_else(|| {
        LitematicError::new(
            LitematicErrorCode::InvalidPalette,
            format!("region '{name}' missing BlockStatePalette"),
        )
    })?;

    if palette.element_type != NbtTagType::Compound || palette.values.is_empty() {
        return Err(LitematicError::new(
            LitematicErrorCode::InvalidPalette,
            format!("region '{name}' palette must be a non-empty compound list"),
        ));
    }
    if palette.values.len() > limits.max_palette_entries {
        return Err(resource_limit(format!(
            "region '{name}' palette {} exceeds limit {}",
            palette.values.len(),
            limits.max_palette_entries
        )));
    }

    let block_states = take_long_array(
        &mut region,
        "BlockStates",
        LitematicErrorCode::InvalidLitematic,
    )?
    .ok_or_else(|| {
        LitematicError::new(
            LitematicErrorCode::InvalidLitematic,
            format!("region '{name}' missing BlockStates"),
        )
    })?;

    if block_states.is_empty() {
        return Err(LitematicError::new(
            LitematicErrorCode::BlockStateDataLengthMismatch,
            format!("region '{name}' BlockStates is empty"),
        ));
    }

    let block_entities = take_list(
        &mut region,
        "TileEntities",
        LitematicErrorCode::InvalidBlockEntity,
    )?
    .unwrap_or_else(empty_compound_list);
    validate_list(
        &block_entities,
        limits.max_block_entities,
        LitematicErrorCode::InvalidBlockEntity,
        LitematicErrorCode::BlockEntityLimitExceeded,
        &format!("region '{name}' block entities"),
    )?;

    let entities = take_list(
        &mut region,
        "Entities",
        LitematicErrorCode::InvalidLitematic,
    )?
    .unwrap_or_else(empty_compound_list);
    validate_list(
        &entities,
        limits.max_entities,
        LitematicErrorCode::InvalidLitematic,
        LitematicErrorCode::ResourceLimit,
        &format!("region '{name}' entities"),
    )?;

    let pending_block_ticks = take_list(
        &mut region,
        "PendingBlockTicks",
        LitematicErrorCode::InvalidLitematic,
    )?
    .unwrap_or_else(empty_compound_list);
    validate_list(
        &pending_block_ticks,
        limits.max_pending_ticks,
        LitematicErrorCode::InvalidLitematic,
        LitematicErrorCode::ResourceLimit,
        &format!("region '{name}' pending block ticks"),
    )?;

    let pending_fluid_ticks = take_list(
        &mut region,
        "PendingFluidTicks",
        LitematicErrorCode::InvalidLitematic,
    )?
    .unwrap_or_else(empty_compound_list);
    validate_list(
        &pending_fluid_ticks,
        limits.max_pending_ticks,
        LitematicErrorCode::InvalidLitematic,
        LitematicErrorCode::ResourceLimit,
        &format!("region '{name}' pending fluid ticks"),
    )?;

    Ok(RawRegion {
        name,
        format_version,
        position,
        size,
        origin,
        size_abs,
        volume,
        data_version,
        palette,
        block_states,
        block_entities,
        entities,
        pending_block_ticks,
        pending_fluid_ticks,
    })
}

fn normalize_bounds(
    position: [i32; 3],
    size: [i32; 3],
    max_coordinate: i64,
    region_name: &str,
) -> Result<([i64; 3], [u32; 3], u64), LitematicError> {
    let mut origin = [0i64; 3];
    let mut size_abs = [0u32; 3];

    for axis in 0..3 {
        let start = i64::from(position[axis]);
        let extent = i64::from(size[axis]);

        if extent == 0 {
            return Err(LitematicError::new(
                LitematicErrorCode::InvalidRegionBounds,
                format!("region '{region_name}' has a zero size on axis {axis}"),
            ));
        }

        let end_offset = if extent > 0 { extent - 1 } else { extent + 1 };
        let end = start.checked_add(end_offset).ok_or_else(|| {
            LitematicError::new(
                LitematicErrorCode::InvalidRegionBounds,
                format!("region '{region_name}' bounds overflow on axis {axis}"),
            )
        })?;

        let min = start.min(end);
        let max = start.max(end);

        if min < -max_coordinate || max > max_coordinate {
            return Err(LitematicError::new(
                LitematicErrorCode::InvalidRegionBounds,
                format!(
                    "region '{region_name}' coordinate range [{min}, {max}] exceeds ±{max_coordinate}"
                ),
            ));
        }

        let absolute = max
            .checked_sub(min)
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| {
                LitematicError::new(
                    LitematicErrorCode::InvalidRegionBounds,
                    format!("region '{region_name}' size overflow on axis {axis}"),
                )
            })?;

        size_abs[axis] = u32::try_from(absolute).map_err(|_| {
            LitematicError::new(
                LitematicErrorCode::InvalidRegionBounds,
                format!("region '{region_name}' size does not fit in u32"),
            )
        })?;
        origin[axis] = min;
    }

    let volume = u64::from(size_abs[0])
        .checked_mul(u64::from(size_abs[1]))
        .and_then(|value| value.checked_mul(u64::from(size_abs[2])))
        .ok_or_else(|| {
            LitematicError::new(
                LitematicErrorCode::InvalidRegionBounds,
                format!("region '{region_name}' volume overflow"),
            )
        })?;

    Ok((origin, size_abs, volume))
}

fn validate_list(
    list: &NbtList,
    max: usize,
    invalid_code: LitematicErrorCode,
    limit_code: LitematicErrorCode,
    label: &str,
) -> Result<(), LitematicError> {
    if list.values.len() > max {
        return Err(LitematicError::new(
            limit_code,
            format!("{label} count {} exceeds limit {max}", list.values.len()),
        ));
    }
    if !list.values.is_empty() && list.element_type != NbtTagType::Compound {
        return Err(LitematicError::new(
            invalid_code,
            format!("{label} must be a compound list"),
        ));
    }
    Ok(())
}

fn empty_compound_list() -> NbtList {
    NbtList::new(NbtTagType::End, Vec::new())
}

fn take_value(compound: &mut NbtCompound, key: &str) -> Option<NbtValue> {
    compound.entries.remove(key)
}

fn take_i64(
    compound: &mut NbtCompound,
    key: &str,
    code: LitematicErrorCode,
) -> Result<Option<i64>, LitematicError> {
    match take_value(compound, key) {
        None => Ok(None),
        Some(value) => number_to_i64(&value).map(Some).ok_or_else(|| {
            LitematicError::new(
                code,
                format!("{key} must be numeric, got {}", value.tag_type().as_str()),
            )
        }),
    }
}

fn take_i32(
    compound: &mut NbtCompound,
    key: &str,
    code: LitematicErrorCode,
) -> Result<Option<i32>, LitematicError> {
    match take_i64(compound, key, code)? {
        None => Ok(None),
        Some(value) => i32::try_from(value).map(Some).map_err(|_| {
            LitematicError::new(code, format!("{key} value {value} does not fit in i32"))
        }),
    }
}

fn take_string(
    compound: &mut NbtCompound,
    key: &str,
    code: LitematicErrorCode,
) -> Result<Option<String>, LitematicError> {
    match take_value(compound, key) {
        None => Ok(None),
        Some(NbtValue::String(value)) => Ok(Some(value)),
        Some(value) => Err(LitematicError::new(
            code,
            format!("{key} must be a string, got {}", value.tag_type().as_str()),
        )),
    }
}

fn take_compound(
    compound: &mut NbtCompound,
    key: &str,
    code: LitematicErrorCode,
) -> Result<Option<NbtCompound>, LitematicError> {
    match take_value(compound, key) {
        None => Ok(None),
        Some(NbtValue::Compound(value)) => Ok(Some(value)),
        Some(value) => Err(LitematicError::new(
            code,
            format!(
                "{key} must be a compound, got {}",
                value.tag_type().as_str()
            ),
        )),
    }
}

fn take_list(
    compound: &mut NbtCompound,
    key: &str,
    code: LitematicErrorCode,
) -> Result<Option<NbtList>, LitematicError> {
    match take_value(compound, key) {
        None => Ok(None),
        Some(NbtValue::List(value)) => Ok(Some(value)),
        Some(value) => Err(LitematicError::new(
            code,
            format!("{key} must be a list, got {}", value.tag_type().as_str()),
        )),
    }
}

fn take_long_array(
    compound: &mut NbtCompound,
    key: &str,
    code: LitematicErrorCode,
) -> Result<Option<Vec<i64>>, LitematicError> {
    match take_value(compound, key) {
        None => Ok(None),
        Some(NbtValue::LongArray(value)) => Ok(Some(value)),
        Some(value) => Err(LitematicError::new(
            code,
            format!(
                "{key} must be a long array, got {}",
                value.tag_type().as_str()
            ),
        )),
    }
}

fn take_int_array_len(
    compound: &mut NbtCompound,
    key: &str,
    code: LitematicErrorCode,
) -> Result<Option<usize>, LitematicError> {
    match take_value(compound, key) {
        None => Ok(None),
        Some(NbtValue::IntArray(value)) => Ok(Some(value.len())),
        Some(value) => Err(LitematicError::new(
            code,
            format!(
                "{key} must be an int array, got {}",
                value.tag_type().as_str()
            ),
        )),
    }
}

fn take_vec3i(
    compound: &mut NbtCompound,
    key: &str,
    code: LitematicErrorCode,
) -> Result<Option<[i32; 3]>, LitematicError> {
    match take_value(compound, key) {
        None => Ok(None),
        Some(NbtValue::Compound(mut value)) => {
            let x = take_i32(&mut value, "x", code)?
                .ok_or_else(|| LitematicError::new(code, format!("{key}.x missing")))?;
            let y = take_i32(&mut value, "y", code)?
                .ok_or_else(|| LitematicError::new(code, format!("{key}.y missing")))?;
            let z = take_i32(&mut value, "z", code)?
                .ok_or_else(|| LitematicError::new(code, format!("{key}.z missing")))?;
            Ok(Some([x, y, z]))
        }
        Some(NbtValue::List(list)) => {
            if list.element_type != NbtTagType::Int || list.values.len() != 3 {
                return Err(LitematicError::new(
                    code,
                    format!("{key} must contain exactly three ints"),
                ));
            }

            let mut result = [0i32; 3];
            for (index, value) in list.values.iter().enumerate() {
                match value {
                    NbtValue::Int(value) => result[index] = *value,
                    other => {
                        return Err(LitematicError::new(
                            code,
                            format!(
                                "{key}[{index}] must be an int, got {}",
                                other.tag_type().as_str()
                            ),
                        ));
                    }
                }
            }
            Ok(Some(result))
        }
        Some(value) => Err(LitematicError::new(
            code,
            format!(
                "{key} must be a compound or int list, got {}",
                value.tag_type().as_str()
            ),
        )),
    }
}

pub(crate) fn number_to_i64(value: &NbtValue) -> Option<i64> {
    match value {
        NbtValue::Byte(value) => Some(i64::from(*value)),
        NbtValue::Short(value) => Some(i64::from(*value)),
        NbtValue::Int(value) => Some(i64::from(*value)),
        NbtValue::Long(value) => Some(*value),
        _ => None,
    }
}

fn invalid_litematic(message: impl Into<String>) -> LitematicError {
    LitematicError::new(LitematicErrorCode::InvalidLitematic, message)
}

fn resource_limit(message: impl Into<String>) -> LitematicError {
    LitematicError::new(LitematicErrorCode::ResourceLimit, message)
}
