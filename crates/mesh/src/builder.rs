use crate::catalog::BlockCatalog;
use crate::model::{ModelQuad, NO_TEXTURE};
use crate::view::SceneBlockView;
use crate::{Mesh, MeshError, MeshLimits, MeshPage, MeshStatistics, Quad, Vertex, encode_uv};
use litematic_scene::{BlockStateKey, Scene};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread;

const MAX_WORKERS: usize = 256;

/// Build a page-local mesh from a scene using the supplied catalog.
pub fn build_mesh(
    scene: &Scene,
    catalog: &dyn BlockCatalog,
    limits: &MeshLimits,
) -> Result<Mesh, MeshError> {
    let view = SceneBlockView::new(scene, limits)?;
    let mut pages: BTreeMap<[i32; 3], Vec<Quad>> = BTreeMap::new();
    let mut statistics = MeshStatistics::default();

    for coord in view.page_keys().collect::<Vec<_>>() {
        let origin = page_origin_of(coord)?;
        let page = build_page(scene, &view, catalog, origin)?;
        add_page(&mut pages, &mut statistics, page, limits)?;
    }

    Ok(finish(pages, statistics))
}

/// Build the same mesh as [`build_mesh`] with pages spread across threads.
///
/// Pages are merged in origin order, so the result is identical to the serial
/// builder. The effective worker count is capped at 256. The catalog must be
/// `Sync`; use `MaterialSnapshot` or `ColorSnapshot` rather than the mutable
/// catalogs.
pub fn build_mesh_parallel<C>(
    scene: &Scene,
    catalog: &C,
    limits: &MeshLimits,
    threads: u32,
) -> Result<Mesh, MeshError>
where
    C: BlockCatalog + Sync,
{
    // Avoid building the page index twice when the request is effectively serial.
    if worker_count(threads, usize::MAX) <= 1 {
        return build_mesh(scene, catalog, limits);
    }

    let view = SceneBlockView::new(scene, limits)?;
    let keys = view.page_keys().collect::<Vec<_>>();
    let workers = worker_count(threads, keys.len());
    if workers <= 1 {
        return build_mesh(scene, catalog, limits);
    }

    let cursor = AtomicUsize::new(0);
    let abort = AtomicBool::new(false);
    let quads = AtomicU64::new(0);
    let vertices = AtomicU64::new(0);
    let page_count = AtomicU64::new(0);

    let results: Result<Vec<PageBuild>, MeshError> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| -> Result<Vec<PageBuild>, MeshError> {
                    let mut local = Vec::new();
                    loop {
                        if abort.load(Ordering::Relaxed) {
                            break;
                        }
                        let index = cursor.fetch_add(1, Ordering::Relaxed);
                        let Some(&coord) = keys.get(index) else {
                            break;
                        };

                        let origin = page_origin_of(coord)?;
                        let page = build_page(scene, &view, catalog, origin)?;
                        if !page.quads.is_empty() {
                            let count = page.quads.len() as u64;
                            let total_quads = quads.fetch_add(count, Ordering::Relaxed) + count;
                            let total_vertices =
                                vertices.fetch_add(count * 4, Ordering::Relaxed) + count * 4;
                            let total_pages = page_count.fetch_add(1, Ordering::Relaxed) + 1;
                            if total_quads > limits.max_quads as u64
                                || total_vertices > limits.max_vertices as u64
                                || total_pages > limits.max_pages as u64
                            {
                                abort.store(true, Ordering::Relaxed);
                            }
                        }
                        local.push(page);
                    }
                    Ok(local)
                })
            })
            .collect();

        let mut all = Vec::new();
        for handle in handles {
            all.extend(handle.join().expect("mesh worker panicked")?);
        }
        Ok(all)
    });

    if abort.load(Ordering::Relaxed) {
        return Err(limit_error(
            quads.load(Ordering::Relaxed),
            vertices.load(Ordering::Relaxed),
            page_count.load(Ordering::Relaxed),
            limits,
        ));
    }

    let mut all = results?;
    all.sort_by_key(|page| page.origin);

    let mut pages = BTreeMap::new();
    let mut statistics = MeshStatistics::default();
    for page in all {
        merge_page(&mut pages, &mut statistics, page);
    }

    Ok(finish(pages, statistics))
}

struct PageBuild {
    origin: [i32; 3],
    quads: Vec<Quad>,
    statistics: MeshStatistics,
}

