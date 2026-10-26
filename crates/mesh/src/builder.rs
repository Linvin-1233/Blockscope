use crate::catalog::BlockCatalog;
use crate::greedy::merge_page_quads;
use crate::model::{ModelQuad, NO_TEXTURE};
use crate::page::{PageGrid, PaletteBlock, PaletteTables};
use crate::view::SceneBlockView;
use crate::{
    Mesh, MeshBuildOptions, MeshError, MeshLimits, MeshPage, MeshStatistics, Quad, Vertex,
    encode_uv,
};
use litematic_scene::Scene;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

const MAX_WORKERS: usize = 256;
const STREAM_PAGES_PER_CHUNK: usize = 32;

/// Build a page-local mesh from a scene using the supplied catalog.
pub fn build_mesh(
    scene: &Scene,
    catalog: &dyn BlockCatalog,
    limits: &MeshLimits,
) -> Result<Mesh, MeshError> {
    build_mesh_with_options(scene, catalog, limits, MeshBuildOptions::default())
}

/// Build a page-local mesh with explicit construction options.
pub fn build_mesh_with_options(
    scene: &Scene,
    catalog: &dyn BlockCatalog,
    limits: &MeshLimits,
    options: MeshBuildOptions,
) -> Result<Mesh, MeshError> {
    let view = SceneBlockView::new(scene, limits)?;
    let keys = view.page_keys().collect::<Vec<_>>();
    build_mesh_with_view(scene, &view, catalog, &keys, limits, options)
}

fn build_mesh_with_view<C: BlockCatalog + ?Sized>(
    scene: &Scene,
    view: &SceneBlockView<'_>,
    catalog: &C,
    keys: &[[i32; 3]],
    limits: &MeshLimits,
    options: MeshBuildOptions,
) -> Result<Mesh, MeshError> {
    let tables = PaletteTables::new(scene, catalog);
    let mut pages: BTreeMap<[i32; 3], Vec<Quad>> = BTreeMap::new();
    let mut statistics = MeshStatistics::default();

    for &coord in keys {
        let origin = page_origin_of(coord)?;
        let page = build_page(scene, view, &tables, origin, options)?;
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
    build_mesh_parallel_with_options(scene, catalog, limits, threads, MeshBuildOptions::default())
}

/// Build a mesh in parallel with explicit construction options.
pub fn build_mesh_parallel_with_options<C>(
    scene: &Scene,
    catalog: &C,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
) -> Result<Mesh, MeshError>
where
    C: BlockCatalog + Sync,
{
    // Avoid building the page index twice when the request is effectively serial.
    if worker_count(threads, usize::MAX) <= 1 {
        return build_mesh_with_options(scene, catalog, limits, options);
    }

    let view = SceneBlockView::new(scene, limits)?;
    let keys = view.page_keys().collect::<Vec<_>>();
    let workers = worker_count(threads, keys.len());
    if workers <= 1 {
        return build_mesh_with_view(scene, &view, catalog, &keys, limits, options);
    }

    let tables = PaletteTables::new(scene, catalog);
    let cursor = AtomicUsize::new(0);
    let abort = AtomicBool::new(false);
    let quads = AtomicU64::new(0);
    let vertices = AtomicU64::new(0);
    let page_count = AtomicU64::new(0);

    let results: Result<Vec<WorkerBuild>, MeshError> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| -> Result<WorkerBuild, MeshError> {
                    let mut local_pages = Vec::new();
                    let mut local_stats = MeshStatistics::default();
                    loop {
                        if abort.load(Ordering::Relaxed) {
                            break;
                        }
                        let index = cursor.fetch_add(1, Ordering::Relaxed);
                        let Some(&coord) = keys.get(index) else {
                            break;
                        };

                        let origin = match page_origin_of(coord) {
                            Ok(origin) => origin,
                            Err(error) => {
                                abort.store(true, Ordering::Relaxed);
                                return Err(error);
                            }
                        };
                        let page = match build_page(scene, &view, &tables, origin, options) {
                            Ok(page) => page,
                            Err(error) => {
                                abort.store(true, Ordering::Relaxed);
                                return Err(error);
                            }
                        };

                        accumulate_statistics(&mut local_stats, &page.statistics);

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
                            local_pages.push((page.origin, page.quads));
                        }
                    }
                    Ok(WorkerBuild {
                        pages: local_pages,
                        statistics: local_stats,
                    })
                })
            })
            .collect();

        let mut all = Vec::new();
        for handle in handles {
            all.push(handle.join().expect("mesh worker panicked")?);
        }
        Ok(all)
    });

    let worker_results = results?;
    if abort.load(Ordering::Relaxed) {
        return Err(limit_error(
            quads.load(Ordering::Relaxed),
            vertices.load(Ordering::Relaxed),
            page_count.load(Ordering::Relaxed),
            limits,
        ));
    }

    let mut pages = BTreeMap::new();
    let mut statistics = MeshStatistics::default();
    for worker_build in worker_results {
        for (origin, page_quads) in worker_build.pages {
            pages.insert(origin, page_quads);
        }
        accumulate_statistics(&mut statistics, &worker_build.statistics);
    }

    Ok(finish(pages, statistics))
}

