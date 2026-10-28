use crate::{MeshError, MeshLimits};
use litematic_scene::{BlockStateKey, Region, Scene};
use std::collections::{BTreeMap, BTreeSet};

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
        let region_pages = scene
            .regions
            .iter()
            .map(region_page_bounds)
            .collect::<Result<Vec<_>, _>>()?;

        if scene.statistics.air == 0 {
            return Ok(Self {
                scene,
                page_index: bbox_page_index(&region_pages, limits.max_pages)?,
            });
        }

        let mut occupied: BTreeSet<[i32; 3]> = BTreeSet::new();
        for (region_index, region) in scene.regions.iter().enumerate() {
            occupied.extend(region_occupied_pages(
                region,
                region_pages[region_index],
                limits.max_pages,
            )?);
        }
        if occupied.len() > limits.max_pages {
            return Err(MeshError::page_limit(format!(
                "mesh exceeds {} pages",
                limits.max_pages
            )));
        }

        // An occupied page still indexes every region that covers it. This keeps
        // overlap resolution exact even when a later region has only air there.
        let mut page_index: BTreeMap<[i32; 3], Vec<usize>> = BTreeMap::new();
        for page in occupied {
            let mut regions = Vec::new();
            for (region_index, (min, max)) in region_pages.iter().enumerate() {
                if (0..3).all(|axis| page[axis] >= min[axis] && page[axis] <= max[axis]) {
                    regions.push(region_index);
                }
            }
            page_index.insert(page, regions);
        }

        Ok(Self { scene, page_index })
    }

    pub fn resolve(&self, position: [i64; 3]) -> Option<ResolvedBlockRef<'a>> {
        let page = self.page_key(position)?;
        if let Some(regions) = self.page_index.get(&page) {
            for &region_index in regions.iter().rev() {
                let region = &self.scene.regions[region_index];
                if let Some(state) = region_state(region, position) {
                    return Some(ResolvedBlockRef {
                        region_index,
                        state,
                    });
                }
            }
        }

        // Air-only pages are not meshed and therefore absent from the index.
        // Resolve them through the regions so the public lookup still returns
        // the air state rather than None.
        for (region_index, region) in self.scene.regions.iter().enumerate().rev() {
            if let Some(state) = region_state(region, position) {
                return Some(ResolvedBlockRef {
                    region_index,
                    state,
                });
            }
        }

        None
    }

    /// Regions whose bounding box covers `page`, in scene order.
    pub(crate) fn regions_for_page(&self, page: [i32; 3]) -> &[usize] {
        self.page_index.get(&page).map(Vec::as_slice).unwrap_or(&[])
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

    /// Page coordinates covered by any region, in deterministic order.
    pub fn page_keys(&self) -> impl Iterator<Item = [i32; 3]> + '_ {
        self.page_index.keys().copied()
    }
}

fn bbox_page_index(
    region_pages: &[([i32; 3], [i32; 3])],
    max_pages: usize,
) -> Result<BTreeMap<[i32; 3], Vec<usize>>, MeshError> {
    let mut page_index: BTreeMap<[i32; 3], Vec<usize>> = BTreeMap::new();
    for (region_index, (min, max)) in region_pages.iter().enumerate() {
        for x in min[0]..=max[0] {
            for y in min[1]..=max[1] {
                for z in min[2]..=max[2] {
                    let page = [x, y, z];
                    if !page_index.contains_key(&page) && page_index.len() >= max_pages {
                        return Err(page_limit(max_pages));
                    }
                    page_index.entry(page).or_default().push(region_index);
                }
            }
        }
    }
    Ok(page_index)
}

fn region_page_bounds(region: &Region) -> Result<([i32; 3], [i32; 3]), MeshError> {
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
    Ok((min, max))
}

/// Pages containing at least one non-air block in `region`.
///
/// The scan decodes the packed stream sequentially and only divides once per
/// y/z row, so sparse regions do not pay for every page in their bounding box.
fn region_occupied_pages(
    region: &Region,
    bounds: ([i32; 3], [i32; 3]),
    max_pages: usize,
) -> Result<BTreeSet<[i32; 3]>, MeshError> {
    let size_x = region.size[0] as usize;
    let size_y = region.size[1] as usize;
    let size_z = region.size[2] as usize;
    let mut pages = BTreeSet::new();
    if size_x == 0 || size_y == 0 || size_z == 0 {
        return Ok(pages);
    }

    let air: Vec<bool> = region.palette.iter().map(BlockStateKey::is_air).collect();
    if !air.iter().any(|is_air| *is_air) {
        let (min, max) = bounds;
        let count = (0..3)
            .map(|axis| u64::try_from(max[axis] - min[axis] + 1).unwrap_or(u64::MAX))
            .try_fold(1u64, |total, value| total.checked_mul(value))
            .unwrap_or(u64::MAX);
        if count > max_pages as u64 {
            return Err(page_limit(max_pages));
        }
        for x in min[0]..=max[0] {
            for y in min[1]..=max[1] {
                for z in min[2]..=max[2] {
                    pages.insert([x, y, z]);
                }
            }
        }
        return Ok(pages);
    }

    let origin_page_x = page_coord(region.origin[0])?;
    let x_offset = region.origin[0].rem_euclid(16) as u32;
    let mut last_page = None;
    let mut values = region.states.iter();
    for local_y in 0..size_y {
        let page_y = page_coord(region.origin[1] + local_y as i64)?;
        for local_z in 0..size_z {
            let page_z = page_coord(region.origin[2] + local_z as i64)?;
            let mut page_x = origin_page_x;
            let mut x_in_page = x_offset;
            for _ in 0..size_x {
                let Some(value) = values.next() else {
                    return Ok(pages);
                };
                let is_air = air.get(value as usize).copied().unwrap_or(true);
                if !is_air {
                    let page = [page_x, page_y, page_z];
                    if last_page != Some(page) {
                        pages.insert(page);
                        if pages.len() > max_pages {
                            return Err(page_limit(max_pages));
                        }
                        last_page = Some(page);
                    }
                }

                x_in_page += 1;
                if x_in_page == 16 {
                    x_in_page = 0;
                    page_x += 1;
                }
            }
        }
    }
    Ok(pages)
}

fn page_limit(max_pages: usize) -> MeshError {
    MeshError::page_limit(format!("mesh exceeds {max_pages} pages"))
}

fn region_state(region: &Region, position: [i64; 3]) -> Option<&BlockStateKey> {
    let palette_id = region_palette_id(region, position)?;
    region.palette.get(palette_id as usize)
}

/// Palette id at `position`, or `None` when the position is outside the
/// region or stores an id that the palette cannot name.
pub(crate) fn region_palette_id(region: &Region, position: [i64; 3]) -> Option<u32> {
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

    let palette_id = region.states.get(index);
    (usize::try_from(palette_id).ok()? < region.palette.len()).then_some(palette_id)
}

fn page_coord(value: i64) -> Result<i32, MeshError> {
    i32::try_from(value.div_euclid(16)).map_err(|_| {
        MeshError::coordinate(format!(
            "world coordinate {value} does not fit in i32 page space"
        ))
    })
}
