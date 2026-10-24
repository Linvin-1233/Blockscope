use crate::{MeshError, MeshLimits};
use litematic_scene::{BlockStateKey, Region, Scene};
use std::collections::BTreeMap;

/// A resolved world position.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedBlockRef<'a> {
    pub region_index: usize,
    pub state: &'a BlockStateKey,
}

/// Resolves world positions across overlapping regions with a page index.
#[derive(Debug)]
pub struct SceneBlockView<'a> {
    scene: &'a Scene,
    page_index: BTreeMap<[i32; 3], Vec<usize>>,
}

impl<'a> SceneBlockView<'a> {
    pub fn new(scene: &'a Scene, limits: &MeshLimits) -> Result<Self, MeshError> {
        let mut page_index: BTreeMap<[i32; 3], Vec<usize>> = BTreeMap::new();

        for (region_index, region) in scene.regions.iter().enumerate() {
            let min = [
                page_coord(region.origin[0])?,
                page_coord(region.origin[1])?,
                page_coord(region.origin[2])?,
            ];
            let max = [
                page_coord(region.origin[0] + i64::from(region.size[0]) - 1)?,
                page_coord(region.origin[1] + i64::from(region.size[1]) - 1)?,
                page_coord(region.origin[2] + i64::from(region.size[2]) - 1)?,
            ];

            for x in min[0]..=max[0] {
                for y in min[1]..=max[1] {
                    for z in min[2]..=max[2] {
                        let key = [x, y, z];
                        if !page_index.contains_key(&key) && page_index.len() >= limits.max_pages {
                            return Err(MeshError::page_limit(format!(
                                "mesh exceeds {} pages",
                                limits.max_pages
                            )));
                        }
                        page_index.entry(key).or_default().push(region_index);
                    }
                }
            }
        }

        Ok(Self { scene, page_index })
    }

    pub fn resolve(&self, position: [i64; 3]) -> Option<ResolvedBlockRef<'a>> {
        let page = self.page_key(position)?;
        let regions = self.page_index.get(&page)?;

        for &region_index in regions.iter().rev() {
            let region = &self.scene.regions[region_index];
            if let Some(state) = region_state(region, position) {
                return Some(ResolvedBlockRef {
                    region_index,
                    state,
                });
            }
        }

        None
    }

    pub fn page_key(&self, position: [i64; 3]) -> Option<[i32; 3]> {
        Some([
            i32::try_from(position[0].div_euclid(16)).ok()?,
            i32::try_from(position[1].div_euclid(16)).ok()?,
            i32::try_from(position[2].div_euclid(16)).ok()?,
        ])
    }

    pub fn pages(&self) -> usize {
        self.page_index.len()
    }
}

fn region_state(region: &Region, position: [i64; 3]) -> Option<&BlockStateKey> {
    let local = [
        position[0].checked_sub(region.origin[0])?,
        position[1].checked_sub(region.origin[1])?,
        position[2].checked_sub(region.origin[2])?,
    ];

    for (axis, value) in local.iter().enumerate() {
        if *value < 0 || *value >= i64::from(region.size[axis]) {
            return None;
        }
    }

    let x = u64::try_from(local[0]).ok()?;
    let y = u64::try_from(local[1]).ok()?;
    let z = u64::try_from(local[2]).ok()?;
    let size_x = u64::from(region.size[0]);
    let size_z = u64::from(region.size[2]);
    let index = y
        .checked_mul(size_x)?
        .checked_mul(size_z)?
        .checked_add(z.checked_mul(size_x)?)?
        .checked_add(x)?;

    let palette_id = usize::try_from(region.states.get(index)).ok()?;
    region.palette.get(palette_id)
}

fn page_coord(value: i64) -> Result<i32, MeshError> {
    i32::try_from(value.div_euclid(16)).map_err(|_| {
        MeshError::coordinate(format!(
            "world coordinate {value} does not fit in i32 page space"
        ))
    })
}
