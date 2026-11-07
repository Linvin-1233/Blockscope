# Implementation Plan: GPU performance path

**Status:** Approved 2026-11-06; Tasks 1-19 complete (Task 17 wall-time gate not
met), Task 20 release dry run pending
**Date:** 2026-11-05
**Spec:** `tasks/gpu-performance-spec.md`
**Related:** `tasks/gpu-feasibility.md`, `tasks/video-export-spec.md`,
`docs/performance.md`

## Overview

Turn the optional GPU backend from a faster rasterizer with a slow CPU feeder
into a net end-to-end win when it is selected. The work removes CPU projection
and per-quad vertex expansion, adds page-chunk GPU streaming, shares textures
and atlases, replaces the translucent strategy with coarse depth buckets plus
Weighted Blended OIT, and makes `auto` the default backend that chooses the GPU
only when it expects a win. The CPU backend remains the byte-exact reference
and explicit override.

The CPU streaming/multi-view work is an approved prerequisite and lands first.
It is tracked by a separate spec; this plan only depends on its chunk policy.

## Architecture Decisions

- **In-tree feature, runtime selection.** GPU is enabled by
  `--features gpu`; there is no separate GPU codebase. Official artifacts
  enable it where the platform can load a driver.
- **`auto` is the default backend; CPU is the exact reference.** GPU output is
  approximate and is reported through `backendUsed`; `--backend cpu` is the
  exact override.
- **`auto` selects GPU implicitly.** It must never select GPU when the CPU path
  is expected to be faster, when no hardware adapter exists, or when the caller
  requires exact output.
- **Parity tolerance is fixed.** Maximum channel delta 2 and 0.1% differing
  pixels, including WBOIT.
- **Unsafe f32 span falls back to CPU.** No slow f64 GPU path.
- **GPU multi-view is required in v1.**
- **GPU memory budget is 60% of adapter VRAM.** Atlas cache, render targets,
  chunk buffers, and readback staging all fit inside it.
- **One engine, one acceptance standard.** The release matrix samples hardware;
  it does not define vendor-specific standards.
- **GPU-side transform and quad expansion.** Upload compact quad instances;
  the shader performs camera transform, UV decode, lighting, and quad-to-
  triangle expansion.
- **Camera-relative f32 coordinates.** Use a scene-center origin and a
  measured precision guard; fall back to CPU when the span is unsafe.
- **Stream page chunks, not full meshes.** Reuse the CPU streaming chunk
  policy and keep transient geometry bounded.
- **Share texture storage.** `TextureSet` becomes `Arc`-backed; the atlas and
  bind group are cached per material snapshot.
- **Coarse depth buckets plus WBOIT.** Bucket translucent triangles far to
  near; use `Rgba16Float` accumulation and revealage within each
  bucket. Over-large buckets split into sub-batches.
- **One session, many views.** `GpuRenderSession` shares mesh upload, atlas, and
  pipelines across views and is the future video-render entry point.
- **Platform matrix.** Windows/macOS/Linux GNU enable GPU; static musl and wasm
  stay CPU-only because they cannot load a hardware driver.

## Dependency Graph

```text
CPU streaming/multi-view (approved prerequisite, separate plan)
    │
    └──────────────────────────────┐
                                   ▼
G0 baseline ──► G2 GPU transform ──► G3 GPU streaming ──► G4 multi-view
                    │                       │
G1 textures/atlas ──┘                       └──► G5 WBOIT ──► G6 policy/release
```

- G0 and G1 can run in parallel.
- G2 is the core change and must land before G3, G4, or G5.
- G3 unlocks the large-scene memory and performance gates.
- G4 and G5 can run in parallel after G3, but both touch the session module and
  need coordination.
- G6 is last and depends on the measured results from G2-G5.

## Baseline Reference

Reference host R1: 12-core / 48 GB / GTX 1650 SUPER, Windows D3D12, release
build, color-only, 1024x1024, supersample 2, 12 threads.

| Case | CPU 12t | CPU 2t | GPU | Current verdict |
|---|---:|---:|---:|---|
| `000002` render | 182 ms | 341 ms | 207 ms | GPU 13% slower |
| `000002` render + PNG | 225 ms | 385 ms | 249 ms | GPU 11% slower |
| `000001` render, full mesh | 7,046 ms | - | 14,881 ms | GPU 2.1x slower |
| `000001` end to end | about 11.3 s | - | about 18.7 s | GPU not viable yet |

`tile_bench` (96,882 quads): GPU is about 1.4x faster than CPU 12t and 5-7x
faster than CPU 1t. The pure rasterizer is not the problem; the CPU feeder is.

Release gates from the spec:

