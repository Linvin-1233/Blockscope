# Phase 3 Task List 鈥?CPU PNG Renderer

**Status:** Phase 3A/3B complete; Phase 3C deferred.  
**Plan:** `tasks/phase-3-plan.md`

## Task P3-1: AO vertex attribute

**Description:** Compute per-vertex ambient occlusion in the mesh builder.

**Acceptance criteria:**

- [x] `Vertex` carries an `ao` value.
- [x] Boundary vertices sample the three AO neighbor cells.
- [x] Interior and cross quads default to `ao = 1.0`.
- [x] AO is deterministic and bounded in `(0, 1]`.
- [x] Existing mesh/export tests still pass.

**Verification:**

- [x] A stacked-cube fixture produces at least one vertex with `ao < 0.99`.
- [x] Isolated cubes keep `ao = 1.0`.
- [x] `cargo test -p litematic-mesh` passes.

**Dependencies:** Phase 2  
**Estimated scope:** Medium

## Task P3-2: Render options and lighting profiles

**Description:** Define renderer options, backgrounds, lighting profiles, and
validation.

**Acceptance criteria:**

- [x] `RenderOptions` covers width, height, yaw, elevation, background, lighting,
      tile size, padding, and pixel limit.
- [x] `flat-v1` and `simple-ao-v1` exist.
- [x] `Background` supports transparent and solid RGBA.
- [x] Invalid dimensions, elevation, padding, and oversized outputs fail with
      stable codes.

**Verification:**

- [x] Default options are valid.
- [x] Oversized output returns `OUTPUT_TOO_LARGE`.
- [x] Invalid elevation returns `INVALID_RENDER_OPTIONS`.

**Dependencies:** P3-1  
**Estimated scope:** Small

## Task P3-3: Orthographic camera and fit

**Description:** Build a yaw/elevation orthographic camera that fits the scene.

**Acceptance criteria:**

- [x] Camera basis uses yaw and elevation degrees.
- [x] Scene bounds are centered and fitted with padding.
- [x] Projection produces screen x/y and camera depth.
- [x] Degenerate elevation and padding fail cleanly.
- [x] Negative world coordinates are supported.

**Verification:**

- [x] A unit cube renders inside the padded image area.
- [x] Camera scale is positive and finite.
- [x] Elevation 90掳 is rejected.

**Dependencies:** P3-2  
**Estimated scope:** Medium

## Task P3-4: Tile rasterizer and compositing

**Description:** Rasterize mesh triangles into tile color/depth buffers.

**Acceptance criteria:**

- [x] Tiles cover the full image including partial edge tiles.
- [x] Backface culling uses the camera forward vector.
- [x] Depth test uses orthographic depth.
- [x] Cutout alpha below 0.5 is discarded.
- [x] Translucent triangles are sorted far-to-near per tile and blended
      without depth writes.
- [x] Transparent and solid backgrounds are correct.
- [x] Render statistics count tiles, triangles, layers, and pixels.

**Verification:**

- [x] Transparent background leaves corners at alpha 0.
- [x] Solid background fills corners with the requested color.
- [x] Water renders with partial alpha.
- [x] Flat and AO profiles differ.
- [x] Empty mesh returns background only.

**Dependencies:** P3-3  
**Estimated scope:** Large

## Task P3-5: PNG encoder and `renderd render` CLI

**Description:** Encode RGBA output as PNG and expose the renderer through the
CLI.

**Acceptance criteria:**

- [x] PNG encoder writes RGBA8 with a valid signature.
- [x] `renderd render <file> --output out.png` works.
- [x] Width, height, yaw, elevation, background, and lighting are configurable.
- [x] `--json` prints render statistics.
- [x] Invalid background/lighting returns exit code 2.
- [x] Output write failures return a stable IO error.

**Verification:**

- [x] CLI render test writes a PNG.
- [x] PNG header signature is present.
- [x] JSON statistics parse.
- [x] Invalid options fail with `INVALID_RENDER_OPTIONS`.

**Dependencies:** P3-4  
**Estimated scope:** Medium

## Task P3-6: Renderer tests and documentation

**Description:** Document the camera, rasterizer, lighting, CLI, and limitations.

**Acceptance criteria:**

- [x] `docs/rendering.md` covers camera convention, tiling, depth, culling,
      cutout/translucent compositing, lighting, limits, and limitations.
- [x] README links the render command and document.
- [x] Windows and Linux checks pass.
- [x] No Minecraft assets are committed.
- [ ] Human review before Phase 4.

**Verification:**

- [x] `cargo fmt --all --check` passes.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [x] `cargo test --workspace` passes on Windows and WSL.
- [x] `renderd render` demo succeeds on a synthetic fixture.

**Dependencies:** P3-4, P3-5  
**Estimated scope:** Medium

## Task P3-7: Golden PNG and performance corpus

**Description:** Add committed golden images and repeatable timing benchmarks.

**Acceptance criteria:**

- [x] Golden PNGs cover cube, partial shapes, water, negative coordinates, and
      AO.
- [x] Benchmarks record parse, mesh, render, PNG, and buffer sizes.
- [x] Peak RSS is reported on Linux; other platforms report `null` instead of
      guessing.
- [ ] Performance thresholds are based on real QQBot projections.

**Verification:**

- [x] Golden PNG test compares bytes exactly and supports `UPDATE_GOLDEN=1`.
- [x] `renderd bench --json` parses in a CLI test.
- [x] Synthetic `tile_bench` example prints single, dual, and auto-thread
      timings.

**Dependencies:** P3-6  
**Estimated scope:** Medium

## Task P3-8: Parallel tiles and triangle binning

**Description:** Parallelize tile rasterization and avoid scanning the whole
triangle list per tile.

**Acceptance criteria:**

- [x] Triangles are binned by covered tile bounds.
- [x] Scoped workers pull tile indices from an atomic cursor.
- [x] Each worker owns its tile color and depth buffers.
- [x] Results merge in tile order for deterministic output and statistics.
- [x] `threads` is configurable; 0 selects available parallelism.
- [x] Worker count does not change pixels or counters.

**Verification:**

- [x] `parallel_tiles_match_single_thread` compares RGBA and statistics.
- [x] `tile_bench` reports scaling for 1, 2, and auto threads.
- [x] Golden PNGs still match.

**Dependencies:** P3-4  
**Estimated scope:** Medium

## Task P3-9: Supersampling and multi-view CLI

**Description:** Add anti-aliasing and two-view output for QQ previews.

**Acceptance criteria:**

- [x] Supersampling supports factors 1-4; default is 2.
- [x] Downsampling works in premultiplied-alpha space.
- [x] Internal dimensions have their own pixel limit.
- [x] `--views 45,225` writes one PNG per yaw with a suffix.
- [x] Render statistics include supersample and internal dimensions.
- [x] Validation rejects zero or excessive factors and oversized buffers.

**Verification:**

- [x] `supersampling_changes_edge_pixels` proves SSAA affects edges.
- [x] `render_multi_view_writes_two_files` checks suffixed outputs.
- [x] JSON statistics expose `supersample` and internal size.
- [x] Golden PNGs use the default supersample factor.

**Dependencies:** P3-5  
**Estimated scope:** Medium