fn build_page<C: BlockCatalog + ?Sized>(
    scene: &Scene,
    view: &SceneBlockView<'_>,
    catalog: &C,
    origin: [i32; 3],
) -> Result<PageBuild, MeshError> {
    let base = [
        i64::from(origin[0]),
        i64::from(origin[1]),
        i64::from(origin[2]),
    ];
    let end = [base[0] + 16, base[1] + 16, base[2] + 16];
    let mut page = PageBuild {
        origin,
        quads: Vec::new(),
        statistics: MeshStatistics::default(),
    };

    for (region_index, region) in scene.regions.iter().enumerate() {
        let region_end = [
            region.origin[0] + i64::from(region.size[0]),
            region.origin[1] + i64::from(region.size[1]),
            region.origin[2] + i64::from(region.size[2]),
        ];
        let start = [
            base[0].max(region.origin[0]),
            base[1].max(region.origin[1]),
            base[2].max(region.origin[2]),
        ];
        let stop = [
            end[0].min(region_end[0]),
            end[1].min(region_end[1]),
            end[2].min(region_end[2]),
        ];
        if (0..3).any(|axis| start[axis] >= stop[axis]) {
            continue;
        }

        for y in start[1]..stop[1] {
            for z in start[2]..stop[2] {
                for x in start[0]..stop[0] {
                    let world = [x, y, z];
                    let Some(resolved) = view.resolve(world) else {
                        continue;
                    };
                    if resolved.region_index != region_index {
                        continue;
                    }

                    page.statistics.blocks_visited += 1;
                    if resolved.state.is_air() {
                        continue;
                    }

                    page.statistics.blocks_rendered += 1;
                    let block = catalog.resolve(resolved.state);
                    if !block.known {
                        page.statistics.unknown_blocks += 1;
                    }

                    let local = [
                        (world[0] - base[0]) as f32,
                        (world[1] - base[1]) as f32,
                        (world[2] - base[2]) as f32,
                    ];

                    for model_quad in &block.model.quads {
                        if should_cull(view, catalog, resolved.state, world, model_quad) {
                            continue;
                        }

                        let layer = model_quad.layer.unwrap_or(block.layer);
                        page.statistics.add_quad(layer);

                        let color = if model_quad.texture == NO_TEXTURE {
                            block.color
                        } else {
                            model_quad.tint
                        };
                        let ambient_occlusion = block.model.ambient_occlusion;
                        let vertices = std::array::from_fn(|index| {
                            let corner = model_quad.vertices[index];
                            Vertex {
                                position: [
                                    local[0] + corner[0] / 16.0,
                                    local[1] + corner[1] / 16.0,
                                    local[2] + corner[2] / 16.0,
                                ],
                                normal: model_quad.normal,
                                color: color.to_array(),
                                uv: encode_uv(model_quad.uv[index]),
                                texture: model_quad.texture,
                                shade: model_quad.shade,
                                ao: if ambient_occlusion {
                                    vertex_ao(view, catalog, world, model_quad, corner)
                                } else {
                                    3
                                },
                            }
                        });

                        page.quads.push(Quad { vertices, layer });
                    }
                }
            }
        }
    }

    Ok(page)
}

fn add_page(
    pages: &mut BTreeMap<[i32; 3], Vec<Quad>>,
    statistics: &mut MeshStatistics,
    page: PageBuild,
    limits: &MeshLimits,
) -> Result<(), MeshError> {
    if page.quads.is_empty() {
        merge_page(pages, statistics, page);
        return Ok(());
    }

    let quads = page.quads.len() as u64;
    if statistics.quads + quads > limits.max_quads as u64 {
        return Err(MeshError::limit(format!(
            "mesh exceeds {} quads",
            limits.max_quads
        )));
    }
    if statistics.vertices + quads * 4 > limits.max_vertices as u64 {
        return Err(MeshError::limit(format!(
            "mesh exceeds {} vertices",
            limits.max_vertices
        )));
    }
    if !pages.contains_key(&page.origin) && pages.len() >= limits.max_pages {
        return Err(MeshError::page_limit(format!(
            "mesh exceeds {} pages",
            limits.max_pages
        )));
    }

    merge_page(pages, statistics, page);
    Ok(())
}

fn merge_page(
    pages: &mut BTreeMap<[i32; 3], Vec<Quad>>,
    statistics: &mut MeshStatistics,
    page: PageBuild,
) {
    statistics.blocks_visited += page.statistics.blocks_visited;
    statistics.blocks_rendered += page.statistics.blocks_rendered;
    statistics.unknown_blocks += page.statistics.unknown_blocks;
    statistics.quads += page.statistics.quads;
    statistics.vertices += page.statistics.vertices;
    statistics.opaque_quads += page.statistics.opaque_quads;
    statistics.cutout_quads += page.statistics.cutout_quads;
    statistics.translucent_quads += page.statistics.translucent_quads;

    if !page.quads.is_empty() {
        pages.insert(page.origin, page.quads);
    }
}