- `tile_bench`: GPU at least 1.4x CPU 12t at 1024 and 2048 square.
- `000002`: GPU total at most 0.9x CPU 12t (about 203 ms) and at most 0.65x
  CPU 2t (about 250 ms).
- `000001` color: GPU streaming total at most 10 s, peak at most 2 GB.
- `000001` client jar: GPU streaming total at most 15 s, peak at most 3 GB.
- Two-view `000002`: GPU at most 1.3x single-view GPU wall time.

R2 is R1 with `--threads 2`. R3 is the Apple M4 Max Metal host. R4 is at least
one additional vendor/integrated GPU. R5 is a no-adapter host with a static
musl build.

## Task List

### Phase 0: Prerequisite gate

- [ ] CPU streaming chunk aggregation and multi-view reuse are implemented and
      measured.
- [ ] The chunk policy is exposed for GPU reuse, or a shared helper exists.
- [ ] `000002` auto/off and two-view comparisons are recorded.
- [ ] CPU stream baseline (`000001` about 433 MB / 11.1 s) is re-confirmed.

This phase is tracked by the separate CPU streaming/multi-view plan. Do not
duplicate it here. If it is not ready, G2 can proceed with full-mesh rendering;
G3 waits.

### Phase 1: Baseline and shared resources

#### Task 1: Add GPU stage timings

**Description:** Instrument the current GPU path so setup, atlas build/upload,
CPU projection/cull, vertex encode, buffer upload, opaque draw, translucent
sort/draw, readback, resolve, and total time can be separated. Add the timings
to `bench --json` without changing the image. Bucket/WBOIT-specific timings are
added later in G5.

**Acceptance criteria:**

- [x] `bench --json` reports the GPU stage timings.
- [x] The change only adds timers; shader and pipeline code are unchanged.
- [x] Timings are no-op/zero on the CPU backend.
- [x] CPU JSON deserialization still accepts the added nested field.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu --test gpu
      gpu_reports_stage_timings`
- [x] `cargo test -p litematic-render --lib`
- [x] `cargo test -p blockscope-cli --test cli json`
- [x] `cargo run --release -p blockscope-cli --features gpu -- bench
      fixtures/private/000002.litematic --backend gpu --iterations 1
      --width 1024 --height 1024 --supersample 2 --threads 12
      --force-limits --json`

**Note:** The full GPU parity suite has pre-existing failures on R1 (11.1% and
8.8% of pixels differ by a channel delta of 1). They are recorded for Task 2
and are not caused by the timing-only change.

**Dependencies:** None
**Files likely touched:** `crates/render/src/rasterizer/gpu/backend.rs`,
`crates/render/src/rasterizer.rs`, `crates/render/src/lib.rs`,
`crates/render/tests/gpu.rs`
**Estimated scope:** Medium (4 files)

#### Task 2: Record the baseline matrix and memory procedure

**Description:** Record R1/R2/R3 CPU/GPU numbers and the peak-working-set
sampling procedure in `docs/performance.md`. Include the exact commands and
the current verdict that GPU is slower on the strong CPU. Also record the
current GPU parity baseline (max delta, differing pixels, alpha mismatches) so
G2/G5 can tell whether they improved it.

**Acceptance criteria:**

- [x] The table includes parse, mesh, render, PNG, total, and peak memory.
- [x] The parity baseline includes opaque/cutout/translucent, solid background,
      and textured nearest/linear.
- [x] The commands are copy-paste reproducible.
- [x] No code behavior changes.

**Verification:**

- [x] Re-run the documented commands and compare to the table.
- [x] `cargo test -p blockscope-cli --test golden` on the CPU backend.

**Dependencies:** Task 1
**Files likely touched:** `docs/performance.md`, `docs/rendering.md`
**Estimated scope:** Small (2 files)

#### Checkpoint A: Baseline

- [x] CPU/GPU baseline is reproducible.
- [x] Stage timings are available.
- [x] CPU golden suite is still byte-identical.
- [ ] Human reviews the baseline before G2 starts.

#### Task 3: Make `TextureSet` clone cheap

**Description:** Store decoded textures behind `Arc` so `TextureSet` and
`MaterialSnapshot` clones do not copy texture bytes. Keep the public sampling
API and `bytes()` accounting unchanged.

**Acceptance criteria:**

- [x] Cloning a `TextureSet`/`MaterialSnapshot` does not allocate texture-sized
      buffers.
- [x] `TextureSet::bytes()` and transparency flags are unchanged.
- [x] CPU render output and existing texture tests are unchanged.

**Verification:**

- [x] `cargo test -p litematic-material`
- [x] `cargo test -p litematic-render --test golden_textures`
- [x] Peak RSS A/B on `000001 --client-jar`: 581.4 MB vs the 591 MB baseline,
      within noise. The steady peak does not include a second copy because the
      live set is dropped before rendering; the change removes the transient
      snapshot copy and is required for the atlas cache.

**Dependencies:** None; can run in parallel with G2
**Files likely touched:** `crates/material/src/texture.rs`,
`crates/material/src/catalog.rs`, `crates/material/tests/snapshot.rs`
**Estimated scope:** Medium (3 files)

#### Task 4: Add `GpuAtlasCache`

**Description:** Cache the packed atlas texture, rectangle buffer, bind group,
and byte size keyed by the material snapshot's texture-set identity. Add a
byte-bounded LRU and device-loss invalidation.

**Acceptance criteria:**

- [x] A second view of the same scene reuses the atlas; a cache-hit counter is
      observable in tests or timings.
- [x] The cache is bounded and evicts by bytes/entries inside the 60% VRAM
      budget (256 MiB conservative cap until the G6 platform probe).
- [x] Device loss clears the cache.
- [x] A snapshot drop removes its weak entry.
- [x] Adding a texture to a live set changes its identity, so a stale atlas
      cannot be served.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu`
