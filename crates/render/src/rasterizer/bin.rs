use super::geometry::{bounds, quad_bounds};
use super::project::{QuadRender, Triangle};
pub(super) use litematic_scene::worker_count;
#[derive(Clone, Copy, Debug)]
pub(super) struct TileJob {
    pub(super) index: usize,
    pub(super) x0: u32,
    pub(super) y0: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

pub(super) fn tile_jobs(width: u32, height: u32, tile_size: u32) -> Vec<TileJob> {
    let tiles_x = width.div_ceil(tile_size);
    let tiles_y = height.div_ceil(tile_size);
    let mut jobs = Vec::with_capacity((tiles_x * tiles_y) as usize);

    for tile_y in 0..tiles_y {
        for tile_x in 0..tiles_x {
            let x0 = tile_x * tile_size;
            let y0 = tile_y * tile_size;
            jobs.push(TileJob {
                index: jobs.len(),
                x0,
                y0,
                width: tile_size.min(width - x0),
                height: tile_size.min(height - y0),
            });
        }
    }

    jobs
}

/// Group quads by the tiles their screen bounds cover, so each tile only visits
/// candidates that can actually write a fragment.
pub(super) fn bin_quads(
    quads: &[QuadRender],
    width: u32,
    height: u32,
    tile_size: u32,
) -> Vec<Vec<u32>> {
    let tiles_x = width.div_ceil(tile_size);
    let tiles_y = height.div_ceil(tile_size);
    let mut bins = vec![Vec::new(); (tiles_x * tiles_y) as usize];
    let tile_size = f64::from(tile_size);
    let max_tile_x = f64::from(tiles_x - 1);
    let max_tile_y = f64::from(tiles_y - 1);

    for (index, quad) in quads.iter().enumerate() {
        let bbox = quad_bounds(&quad.points);
        let min_x = (bbox[0] / tile_size).floor().clamp(0.0, max_tile_x) as u32;
        let max_x = (bbox[2] / tile_size).floor().clamp(0.0, max_tile_x) as u32;
        let min_y = (bbox[1] / tile_size).floor().clamp(0.0, max_tile_y) as u32;
        let max_y = (bbox[3] / tile_size).floor().clamp(0.0, max_tile_y) as u32;

        for tile_y in min_y..=max_y {
            for tile_x in min_x..=max_x {
                bins[(tile_y * tiles_x + tile_x) as usize].push(index as u32);
            }
        }
    }

    bins
}

/// Group translucent triangles by tile; only these need per-tile sorting.
pub(super) fn bin_triangles(
    triangles: &[Triangle],
    width: u32,
    height: u32,
    tile_size: u32,
) -> Vec<Vec<u32>> {
    let tiles_x = width.div_ceil(tile_size);
    let tiles_y = height.div_ceil(tile_size);
    let mut bins = vec![Vec::new(); (tiles_x * tiles_y) as usize];
    let tile_size = f64::from(tile_size);
    let max_tile_x = f64::from(tiles_x - 1);
    let max_tile_y = f64::from(tiles_y - 1);

    for (index, triangle) in triangles.iter().enumerate() {
        let points = triangle.points_f64();
        let bbox = bounds(&points);
        let min_x = (bbox[0] / tile_size).floor().clamp(0.0, max_tile_x) as u32;
        let max_x = (bbox[2] / tile_size).floor().clamp(0.0, max_tile_x) as u32;
        let min_y = (bbox[1] / tile_size).floor().clamp(0.0, max_tile_y) as u32;
        let max_y = (bbox[3] / tile_size).floor().clamp(0.0, max_tile_y) as u32;

        for tile_y in min_y..=max_y {
            for tile_x in min_x..=max_x {
                bins[(tile_y * tiles_x + tile_x) as usize].push(index as u32);
            }
        }
    }

    bins
}

/// Pick a tile size that leaves enough independent tiles to balance workers.
///
/// Tiling does not change the output: every pixel sees the same candidate
/// quads in the same order. But the configured 64px tile leaves only 16 tiles
/// at 256x256, so with 12 workers the slowest runs two tiles while the rest
/// idle. Target a fixed 1024 tiles so statistics stay independent of the
/// thread count, and never exceed the configured size. At a 1024x1024 internal
/// image this yields 32px tiles, which measured faster than 64px; at
/// 2048x2048 it keeps 64px tiles, which measured faster than 32px.
pub(super) fn effective_tile_size(width: u32, height: u32, requested: u32) -> u32 {
    let requested = requested.max(1);
    let pixels = f64::from(width) * f64::from(height);
    let side = (pixels / 1024.0).sqrt().ceil() as u32;
    let floor = requested.min(8);

    side.clamp(floor, requested)
}