/// Build a mesh in bounded page chunks and hand each chunk to `sink`.
///
/// Pages are emitted in the same origin order as [`build_mesh`]. The sink may
/// render or serialize the pages and must not retain the slice after returning;
/// the builder drops the chunk before constructing the next one.
pub fn build_mesh_streaming<C, F>(
    scene: &Scene,
    catalog: &C,
    limits: &MeshLimits,
    threads: u32,
    sink: F,
) -> Result<MeshStatistics, MeshError>
where
    C: BlockCatalog + Sync,
    F: FnMut(&[MeshPage]),
{
    build_mesh_streaming_with_options(
        scene,
        catalog,
        limits,
        threads,
        MeshBuildOptions::default(),
        sink,
    )
}

/// Build a streaming mesh with explicit construction options.
pub fn build_mesh_streaming_with_options<C, F>(
    scene: &Scene,
    catalog: &C,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
    mut sink: F,
) -> Result<MeshStatistics, MeshError>
where
    C: BlockCatalog + Sync,
    F: FnMut(&[MeshPage]),
{
    let view = SceneBlockView::new(scene, limits)?;
    let keys = view.page_keys().collect::<Vec<_>>();
    let tables = PaletteTables::new(scene, catalog);
    let mut statistics = MeshStatistics::default();

    for chunk in keys.chunks(STREAM_PAGES_PER_CHUNK) {
        let pages = build_page_chunk(scene, &view, &tables, chunk, threads, options)?;
        let mut mesh_pages = Vec::new();
        for page in pages {
            accumulate_statistics(&mut statistics, &page.statistics);
            if statistics.quads > limits.max_quads as u64 {
                return Err(MeshError::limit(format!(
                    "mesh exceeds {} quads",
                    limits.max_quads
                )));
            }
            if statistics.vertices > limits.max_vertices as u64 {
                return Err(MeshError::limit(format!(
                    "mesh exceeds {} vertices",
                    limits.max_vertices
                )));
            }
            if page.quads.is_empty() {
                continue;
            }
            if statistics.pages + 1 > limits.max_pages as u64 {
                return Err(MeshError::page_limit(format!(
                    "mesh exceeds {} pages",
                    limits.max_pages
                )));
            }

            statistics.pages += 1;
            mesh_pages.push(MeshPage {
                origin: page.origin,
                quads: page.quads,
            });
        }

        if !mesh_pages.is_empty() {
            sink(&mesh_pages);
        }
    }

    Ok(statistics)
}