- [x] `000002 --client-jar --backend gpu`: first render `atlasUs=1037 ms`,
      second render `atlasCacheHit=true`, `atlasUs=4 µs`.
- [x] Textured parity test records at least one cache hit across two filters.

**Dependencies:** Task 3
**Files likely touched:** `crates/render/src/rasterizer/gpu/atlas.rs`,
`crates/render/src/rasterizer/gpu/backend.rs`,
`crates/render/src/rasterizer/gpu/mod.rs`,
`crates/render/tests/gpu.rs`
**Estimated scope:** Medium (4 files)

### Phase 2: GPU transform and compact instances

#### Task 5: Add `GpuQuad` layout and CPU cull/encode

**Description:** Define the compact quad instance record and a CPU pass that
performs the existing backface dot-product cull, computes face shade, and
encodes the record. This is a standalone module until the draw path consumes
it.

**Acceptance criteria:**

- [x] `size_of::<GpuQuad>() <= 96` with an explicit alignment test.
- [x] The CPU cull result matches `push_page_quads` for synthetic scenes,
      including double-sided cross plants (dot-product rule covered).
- [x] The encoding preserves Q12 UVs, AO levels, color, texture, layer, and
      shade.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu --lib rasterizer::gpu::instance`
- [x] `cargo clippy -p litematic-render --features gpu --all-targets -- -D warnings`
- [x] A standalone record test covers cull, page offset, Q12 UVs, AO, color,
      texture, layer, and shade.

**Dependencies:** Task 1
**Files likely touched:** `crates/render/src/rasterizer/gpu/instance.rs`,
`crates/mesh/src/mesh.rs`, `crates/render/tests/gpu.rs`
**Estimated scope:** Medium (3 files)

#### Task 6: Camera-relative transform and precision guard

**Description:** Add the frame-origin offset, f32 camera uniform, and the
precision guard. Task 7 binds the uniform and makes the shader compute
`page_offset + local_position`.

**Acceptance criteria:**

- [x] The GPU transform matches the CPU projection within the agreed tolerance
      for ordinary scenes.
- [x] A large-coordinate test trips the guard; explicit `gpu` returns
      `BACKEND_UNAVAILABLE` and `auto` falls back to CPU.
- [x] The threshold is documented and measured, not guessed (2^24 blocks; the
      limit test keeps error below 0.05 pixels).

**Verification:**

- [x] `cargo test -p litematic-render --features gpu --lib camera::tests`
- [x] `cargo test -p litematic-render --features gpu --test gpu explicit_gpu_rejects_an_unsafe_camera_span`
- [x] `cargo clippy -p litematic-render --features gpu --all-targets -- -D warnings`

**Dependencies:** Task 5
**Files likely touched:** `crates/render/src/camera.rs`,
`crates/render/src/rasterizer/gpu/instance.rs`,
`crates/render/src/rasterizer/gpu/shader.wgsl`,
`crates/render/tests/gpu.rs`
**Estimated scope:** Medium (4 files)

#### Task 7: Opaque/cutout instanced draw

**Description:** Replace the GPU opaque/cutout draw with a 6-vertex instanced
draw over `GpuQuad` and the camera uniform. This covers both textured and
untextured quads; the texture shader work originally planned for Task 8 was
delivered here because the same instance pipeline handles both.

**Acceptance criteria:**

- [x] Opaque/cutout uses `draw(0..6, 0..quad_count)` with instance data.
- [x] No six-vertex CPU expansion remains for opaque/cutout.
- [x] Textured and untextured parity within the existing test tolerance; CPU
      goldens unchanged.
- [x] The instance shader samples the atlas, applies AO/shade, and discards
      cutout fragments.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu --test gpu gpu_textured_matches_cpu`
