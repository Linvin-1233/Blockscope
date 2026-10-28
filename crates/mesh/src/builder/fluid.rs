//! Fluid surface geometry.
//!
//! Vanilla water and lava do not render as per-block boxes: each top corner
//! averages the height of the four fluid columns touching it, shared faces
//! disappear, and a fluid with the same fluid above becomes a full block. This
//! module reproduces that shape from the page grid so flowing water reads as a
//! continuous slope instead of a staircase.

use super::{PageBuild, PageMode, push_quad};
use crate::catalog::ResolvedBlock;
use crate::color::Rgba;
use crate::model::{FaceDirection, ModelQuad, NO_TEXTURE, face_uv};
use crate::page::{PageGrid, PaletteTables};
use crate::shape::{FluidKind, FluidState};
use crate::{Quad, Vertex, encode_uv};

/// Height in ninths for a full block (same fluid above).
const FULL_NINTHS: i16 = 9;
/// Sentinel for an opaque full cube next to the fluid.
const SOLID: i16 = -1;
/// Height used by the vanilla corner weighting for source-like columns.
const SOURCE_NINTHS: i16 = 8;

/// Emit the visible faces of one fluid block.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_fluid_block(
    page: &mut PageBuild,
    grid: &PageGrid,
    tables: &PaletteTables,
    local_index: usize,
    local_position: [f32; 3],
    style: &ResolvedBlock,
    fluid: FluidState,
    mode: PageMode,
) {
    let build = mode == PageMode::Build;
    let above = offset_index(local_index, [0, 1, 0]);
    let below = offset_index(local_index, [0, -1, 0]);
    let above_same = is_fluid(grid, tables, above, fluid.kind);
    let below_same = is_fluid(grid, tables, below, fluid.kind);
    let below_occludes = occludes(grid, tables, below);

    let north_index = offset_index(local_index, [0, 0, -1]);
    let south_index = offset_index(local_index, [0, 0, 1]);
    let west_index = offset_index(local_index, [-1, 0, 0]);
    let east_index = offset_index(local_index, [1, 0, 0]);
    let north_visible =
        !is_fluid(grid, tables, north_index, fluid.kind) && !occludes(grid, tables, north_index);
    let south_visible =
        !is_fluid(grid, tables, south_index, fluid.kind) && !occludes(grid, tables, south_index);
    let west_visible =
        !is_fluid(grid, tables, west_index, fluid.kind) && !occludes(grid, tables, west_index);
    let east_visible =
        !is_fluid(grid, tables, east_index, fluid.kind) && !occludes(grid, tables, east_index);

    // Corner heights are only needed when vertices are actually emitted; the
    // count pass only decides which faces survive.
    let (nw, ne, sw, se) = if build {
        let height = |offset: [i32; 3]| {
            height_at(grid, tables, offset_index(local_index, offset), fluid.kind)
        };
        let self_height = height([0, 0, 0]);
        if self_height >= FULL_NINTHS {
            (1.0, 1.0, 1.0, 1.0)
        } else {
            let north = height([0, 0, -1]);
            let south = height([0, 0, 1]);
            let west = height([-1, 0, 0]);
            let east = height([1, 0, 0]);
            (
                corner(self_height, north, west, height([-1, 0, -1])),
                corner(self_height, north, east, height([1, 0, -1])),
                corner(self_height, south, west, height([-1, 0, 1])),
                corner(self_height, south, east, height([1, 0, 1])),
            )
        }
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    if !above_same {
        push_face(
            page,
            build,
            local_position,
            [
                [0.0, nw * 16.0, 0.0],
                [0.0, sw * 16.0, 16.0],
                [16.0, se * 16.0, 16.0],
                [16.0, ne * 16.0, 0.0],
            ],
            FaceDirection::Up,
            style,
        );
    }

    if !below_same && !below_occludes {
        push_face(
            page,
            build,
            local_position,
            [
                [0.0, 0.0, 16.0],
                [0.0, 0.0, 0.0],
                [16.0, 0.0, 0.0],
                [16.0, 0.0, 16.0],
            ],
            FaceDirection::Down,
            style,
        );
    }

    if north_visible {
        push_face(
            page,
            build,
            local_position,
            [
                [0.0, 0.0, 0.0],
                [0.0, nw * 16.0, 0.0],
                [16.0, ne * 16.0, 0.0],
                [16.0, 0.0, 0.0],
            ],
            FaceDirection::North,
            style,
        );
    }

    if south_visible {
        push_face(
            page,
            build,
            local_position,
            [
                [16.0, 0.0, 16.0],
                [16.0, se * 16.0, 16.0],
                [0.0, sw * 16.0, 16.0],
                [0.0, 0.0, 16.0],
            ],
            FaceDirection::South,
            style,
        );
    }

    if west_visible {
        push_face(
            page,
            build,
            local_position,
            [
                [0.0, 0.0, 16.0],
                [0.0, sw * 16.0, 16.0],
                [0.0, nw * 16.0, 0.0],
                [0.0, 0.0, 0.0],
            ],
            FaceDirection::West,
            style,
        );
    }

    if east_visible {
        push_face(
            page,
            build,
            local_position,
            [
                [16.0, 0.0, 0.0],
                [16.0, ne * 16.0, 0.0],
                [16.0, se * 16.0, 16.0],
                [16.0, 0.0, 16.0],
            ],
            FaceDirection::East,
            style,
        );
    }
}

fn offset_index(index: usize, offset: [i32; 3]) -> usize {
    index.wrapping_add_signed(PageGrid::offset_delta(offset))
}

fn is_fluid(grid: &PageGrid, tables: &PaletteTables, index: usize, kind: FluidKind) -> bool {
    tables
        .block(grid.cell_at_index(index))
        .and_then(|block| block.fluid)
        .is_some_and(|fluid| fluid.kind == kind)
}

fn occludes(grid: &PageGrid, tables: &PaletteTables, index: usize) -> bool {
    tables
        .block(grid.cell_at_index(index))
        .is_some_and(|block| block.occludes)
}

/// Vanilla corner height: source-like columns dominate the average so a flat
/// pool stays flat, while lower flowing columns pull the corner down.
fn corner(self_height: i16, a: i16, b: i16, diagonal: i16) -> f32 {
    if a >= FULL_NINTHS || b >= FULL_NINTHS {
        return 1.0;
    }

    let mut sum = 0.0f32;
    let mut weight = 0.0f32;
    let mut add = |height: i16| {
        if height < 0 {
            return;
        }
        let value = f32::from(height) / 9.0;
        let cell_weight = if height >= SOURCE_NINTHS { 10.0 } else { 1.0 };
        sum += value * cell_weight;
        weight += cell_weight;
    };

    if a > 0 || b > 0 {
        if diagonal >= FULL_NINTHS {
            return 1.0;
        }
        add(diagonal);
    }
    add(self_height);
    add(a);
    add(b);

    if weight > 0.0 { sum / weight } else { 0.0 }
}

/// Height of a column in ninths, or `SOLID` for a full opaque cube. Air and
/// partial blocks count as empty columns.
fn height_at(grid: &PageGrid, tables: &PaletteTables, index: usize, kind: FluidKind) -> i16 {
    let Some(block) = tables.block(grid.cell_at_index(index)) else {
        return 0;
    };
    if block.is_air {
        return 0;
    }

    if let Some(fluid) = block.fluid {
        if fluid.kind != kind {
            return 0;
        }
        let above = offset_index(index, [0, 1, 0]);
        if is_fluid(grid, tables, above, kind) {
            return FULL_NINTHS;
        }
        return i16::from(fluid.height_ninths);
    }

    if block.occludes { SOLID } else { 0 }
}

fn push_face(
    page: &mut PageBuild,
    build: bool,
    local_position: [f32; 3],
    model_vertices: [[f32; 3]; 4],
    face: FaceDirection,
    style: &ResolvedBlock,
) {
    let template = face_template(style, face);
    let texture = template.map_or(NO_TEXTURE, |quad| quad.texture);
    let tint = template.map_or(Rgba::WHITE, |quad| quad.tint);
    let layer = template.and_then(|quad| quad.layer).unwrap_or(style.layer);
    let shade = template.is_none_or(|quad| quad.shade);

    page.statistics.add_quad(layer);
    if !build {
        return;
    }

    let uv = face_uv(face, model_vertices);
    let vertices = std::array::from_fn(|index| {
        let corner = model_vertices[index];
        Vertex {
            position: [
                local_position[0] + corner[0] / 16.0,
                local_position[1] + corner[1] / 16.0,
                local_position[2] + corner[2] / 16.0,
            ],
            uv: encode_uv(uv[index]),
        }
    });
    let color = if texture == NO_TEXTURE {
        style.color
    } else {
        tint
    };

    push_quad(
        &mut page.quads,
        Quad {
            vertices,
            normal: face.normal(),
            color: color.to_array(),
            ao: [3, 3, 3, 3],
            texture,
            layer,
            shade,
        },
    );
}

fn face_template(style: &ResolvedBlock, face: FaceDirection) -> Option<&ModelQuad> {
    style.model.quads.iter().find(|quad| quad.face == face)
}
