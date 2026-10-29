use super::{PageCapacityHint, PageMode, build_page, page_origin_of, worker_count};
use crate::catalog::BlockCatalog;
use crate::page::PaletteTables;
use crate::view::SceneBlockView;
use crate::{MeshBuildOptions, MeshError, MeshLimits};
use litematic_scene::Scene;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread;
/// Count the quads a mesh would contain without allocating geometry.
///
/// Stops as soon as `limits` would be exceeded and returns `Ok(None)`, so a
/// caller can reject an oversized mesh without paying for a full build. Greedy
/// merging is not applied; with [`MeshBuildOptions::GREEDY`] the count is an
/// upper bound on the merged mesh.
pub fn count_mesh_quads<C>(
    scene: &Scene,
    catalog: &C,
    limits: &MeshLimits,
    threads: u32,
    options: MeshBuildOptions,
) -> Result<Option<u64>, MeshError>
where
    C: BlockCatalog + Sync + ?Sized,
{
    let view = SceneBlockView::new(scene, limits)?;
    let keys = view.page_keys().collect::<Vec<_>>();
    let tables = PaletteTables::new(scene, catalog);
    let workers = worker_count(threads, keys.len());
    if workers == 1 {
        let mut hint = PageCapacityHint::default();
        let mut total = 0u64;
        let mut total_pages = 0u64;
        for &coord in &keys {
            let origin = page_origin_of(coord)?;
            let page = build_page(
                scene,
                &view,
                &tables,
                origin,
                options,
                &mut hint,
                PageMode::Count,
            )?;
            let count = page.statistics.quads;
            if count == 0 {
                continue;
            }
            total += count;
            total_pages += 1;
            if total > limits.max_quads as u64
                || total.saturating_mul(4) > limits.max_vertices as u64
                || total_pages > limits.max_pages as u64
            {
                return Ok(None);
            }
        }
        return Ok(Some(total));
    }
    let cursor = AtomicUsize::new(0);
    let abort = AtomicBool::new(false);
    let quads = AtomicU64::new(0);
    let pages = AtomicU64::new(0);

    let result = thread::scope(|scope| -> Result<(), MeshError> {
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let view = &view;
            let tables = &tables;
            let keys = &keys;
            let cursor = &cursor;
            let abort = &abort;
            let quads = &quads;
            let pages = &pages;
            handles.push(scope.spawn(move || -> Result<(), MeshError> {
                let mut hint = PageCapacityHint::default();
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
                    let page = match build_page(
                        scene,
                        view,
                        tables,
                        origin,
                        options,
                        &mut hint,
                        PageMode::Count,
                    ) {
                        Ok(page) => page,
                        Err(error) => {
                            abort.store(true, Ordering::Relaxed);
                            return Err(error);
                        }
                    };
                    let count = page.statistics.quads;
                    if count == 0 {
                        continue;
                    }
                    let total = quads.fetch_add(count, Ordering::Relaxed) + count;
                    let total_pages = pages.fetch_add(1, Ordering::Relaxed) + 1;
                    if total > limits.max_quads as u64
                        || total.saturating_mul(4) > limits.max_vertices as u64
                        || total_pages > limits.max_pages as u64
                    {
                        abort.store(true, Ordering::Relaxed);
                    }
                }
                Ok(())
            }));
        }
        for handle in handles {
            handle.join().expect("mesh count worker panicked")?;
        }
        Ok(())
    });
    result?;

    if abort.load(Ordering::Relaxed) {
        Ok(None)
    } else {
        Ok(Some(quads.load(Ordering::Relaxed)))
    }
}

/// Estimate the quad count from a deterministic sample of pages.
///
/// The estimate exists to decide whether a full [`count_mesh_quads`] pass is
/// worth running. It is not an exact bound: a caller that must not reject a
/// mesh which actually fits should confirm with the full count. Sampling is
/// spread evenly across page order, so dense regions are not over-weighted by
/// the first pages.
pub fn estimate_mesh_quads<C>(
    scene: &Scene,
    catalog: &C,
    limits: &MeshLimits,
    sample_pages: usize,
    threads: u32,
    options: MeshBuildOptions,
) -> Result<u64, MeshError>
where
    C: BlockCatalog + Sync + ?Sized,
{
    let view = SceneBlockView::new(scene, limits)?;
    let keys = view.page_keys().collect::<Vec<_>>();
    let total_pages = keys.len();
    if total_pages == 0 {
        return Ok(0);
    }

    let sample = sample_pages.clamp(1, total_pages);
    let tables = PaletteTables::new(scene, catalog);
    let workers = worker_count(threads, sample);
    if workers == 1 {
        let mut hint = PageCapacityHint::default();
        let mut total = 0u64;
        for position in 0..sample {
            let index = sample_index(position, sample, total_pages);
            let origin = page_origin_of(keys[index])?;
            let page = build_page(
                scene,
                &view,
                &tables,
                origin,
                options,
                &mut hint,
                PageMode::Count,
            )?;
            total += page.statistics.quads;
        }
        let total = u128::from(total) * total_pages as u128;
        return Ok(u64::try_from(total.div_ceil(sample as u128)).unwrap_or(u64::MAX));
    }
    let cursor = AtomicUsize::new(0);
    let abort = AtomicBool::new(false);
    let quads = AtomicU64::new(0);
    let sampled = AtomicU64::new(0);

    let result = thread::scope(|scope| -> Result<(), MeshError> {
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let view = &view;
            let tables = &tables;
            let keys = &keys;
            let cursor = &cursor;
            let abort = &abort;
            let quads = &quads;
            let sampled = &sampled;
            handles.push(scope.spawn(move || -> Result<(), MeshError> {
                let mut hint = PageCapacityHint::default();
                loop {
                    if abort.load(Ordering::Relaxed) {
                        break;
                    }
                    let position = cursor.fetch_add(1, Ordering::Relaxed);
                    if position >= sample {
                        break;
                    }
                    let index = sample_index(position, sample, total_pages);
                    let origin = match page_origin_of(keys[index]) {
                        Ok(origin) => origin,
                        Err(error) => {
                            abort.store(true, Ordering::Relaxed);
                            return Err(error);
                        }
                    };
                    let page = match build_page(
                        scene,
                        view,
                        tables,
                        origin,
                        options,
                        &mut hint,
                        PageMode::Count,
                    ) {
                        Ok(page) => page,
                        Err(error) => {
                            abort.store(true, Ordering::Relaxed);
                            return Err(error);
                        }
                    };
                    quads.fetch_add(page.statistics.quads, Ordering::Relaxed);
                    sampled.fetch_add(1, Ordering::Relaxed);
                }
                Ok(())
            }));
        }
        for handle in handles {
            handle.join().expect("mesh estimate worker panicked")?;
        }
        Ok(())
    });
    result?;

    let sampled = u128::from(sampled.load(Ordering::Relaxed)).max(1);
    let total = u128::from(quads.load(Ordering::Relaxed)) * total_pages as u128;
    Ok(u64::try_from(total.div_ceil(sampled)).unwrap_or(u64::MAX))
}

/// Midpoint of one of `sample` equal strata over `total` pages.
fn sample_index(position: usize, sample: usize, total: usize) -> usize {
    (((2 * position + 1) * total) / (2 * sample)).min(total - 1)
}