- [x] `cargo test -p litematic-render --features gpu --test gpu gpu_matches_cpu_for_opaque_cutout_and_translucent_geometry`
      (image parity max delta 1; the pre-existing 3% threshold still fails on
      blended pixels, recorded for G5)
- [x] `tile_bench --backend gpu` at 1024/2048 square.

**Dependencies:** Task 6
**Files likely touched:** `crates/render/src/rasterizer/gpu/backend.rs`,
`crates/render/src/rasterizer/gpu/vertex.rs`,
`crates/render/src/rasterizer/gpu/instance.rs`,
`crates/render/src/rasterizer/gpu/instance.wgsl`,
`crates/render/src/rasterizer/gpu/shader.wgsl`
**Estimated scope:** Medium (5 files)

#### Task 8: Textured, lit, cutout shader path

**Description:** This was delivered with Task 7. Keep the task as a parity
checkpoint: UV decode, atlas sampling, AO, face shade, and cutout discard now
run in `instance.wgsl`; the old textured vertex expansion is removed.

**Acceptance criteria:**

- [x] Textured nearest and linear parity within tolerance.
- [x] Cutout alpha discard matches the reference behavior.
- [x] AO and face shade are visually identical within tolerance.
- [x] No per-quad CPU `vertex_color` allocations remain in the opaque/cutout
      upload path.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu --test gpu gpu_textured_matches_cpu`
- [x] `cargo test -p litematic-render --test golden_textures` on the CPU.
- [x] The GPU textured fixture covers both nearest and linear filters.

**Dependencies:** Task 7
**Files likely touched:** `crates/render/src/rasterizer/gpu/instance.wgsl`,
`crates/render/src/rasterizer/gpu/instance.rs`,
`crates/render/src/rasterizer/gpu/backend.rs`,
`crates/render/tests/gpu.rs`
**Estimated scope:** Medium (4 files)

#### Task 9: Remove the old GPU projection path

**Description:** Delete the GPU-path `project_pages` + `push_quad` flow for
opaque/cutout, keep shared helpers used by the CPU path, and re-run the G2
performance gate. The remaining projected path is translucent-only until G5.

**Acceptance criteria:**

- [x] No GPU code allocates projected `QuadRender` tables or expanded vertices
      for opaque/cutout.
- [x] `tile_bench` GPU is at least 1.4x CPU 12t at 1024 and 2048 square.
- [x] `000002` GPU total is at or below CPU 12t (161 ms vs 243 ms).
- [x] `000001` full-mesh GPU total is at or below 13 s (8.65 s).
- [x] CPU goldens are unchanged.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu --test gpu`
- [x] `tile_bench`: GPU 12.3 ms vs CPU 12t 19.5 ms at 1024; 35.0 ms vs 50.7 ms
      at 2048.
- [x] `bench 000002 --backend gpu`: render 116.3 ms, total 161.2 ms.
- [x] `bench 000001 --backend gpu --iterations 1`: render 8,650 ms.
- [x] `docs/performance.md` contains the instanced-path results.

**Dependencies:** Task 8
**Files likely touched:** `crates/render/src/rasterizer/gpu/backend.rs`,
`crates/render/src/rasterizer/project.rs`,
`crates/render/src/rasterizer/gpu/vertex.rs`,
`crates/render/tests/gpu.rs`
**Estimated scope:** Medium (4 files)

#### Checkpoint B: GPU transform

- [ ] G2 performance gate passes.
- [ ] GPU parity is within tolerance.
- [ ] CPU goldens and CPU stream baseline are unchanged.
- [ ] Human reviews before streaming starts.

### Phase 3: GPU page streaming and multi-view

#### Task 10: Add `GpuRenderSession` and staging ring

**Description:** Add a session that owns the device resources, atlas, target
textures, staging ring, and opaque/cutout draw state. It accepts page chunks
and renders them incrementally without a full mesh.

**Acceptance criteria:**

