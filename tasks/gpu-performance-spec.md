# Spec: GPU performance path (`gpu-performance`)

**Status:** Approved 2026-11-05
**Date:** 2026-11-04
**Last updated:** 2026-11-05
**Owner:** Rendering / GPU
**Depends on:** `litematic-render` feature `gpu` (wgpu 30), CPU streaming chunk policy
**Related:** `tasks/gpu-feasibility.md`, `tasks/video-export-spec.md`, `docs/performance.md`

## Decisions incorporated (2026-11-05)

1. Deployment targets every supported platform. The same source builds GPU
   support wherever wgpu can load a hardware driver; everywhere else the same
   binary or artifact uses the CPU path.
2. `auto` is the default backend and selects the CPU or GPU implicitly. The CPU
   backend remains the byte-exact reference and an explicit override.
3. The CPU streaming/multi-view work lands first.
4. Translucency uses coarse external depth bucketing followed by Weighted
   Blended OIT, not a single global sort or a per-tile-only path.
5. GPU support is an in-tree feature, not a separate codebase. Official
   artifacts enable it where the platform can load drivers; the only separate
   artifact is the static-musl/wasm CPU fallback required by platform limits.
6. GPU parity tolerance is a maximum channel delta of 2 and at most 0.1% of
   pixels differing. WBOIT translucency is covered by this tolerance.
7. An unsafe f32 camera-relative scene span falls back to the CPU. There is no
   slow f64 GPU path.
8. GPU multi-view is required in v1, not deferred.
9. The GPU working budget is 60% of adapter VRAM. The atlas cache, render
   targets, chunk buffers, and readback staging all fit inside that budget.
10. There is one engine and one acceptance standard: wgpu for every platform,
    the same parity tolerance, the same `auto` policy, and the same performance
    gates. The release test matrix only samples hardware where the same suite
    runs; it does not define vendor-specific standards.
11. `serve` supports GPU streaming in v1.
12. Static musl and wasm are accepted CPU-only deployments.
13. Bucket count, WBOIT weighting, and the per-bucket record cap are calibrated
    by the reference benchmarks; they are not product decisions.

## Assumptions I'm making

1. GPU support is compiled into official builds where the platform can load a
   hardware driver. `auto` is the default backend; the CPU backend remains the
   only byte-exact reference and an explicit override.
2. "GPU performance route" means a second-generation GPU path, not another
   tuning pass over the current CPU-projection + CPU-vertex-expansion design.
3. Target hardware is every wgpu-supported hardware adapter on Windows, Linux,
   and macOS: discrete and integrated GPUs from the major vendors. Software
   adapters are rejected. wasm/Cloudflare and static-musl builds use the CPU
   path because they cannot load a GPU driver.
4. Large-coordinate scenes can fall back to the CPU when f32 GPU precision is
   not safe. The fallback is documented and observable.
5. The CPU streaming/multi-view fix is a separate, earlier spec. This spec
   reuses its page-chunk policy where possible and does not duplicate it.
6. GPU output is visually approximate. The default `auto` backend may choose it
   implicitly; callers that require byte-exact output select `cpu`. The
   exactness boundary is documented and reported through `backendUsed`.
7. `TextureSet` storage may become `Arc`-shared. This is output-neutral and is a
   prerequisite for atlas caching.
8. GPU work does not change scene hashes, manifests, mesh statistics, or CPU
   golden PNGs.
9. No new external dependency is required beyond the existing optional
   `wgpu`, `pollster`, and `bytemuck` dependencies.
10. Static musl and wasm are the only intentionally CPU-only deployments. A
    static binary cannot load a Vulkan ICD at runtime, and a Worker isolate has
    no GPU. This is a platform constraint, not a missing feature.

## Objective

Make the GPU backend a net end-to-end win whenever it is selected, instead of
only a faster rasterizer wrapped in a slower CPU feeder. "Supported" means the
same source builds on Windows, Linux, and macOS; a machine with a usable
hardware adapter may use the GPU, and a machine without one, including
static-musl and wasm, uses the CPU path with no behavior break.

Today the GPU path accelerates fragment work, but it still:

- projects every quad on the CPU;
- expands every quad into six 36-byte vertices on the CPU;
- requires a complete mesh, so large scenes pay the full-mesh memory peak;
- rebuilds the texture atlas on every render;
- retains all translucent triangles on the CPU for a global sort.

The result is that on a 12-core CPU the current GPU path is slower than the CPU
backend end to end, and on the largest fixture it is about 2.1x slower while
using about 18x the streamed memory.

Success means:

- the GPU receives compact mesh data and performs camera transform, UV decode,
  lighting, and quad-to-triangle expansion itself;
- the GPU path can render from streamed page chunks with memory close to the
  CPU streaming path;
- a material snapshot's atlas is built once and reused across views and
  requests;
- translucent rendering is bounded in memory by coarse depth buckets and uses
  Weighted Blended OIT within each bucket;
- `auto` may choose the GPU implicitly, but never when the measured or
  estimated CPU path is faster;
