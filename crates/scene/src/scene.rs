use crate::error::SceneError;
use crate::{
    BlockStatistics, Bounds, Diagnostic, DiagnosticCode, DiagnosticSeverity, Region, SceneMetadata,
};

const MAX_OVERLAP_DIAGNOSTICS: usize = 64;

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
    pub fn new(
        metadata: SceneMetadata,
        regions: Vec<Region>,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Self, SceneError> {
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