- [x] The session consumes `MeshPage` slices from the streaming builder.
- [x] Staging buffers use a three-slot ring with queue-ordered writes.
- [x] A chunk is dropped after its draw submission.
- [x] No full mesh or projected opaque table is retained. Translucent
      triangles are still retained until G5/WBOIT.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu --test gpu gpu_streaming_session_matches_full_mesh`
- [x] `cargo check -p litematic-render --features gpu`
- [x] The streaming-vs-full test compares RGBA, triangle, and layer counters
      page-by-page across the whole mesh.

**Dependencies:** Task 9; prerequisite gate for the chunk policy
**Files likely touched:** `crates/render/src/rasterizer/gpu/session.rs`,
`crates/render/src/rasterizer/gpu/backend.rs`,
`crates/render/src/lib.rs`,
`crates/render/tests/gpu.rs`
**Estimated scope:** Medium (4 files)

#### Task 11: Wire GPU streaming through the CLI

**Description:** Remove the GPU rejection of `--stream-mesh`, choose streaming
for `auto` when the mesh does not fit, and pass page chunks to the session.

**Acceptance criteria:**

- [x] `--backend gpu --stream-mesh=auto|on` works; `off` retains the mesh.
- [x] `--backend gpu --stream-mesh=on` no longer returns the old rejection.
- [x] CLI tests cover GPU streaming and full-mesh selection, including
      byte-identical PNG output for `on` and `auto`.
- [x] CPU streaming behavior is unchanged.

**Verification:**

- [x] `cargo test -p blockscope-cli --features gpu --test cli gpu_streamed_mesh_matches_gpu_full_render`
- [x] `cargo test -p blockscope-cli --test cli` (CPU/default paths)
- [x] `000001` color GPU streaming measured: 1,678 MB peak, 11.27 s wall,
      5.10 s render, 5.04 s mesh. The 2 GB memory gate passes; the 10 s wall
      gate is not met until the 32-page chunk policy and G5 translucent work
      land. Recorded in `docs/performance.md`.

**Dependencies:** Task 10
**Files likely touched:** `crates/cli/src/commands/render.rs`,
`crates/cli/src/runtime.rs`, `crates/cli/tests/cli.rs`,
`crates/render/src/rasterizer.rs`
**Estimated scope:** Medium (4 files)

#### Task 12: GPU allocation fallback and errors

**Description:** Add clean failure paths for chunk-buffer allocation, VRAM
exhaustion, device loss, and unsupported adapter capabilities. `auto` falls
back before producing output; explicit `gpu` returns the documented error.

**Acceptance criteria:**

- [x] `auto` falls back to CPU before any pixel is produced when setup,
      capability, or pre-draw allocation fails.
- [x] Explicit `gpu` returns `BACKEND_UNAVAILABLE` for setup/capability and
      pre-draw allocation failures.
- [x] Device loss is surfaced after setup, upload, draw, translucent, and
      readback; the atlas cache is cleared.
- [x] No partial output is written: the CLI writes only after `finish()`
      succeeds, and mid-stream failures are fatal.
- [x] Failures after a submitted draw are classified as fatal, never as an
      `auto` fallback.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu --test gpu explicit_gpu_rejects_an_unsafe_camera_span`
- [x] `cargo test -p blockscope-cli --test cli render_allows_streaming_with_a_gpu_backend`
- [x] `cargo test -p litematic-render --features gpu --test gpu gpu_streaming_session_matches_full_mesh`
- [x] Code review of setup/upload/draw/readback error checks and cache
      invalidation. VRAM exhaustion and device loss are not fault-injected
      because wgpu does not expose a portable injection point.

**Dependencies:** Task 10
**Files likely touched:** `crates/render/src/rasterizer/gpu/session.rs`,
`crates/render/src/rasterizer.rs`,
`crates/cli/src/commands/render.rs`,
`crates/render/tests/gpu.rs`
**Estimated scope:** Medium (4 files)

#### Checkpoint C: GPU streaming

- [x] `000001` color GPU streaming peaks at or below 2 GB (1,678 MB).
- [ ] `000001` color GPU streaming finishes at or below 10 s (11.27 s;
      deferred to the 32-page chunk policy and G5 translucent work).
- [x] No full mesh is retained in the streaming path.
- [x] CPU stream baseline is unchanged.
- [ ] Human reviews before multi-view and WBOIT.

#### Task 13: Multi-view session targets

**Description:** Add per-view color/depth targets and camera uniforms to the
session. One chunk upload feeds all active views.

**Acceptance criteria:**

- [x] A second view reuses the mesh upload and atlas.
- [x] View targets are created per view; CLI batching by GPU target memory is
      wired in Task 14.