- the CPU backend, golden tests, and exact-output contract are unchanged;
- official builds enable the GPU feature wherever the platform can load a
  driver, and the same source still runs CPU-only where it cannot.

Primary user stories:

```bash
# The default auto policy picks CPU or GPU.
blockscope render build.litematic --output out.png

# A 2-core host with a GPU is the target niche.
blockscope render build.litematic --output out.png --backend gpu --threads 2

# A large scene streams through the GPU instead of materializing the full mesh.
blockscope render huge.litematic --output out.png --backend gpu --stream-mesh=on

# A service can expose the approximate preview backend next to exact CPU output.
blockscope serve --client-jar client.jar --threads 4
```

## Measured baseline

These numbers were measured on 2026-11-04 with the current release build. The
host is a 12-core / 48 GB Windows machine with an NVIDIA GTX 1650 SUPER (4 GB),
D3D12, color-only materials, 1024x1024 output, supersample 2, 12 threads unless
noted. `000002` is 370x370x370, 50.65M blocks, 2.3M quads. `000001` is the
9.39M-block stress fixture and produced 87.9M quads in this build.

| Case | CPU 12t | CPU 2t | GPU | Notes |
|---|---:|---:|---:|---|
| `000002` render | 182 ms | 341 ms | 207 ms | `bench`, median of 3 |
| `000002` render + PNG | 225 ms | 385 ms | 249 ms | GPU is 11% slower than CPU 12t |
| `000001` mesh | 4,047 ms | - | 3,516 ms | mesh is CPU work in both paths |
| `000001` render | 7,046 ms | - | 14,881 ms | GPU is 2.1x slower |
| `000001` end to end | about 11.3 s | - | about 18.7 s | includes parse, mesh, render, PNG |

`tile_bench` synthetic scene (96,882 quads, 18 pages), median of 5:

| Resolution | CPU 1t | CPU 12t | GPU | GPU vs CPU 12t |
|---|---:|---:|---:|---:|
| 1024x1024, ss1 | 88.8 ms | 21.9 ms | 15.8 ms | 1.39x faster |
| 2048x2048, ss1 | 247.7 ms | 51.3 ms | 36.2 ms | 1.42x faster |

The pure rasterizer is faster on this GPU, but the real pipeline loses because
the CPU still does the expensive per-quad projection and vertex expansion. The
current GPU path builds 6 vertices per quad at 36 bytes each; `000001` therefore
stages and uploads roughly 19 GB of vertex data per render.

The existing feasibility measurements on an Apple M4 Max show the same
direction on a strong CPU: 16-thread CPU rasterization beat the integrated GPU
at 1024, 2048, and 4096 square outputs. GPU output was not byte-exact: about
0.08% of pixels differed by at most one channel value on Metal.

The current large-scene memory comparison is equally important:

| Path | `000001` color | `000001` client jar |
|---|---:|---:|
| CPU stream | 433-435 MB | 591 MB |
| Full mesh | 7.68 GB | 18.9 GB |

The GPU backend requires a complete mesh today, so choosing GPU also chooses
the full-mesh memory peak.

Current GPU parity on R1 (96x96 reference scenes) is worse than the target:
opaque/cutout/translucent differs on 1,021 of 9,216 pixels (11.1%) with a
maximum channel delta of 1; a solid background differs on 815 of 9,216 pixels
(8.8%), also delta 1, with no alpha-coverage mismatches. Textured nearest is
bit-identical; linear filtering was 44 of 9,216 pixels on the M4 host. This is
pre-existing blend-rounding behavior, not a timing regression. The 0.1% gate is
a target for the second-generation path; G2 and G5 own closing it or recording
the measured exception.

## Goals

1. Remove CPU projection and per-quad vertex expansion from the GPU path.
2. Upload compact quad instance data, not six expanded vertices per quad.
3. Make the GPU path compatible with page-streamed mesh construction.
4. Cache the texture atlas and bind group for a material snapshot.
5. Implement coarse external depth bucketing plus Weighted Blended OIT for
   bounded-memory translucent rendering.
6. Share one mesh build and one upload across multi-view renders.
7. Make backend selection honest about exactness and expected performance,
   including implicit GPU selection under `auto`.
8. Make `auto` the default backend, with CPU as the exact override, and enable
   GPU support in official builds wherever drivers can load.

## Non-goals

- Replacing the CPU backend as the golden reference or exact override.
- Byte-exact GPU output across vendors and drivers.
- A slow f64 GPU projection path for scenes outside the f32-safe span; those
  scenes fall back to the CPU.
- GPU mesh construction, NBT parsing, scene construction, PNG encoding, or
  material decoding.
- CPU streaming chunk and multi-view reuse work; that is a separate,
  higher-priority spec.
- Video encoding; `tasks/video-export-spec.md` owns that. This spec may expose a
  `GpuRenderSession` that video export can reuse later.
- LOD, new file formats, new texture features, or new lighting models.
- Multi-GPU, distributed rendering, or WebGPU/wasm GPU support.
- Software adapters (lavapipe, WARP, and similar) as a GPU fallback.
- GPU acceleration on static-musl Linux or wasm; those deployments use the CPU
  path because the platform cannot load a hardware driver.
