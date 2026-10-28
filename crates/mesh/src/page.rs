//! Page-local block cache for the mesh builder.
//!
//! A page covers 16x16x16 blocks. Culling and ambient occlusion ask about the
//! same block and its neighbours several times per model quad, so resolving
//! every query through the scene region index and the material catalog makes
//! those lookups dominate meshing time. This module resolves a page plus a
//! one-block border once and then serves the inner loop from an array.

use crate::catalog::{BlockCatalog, ResolvedBlock};
use crate::model::FaceDirection;
use crate::shape::{self, FluidState};
use crate::view::{SceneBlockView, region_palette_id};
use litematic_scene::{BlockStateKey, Scene};
use std::collections::HashMap;

const PAGE: usize = 16;
const GRID: usize = PAGE + 2;

/// A block reference stored in a page grid: a region index and a palette id.
#[derive(Clone, Copy)]
pub(crate) struct Cell {
    pub(crate) region: u32,
    pub(crate) palette: u32,
}

impl Cell {
    pub(crate) const EMPTY: Self = Self {
        region: u32::MAX,
        palette: 0,
    };

    pub(crate) const fn is_empty(self) -> bool {
        self.region == u32::MAX
    }
}

/// One region palette entry after material resolution.
pub(crate) struct PaletteBlock {
    pub(crate) block: ResolvedBlock,
    /// Canonical state id shared by equal states in every region.
    pub(crate) state_id: u32,
    pub(crate) is_air: bool,
    pub(crate) occludes: bool,
    /// True when every model quad has a cullface, so an occluded neighbourhood
    /// removes the whole model without visiting its faces.
    pub(crate) all_quads_cullable: bool,
    /// Fluid shape for a pure fluid or the water layer of a waterlogged block.
    pub(crate) fluid: Option<FluidState>,
    /// Waterlogged blocks keep their own model and add a shaped water layer.
    pub(crate) waterlogged: bool,
}

/// Resolved palettes for every region in the scene.
pub(crate) struct PaletteTables {
    regions: Vec<Vec<PaletteBlock>>,
    water_style: Option<ResolvedBlock>,
}

impl PaletteTables {
    pub(crate) fn new<C: BlockCatalog + ?Sized>(scene: &Scene, catalog: &C) -> Self {
        let mut state_ids: HashMap<&BlockStateKey, u32> = HashMap::new();
        let mut regions = Vec::with_capacity(scene.regions.len());
        let mut needs_water_style = false;

        for region in &scene.regions {
            let mut blocks = Vec::with_capacity(region.palette.len());
            for state in &region.palette {
                let next_id = state_ids.len() as u32;
                let state_id = *state_ids.entry(state).or_insert(next_id);
                let block = catalog.resolve(state);
                let is_air = state.is_air();
                let occludes = block.occludes();
                let all_quads_cullable = !block.model.quads.is_empty()
                    && block.model.quads.iter().all(|quad| quad.cullface.is_some());
                let pure_fluid = shape::fluid_state(state);
                let waterlogged = pure_fluid.is_none() && !occludes && shape::waterlogged(state);
                let fluid = pure_fluid.or_else(|| {
                    if waterlogged {
                        shape::waterlogged_fluid(state)
                    } else {
                        None
                    }
                });
                needs_water_style |= waterlogged;

                blocks.push(PaletteBlock {
                    block,
                    state_id,
                    is_air,
                    occludes,
                    all_quads_cullable,
                    fluid,
                    waterlogged,
                });
            }
            regions.push(blocks);
        }

        let water_style =
            needs_water_style.then(|| catalog.resolve(&BlockStateKey::simple("minecraft:water")));
        Self {
            regions,
            water_style,
        }
    }

    pub(crate) fn water_style(&self) -> Option<&ResolvedBlock> {
        self.water_style.as_ref()
    }

    pub(crate) fn block(&self, cell: Cell) -> Option<&PaletteBlock> {
        if cell.is_empty() {
            return None;
        }
        self.regions
            .get(cell.region as usize)?
            .get(cell.palette as usize)
    }
}

/// A 16x16x16 page plus its one-block border, resolved in one pass.
pub(crate) struct PageGrid {
    cells: Vec<Cell>,
    occludes: Vec<bool>,
    inner_non_air: u32,
}