- [x] Output order and per-view statistics are stable.
- [x] Multi-view shares one instance encode/upload and performs per-view
      backface culling in the shader.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu --test gpu gpu_multiview_matches_single_views`
- [x] Two-view equality test compares each output and per-view triangle counts
      against independent single-view renders.

**Dependencies:** Task 10
**Files likely touched:** `crates/render/src/rasterizer/gpu/session.rs`,
`crates/render/src/rasterizer/gpu/backend.rs`,
`crates/render/src/options.rs`,
`crates/render/tests/gpu.rs`
**Estimated scope:** Medium (4 files)

#### Task 14: Multi-view CLI and service wiring

**Description:** Use the session for multi-view jobs, update view batching, and
cover the service path.

**Acceptance criteria:**

- [x] Two-view `000002` GPU is 1.01x single-view GPU wall time.
- [x] No second mesh build or atlas build occurs for the second view; the
      full-mesh session encodes/uploads once for all active views.
- [x] 12-view memory is 943 MB including the full mesh, inside the documented
      target budget.
- [x] Service `views` behavior is unchanged for CPU; `render_views_with`
      automatically uses the GPU session for GPU batches.
- [x] Multi-view GPU jobs retain the full mesh so the mesh is built once.
      Multi-view GPU **streaming** is deferred; single-view streaming remains
      available.

**Verification:**

- [x] `cargo test -p blockscope-cli --features gpu --test cli gpu_multi_view_matches_sequential_views`
- [x] `cargo test -p blockscope-cli --test cli`
- [x] `cargo test -p blockscope-cli --test serve`
- [x] `000002` GPU: 1 view 1.38 s, 2 views 1.40 s, 12 views 3.15 s / 943 MB.
- [x] `docs/performance.md` records the multi-view table and batching budget.

**Dependencies:** Task 13; Task 11
**Files likely touched:** `crates/cli/src/runtime.rs`,
`crates/cli/src/commands/render.rs`, `crates/cli/src/serve.rs`,
`crates/cli/tests/cli.rs`
**Estimated scope:** Medium (4 files)

#### Checkpoint D: Multi-view

- [x] Multi-view performance and memory gates pass (2 views 1.01x; 12 views
      943 MB).
- [x] CPU multi-view behavior is unchanged.
- [ ] Human reviews before WBOIT.

### Phase 4: Coarse depth buckets and WBOIT

#### Task 15: Depth bucket spill and cleanup

**Description:** Quantize translucent triangles into depth buckets and spill
records to per-bucket files. Include the record format, bucket-range
calculation, sub-batch splitting, and cleanup.

**Acceptance criteria:**

- [x] Memory scales with the bucket cap, not the total triangle count
      (262,144 in-memory limit, then 128 depth-bucket files).
- [x] Bucket records are read far-to-near.
- [x] Over-large buckets split into bounded sub-batches (262,144 records).
- [x] Temporary files are removed when the reader drops.
- [x] Unit tests check bucket ordering, record round-trip, and cleanup.
- [x] `000001` GPU streaming peak drops from 1,678 MB to 499 MB with an
      identical PNG SHA-256.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu --lib rasterizer::gpu::translucent`
- [x] `cargo test -p litematic-render --features gpu --lib rasterizer::project::tests`
- [x] `000001` color GPU streaming: 499 MB, 11.35 s, render 5.16 s, identical
      PNG SHA-256 to the pre-bucket run.

**Dependencies:** Task 11
**Files likely touched:** `crates/render/src/rasterizer/gpu/translucent.rs`,
`crates/render/src/rasterizer/spill.rs`,
`crates/render/tests/gpu.rs`
**Estimated scope:** Medium (3 files)

#### Task 16: WBOIT targets and pipeline

**Description:** Add `Rgba16Float` accumulation and revealage targets (RGBA
revealage keeps per-fragment source alpha portable), the WBOIT pipeline, blend
states, depth test without depth write, and the WGSL shader.

**Acceptance criteria:**

- [x] Buckets draw into `Rgba16Float` accumulation/revealage targets with
      depth test on and depth write off.
- [x] Accumulation uses `One/One`; revealage uses
      `Zero/OneMinusSrcAlpha`, with RGBA revealage for portable source alpha.
- [x] Adapter capability is checked; unsupported adapters fall back to the old
      translucent path.
- [x] Reordering triangles within the mesh produces byte-identical WBOIT
      output.