fn build_page_chunk(
    scene: &Scene,
    view: &SceneBlockView<'_>,
    tables: &PaletteTables,
    keys: &[[i32; 3]],
    threads: u32,
    options: MeshBuildOptions,
) -> Result<Vec<PageBuild>, MeshError> {
    let origins = keys
        .iter()
        .map(|&coord| page_origin_of(coord))
        .collect::<Result<Vec<_>, _>>()?;
    let workers = worker_count(threads, origins.len());
    let slots: Vec<OnceLock<PageBuild>> = (0..origins.len()).map(|_| OnceLock::new()).collect();
    let cursor = AtomicUsize::new(0);
    let error = Mutex::new(None);

    if workers <= 1 {
        for (index, &origin) in origins.iter().enumerate() {
            let page = build_page(scene, view, tables, origin, options)?;
            slots[index]
                .set(page)
                .map_err(|_| MeshError::limit("duplicate mesh page slot"))?;
        }
    } else {
        thread::scope(|scope| {
            for _ in 0..workers {
                let slots = &slots;
                let cursor = &cursor;
                let error = &error;
                let origins = &origins;
                scope.spawn(move || {
                    loop {
                        let index = cursor.fetch_add(1, Ordering::Relaxed);
                        let Some(&origin) = origins.get(index) else {
                            break;
                        };
                        match build_page(scene, view, tables, origin, options) {
                            Ok(page) => {
                                assert!(slots[index].set(page).is_ok(), "mesh page slot set twice");
                            }
                            Err(page_error) => {
                                let mut guard = error.lock().expect("mesh error mutex poisoned");
                                if guard.is_none() {
                                    *guard = Some(page_error);
                                }
                                break;
                            }
                        }
                    }
                });
            }
        });
    }

    let error = error.into_inner().expect("mesh error mutex poisoned");
    if let Some(error) = error {
        return Err(error);
    }

    let mut pages = Vec::with_capacity(origins.len());
    for slot in slots {
        if let Some(page) = slot.into_inner() {
            pages.push(page);
        }
    }
    Ok(pages)
}

struct WorkerBuild {
    pages: Vec<([i32; 3], Vec<Quad>)>,
    statistics: MeshStatistics,
}

struct PageBuild {
    origin: [i32; 3],
    quads: Vec<Quad>,
    statistics: MeshStatistics,
}