- Reproducing the CPU's exact per-tile translucent ordering on the GPU; the
  selected GPU strategy is visually approximate by design.

## Sequencing

The recommended order across the performance work is:

1. CPU streaming chunk aggregation and multi-view mesh reuse. This is an
   approved prerequisite and lands first; G2/G3 reuse its chunk policy.
2. This GPU performance path.
3. Texture sharing, serve cache, and sparse parser work as separately scoped
   follow-ups.

Within this spec, G2 is the core change. G3 and G4 depend on it. G1 can run in
parallel with G0/G2. G5 can start after G3. G6 is last.

## Tech Stack

- Rust 2024, `wgpu` 30, WGSL, `bytemuck`, `pollster`.
- `litematic-render` feature `gpu`, enabled by the CLI feature `gpu`.
- Existing `litematic-mesh` structures, with `repr(C)` and `bytemuck` support
  where needed.
- Existing `litematic-material::TextureSet` and `MaterialSnapshot`.
- WBOIT uses `Rgba16Float` accumulation and revealage targets with a full-screen
  composite pass before readback. Revealage stays RGBA so
  `OneMinusSrcAlpha` has a portable per-fragment source alpha.
- No new external dependency.
- GPU tests skip when no hardware adapter is available.

## Commands

```bash
# Build and focused tests
cargo build --workspace
cargo build -p blockscope-cli --features gpu
cargo test -p litematic-render
cargo test -p litematic-render --features gpu
cargo test -p blockscope-cli --features gpu

# GPU parity and smoke tests
cargo test -p litematic-render --features gpu --test gpu
cargo test -p blockscope-cli --features gpu --test cli

# Rasterizer and end-to-end benchmarks
cargo run --release -p litematic-render --features gpu --example tile_bench -- \
  --backend cpu --iterations 5
cargo run --release -p litematic-render --features gpu --example tile_bench -- \
  --backend gpu --iterations 5
cargo run --release -p blockscope-cli --features gpu -- bench \
  fixtures/private/000002.litematic \
  --backend cpu --threads 12 --force-limits --iterations 3 --json
cargo run --release -p blockscope-cli --features gpu -- bench \
  fixtures/private/000002.litematic \
  --backend gpu --threads 12 --force-limits --iterations 3 --json

# Streaming and multi-view comparisons
cargo run --release -p blockscope-cli --features gpu -- render \
  fixtures/private/000001.litematic --output out-gpu.png \
  --backend gpu --stream-mesh=on --force-limits
cargo run --release -p blockscope-cli --features gpu -- render \
  fixtures/private/000002.litematic --output out-views.png \
  --backend gpu --views 45,225 --force-limits

# Full gate
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
just check
```

GPU parity and performance runs require a hardware adapter. The test harness
must print `BACKEND_UNAVAILABLE` and skip, not fail, when no adapter exists.

## Project Structure

```text
crates/render/src/rasterizer/gpu/
  backend.rs        device, pipelines, frame targets, draw submission
  instance.rs       compact GpuQuad layout and CPU-side upload encoding
  atlas.rs          texture atlas packing and cached GPU atlas resources
  translucent.rs    coarse depth buckets, spill, WBOIT passes, composite
  resolve.rs        readback and supersample resolve
  shader.wgsl       vertex transform, lighting, UV decode, sampling
  wboit.wgsl        WBOIT accumulation, composite, and bucket draws
  session.rs        reusable GPU render session for views and frames
crates/render/src/rasterizer.rs       backend dispatch and statistics
crates/render/src/options.rs          backend policy options
crates/render/src/lib.rs              public session/backend exports
crates/material/src/texture.rs        Arc-shared texture storage
crates/material/src/catalog.rs        snapshot texture ownership
crates/cli/src/commands/render.rs     CLI GPU + streaming flags
crates/cli/src/runtime.rs             view batching and session orchestration
crates/cli/src/commands/bench.rs      stage timings and benchmark output
crates/cli/src/serve.rs               per-request backend/session policy
crates/render/tests/gpu.rs            parity and device-failure tests
crates/cli/tests/cli.rs               CLI smoke tests
docs/rendering.md                     GPU path, parity, determinism
docs/performance.md                   baseline and after tables
docs/deployment.md                    platform GPU build and drivers
```

## Code Style

Use `repr(C)` + `bytemuck` for GPU upload records, explicit field order, and
comments that explain the layout or precision constraint rather than restating
the field names.

```rust
/// One quad uploaded as instance data. The shader expands it into two
/// triangles; the CPU never builds the six expanded vertices.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuQuad {
    positions: [[f32; 3]; 4], // page-local
    uvs: [[i16; 2]; 4],       // Q12, decoded in the shader
    page_offset: [f32; 3],    // page origin relative to the frame origin
    color: [u8; 4],
    ao: [u8; 4],
    shade: f32,
    texture: u16,
    flags: u8,
    _pad: [u8; 1],
}
```

Conventions:

