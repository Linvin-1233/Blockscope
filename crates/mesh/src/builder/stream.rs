use super::{
    PageBuild, PageCapacityHint, PageMode, accumulate_statistics, build_page, page_origin_of,
    worker_count,
};
use crate::catalog::BlockCatalog;
use crate::page::PaletteTables;
use crate::view::SceneBlockView;
use crate::{MeshBuildOptions, MeshError, MeshLimits, MeshPage, MeshStatistics};
use litematic_scene::Scene;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

const STREAM_PAGES_PER_CHUNK: usize = 32;
/// Build a mesh in bounded page chunks and hand each chunk to `sink`.
///
/// Pages are emitted in the same origin order as [`super::build_mesh`]. The sink may
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
        let mut hint = PageCapacityHint::default();
        for (index, &origin) in origins.iter().enumerate() {
            let page = build_page(
                scene,
                view,
                tables,
                origin,
                options,
                &mut hint,
                PageMode::Build,
            )?;
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
                    let mut hint = PageCapacityHint::default();
                    loop {
                        let index = cursor.fetch_add(1, Ordering::Relaxed);
                        let Some(&origin) = origins.get(index) else {
                            break;
                        };
                        match build_page(
                            scene,
                            view,
                            tables,
                            origin,
                            options,
                            &mut hint,
                            PageMode::Build,
                        ) {
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