fn finish(pages: BTreeMap<[i32; 3], Vec<Quad>>, mut statistics: MeshStatistics) -> Mesh {
    statistics.pages = pages.len() as u64;
    let pages = pages
        .into_iter()
        .map(|(origin, quads)| MeshPage { origin, quads })
        .collect();
    Mesh { pages, statistics }
}

fn limit_error(quads: u64, vertices: u64, pages: u64, limits: &MeshLimits) -> MeshError {
    if quads > limits.max_quads as u64 {
        MeshError::limit(format!("mesh exceeds {} quads", limits.max_quads))
    } else if vertices > limits.max_vertices as u64 {
        MeshError::limit(format!("mesh exceeds {} vertices", limits.max_vertices))
    } else {
        debug_assert!(pages > limits.max_pages as u64);
        MeshError::page_limit(format!("mesh exceeds {} pages", limits.max_pages))
    }
}

fn worker_count(requested: u32, page_count: usize) -> usize {
    let requested = if requested == 0 {
        thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
    } else {
        requested as usize
    };

    requested.clamp(1, MAX_WORKERS.min(page_count.max(1)))
}

fn page_origin_of(coord: [i32; 3]) -> Result<[i32; 3], MeshError> {
    let mut origin = [0i32; 3];
    for (axis, value) in origin.iter_mut().enumerate() {
        let page = i64::from(coord[axis]) * 16;
        *value = i32::try_from(page).map_err(|_| {
            MeshError::coordinate(format!("page origin {page} does not fit in i32"))
        })?;
    }
    Ok(origin)
}

fn should_cull<C: BlockCatalog + ?Sized>(
    view: &SceneBlockView<'_>,
    catalog: &C,
    state: &BlockStateKey,
    world: [i64; 3],
    quad: &ModelQuad,
) -> bool {
    let Some(cullface) = quad.cullface else {
        return false;
    };

    let offset = cullface.offset();
    let neighbor_position = [
        world[0] + offset[0],
        world[1] + offset[1],
        world[2] + offset[2],
    ];

    let Some(neighbor) = view.resolve(neighbor_position) else {
        return false;
    };
    if neighbor.state.is_air() {
        return false;
    }

    let neighbor_block = catalog.resolve(neighbor.state);
    if neighbor_block.occludes() {
        return true;
    }

    neighbor.state == state
}

fn vertex_ao<C: BlockCatalog + ?Sized>(
    view: &SceneBlockView<'_>,
    catalog: &C,
    world: [i64; 3],
    quad: &ModelQuad,
    corner: [f32; 3],
) -> u8 {
    let Some(face) = quad.cullface else {
        return 3;
    };

    let (u_axis, v_axis) = face.tangents();
    let u = corner[u_axis];
    let v = corner[v_axis];
    if !(u == 0.0 || u == 16.0) || !(v == 0.0 || v == 16.0) {
        return 3;
    }

    let du = if u == 16.0 { 1 } else { -1 };
    let dv = if v == 16.0 { 1 } else { -1 };
    let normal = face.offset();
    let side1 = shifted(world, normal, u_axis, du);
    let side2 = shifted(world, normal, v_axis, dv);
    let corner_pos = {
        let mut pos = shifted(world, normal, u_axis, du);
        pos[v_axis] += dv;
        pos
    };

    let s1 = occludes(view, catalog, side1);
    let s2 = occludes(view, catalog, side2);
    let c = occludes(view, catalog, corner_pos);
    if s1 && s2 {
        return 0;
    }

    match u8::from(s1) + u8::from(s2) + u8::from(c) {
        0 => 3,
        1 => 2,
        2 => 1,
        _ => 0,
    }
}

fn shifted(world: [i64; 3], base: [i64; 3], axis: usize, delta: i64) -> [i64; 3] {
    let mut position = [world[0] + base[0], world[1] + base[1], world[2] + base[2]];
    position[axis] += delta;
    position
}

fn occludes<C: BlockCatalog + ?Sized>(
    view: &SceneBlockView<'_>,
    catalog: &C,
    position: [i64; 3],
) -> bool {
    let Some(neighbor) = view.resolve(position) else {
        return false;
    };
    if neighbor.state.is_air() {
        return false;
    }
    catalog.resolve(neighbor.state).occludes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_count_is_capped() {
        assert_eq!(worker_count(u32::MAX, usize::MAX), MAX_WORKERS);
        assert_eq!(worker_count(1, 100), 1);
        assert_eq!(worker_count(0, 1), 1);
        assert!(worker_count(0, usize::MAX) <= MAX_WORKERS);
    }
}