fn build_page(
    scene: &Scene,
    view: &SceneBlockView<'_>,
    tables: &PaletteTables,
    origin: [i32; 3],
    options: MeshBuildOptions,
) -> Result<PageBuild, MeshError> {
    let base = [
        i64::from(origin[0]),
        i64::from(origin[1]),
        i64::from(origin[2]),
    ];
    let end = [base[0] + 16, base[1] + 16, base[2] + 16];
    let grid = PageGrid::fill(scene, view, tables, origin);
    let page_key = [
        origin[0].div_euclid(16),
        origin[1].div_euclid(16),
        origin[2].div_euclid(16),
    ];
    let mut page = PageBuild {
        origin,
        quads: Vec::new(),
        statistics: MeshStatistics::default(),
    };

    for &region_index in view.regions_for_page(page_key) {
        let region = &scene.regions[region_index];
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
                    let local = [
                        usize::try_from(x - base[0] + 1).expect("page-local x"),
                        usize::try_from(y - base[1] + 1).expect("page-local y"),
                        usize::try_from(z - base[2] + 1).expect("page-local z"),
                    ];
                    let cell = grid.get(local);
                    if cell.is_empty() || cell.region as usize != region_index {
                        continue;
                    }
                    let Some(palette_block) = tables.block(cell) else {
                        continue;
                    };
                    let local_index = PageGrid::local_index(local);

                    page.statistics.blocks_visited += 1;
                    if palette_block.is_air {
                        continue;
                    }

                    page.statistics.blocks_rendered += 1;
                    if !palette_block.block.known {
                        page.statistics.unknown_blocks += 1;
                    }

                    if palette_block.occludes
                        && palette_block.all_quads_cullable
                        && grid.all_neighbors_occlude(local_index)
                    {
                        continue;
                    }

                    let local_position = [
                        (x - base[0]) as f32,
                        (y - base[1]) as f32,
                        (z - base[2]) as f32,
                    ];

                    for model_quad in &palette_block.block.model.quads {
                        if should_cull(&grid, tables, local_index, palette_block, model_quad) {
                            continue;
                        }

                        let layer = model_quad.layer.unwrap_or(palette_block.block.layer);
                        page.statistics.add_quad(layer);

                        let color = if model_quad.texture == NO_TEXTURE {
                            palette_block.block.color
                        } else {
                            model_quad.tint
                        };
                        let ao = if palette_block.block.model.ambient_occlusion {
                            std::array::from_fn(|index| {
                                let corner = model_quad.vertices[index];
                                vertex_ao(&grid, local_index, model_quad, corner)
                            })
                        } else {
                            [3, 3, 3, 3]
                        };
                        let vertices = std::array::from_fn(|index| {
                            let corner = model_quad.vertices[index];
                            Vertex {
                                position: [
                                    local_position[0] + corner[0] / 16.0,
                                    local_position[1] + corner[1] / 16.0,
                                    local_position[2] + corner[2] / 16.0,
                                ],
                                uv: encode_uv(model_quad.uv[index]),
                            }
                        });

                        page.quads.push(Quad {
                            vertices,
                            normal: model_quad.normal,
                            color: color.to_array(),
                            ao,
                            texture: model_quad.texture,
                            layer,
                            shade: model_quad.shade,
                        });
                    }
                }
            }
        }
    }

    if options.greedy {
        merge_page_quads(&mut page.quads, &mut page.statistics);
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
    accumulate_statistics(statistics, &page.statistics);
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
fn accumulate_statistics(target: &mut MeshStatistics, source: &MeshStatistics) {
    target.blocks_visited += source.blocks_visited;
    target.blocks_rendered += source.blocks_rendered;
    target.unknown_blocks += source.unknown_blocks;
    target.quads += source.quads;
    target.vertices += source.vertices;
    target.opaque_quads += source.opaque_quads;
    target.cutout_quads += source.cutout_quads;
    target.translucent_quads += source.translucent_quads;
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

fn should_cull(
    grid: &PageGrid,
    tables: &PaletteTables,
    local_index: usize,
    block: &PaletteBlock,
    quad: &ModelQuad,
) -> bool {
    let Some(cullface) = quad.cullface else {
        return false;
    };

    let index = local_index.wrapping_add_signed(PageGrid::face_delta(cullface));
    if grid.occludes_index(index) {
        return true;
    }
    let Some(neighbor) = tables.block(grid.cell_at_index(index)) else {
        return false;
    };
    if neighbor.is_air {
        return false;
    }

    neighbor.state_id == block.state_id
}

fn vertex_ao(grid: &PageGrid, local_index: usize, quad: &ModelQuad, corner: [f32; 3]) -> u8 {
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
    let side1 = shifted(normal, u_axis, du);
    let side2 = shifted(normal, v_axis, dv);
    let corner_pos = {
        let mut offset = shifted(normal, u_axis, du);
        offset[v_axis] += dv as i32;
        offset
    };

    let s1 = occludes_offset(grid, local_index, side1);
    let s2 = occludes_offset(grid, local_index, side2);
    let c = occludes_offset(grid, local_index, corner_pos);
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

fn occludes_offset(grid: &PageGrid, local_index: usize, offset: [i32; 3]) -> bool {
    grid.occludes_index(local_index.wrapping_add_signed(PageGrid::offset_delta(offset)))
}

fn shifted(base: [i64; 3], axis: usize, delta: i64) -> [i32; 3] {
    let mut position = [base[0] as i32, base[1] as i32, base[2] as i32];
    position[axis] += delta as i32;
    position
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