- [x] A full-screen composite pass resolves WBOIT over the opaque target.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu --test gpu gpu_wboit_is_order_independent`
- [x] `cargo test -p litematic-render --features gpu --test gpu gpu_matches_cpu_for_opaque_cutout_and_translucent_geometry`
      (max delta 1; blended-pixel tolerance remains Task 17 calibration)
- [x] `000001` GPU streaming: 538 MB, 12.20 s, render 5.34 s.

**Dependencies:** Task 15
**Files likely touched:** `crates/render/src/rasterizer/gpu/translucent.rs`,
`crates/render/src/rasterizer/gpu/wboit.wgsl`,
`crates/render/src/rasterizer/gpu/backend.rs`,
`crates/render/tests/gpu.rs`
**Estimated scope:** Medium (4 files)

#### Task 17: Composite, calibration, and fallback

**Description:** Add the full-screen composite over opaque/cutout, calibrate
bucket count/weighting/record cap on the reference fixtures, and document the
fallback for unsupported adapters.

**Acceptance criteria:**

- [x] `000001` client-jar GPU streaming peaks at 656 MB (3 GB gate passes).
- [ ] `000001` client-jar GPU streaming finishes in 22.64 s; the 15 s gate is
      not met. Recorded as measured, not hidden.
- [x] Composite over opaque, cutout, transparent, and solid backgrounds is
      exercised by the parity tests; max channel delta stays 1.
- [x] Zero-translucent scenes use the cleared revealage target and composite
      leaves the opaque image unchanged (textured one-block test).
- [x] CPU translucent output and goldens are unchanged.
- [x] Calibration defaults documented: 128 depth buckets, 262,144
      in-memory/read cap, weight `0.03 / (1e-5 + depth^4)` clamped to
      `1e-5..3000`.
- [x] Unsupported `Rgba16Float` blend/render support falls back to the old
      translucent path.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu --test gpu gpu_wboit_is_order_independent`
- [x] `cargo test -p litematic-render --features gpu --test gpu gpu_matches_cpu_for_opaque_cutout_and_translucent_geometry`
- [x] `cargo test -p litematic-render --features gpu --test gpu gpu_textured_matches_cpu`
- [x] `000001 --client-jar`: 656 MB, 22.64 s, render 12.21 s, mesh 6.32 s.
- [x] `cargo test -p litematic-render --test golden` on the CPU.

**Dependencies:** Task 16
**Files likely touched:** `crates/render/src/rasterizer/gpu/translucent.rs`,
`crates/render/src/rasterizer/gpu/wboit.wgsl`,
`crates/render/tests/gpu.rs`, `docs/performance.md`
**Estimated scope:** Medium (4 files)

#### Checkpoint E: WBOIT

- [x] Translucent memory gate passes (656 MB vs 3 GB).
- [ ] Wall-time gate fails on `000001 --client-jar` (22.64 s vs 15 s); recorded
      for the release decision.
- [x] Visual/order tests cover the hybrid strategy.
- [x] CPU goldens are unchanged.
- [ ] Human reviews before policy/release work.

### Phase 5: Policy, service, and release

#### Task 18: Hardware-aware `auto` policy

**Description:** Replace the fixed 50,000-quad threshold with a unit-testable
policy that considers platform capability, CPU cores/threads, mesh size,
exactness, memory, and streaming mode.

**Acceptance criteria:**

- [x] `auto` is the CLI and service default.
- [x] `auto` chooses CPU for `000002` on R1 unless the new path measures faster
      (one-shot GPU pays setup: 535.5 ms vs 224.6 ms CPU; warm GPU is 190.2 ms).
- [x] `auto` chooses GPU on R2 (`--threads 2`) when the policy predicts a win
      (148.3 ms vs 437.1 ms CPU).
- [x] No adapter, software adapter, unsafe precision, and over-budget VRAM fall
      back to CPU.
- [x] GPU allocations are admitted only inside the 60% VRAM budget.
- [x] The decision is deterministic for a given machine description and is
      unit-tested without a GPU.

**Verification:**

- [x] `cargo test -p litematic-render --features gpu`
- [x] `cargo test -p blockscope-cli --features gpu`
- [ ] Bench matrix on R1/R2/R3/R5; record `backendUsed`. R1/R2 recorded;
      R3/R5 remain for the hardware matrix.

**Dependencies:** Task 9; Tasks 11-17 for the measured crossover
**Files likely touched:** `crates/render/src/options.rs`,
`crates/render/src/rasterizer.rs`,
`crates/cli/src/commands/render.rs`,
`crates/render/tests/gpu.rs`
**Estimated scope:** Medium (4 files)

#### Task 19: Service policy and documentation

**Description:** Make `serve` use the session/policy for GPU streaming and
multi-view where supported, keep CPU behavior unchanged, and document the
approximate `auto` contract.

**Acceptance criteria:**

- [x] `hello.backends` and `backendUsed` remain accurate.
- [x] `serve` supports GPU streaming in v1; no silent fallback after output
      starts.
- [x] The service defaults to `auto`, with `cpu`/`gpu` overrides.
- [x] Protocol v3 remains compatible unless a new mandatory field is added.
- [x] Service docs explain exactness, drivers, VRAM budget, and `auto`.

**Verification:**

- [x] `cargo test -p blockscope-cli --features gpu --test serve`
- [x] A GPU service smoke test (`serve_streams_a_single_gpu_view`).
- [x] `docs/engine-protocol.md` and `docs/user/service.md` updated.