- Keep the hot path monomorphized; no trait objects per quad.
- Prefer explicit structs over tuple-heavy APIs.
- Keep files under 800 lines; split GPU shader/instance/session modules.
- Comments may be Chinese for domain intent; identifiers stay English.
- No emoji.

## Design

### 1. Camera-relative GPU coordinates

The CPU projection uses f64 world coordinates. Uploading absolute world
positions as f32 would lose precision on large builds (`f32` has roughly
2-block resolution near 30M). The GPU path uses camera-relative coordinates:

- `frame_origin` is the scene center rounded to an integer triple.
- Each page contributes `page_offset = (page.origin - frame_origin) as f32`.
- The vertex shader computes `relative = page_offset + page_local_position`.
- The camera matrix maps `relative` to clip space.

For orthographic projection, translation is separable, so this preserves
precision for ordinary scene spans. Add a guard: if the scene span or the
relative coordinate magnitude exceeds the tested f32-safe threshold, the GPU
path reports `BACKEND_UNAVAILABLE` (or falls back to CPU under `auto`) instead
of producing a silently wrong image. The threshold is measured, not guessed,
and is documented with the precision tests.

### 2. Compact quad instances and GPU expansion

The mesh already stores a compact `Quad`: four 16-byte vertices plus
quad-uniform color, normal, AO, texture, layer, and shade. The GPU path uploads
a `GpuQuad` derived from that data, once per page chunk. Target layout is
96 bytes or less per quad, versus the current 216 bytes per quad of expanded
36-byte vertices.

Draw with a shared six-index buffer and instancing:

```text
draw_indexed(0..6, 0..quad_count)
```

The vertex shader uses the index (`0,1,2,0,2,3`) to select a corner and the
instance index to select the quad. The CPU no longer builds projected
`QuadRender` tables or expanded vertex vectors for opaque/cutout geometry.

Backface culling stays on the CPU as a cheap per-quad dot product, matching the
existing `dot(normal, camera.forward) >= 0` rule. This preserves culling
semantics, reduces upload volume, and avoids relying on winding conventions
inside the shader. The CPU cull pass must remain output-equivalent to
`push_page_quads`.

Face shade is computed per quad during the same CPU pass and uploaded as one
f32. Ambient occlusion stays as four levels; the shader reconstructs the same
`AO_LEVELS` table. UVs stay Q12 and are decoded in the shader. The shader
produces the same interpolated color, texture sample, cutout discard, and
premultiplied blend order as the current GPU path.

### 3. Shader pipeline

Vertex stage:

- select one of four corners via the shared index buffer;
- add `page_offset` to the page-local position;
- apply the camera matrix;
- pass color, AO level, shade, UV, texture id, and flags to the fragment stage.

Fragment stage:

- decode Q12 UVs;
- look up the atlas rectangle for the texture id;
- sample nearest or linear using the existing wrap and half-texel rules;
- apply face shade, vertex AO, and tint;
- discard cutout fragments below the alpha threshold;
- write premultiplied color for opaque/cutout blending.

Translucent fragments use the separate WBOIT pipeline and targets described in
section 6. The CPU resolve/downsample order stays unchanged so the CPU/GPU
comparison remains meaningful.

### 4. Streaming lifecycle

The GPU renderer consumes the same page chunks as the CPU streaming builder:

1. build a page chunk on the mesh workers;
2. cull and encode compact quad instances on the CPU;
3. upload one instance buffer and the chunk's page offsets;
4. issue opaque/cutout draws;
5. push translucent geometry into the bounded translucent store;
6. drop the chunk and reuse the staging buffer.

No complete mesh or projected quad table is retained. The staging buffers use a
small ring (two or three in flight) so a chunk cannot be overwritten before the
GPU has consumed it. Buffer capacity is chosen from the existing chunk budget
(for example 300k-600k quads) and shrinks only when a chunk is much smaller.
The exact chunk size is shared with the CPU streaming policy where possible.

`--backend gpu` no longer rejects `--stream-mesh=on`; `auto` may choose the
streaming form when the mesh would not fit the memory budget. If the GPU
adapter cannot allocate the required chunk buffers, an explicit `gpu` request
fails with `BACKEND_UNAVAILABLE`; `auto` falls back to the CPU before any pixel
is produced.

### 5. Atlas cache

`TextureSet` becomes cheap to clone by storing `Arc<Texture>` or `Arc<[u8]>`
inside it. `MaterialSnapshot` keeps an `Arc<TextureSet>` (or an equivalent
stable owner) so the GPU cache can key on identity rather than on a hash of all
texture bytes.

`GpuAtlasCache` is process-wide and keyed by the snapshot's texture-set
identity. It stores:

- the packed RGBA atlas texture;
- the atlas rectangle storage buffer;
- the bind group layout/bind group;
- the atlas byte size for eviction.

The cache is bounded by bytes and entry count, invalidated on device loss, and
is safe to share across views and requests. A cache hit avoids repacking and
re-uploading every texture. A `MaterialSnapshot` that is dropped evicts its
weak entry.

