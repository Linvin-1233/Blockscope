//! Conservative greedy merging for page-local quads.
//!
//! Only axis-aligned unit faces with identical material, lighting, and UV
//! mapping are merged. Translucent faces are excluded because their draw order
//! is significant, and non-uniform ambient occlusion is excluded because a
//! larger quad cannot reproduce per-corner variation at the original cell
//! boundaries.

use crate::{MeshStatistics, NO_TEXTURE, Quad, RenderLayer, UV_SCALE};
use std::collections::HashMap;

const PAGE: usize = 16;
const CELLS: usize = PAGE * PAGE;
const NONE: u32 = u32::MAX;
const BLOCKED: u32 = u32::MAX - 1;
const EPSILON: f32 = 1e-4;

/// Textured merges are capped so Q12 UVs stay inside `i16`. One block adds
/// about one UV unit, and seven blocks keep the far corner below 8.
const MAX_TEXTURED_SPAN: usize = 7;

enum Group {
    One { cell: u16, index: u32 },
    Many(Vec<(u16, u32)>),
    Blocked,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct MergeKey {
    axis: u8,
    sign: i8,
    plane: i16,
    texture: u16,
    color: [u8; 4],
    layer: u8,
    shade: bool,
    ao: [u8; 4],
    uvs: [[i16; 2]; 4],
}

struct FaceInfo {
    axis: usize,
    sign: i8,
    plane: i16,
    u: usize,
    v: usize,
}

impl FaceInfo {
    fn cell(&self) -> usize {
        self.v * PAGE + self.u
    }
}

fn tangents(axis: usize) -> (usize, usize) {
    match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

fn normal_axis(normal: [f32; 3]) -> Option<(usize, i8)> {
    if !normal.iter().all(|value| value.is_finite()) {
        return None;
    }
    let axis = (0..3).max_by(|a, b| normal[*a].abs().total_cmp(&normal[*b].abs()))?;
    if normal.iter().filter(|value| value.abs() > 0.5).count() != 1 {
        return None;
    }
    Some((axis, if normal[axis] > 0.0 { 1 } else { -1 }))
}

fn unit_face(quad: &Quad) -> Option<FaceInfo> {
    let (axis, sign) = normal_axis(quad.normal)?;
    for (index, value) in quad.normal.iter().enumerate() {
        let expected = if index == axis { f32::from(sign) } else { 0.0 };
        if (value - expected).abs() > EPSILON {
            return None;
        }
    }
    let plane = quad.vertices[0].position[axis];
    if quad
        .vertices
        .iter()
        .any(|vertex| (vertex.position[axis] - plane).abs() > EPSILON)
    {
        return None;
    }
    let rounded_plane = plane.round();
    if (rounded_plane - plane).abs() > EPSILON {
        return None;
    }

    let (u_axis, v_axis) = tangents(axis);
    let mut min_u = f32::INFINITY;
    let mut max_u = f32::NEG_INFINITY;
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    for vertex in &quad.vertices {
        min_u = min_u.min(vertex.position[u_axis]);
        max_u = max_u.max(vertex.position[u_axis]);
        min_v = min_v.min(vertex.position[v_axis]);
        max_v = max_v.max(vertex.position[v_axis]);
    }
    if (max_u - min_u - 1.0).abs() > EPSILON || (max_v - min_v - 1.0).abs() > EPSILON {
        return None;
    }

    let u = min_u.round();
    let v = min_v.round();
    if (u - min_u).abs() > EPSILON || (v - min_v).abs() > EPSILON {
        return None;
    }
    if !(0.0..PAGE as f32).contains(&u) || !(0.0..PAGE as f32).contains(&v) {
        return None;
    }
    for vertex in &quad.vertices {
        let du = (vertex.position[u_axis] - u).round();
        let dv = (vertex.position[v_axis] - v).round();
        if !(0.0..=1.0).contains(&du)
            || !(0.0..=1.0).contains(&dv)
            || (vertex.position[u_axis] - u - du).abs() > EPSILON
            || (vertex.position[v_axis] - v - dv).abs() > EPSILON
        {
            return None;
        }
    }

    Some(FaceInfo {
        axis,
        sign,
        plane: rounded_plane as i16,
        u: u as usize,
        v: v as usize,
    })
}

fn uniform_ao(quad: &Quad) -> bool {
    quad.ao.iter().all(|value| *value == quad.ao[0])
}

fn layer_index(layer: RenderLayer) -> usize {
    match layer {
        RenderLayer::Opaque => 0,
        RenderLayer::Cutout => 1,
        RenderLayer::Translucent => 2,
    }
}

fn merge_quad(template: &Quad, u: usize, v: usize, width: usize, height: usize) -> Option<Quad> {
    let (axis, _) = normal_axis(template.normal)?;
    let (u_axis, v_axis) = tangents(axis);
    let u0 = u as f32;
    let v0 = v as f32;

    let mut du = [0i32; 4];
    let mut dv = [0i32; 4];
    let mut corners: [Option<[i32; 2]>; 4] = [None; 4];

    for (index, vertex) in template.vertices.iter().enumerate() {
        let local_u = vertex.position[u_axis] - u0;
        let local_v = vertex.position[v_axis] - v0;
        let nu = local_u.round() as i32;
        let nv = local_v.round() as i32;
        if !(0..=1).contains(&nu)
            || !(0..=1).contains(&nv)
            || (local_u - nu as f32).abs() > EPSILON
            || (local_v - nv as f32).abs() > EPSILON
        {
            return None;
        }
        du[index] = nu;
        dv[index] = nv;
        if template.texture != NO_TEXTURE {
            let corner = (nv * 2 + nu) as usize;
            if corners[corner].is_some() {
                return None;
            }
            corners[corner] = Some([i32::from(vertex.uv[0]), i32::from(vertex.uv[1])]);
        }
    }

    let mut merged = template.clone();
    for index in 0..4 {
        let global_u = du[index] * width as i32;
        let global_v = dv[index] * height as i32;
        merged.vertices[index].position[u_axis] = u0 + global_u as f32;
        merged.vertices[index].position[v_axis] = v0 + global_v as f32;
    }

    if template.texture == NO_TEXTURE {
        return Some(merged);
    }

    let uv00 = corners[0]?;
    let uv10 = corners[1]?;
    let uv01 = corners[2]?;
    let uv11 = corners[3]?;
    let delta_u = [uv10[0] - uv00[0], uv10[1] - uv00[1]];
    let delta_v = [uv01[0] - uv00[0], uv01[1] - uv00[1]];
    if uv11
        != [
            uv00[0] + delta_u[0] + delta_v[0],
            uv00[1] + delta_u[1] + delta_v[1],
        ]
    {
        return None;
    }

    // A merged quad interpolates UVs continuously. That only repeats the
    // original per-block mapping when one block advances by a whole number of
    // texture periods; sub-rect UVs would sample a different part of the
    // texture in the next cell.
    let period = UV_SCALE as i32;
    if [delta_u[0], delta_u[1], delta_v[0], delta_v[1]]
        .iter()
        .any(|delta| delta % period != 0)
    {
        return None;
    }

    for index in 0..4 {
        let global_u = du[index] * width as i32;
        let global_v = dv[index] * height as i32;
        let uv = [
            uv00[0] + global_u * delta_u[0] + global_v * delta_v[0],
            uv00[1] + global_u * delta_u[1] + global_v * delta_v[1],
        ];
        if !(i32::from(i16::MIN)..=i32::from(i16::MAX)).contains(&uv[0])
            || !(i32::from(i16::MIN)..=i32::from(i16::MAX)).contains(&uv[1])
        {
            return None;
        }
        merged.vertices[index].uv = [uv[0] as i16, uv[1] as i16];
    }

    Some(merged)
}

fn valid_index(index: u32, quad_count: usize) -> bool {
    index != NONE && index != BLOCKED && (index as usize) < quad_count
}

fn merge_group(
    quads: &mut [Quad],
    cells: &[u32; CELLS],
    removed: &mut [bool],
    removed_by_layer: &mut [u64; 3],
    max_span: usize,
) {
    let quad_count = quads.len();
    let mut visited = [false; CELLS];

    for v in 0..PAGE {
        for u in 0..PAGE {
            let start = v * PAGE + u;
            if visited[start] || !valid_index(cells[start], quad_count) {
                continue;
            }

            let mut width = 1;
            while u + width < PAGE && width < max_span {
                let cell = start + width;
                if visited[cell] || !valid_index(cells[cell], quad_count) {
                    break;
                }
                width += 1;
            }

            let mut height = 1;
            'grow: while v + height < PAGE && height < max_span {
                let row = (v + height) * PAGE + u;
                for offset in 0..width {
                    let cell = row + offset;
                    if visited[cell] || !valid_index(cells[cell], quad_count) {
                        break 'grow;
                    }
                }
                height += 1;
            }

            for row in v..v + height {
                for column in u..u + width {
                    visited[row * PAGE + column] = true;
                }
            }

            if width * height == 1 {
                continue;
            }

            let template_index = cells[start] as usize;
            let Some(merged) = merge_quad(&quads[template_index], u, v, width, height) else {
                continue;
            };
            let layer = layer_index(quads[template_index].layer);
            quads[template_index] = merged;

            for row in v..v + height {
                for column in u..u + width {
                    let cell = row * PAGE + column;
                    let index = cells[cell];
                    if index == cells[start] {
                        continue;
                    }
                    if valid_index(index, quad_count) && !removed[index as usize] {
                        removed[index as usize] = true;
                        removed_by_layer[layer] += 1;
                    }
                }
            }
        }
    }
}

/// Merge eligible unit faces in `quads` and adjust `statistics` in place.
///
/// The function is deterministic and preserves the relative order of surviving
/// quads. Quads that cannot be merged are left untouched.
pub(crate) fn merge_page_quads(quads: &mut Vec<Quad>, statistics: &mut MeshStatistics) {
    if quads.len() < 2 {
        return;
    }

    let mut groups: HashMap<MergeKey, Group> = HashMap::new();
    for (index, quad) in quads.iter().enumerate() {
        let Some(face) = unit_face(quad) else {
            continue;
        };
        if quad.layer == RenderLayer::Translucent || !uniform_ao(quad) {
            continue;
        }

        let key = MergeKey {
            axis: face.axis as u8,
            sign: face.sign,
            plane: face.plane,
            texture: quad.texture,
            color: quad.color,
            layer: layer_index(quad.layer) as u8,
            shade: quad.shade,
            ao: quad.ao,
            uvs: if quad.texture == NO_TEXTURE {
                [[0, 0]; 4]
            } else {
                [
                    quad.vertices[0].uv,
                    quad.vertices[1].uv,
                    quad.vertices[2].uv,
                    quad.vertices[3].uv,
                ]
            },
        };

        let cell = face.cell() as u16;
        let index = index as u32;
        match groups.entry(key) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(Group::One { cell, index });
            }
            std::collections::hash_map::Entry::Occupied(mut slot) => match slot.get_mut() {
                Group::One {
                    cell: first_cell,
                    index: first_index,
                } => {
                    if *first_cell == cell {
                        *slot.get_mut() = Group::Blocked;
                    } else {
                        *slot.get_mut() =
                            Group::Many(vec![(*first_cell, *first_index), (cell, index)]);
                    }
                }
                Group::Many(items) => items.push((cell, index)),
                Group::Blocked => {}
            },
        }
    }

    if groups.is_empty() {
        return;
    }

    let mut removed = vec![false; quads.len()];
    let mut removed_by_layer = [0u64; 3];
    for (key, group) in groups {
        let Group::Many(items) = group else {
            continue;
        };
        let mut cells = [NONE; CELLS];
        for (cell, index) in items {
            let slot = &mut cells[cell as usize];
            *slot = if *slot == NONE { index } else { BLOCKED };
        }
        let max_span = if key.texture == NO_TEXTURE {
            PAGE
        } else {
            MAX_TEXTURED_SPAN
        };
        merge_group(quads, &cells, &mut removed, &mut removed_by_layer, max_span);
    }

    let removed_total: u64 = removed_by_layer.iter().sum();
    if removed_total == 0 {
        return;
    }
    let original_len = quads.len();

    let mut write = 0;
    for (read, &is_removed) in removed.iter().enumerate() {
        if is_removed {
            continue;
        }
        if write != read {
            quads.swap(write, read);
        }
        write += 1;
    }
    quads.truncate(write);
    // Reclaim page memory only when merging actually removed a large fraction
    // of the page; tiny reductions are not worth the copy on random palettes.
    if removed_total * 10 >= original_len as u64 {
        quads.shrink_to_fit();
    }

    statistics.quads = statistics.quads.saturating_sub(removed_total);
    statistics.vertices = statistics.vertices.saturating_sub(removed_total * 4);
    statistics.opaque_quads = statistics.opaque_quads.saturating_sub(removed_by_layer[0]);
    statistics.cutout_quads = statistics.cutout_quads.saturating_sub(removed_by_layer[1]);
    statistics.translucent_quads = statistics
        .translucent_quads
        .saturating_sub(removed_by_layer[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Vertex, ambient_occlusion_level};

    fn unit_quad(x: usize, y: usize, texture: u16) -> Quad {
        let x = x as f32;
        let y = y as f32;
        let vertices = [
            Vertex {
                position: [x, y, 0.0],
                uv: [0, 0],
            },
            Vertex {
                position: [x + 1.0, y, 0.0],
                uv: [4096, 0],
            },
            Vertex {
                position: [x + 1.0, y + 1.0, 0.0],
                uv: [4096, 4096],
            },
            Vertex {
                position: [x, y + 1.0, 0.0],
                uv: [0, 4096],
            },
        ];
        Quad {
            vertices,
            normal: [0.0, 0.0, -1.0],
            color: [255, 255, 255, 255],
            ao: [3, 3, 3, 3],
            texture,
            layer: RenderLayer::Opaque,
            shade: false,
        }
    }

    fn stats_for(quads: &[Quad]) -> MeshStatistics {
        let mut statistics = MeshStatistics::default();
        for quad in quads {
            statistics.add_quad(quad.layer);
        }
        statistics
    }

    fn merge(quads: &mut Vec<Quad>) -> MeshStatistics {
        let mut statistics = stats_for(quads);
        merge_page_quads(quads, &mut statistics);
        statistics
    }

    #[test]
    fn merges_adjacent_unit_faces() {
        let mut quads = vec![
            unit_quad(0, 0, 1),
            unit_quad(1, 0, 1),
            unit_quad(0, 1, 1),
            unit_quad(1, 1, 1),
        ];
        let stats = merge(&mut quads);

        assert_eq!(quads.len(), 1);
        assert_eq!(stats.quads, 1);
        assert_eq!(stats.vertices, 4);
        assert_eq!(stats.opaque_quads, 1);
        let merged = &quads[0];
        assert_eq!(merged.vertices[0].position, [0.0, 0.0, 0.0]);
        assert_eq!(merged.vertices[1].position, [2.0, 0.0, 0.0]);
        assert_eq!(merged.vertices[2].position, [2.0, 2.0, 0.0]);
        assert_eq!(merged.vertices[3].position, [0.0, 2.0, 0.0]);
        assert_eq!(merged.vertices[2].uv, [8192, 8192]);
        assert_eq!(ambient_occlusion_level(merged.ao[0]), 1.0);
    }

    #[test]
    fn keeps_faces_with_different_textures() {
        let mut quads = vec![unit_quad(0, 0, 1), unit_quad(1, 0, 2)];
        let stats = merge(&mut quads);
        assert_eq!(quads.len(), 2);
        assert_eq!(stats.quads, 2);
    }

    #[test]
    fn keeps_faces_with_non_uniform_ao() {
        let mut quads = vec![unit_quad(0, 0, 1), unit_quad(1, 0, 1)];
        quads[0].ao = [3, 3, 3, 2];
        let stats = merge(&mut quads);
        assert_eq!(quads.len(), 2);
        assert_eq!(stats.quads, 2);
    }

    #[test]
    fn keeps_translucent_faces() {
        let mut quads = vec![unit_quad(0, 0, 1), unit_quad(1, 0, 1)];
        for quad in &mut quads {
            quad.layer = RenderLayer::Translucent;
        }
        let stats = merge(&mut quads);
        assert_eq!(quads.len(), 2);
        assert_eq!(stats.translucent_quads, 2);
    }

    #[test]
    fn caps_textured_spans_to_keep_q12_uvs_in_range() {
        let mut quads = Vec::new();
        for y in 0..8 {
            for x in 0..8 {
                quads.push(unit_quad(x, y, 1));
            }
        }
        merge(&mut quads);
        assert_eq!(quads.len(), 4);
        assert!(quads.iter().all(|quad| {
            quad.vertices
                .iter()
                .all(|vertex| vertex.uv[0] <= 7 * 4096 && vertex.uv[1] <= 7 * 4096)
        }));
    }

    #[test]
    fn untextured_faces_can_merge_across_the_whole_page() {
        let mut quads = Vec::new();
        for y in 0..8 {
            for x in 0..8 {
                quads.push(unit_quad(x, y, NO_TEXTURE));
            }
        }
        merge(&mut quads);
        assert_eq!(quads.len(), 1);
    }

    #[test]
    fn keeps_faces_with_sub_rect_uvs() {
        let mut quads = vec![unit_quad(0, 0, 1), unit_quad(1, 0, 1)];
        for quad in &mut quads {
            quad.vertices[1].uv = [2048, 0];
            quad.vertices[2].uv = [2048, 4096];
            quad.vertices[3].uv = [0, 4096];
        }
        let stats = merge(&mut quads);
        assert_eq!(quads.len(), 2);
        assert_eq!(stats.quads, 2);
    }

    #[test]
    fn keeps_faces_that_are_not_unit_squares() {
        let mut quads = vec![unit_quad(0, 0, 1), unit_quad(1, 0, 1)];
        quads[1].vertices[2].position[0] = 1.5;
        let stats = merge(&mut quads);
        assert_eq!(quads.len(), 2);
        assert_eq!(stats.quads, 2);
    }

    #[test]
    fn duplicate_quads_do_not_disappear() {
        let mut quads = vec![unit_quad(0, 0, 1), unit_quad(0, 0, 1), unit_quad(1, 0, 1)];
        let stats = merge(&mut quads);
        assert_eq!(quads.len(), 3);
        assert_eq!(stats.quads, 3);
    }
}