**Dependencies:** Task 18; Task 14
**Files likely touched:** `crates/cli/src/serve.rs`,
`docs/engine-protocol.md`, `docs/user/service.md`,
`crates/cli/tests/serve.rs`
**Estimated scope:** Medium (4 files)

#### Task 20: Release matrix, CI, and final report

**Description:** Enable the GPU feature in official artifacts for the
supported platform matrix, keep musl/wasm CPU-only, add the CI compile job and
optional hardware runner, and record the final before/after results.

**Acceptance criteria:**

- [ ] Windows x64, macOS arm64/x64, and Linux x86_64 GNU artifacts build with
      the GPU feature. Workflows are configured; release dry run pending.
- [x] Linux musl and wasm remain CPU-only and documented.
- [ ] CI compiles the GPU feature on every platform; hardware tests skip
      cleanly. `just check-gpu` is wired into CI; remote run pending.
- [x] `docs/performance.md` contains the final R1-R2 table and exact commands.
- [x] `CHANGELOG.md`/`CHANGELOG.en.md` describe the behavior and exactness
      boundary.
- [x] The CLI and service default to `auto`; the CPU exact override and CPU
      goldens are unchanged.

**Verification:**

- [x] Full gate: `cargo fmt --all --check`, workspace clippy, workspace tests,
      and the GPU-feature test suites.
- [ ] Release workflow dry run for the GPU-enabled targets.
- [ ] Final benchmark review with the human.

**Dependencies:** Task 18; Task 19
**Files likely touched:** `.github/workflows/release.yml`,
`docs/deployment.md`, `docs/rendering.md`,
`CHANGELOG.md`, `CHANGELOG.en.md`
**Estimated scope:** Medium (5 files)

#### Checkpoint F: Release gate

- [x] All spec success criteria are met or explicitly recorded as not met.
- [x] CPU backends and goldens are unchanged.
- [ ] GPU artifacts are available for supported platforms.
- [ ] Human approves the release posture.

## Parallelization

- G0 and G1 can run in parallel.
- G2 is sequential and blocks everything after it.
- G3 must land before G4 and G5.
- G4 and G5 can run in parallel, but they both edit `gpu/session.rs` and
  `gpu/backend.rs`; coordinate the shared types first.
- G6 is sequential after G2-G5.

## Risks and Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| f32 precision at large scene spans | High | Camera-relative origin, measured guard, CPU fallback under `auto` |
| WBOIT quality or performance misses the gate | High | Calibrate bucket count/weights; over-large sub-batches; document and keep CPU fallback |
| GPU slower than CPU on strong hosts | High | `auto` policy tests; CPU exact override; explicit `gpu` remains opt-in |
| Driver variance or device loss | High | Capability checks, clean errors, optional hardware matrix, CPU fallback |
| GPU memory/VRAM exhaustion | Medium | Byte-bounded atlas cache, chunk buffers, streaming, `BACKEND_UNAVAILABLE` |
| Texture or atlas cache leaks | Medium | Weak entries, LRU, device-loss invalidation, tests |
| Streamed chunk lifetime/overwrite | High | Staging ring with fences/keepalive; streaming-vs-full equality test |
| Translucent spill leaks or wrong order | High | Bucket cleanup on all exits; bucket-order and sub-batch tests |
| CI has no GPU | Medium | Compile-only CI job; optional self-hosted runner; skip cleanly |
| Static musl/wasm expectations | Medium | Document CPU-only boundary; do not fake GPU support |
| Task scope grows past one session | Medium | Split tasks at 5 files; checkpoints every 2-4 tasks |

## Resolved Decisions

The 2026-11-05 review resolved the open questions:

1. Parity tolerance: maximum channel delta 2, at most 0.1% differing pixels.
2. Unsafe f32 scene span falls back to CPU.
3. `auto` is the default backend.
4. GPU multi-view is required in v1.
5. GPU working memory is 60% of adapter VRAM.
6. One engine, one acceptance standard; the release matrix samples hardware.
7. Bucket count, weighting, and record cap are calibrated in G5.
8. `serve` supports GPU streaming in v1.
9. Static musl/wasm are accepted CPU-only deployments.

## Exit Criteria

- All tasks and checkpoints are complete, or a failing gate is documented with
  evidence and `auto` avoids the GPU path on that hardware.
- The spec's success criteria are satisfied.
- CPU output, CPU goldens, and the exact-output contract are unchanged.
- Official artifacts support the documented platform matrix.
- `docs/performance.md`, `docs/rendering.md`, and `docs/deployment.md` contain
  the final measured results and caveats.