The total GPU working budget is 60% of adapter VRAM. Render targets, chunk
buffers, readback staging, and the atlas cache all fit inside that budget. The
atlas cache evicts before an allocation would exceed the budget; it does not
compete with the current frame's render targets. The first version may cache
only one atlas per process; the byte-bounded LRU is the target if `serve` keeps
multiple snapshots.

### 6. Translucent geometry: coarse depth buckets + WBOIT

Opaque and cutout quads draw as chunks arrive. Translucent quads use a hybrid
of coarse ordering and order-independent blending:

1. **Coarse external depth bucketing.** Each translucent triangle gets a
   quantized depth key over the camera depth range. The initial target is 64-256
   buckets; the exact count is calibrated on the reference fixtures. Records are
   written to per-bucket files instead of being retained in one in-memory
   vector, so memory scales with the bucket cap rather than the total triangle
   count.
2. **Far-to-near bucket order.** Buckets are processed from the farthest depth
   range to the nearest. This preserves the large-scale layering that a single
   global depth range needs and keeps depth-separated water, glass, and ice
   readable.
3. **WBOIT within a bucket.** Each bucket draws into two offscreen targets:
   `Rgba16Float` accumulation and `Rgba16Float` revealage. The revealage target
   uses RGBA rather than R16Float because WebGPU's `OneMinusSrcAlpha` blend
   factor needs a per-fragment source alpha, which a single-channel fragment
   output cannot provide portably. Depth test is enabled against the
   opaque/cutout depth buffer; depth write is disabled. Weighted blended OIT
   makes the result independent of triangle order inside the bucket.
4. **Bounded sub-batches.** Because WBOIT does not care about order within a
   bucket, an over-large bucket can be split into arbitrary sub-batches and
   drawn sequentially into the same accumulation targets. The per-bucket record
   cap therefore bounds CPU spill memory and upload size without changing the
   compositing result.
5. **Composite.** A full-screen pass resolves the WBOIT accumulation and
   revealage targets over the opaque/cutout color target. The existing
   readback, premultiplied resolve, and supersample downsample then run
   unchanged.

The bucket count, weighting function, and per-bucket record cap are calibrated,
not guessed. The CPU per-tile sort remains the reference for CPU output. GPU
translucent output is visually approximate on intersecting surfaces and is
covered by tolerance/visual tests, not byte-exact goldens.

If an adapter cannot create the required MRT/float targets or blend modes, the
GPU path either uses a documented lower-precision fallback or returns
`BACKEND_UNAVAILABLE`; `auto` then uses the CPU before producing output. The
CPU `TranslucentStore` spill semantics and cleanup rules remain the reference.
Temporary files must be removed on success and on every normal error path.

### 7. Multi-view and `GpuRenderSession`

A `GpuRenderSession` owns the device resources, atlas, staging ring, and one or
more view targets. It exposes:

```text
new(scene, mesh, options, textures) -> session
render_view(camera, options) -> image
render_views(cameras, options) -> images
finish() -> cleanup
```

For multi-view, one mesh build and one chunk upload feed all active views. Each
view has its own color/depth target and camera uniform; the same instance
buffer is drawn once per view. The atlas and pipelines are shared. Views are
batched by GPU target memory, not only by output RGBA bytes.

This session is also the natural follow-up for video export: keep the mesh and
atlas resident, change only the camera between frames. Video export remains out
of scope here.

GPU multi-view is required in v1. If it misses a memory or determinism gate,
`auto` must select the CPU for multi-view jobs rather than silently claiming a
GPU multi-view result; explicit `gpu` reports the limitation. This decision is
recorded, not implied.

### 8. Backend policy and exactness

`auto` is the default backend. The CPU backend remains the exact reference and
an explicit override. The GPU is selected when:

- the caller explicitly selects `--backend gpu` / service `"backend":"gpu"`, or
- the default `auto` policy finds the feature compiled, a hardware adapter
  present, and a predicted win.

Because the default `auto` can produce approximate output, the CLI and service
documentation must say so, and every result reports `backendUsed`. Callers that
require byte-exact output select `cpu`.

`auto` must consider:

- whether the platform can load a hardware driver at all;
- CPU core count and the caller's thread budget;
- mesh quad count and estimated raster share;
- whether the caller requires exact output;
- available system memory and GPU memory;
- whether the scene is streaming or full-mesh;
- the 60% VRAM working budget, including atlas cache headroom;
- the measured crossover for the current hardware family, not just the fixed
  50,000-quad threshold.

The fixed threshold is removed or becomes one input among several. The same
policy runs on every platform; there are no vendor-specific exceptions. On the
12-core reference host, `auto` must not choose GPU for `000002` unless the new
path measures faster. If hardware detection is uncertain, prefer CPU.

The service protocol keeps the existing `backend` request field and
`backendUsed` result field. New optional statistics fields are acceptable; a
protocol version bump is required only if a field becomes mandatory or changes
meaning.

### 9. Release and deployment

GPU support is an in-tree feature, not a separate codebase. There is no
"GPU build" in the sense of a fork or a second implementation; the same source
is compiled with `--features gpu` and selects a backend at runtime.

