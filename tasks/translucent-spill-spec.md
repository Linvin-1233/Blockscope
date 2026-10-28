# Spec: Exact Translucent Spill (C1)

**Status:** Implemented and measured
**Date:** 2026-10-28
**Depends on:** `render-memory-plan.md` A+B

## Objective

Bound the memory retained for translucent triangles during streamed rendering
without changing the image. Above a triangle budget, spill records to temporary
bucket files; at finish, process one bucket at a time, sort each tile, and
rasterize. The output must be byte-identical to the in-memory path.

## Problem

`StreamingRenderer` keeps every translucent triangle until `finish()` so each
tile can sort them far-to-near. On `000001` with the client jar that is 12.59M
triangles at 72 B, about 906 MB. It dominates the 1.52 GB streamed peak.

## Design

- Keep the existing in-memory path below a default budget of 262,144 triangles
  (about 18 MB). Small scenes never touch disk.
- Above the budget, create a temporary directory with 64 bucket files (4x4
  tiles per bucket). Each projected triangle is appended once per tile its
  screen bounding box covers, with the tile index in the record.
- A record is 76 bytes: tile index u32, 9 f32 points, RGBA8 color, f32 shade,
  3 AO bytes, 6 i16 UVs, u16 texture, f64 depth key, 3 padding bytes.
- At `finish`, flush and close the writers, then read one bucket at a time.
  Group records by tile, stable-sort each tile's triangles by the stored f64
  depth key, and rasterize. Release the bucket before reading the next one.
- Temporary files are removed when the reader drops. Process errors map to a
  new `TEMP_STORAGE_FAILED` render error.
- The order per tile is the projection order because records are appended in
  page order; the stable sort therefore matches the current global bin path.

## Non-goals

- Approximate chunk order, OIT, or changing the rasterization result.
- Compressing the spill records further.
- Wiring `serve` to streamed rendering (a separate protocol change).

## Testing

- Unit test in `stream.rs`: force the budget to 1 triangle and compare the
  streamed output with the table path on a water/stone scene.
- Existing `streaming_renderer_matches_the_table_path_page_by_page` and
  `streaming_matches_the_table_path` continue to pass.
- Golden PNG/texture tests remain byte-identical.
- Measure `000001` with the client jar; compare the PNG SHA-256 with the
  in-memory build and record peak working set.

## Success Criteria

- Streamed `000001` + client jar peak is at most 0.8 GB (target 0.5-0.7 GB).
  Measured: **618 MB**.
- PNG SHA-256 is identical to the pre-spill auto-stream build. Verified.
- Full workspace `fmt`, `clippy`, and tests pass.
- `docs/performance.md` and `CHANGELOG` record the change.