impl PageGrid {
    pub(crate) fn fill(
        scene: &Scene,
        view: &SceneBlockView<'_>,
        tables: &PaletteTables,
        page_origin: [i32; 3],
    ) -> Self {
        let base = [
            i64::from(page_origin[0]),
            i64::from(page_origin[1]),
            i64::from(page_origin[2]),
        ];
        let base_page = [
            base[0].div_euclid(16) as i32,
            base[1].div_euclid(16) as i32,
            base[2].div_euclid(16) as i32,
        ];

        // Resolve the candidate regions for the 3x3x3 page neighbourhood once.
        let empty: &[usize] = &[];
        let mut page_regions = [[[empty; 3]; 3]; 3];
        for (ox, x_slice) in page_regions.iter_mut().enumerate() {
            for (oy, y_slice) in x_slice.iter_mut().enumerate() {
                for (oz, slot) in y_slice.iter_mut().enumerate() {
                    *slot = view.regions_for_page([
                        base_page[0].saturating_add(ox as i32 - 1),
                        base_page[1].saturating_add(oy as i32 - 1),
                        base_page[2].saturating_add(oz as i32 - 1),
                    ]);
                }
            }
        }

        let mut grid = Self {
            cells: vec![Cell::EMPTY; GRID * GRID * GRID],
            occludes: vec![false; GRID * GRID * GRID],
            inner_non_air: 0,
        };
        for ly in 0..GRID {
            for lz in 0..GRID {
                for lx in 0..GRID {
                    let candidates =
                        page_regions[page_offset(lx)][page_offset(ly)][page_offset(lz)];
                    if candidates.is_empty() {
                        continue;
                    }

                    let world = [
                        base[0] + lx as i64 - 1,
                        base[1] + ly as i64 - 1,
                        base[2] + lz as i64 - 1,
                    ];
                    for &region_index in candidates.iter().rev() {
                        let region = &scene.regions[region_index];
                        if let Some(palette) = region_palette_id(region, world) {
                            grid.set(
                                [lx, ly, lz],
                                Cell {
                                    region: region_index as u32,
                                    palette,
                                },
                                tables,
                            );
                            break;
                        }
                    }
                }
            }
        }
        grid
    }

    pub(crate) fn get(&self, local: [usize; 3]) -> Cell {
        self.cells[Self::local_index(local)]
    }

    /// Flat index of a page-local cell.
    pub(crate) fn local_index(local: [usize; 3]) -> usize {
        local[0] + GRID * (local[2] + GRID * local[1])
    }

    /// Flat index delta for an offset whose components are in `-1..=1`.
    pub(crate) fn offset_delta(offset: [i32; 3]) -> isize {
        offset[0] as isize
            + GRID as isize * (offset[2] as isize + GRID as isize * offset[1] as isize)
    }

    pub(crate) fn face_delta(face: FaceDirection) -> isize {
        let offset = face.offset();
        Self::offset_delta([offset[0] as i32, offset[1] as i32, offset[2] as i32])
    }

    pub(crate) fn cell_at_index(&self, index: usize) -> Cell {
        self.cells[index]
    }

    pub(crate) fn occludes_index(&self, index: usize) -> bool {
        self.occludes[index]
    }

    /// True when all six neighbours are present and occlude.
    ///
    /// `local_index` must belong to a block cell, not the page border, so every
    /// face offset stays inside the grid.
    pub(crate) fn all_neighbors_occlude(&self, local_index: usize) -> bool {
        FaceDirection::ALL.into_iter().all(|face| {
            self.occludes_index(local_index.wrapping_add_signed(Self::face_delta(face)))
        })
    }

    fn set(&mut self, local: [usize; 3], cell: Cell, tables: &PaletteTables) {
        let index = Self::local_index(local);
        self.cells[index] = cell;
        let block = tables.block(cell);
        self.occludes[index] = block.is_some_and(|block| block.occludes);
        let inner = local.iter().all(|value| (1..GRID - 1).contains(value));
        if inner && block.is_some_and(|block| !block.is_air) {
            self.inner_non_air += 1;
        }
    }

    /// Non-air blocks inside the page, excluding the one-block border.
    pub(crate) fn inner_non_air(&self) -> u32 {
        self.inner_non_air
    }
}

fn page_offset(index: usize) -> usize {
    if index == 0 {
        0
    } else if index == GRID - 1 {
        2
    } else {
        1
    }
}