Official artifacts enable the feature where the platform can load a hardware
driver:

| Platform | Primary backend | GPU feature | Fallback |
| --- | --- | --- | --- |
| Windows x64 (MSVC) | D3D12, Vulkan | enabled | CPU |
| macOS arm64 / x64 | Metal | enabled | CPU |
| Linux x86_64 GNU | Vulkan | enabled | CPU |
| Linux x86_64 musl | none | disabled | CPU |
| wasm / Cloudflare Workers | none | disabled | CPU |
| Other targets | wgpu if available | source build | CPU |

Static musl cannot load a Vulkan ICD at runtime, and a Worker isolate has no
GPU. Those are platform constraints, not missing code paths. The source build
still supports GPU on any target where wgpu can load a hardware adapter.

There is one engine and one acceptance standard. Every GPU-capable platform
runs the same wgpu feature set, the same parity tolerance, the same `auto`
policy, and the same performance gates. The platform matrix is where that
single suite is executed, not a set of vendor-specific standards.

Document drivers, device selection, `WGPU_BACKEND`, headless container
passthrough, and the non-exact output contract. CI compiles the GPU feature on
every platform; hardware parity and performance tests run on an optional
self-hosted GPU runner and skip otherwise. Release artifacts are not split into
a "GPU" and "non-GPU" code line; only the platform-capability matrix decides
whether the feature is compiled.

## Implementation Slices

### G0: Baseline and stage instrumentation

- Add per-stage timings to the `bench --json` output: atlas build/upload,
  CPU cull/encode, buffer allocation/upload, opaque draw submit/wait,
  translucent bucket encode/spill, WBOIT draw/composite, readback, resolve,
  and PNG. Keep the public render statistics compatible.
- Record the current CPU/GPU matrix on the reference hosts in
  `docs/performance.md`.
- Add a small script or documented command for peak working-set sampling.

Acceptance:

- the same image is produced as today;
- timings identify projection, upload, draw, and readback separately;
- the baseline table is reproducible from the documented commands.

Files: `crates/render/src/rasterizer/gpu/backend.rs`,
`crates/render/src/rasterizer.rs`, `crates/cli/src/commands/bench.rs`,
`docs/performance.md`.

### G1: Shared texture storage and atlas cache

- Store textures behind `Arc` so `TextureSet`/`MaterialSnapshot` clones do not
  copy texture bytes.
- Add a process-wide, byte-bounded `GpuAtlasCache` keyed by texture-set
  identity.
- Reuse the atlas and bind group across views and requests while the snapshot
  is alive.

Acceptance:

- cloning a snapshot does not allocate texture-sized buffers;
- a second view of the same scene reuses the atlas (instrument a cache hit);
- CPU output and CPU memory baselines do not regress;
- 000001 client-jar peak working set drops by the measured duplicate-texture
  amount, or the change is reverted.

Files: `crates/material/src/texture.rs`, `crates/material/src/catalog.rs`,
`crates/render/src/rasterizer/gpu/atlas.rs`,
`crates/render/src/rasterizer/gpu/backend.rs`.

### G2: GPU-side transform and compact quad instances

- Replace GPU-path `project_pages` + `push_quad` with CPU cull/encode into
  `GpuQuad` and a shader-side camera transform.
- Add the shared index buffer and instanced draw.
- Decode UVs, AO, and lighting in the shader.
- Add the camera-relative precision guard and CPU fallback.

Acceptance:

- the GPU path no longer allocates projected `QuadRender` tables or six
  expanded vertices per quad for opaque/cutout geometry;
- `tile_bench` GPU is at least 1.4x CPU 12t at 1024 and 2048 square outputs;
- `000002` GPU total is at or below CPU 12t total (target 0.9x, about 203 ms);
- `000001` full-mesh GPU total beats the current 18.7 s and reaches 13 s or
  less;
- parity stays within the agreed tolerance; CPU goldens are unchanged.

Files: `crates/render/src/rasterizer/gpu/backend.rs`,
`crates/render/src/rasterizer/gpu/instance.rs`,
`crates/render/src/rasterizer/gpu/vertex.rs`,
`crates/render/src/rasterizer/gpu/shader.wgsl`,
`crates/mesh/src/mesh.rs`, `crates/render/tests/gpu.rs`.

### G3: GPU page streaming

- Consume `build_mesh_streaming` page chunks directly.
- Add the staging-buffer ring and per-chunk draw submission.
- Remove the GPU rejection of `--stream-mesh`.
- Wire the CLI and service single-view paths.

Acceptance:

- `000001` color renders through GPU streaming with peak working set at or
  below 2 GB and total wall time at or below 10 s;
- no full mesh or projected quad table is retained;
- `000002` GPU streaming does not regress versus GPU full mesh;
- `--backend gpu --stream-mesh=auto|on` works; `off` still retains the mesh;
- a failed GPU allocation falls back under `auto` before producing output.

Files: `crates/cli/src/commands/render.rs`, `crates/cli/src/runtime.rs`,
`crates/render/src/rasterizer/gpu/backend.rs`,
`crates/render/src/rasterizer/gpu/session.rs`,
`crates/render/tests/gpu.rs`.

