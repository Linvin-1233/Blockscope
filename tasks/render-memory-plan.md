# Plan: Textured Render Memory

**Status:** A+B+C1 implemented and measured
**Date:** 2026-10-28

## Problem

The textured path is much heavier than the color-only path, but texture storage
is not the cause. `000001` with the client jar:

- material set: about 227 MB;
- retained translucent triangles: 12.59M at 112 B, about 1.41 GB;
- streamed peak: 2.28 GB;
- full-mesh peak: 18.9 GB (existing docs).

The retained translucent triangles are the dominant streamed cost. They are
kept in `StreamingRenderer.translucent` until `finish()` so each tile can sort
them far-to-near.

## Changes

### A: Automatic mesh streaming

- `render` now accepts `--stream-mesh[=auto|on|off]`, default `auto`.
- `auto` streams fixed-LOD renders and keeps the full mesh for `--lod auto`.
- `off` restores the full-mesh path; `on` forces streaming and still rejects
  `--lod auto`.
- Existing `--stream-mesh` without a value means `on`.

### B: Compact translucent triangles

- Projected screen points are stored as f32 (`[[f32; 3]; 3]`).
- The `layer` field is removed because every retained triangle is translucent.
- `depth_key` stays f64 so the sort order is independent of the compact points.
- A unit test pins `size_of::<Triangle>() <= 72`.

## Measurements

Same host, 1024x1024, supersample 2, 12 threads, `--force-limits`.

| Case | Before | After |
|---|---:|---:|
| `000001` + client jar, streamed, memory path | 2.28 GB | **1.52 GB** |
| `000001` + client jar, streamed, exact spill | 2.28 GB | **618 MB** |
| `000002` + client jar, streamed | 217 MB | **216 MB** |
| `000002` + client jar, full mesh | 3.8 GB | 3.8 GB |
| `000001` + client jar, full mesh | 18.9 GB | unchanged |

Default `render` without a flag now takes the streamed path for fixed LOD. The
`000001` client-jar PNG differs from the previous f64 build by 2 pixels out of
1,048,576 (maximum channel delta 4); golden fixtures remain byte-identical.

## Verification

- `cargo test -p litematic-render` (unit, golden PNG, golden textures, render)
- `cargo test -p blockscope-cli --test cli` (streamed vs full, default auto vs
  full, auto-LOD rejection)
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

## C1: exact translucent spill

Implemented in `crates/render/src/rasterizer/spill.rs`:

- below 262,144 retained triangles, the in-memory path is unchanged;
- above it, exact 76-byte records go to 64 temporary bucket files;
- each bucket is read, grouped by tile, stable-sorted by the f64 depth key, and
  rasterized before the next bucket is read;
- records are written in projection order, so the per-tile stable sort matches
  the global bin path;
- `StreamingRenderer::finish` and `render_pages` now return `Result` so
  temporary-storage failures surface as `TEMP_STORAGE_FAILED`.

Measured on `000001` with the client jar: **618 MB**, PNG SHA-256 identical to
the in-memory 1.52 GB build. Render time increased from 14.6 s to 16.2 s.

## Remaining

- `serve` still uses full-mesh rendering and its `Arc<Mesh>` cache; a protocol
  `streamMesh` option is a separate change.
- The spill record is exact but uncompressed; further reduction would need
  approximate ordering or OIT, which changes output.
