use crate::error::{SceneError, SceneErrorCode};
use crate::{
    BlockEntity, BlockStateKey, BlockStatistics, Bounds, Diagnostic, DiagnosticCode,
    DiagnosticSeverity, Region, SceneMetadata,
};
use litematic_nbt::{NbtCompound, NbtValue};

const MAX_OVERLAP_DIAGNOSTICS: usize = 64;
/// Approximate per-entry overhead of the `BTreeMap` backing a compound.
const NBT_ENTRY_OVERHEAD: u64 = 56;

/// The canonical scene handed to the renderer and manifest writer.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub metadata: SceneMetadata,
    pub bounds: Bounds,
    pub regions: Vec<Region>,
    pub statistics: BlockStatistics,
    pub diagnostics: Vec<Diagnostic>,
}

impl Scene {
    /// Estimated heap bytes held by the regions, palettes, block entities, and
    /// their NBT payloads.
    pub fn heap_bytes(&self) -> u64 {
        let mut total = self.regions.capacity() as u64 * std::mem::size_of::<Region>() as u64;
        for region in &self.regions {
            total += region.states.heap_bytes();
            total += region.palette.capacity() as u64 * std::mem::size_of::<BlockStateKey>() as u64;
            for state in &region.palette {
                total += state.name.capacity() as u64;
                for (key, value) in &state.properties {
                    total += (key.capacity() + value.capacity()) as u64;
                }
            }
            total +=
                region.block_entities.capacity() as u64 * std::mem::size_of::<BlockEntity>() as u64;
            for entity in &region.block_entities {
                total += entity.id.capacity() as u64;
                total += nbt_compound_heap_bytes(&entity.data);
            }
        }
        total += self.diagnostics.capacity() as u64 * std::mem::size_of::<Diagnostic>() as u64;
        total
    }

    pub fn new(
        metadata: SceneMetadata,
        regions: Vec<Region>,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Self, SceneError> {
        validate_region_layout(&regions)?;
        let bounds = Bounds::from_regions(&regions)?;
        let statistics = BlockStatistics::from_regions(&regions)?;
        let mut diagnostics = diagnostics;
        add_overlap_diagnostics(&regions, &mut diagnostics);

        Ok(Self {
            metadata,
            bounds,
            regions,
            statistics,
            diagnostics,
        })
    }
}

/// Reject regions whose dimensions and packed state volume disagree.
///
/// The parser builds consistent regions, but `Region` has public fields for
/// tests and embedders. Without this check, a mismatched region would panic in
/// the mesh builder when it computes a packed index from the region size.
fn validate_region_layout(regions: &[Region]) -> Result<(), SceneError> {
    for region in regions {
        if region.palette.is_empty() {
            return Err(SceneError::new(
                SceneErrorCode::RegionLayoutMismatch,
                format!("region '{}' has an empty palette", region.name),
            ));
        }

        let expected = u64::from(region.size[0])
            .checked_mul(u64::from(region.size[1]))
            .and_then(|value| value.checked_mul(u64::from(region.size[2])))
            .ok_or_else(|| {
                SceneError::new(
                    SceneErrorCode::RegionLayoutMismatch,
                    format!("region '{}' dimensions overflow", region.name),
                )
            })?;
        if region.states.volume() != expected {
            return Err(SceneError::new(
                SceneErrorCode::RegionLayoutMismatch,
                format!(
                    "region '{}' has {} packed states for size {:?}",
                    region.name,
                    region.states.volume(),
                    region.size
                ),
            ));
        }
    }
    Ok(())
}

fn nbt_compound_heap_bytes(compound: &NbtCompound) -> u64 {
    let entry_size = std::mem::size_of::<String>() as u64 + std::mem::size_of::<NbtValue>() as u64;
    let mut total = compound.entries.len() as u64 * (entry_size + NBT_ENTRY_OVERHEAD);
    for (key, value) in &compound.entries {
        total += key.capacity() as u64;
        total += nbt_value_heap_bytes(value);
    }
    total
}

fn nbt_value_heap_bytes(value: &NbtValue) -> u64 {
    match value {
        NbtValue::Byte(_)
        | NbtValue::Short(_)
        | NbtValue::Int(_)
        | NbtValue::Long(_)
        | NbtValue::Float(_)
        | NbtValue::Double(_) => 0,
        NbtValue::String(value) => value.capacity() as u64,
        NbtValue::ByteArray(values) => values.capacity() as u64,
        NbtValue::IntArray(values) => values.capacity() as u64 * 4,
        NbtValue::LongArray(values) => values.capacity() as u64 * 8,
        NbtValue::List(list) => {
            list.values.capacity() as u64 * std::mem::size_of::<NbtValue>() as u64
                + list.values.iter().map(nbt_value_heap_bytes).sum::<u64>()
        }
        NbtValue::Compound(compound) => nbt_compound_heap_bytes(compound),
    }
}

fn add_overlap_diagnostics(regions: &[Region], diagnostics: &mut Vec<Diagnostic>) {
    let mut emitted = 0usize;
    let mut skipped = 0usize;

    for left in 0..regions.len() {
        for right in (left + 1)..regions.len() {
            let Some(volume) = overlap_volume(&regions[left], &regions[right]) else {
                continue;
            };

            if emitted < MAX_OVERLAP_DIAGNOSTICS {
                diagnostics.push(
                    Diagnostic::new(
                        DiagnosticCode::RegionOverlap,
                        DiagnosticSeverity::Warning,
                        format!(
                            "regions '{}' and '{}' overlap by {volume} blocks; later region order wins",
                            regions[left].name, regions[right].name
                        ),
                    )
                    .with_region(&regions[left].name),
                );
                emitted += 1;
            } else {
                skipped += 1;
            }
        }
    }

    if skipped > 0 {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::RegionOverlap,
            DiagnosticSeverity::Warning,
            format!("{skipped} additional region overlap diagnostics were suppressed"),
        ));
    }
}

fn overlap_volume(left: &Region, right: &Region) -> Option<u64> {
    let mut volume = 1u64;

    for axis in 0..3 {
        let left_end = left.origin[axis].checked_add(i64::from(left.size[axis]))?;
        let right_end = right.origin[axis].checked_add(i64::from(right.size[axis]))?;
        let min = left.origin[axis].max(right.origin[axis]);
        let max = left_end.min(right_end);

        if max <= min {
            return None;
        }

        volume = volume.checked_mul(u64::try_from(max - min).ok()?)?;
    }

    Some(volume)
}
