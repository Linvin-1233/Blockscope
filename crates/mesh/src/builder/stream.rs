use super::{
    PageBuild, PageCapacityHint, PageMode, PageProgress, accumulate_statistics, build_page,
    page_origin_of, worker_count,
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
    sink: F,
) -> Result<MeshStatistics, MeshError>
where
    C: BlockCatalog + Sync,
    F: FnMut(&[MeshPage]),
{
    build_mesh_streaming_with_options_and_progress(
        scene, catalog, limits, threads, options, None, sink,
    )
}

/// Build a streaming mesh and report page completion after each chunk.
pub fn build_mesh_streaming_with_options_and_progress<C, F>(
    scene: &Scene,
    catalog: &C,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
    progress: Option<PageProgress<'_>>,
    mut sink: F,
) -> Result<MeshStatistics, MeshError>
where
    C: BlockCatalog + Sync,
    F: FnMut(&[MeshPage]),
{
    build_mesh_streaming_impl(
        scene,
        catalog,
        limits,
        threads,
        options,
        progress,
        |pages| sink(&pages),
    )
}

/// Build a streaming mesh and hand each chunk to `sink` by value.
///
/// Unlike the borrowed variants this lets the caller retain or batch pages
/// without cloning them.
pub fn build_mesh_streaming_owned<C, F>(
    scene: &Scene,
    catalog: &C,
    limits: &MeshLimits,
    threads: u32,
    sink: F,
) -> Result<MeshStatistics, MeshError>
where
    C: BlockCatalog + Sync,
    F: FnMut(Vec<MeshPage>),
{
    build_mesh_streaming_owned_with_options(
        scene,
        catalog,
        limits,
        threads,
        MeshBuildOptions::default(),
        sink,
    )
}

/// Build an owned streaming mesh with explicit construction options.
pub fn build_mesh_streaming_owned_with_options<C, F>(
    scene: &Scene,
    catalog: &C,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
    sink: F,
) -> Result<MeshStatistics, MeshError>
where
    C: BlockCatalog + Sync,
    F: FnMut(Vec<MeshPage>),
{
    build_mesh_streaming_owned_with_options_and_progress(
        scene, catalog, limits, threads, options, None, sink,
    )
}

/// Build an owned streaming mesh and report page completion after each chunk.
pub fn build_mesh_streaming_owned_with_options_and_progress<C, F>(
    scene: &Scene,
    catalog: &C,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
    progress: Option<PageProgress<'_>>,
    sink: F,
) -> Result<MeshStatistics, MeshError>
where
    C: BlockCatalog + Sync,
    F: FnMut(Vec<MeshPage>),
{
    build_mesh_streaming_impl(scene, catalog, limits, threads, options, progress, sink)
}

fn build_mesh_streaming_impl<C, F>(
    scene: &Scene,
    catalog: &C,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
    progress: Option<PageProgress<'_>>,
    mut sink: F,
) -> Result<MeshStatistics, MeshError>
where
    C: BlockCatalog + Sync,
    F: FnMut(Vec<MeshPage>),
{
    let view = SceneBlockView::new(scene, limits)?;
    let keys = view.page_keys().collect::<Vec<_>>();
    let total_pages = keys.len() as u64;
    if let Some(progress) = progress {
        progress(0, total_pages);
    }
    let tables = PaletteTables::new(scene, catalog);
    let workers = worker_count(threads, keys.len());
    let mut hints = (0..workers)
        .map(|_| PageCapacityHint::default())
        .collect::<Vec<_>>();
    let mut statistics = MeshStatistics::default();
    let mut completed_pages = 0u64;

    for chunk in keys.chunks(STREAM_PAGES_PER_CHUNK) {
        let pages = build_page_chunk(scene, &view, &tables, chunk, threads, options, &mut hints)?;
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
                float_positions: page.float_positions,
                float_normals: page.float_normals,
            });
        }

        completed_pages += chunk.len() as u64;

        if !mesh_pages.is_empty() {
            sink(mesh_pages);
        }

        if let Some(progress) = progress {
            progress(completed_pages.min(total_pages), total_pages);
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
    hints: &mut [PageCapacityHint],
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
        let hint = &mut hints[0];
        for (index, &origin) in origins.iter().enumerate() {
            let page = build_page(scene, view, tables, origin, options, hint, PageMode::Build)?;
            slots[index]
                .set(page)
                .map_err(|_| MeshError::limit("duplicate mesh page slot"))?;
        }
    } else {
        let hints = &mut hints[..workers];
        thread::scope(|scope| {
            for hint in hints.iter_mut() {
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
                        match build_page(
                            scene,
                            view,
                            tables,
                            origin,
                            options,
                            hint,
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