### G4: Multi-view GPU session

- Share one mesh build, one chunk upload, and one atlas across views.
- Add per-view color/depth targets and camera uniforms.
- Batch views by GPU target memory.
- Expose `GpuRenderSession` for the CLI and a future video path.

Acceptance:

- two-view `000002` wall time is at most 1.3x the single-view GPU wall time;
- no second mesh build or second atlas build occurs for a second view;
- 12-view memory stays within the documented budget;
- output order and per-view statistics remain stable.

Files: `crates/render/src/rasterizer/gpu/session.rs`,
`crates/render/src/rasterizer/gpu/backend.rs`,
`crates/cli/src/runtime.rs`, `crates/cli/src/commands/render.rs`.

### G5: Coarse depth buckets + WBOIT

- Quantize translucent triangles into the calibrated depth buckets and spill
  records per bucket.
- Draw buckets far to near into `Rgba16Float` accumulation and revealage
  targets, with depth test on and depth write off.
- Add the composite pass over the opaque/cutout target.
- Split over-large buckets into sub-batches; WBOIT does not require order within
  a bucket.
- Calibrate bucket count, weighting, and record cap on the reference fixtures.
- Add the adapter-capability fallback and document any lower-precision path.

Acceptance:

- `000001` client-jar GPU streaming peaks at or below 3 GB and finishes at or
  below 15 s;
- translucent memory is bounded by the documented bucket cap, not by total
  triangle count;
- reordering triangles inside a bucket produces the same WBOIT composite within
  the agreed tolerance;
- bucket spill files are removed on success and normal errors;
- visual/tolerance tests cover the hybrid strategy;
- CPU translucent output is unchanged.

Files: `crates/render/src/rasterizer/gpu/translucent.rs`,
`crates/render/src/rasterizer/gpu/wboit.wgsl`,
`crates/render/src/rasterizer/gpu/backend.rs`,
`crates/render/src/rasterizer/spill.rs`,
`crates/render/src/rasterizer/project.rs`,
`crates/render/tests/gpu.rs`.

### G6: Backend policy, service, and release

- Replace the fixed GPU threshold with the policy in section 8.
- Make `auto` the default; keep CPU as the exact override; document `auto` as
  approximate and allow it to choose GPU implicitly.
- Enable the GPU feature in official artifacts for the supported platform
  matrix; keep musl/wasm CPU-only.
- Add the CI compile job and the optional hardware runner.
- Record before/after numbers and remove stale feasibility claims.

Acceptance:

- the CLI and service default to `auto`;
- on the 12-core reference host, `auto` does not choose GPU for `000002`
  unless the new path is measurably faster;
- on a weak-CPU host, `auto` does choose GPU when the policy predicts a win;
- an explicit `gpu` request fails cleanly when no adapter exists;
- the service reports `backendUsed` accurately;
- official Windows, macOS, and Linux GNU artifacts build with the GPU feature;
- the CPU musl artifact remains unchanged and still runs everywhere;
- `docs/performance.md` contains the after table and the exact commands.

Files: `crates/render/src/options.rs`, `crates/cli/src/commands/render.rs`,
`crates/cli/src/serve.rs`, `.github/workflows/release.yml`,
`docs/rendering.md`, `docs/performance.md`, `docs/deployment.md`.

## Testing Strategy

### Unit tests

- `GpuQuad` size, alignment, field encoding, and `bytemuck` round trip.
- Camera-relative transform against the CPU projection over ordinary and
  large scene offsets; guard boundary tests.
- Backface cull parity between the CPU upload pass and `push_page_quads`.
- Atlas cache hit/miss, byte-bounded eviction, and device-loss invalidation.
- Staging-ring reuse does not overwrite an in-flight chunk.
- Streaming chunk lifetime, spill cleanup, and error propagation.

### GPU parity tests

- Keep the existing `crates/render/tests/gpu.rs` scenes: opaque, cutout,
  translucent, textured nearest, textured linear, solid and transparent
  backgrounds.
- Add a larger synthetic scene and at least one real-fixture smoke test.
- Compare RGBA with tolerances, not byte equality:
  - maximum channel delta at most 2 for the reference scenes;
  - differing pixels at most 0.1% for opaque/cutout and textured scenes;
  - no alpha-coverage mismatch;
  - document the translucent ordering exception.
- Add translucent hybrid tests:
  - bucket-order invariance: drawing buckets far-to-near is required; reversing
    the bucket order must change the image;
  - sub-batch invariance: splitting one bucket into arbitrary sub-batches must
    produce the same WBOIT composite within tolerance;
  - composite correctness over opaque, cutout, transparent, and solid
    backgrounds;
  - zero-alpha and near-zero revealage edge cases;
  - adapter capability fallback when float MRT targets are unavailable.
- Run the CPU golden suite unchanged. GPU never participates in the byte-exact
  golden gate.

### Performance tests

Run on the reference matrix:

- R1: 12-core / 48 GB / GTX 1650 SUPER, Windows D3D12.
- R2: R1 with `--threads 2` to represent a weak CPU.
- R3: Apple M4 Max, Metal, from the existing feasibility host.
- R4: at least one additional hardware vendor (AMD or Intel) on Windows or
  Linux, plus one integrated GPU if available.
- R5: a machine with no usable adapter and a static-musl build, to verify the
  CPU fallback and clean `BACKEND_UNAVAILABLE` behavior.

Collect:

- `tile_bench` at 1024, 2048, and 4096 square, ss1 and ss2;
- `bench` for `000002` and `000001`, color-only and client jar;
- `render` for 1, 2, 4, and 12 views;
- peak working set sampled as in `docs/performance.md`;
- GPU stage timings from G0.

Add an `auto` policy matrix: strong CPU, weak CPU, no adapter, software adapter,
exact-output request, streaming scene, and full-mesh scene. The policy must be
deterministic for a given machine description so it can be unit-tested without
a GPU.

Do not gate default CI on hardware-specific GPU numbers. The CPU gate stays
mandatory; the GPU performance table is a release/decision gate.

### CI

- `cargo check`/`clippy` with `--features gpu` on all platforms.
- CPU tests and goldens remain mandatory.
- GPU tests skip cleanly when no adapter is present.
- An optional self-hosted GPU runner runs parity and a small benchmark smoke
  test on pull requests that touch the GPU path.

## Boundaries

### Always

- Keep `auto` as the default backend and the CPU backend as the golden
  reference and exact override.
- Keep all three backends selectable at runtime: `cpu` always works, `gpu` is
  explicit, and `auto` selects GPU only when it predicts a win.
- Enable the GPU feature in official artifacts on platforms that can load a
  driver; keep musl/wasm CPU-only.
- Measure before and after; record kept and reverted experiments.
- Keep scene hashes, manifests, mesh statistics, and CPU goldens unchanged.
- Clean temporary spill files on success and normal error paths.
- Report the backend actually used.
- Document GPU output as vendor-dependent and non-exact.

### Ask first

- Changing the default backend or the meaning of `auto`.
- Changing the public `RenderOptions` or service protocol.
- Adding a required dependency or a new GPU driver requirement.
- Changing the supported platform or release matrix.
- Changing parity tolerances, bucket count, weighting, or the WBOIT strategy.
- Raising the GPU memory or VRAM budget.
- Making GPU streaming the default for large scenes.

### Never

- Remove the `cpu` override or bypass the `auto` policy.
- Weaken, skip, or delete CPU golden tests to accommodate GPU output.
- Use a software adapter as a silent GPU fallback.
- Commit private `.litematic` files, resource packs, or client jars.
- Ship a static musl GPU artifact that cannot load a Vulkan loader.
- Fall back to CPU after partial GPU output without discarding it.
- Claim byte-exact GPU output.

## Success Criteria

- G0-G6 are implemented and reviewed; the CPU gate remains green.
- Reference performance targets on R1/R2:
  - `tile_bench`: GPU at least 1.4x CPU 12t at 1024 and 2048 square.
  - `000002`: GPU total at most 0.9x CPU 12t (about 203 ms) and at most 0.65x
    CPU 2t (about 250 ms).
  - `000001` color: GPU streaming total at most 10 s and peak at most 2 GB.
  - `000001` client jar: GPU streaming total at most 15 s and peak at most 3 GB.
  - two-view `000002`: GPU at most 1.3x single-view GPU wall time.
- GPU parity is at most a channel delta of 2 and 0.1% differing pixels; WBOIT
  translucent differences are covered by the same tolerance.
- The CLI and service default to `auto`; `auto` does not choose GPU when the
  CPU path is expected to be faster on R1, and does choose GPU on R2 with
  `--threads 2` when the policy predicts a win.
- The same source builds and runs on Windows x64, macOS arm64/x64, and Linux
  x86_64 GNU with the GPU feature; musl and wasm stay CPU-only.
- Every GPU allocation stays inside the 60% adapter-VRAM working budget.
- CPU stream baseline (`000001` about 433 MB / 11.1 s) does not regress.
- `docs/performance.md` contains the measured after table and exact commands.
- If a target cannot be met, the spec status becomes "targets not met"; `auto`
  avoids the GPU on that hardware, explicit `gpu` remains available, and the
  result is recorded instead of hidden.

## Resolved Questions

The 2026-11-05 review resolved every open question:

1. GPU/WBOIT parity tolerance is a maximum channel delta of 2 and 0.1%
   differing pixels.
2. Scenes outside the f32-safe span fall back to the CPU.
3. `auto` is the default backend.
4. GPU multi-view is required in v1.
5. GPU working memory is 60% of adapter VRAM.
6. One engine, one acceptance standard. The release matrix only samples
   hardware where the same suite runs.
7. Bucket count, WBOIT weighting, and the per-bucket record cap are calibrated
   by the reference benchmarks as part of G5.
8. `serve` supports GPU streaming in v1.
9. Static musl and wasm are accepted CPU-only deployments.
