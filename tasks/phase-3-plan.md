# Implementation Plan: Phase 3 — CPU PNG Renderer

**Status:** Phase 3A-3C implemented; Phase 3D deferred.  
**Scope:** Render the Phase 2 mesh to a deterministic RGBA PNG with an
orthographic camera, CPU tile rasterizer, two lighting profiles, transparent or
solid backgrounds, supersampling, multi-view output, and a `blockscope render` /
`blockscope bench` CLI.

## Overview

```text
Scene + Mesh
  -> Camera (orthographic fit)
  -> Triangle build (page origins, shading, AO)
  -> Tile rasterizer (depth, backface culling, alpha)
  -> RGBA buffer
  -> PNG encoder / CLI
```

No GPU, Minecraft client, textures, or resource packs are required.

## Confirmed Decisions

- CPU-only renderer, no GPU dependency.
- Orthographic camera, default `isometric_preview_v1` profile.
- Default view: yaw 45°, elevation 30°, 1024x1024, transparent background.
- Lighting profiles: `flat-v1` and `simple-ao-v1`.
- Multi-view output writes one PNG per yaw; contact-sheet composition is Phase 3C.
- PNG output only; GIF is out of scope.
- Supersampling is the Phase 3B anti-aliasing path; no temporal AA.

## Architecture Decisions

### ADR-301: Orthographic camera

Use a yaw/elevation orthographic camera that fits the scene bounds with padding.
Project world positions to screen x/y and camera depth; smaller depth is closer.

### ADR-302: Page-local mesh vertices

The renderer adds `MeshPage.origin` to each page-local vertex before camera
projection. This preserves f32 precision in the mesh and f64 precision in the
camera.

### ADR-303: Tile rasterizer

Rasterize in configurable square tiles. Each tile owns its color and depth
buffers; opaque/cutout triangles run first, translucent triangles are sorted
far-to-near and blended without depth writes.

### ADR-304: AO in the mesh builder

Compute per-vertex ambient occlusion during mesh construction, using the scene
block view and catalog occlusion rules. Store it in `Vertex.ao` so the renderer
does not need a second scene lookup pass.

### ADR-305: Fixed lighting profiles

`flat-v1` uses uniform brightness. `simple-ao-v1` multiplies a fixed directional
face shade by vertex AO. No full Minecraft light propagation or shadows.

### ADR-306: Straight-alpha compositing

Store straight RGBA in the tile buffers and composite translucent surfaces with
source-over alpha. Transparent backgrounds preserve alpha; solid backgrounds are
filled before rasterization.

### ADR-307: Triangle binning

Bucket triangles by the tiles their screen bounding boxes cover before
rasterization. Each tile then visits only its candidates, which keeps large
meshes from multiplying tile count by triangle count.

### ADR-308: Scoped tile workers

Use `std::thread::scope` with an atomic tile cursor instead of a dependency on
rayon. Each tile owns its color and depth buffers. Results merge in tile order so
output and statistics are independent of thread count and scheduling.

### ADR-309: Supersampling

Render at an integer multiple of the output size and box-filter down in
premultiplied-alpha space. This keeps transparent edges from darkening and avoids
inventing a second anti-aliasing contract.

### ADR-310: Golden PNG hashes

Commit small synthetic golden PNGs and compare bytes exactly. This catches
rasterizer, lighting, alpha, and encoder drift without committing Minecraft
assets.

## Phase 3A Scope

- `litematic-render` crate.
- `RenderOptions`, `Background`, `LightingProfile`, `RenderError`.
- Orthographic `Camera` with fit and padding.
- Tile rasterizer with depth test, backface culling, cutout, translucent sort.
- `simple-ao-v1` and `flat-v1`.
- PNG encoder.
- `blockscope render` CLI with width/height/yaw/elevation/background/lighting.
- Unit and CLI tests.

## Phase 3B Implemented

- Multi-view output as separate PNGs.
- Supersampling 1-4 with premultiplied-alpha downsampling.
- Parallel tile workers with deterministic merge.
- Triangle binning per tile.
- Golden PNG corpus for cube, partial shapes, water, negative coordinates, and AO.
- `blockscope bench` with per-stage timings and Linux peak RSS.
- Synthetic rasterizer scaling example (`tile_bench`).

## Phase 3C Implemented

Resource-pack materials and models are implemented in a separate slice:

- `tasks/phase-3c-textures-spec.md`
- `tasks/phase-3c-textures-plan.md`
- `tasks/phase-3c-textures-todo.md`
- `docs/textures.md`

## Phase 3D Deferred

- Contact-sheet composition for multiple views.
- Shadows, full light propagation, and emissive bloom.
- SIMD rasterization and mesh-build parallelism.
- Golden corpus for large real structures and deployment-specific thresholds.
- CI performance budget based on real Litematic workloads.
- Entity models beyond the built-in chest, and animation playback.

## Task Index

| ID | Task | Depends on | Size |
|---|---|---|---|
| P3-1 | AO vertex attribute | Phase 2 | M |
| P3-2 | Render options and lighting profiles | P3-1 | S |
| P3-3 | Orthographic camera and fit | P3-2 | M |
| P3-4 | Tile rasterizer and compositing | P3-3 | L |
| P3-5 | PNG encoder and `blockscope render` CLI | P3-4 | M |
| P3-6 | Renderer tests and documentation | P3-4, P3-5 | M |
| P3-7 | Golden PNG and performance corpus | P3-6 | M |
| P3-8 | Parallel tiles and triangle binning | P3-4 | M |
| P3-9 | Supersampling and multi-view CLI | P3-5 | M |

Detailed acceptance criteria live in `tasks/phase-3-todo.md`.

## Checkpoints

### Checkpoint 3-1: Camera and triangle build

- [x] Camera fit covers bounds and padding.
- [x] Backface culling keeps camera-facing quads.
- [x] AO and lighting attributes are applied.

### Checkpoint 3-2: Raster and PNG

- [x] Transparent and solid backgrounds work.
- [x] Opaque, cutout, and translucent layers render.
- [x] PNG output is deterministic.
- [x] `blockscope render` writes a valid PNG.

### Checkpoint 3-3: Phase 3A/3B acceptance

- [x] `cargo fmt`, clippy, and tests pass on Windows and Linux.
- [x] No Minecraft assets are committed.
- [x] Renderer documentation is linked from README.
- [x] Golden PNGs and benchmark commands exist.
- [ ] Human review before Phase 4.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Orthographic fit clips geometry | Missing edges | Bounds padding and camera span tests |
| Translucent sorting is approximate | Local artifacts | Far-to-near tile sorting, no depth writes |
| AO cost on large scenes | Slow mesh build | Per-vertex boundary sampling; later cache/parallelize |
| Fixed supersampling only | Jagged thin geometry at factor 1 | Default factor 2; SSAA up to 4 |
| f32 color precision | Banding | Straight f32 tile buffers, u8 at final PNG |
| Large output memory | OOM | Pixel limit, internal pixel limit, per-tile buffers |
| Tile scheduling changes output | Non-deterministic PNGs | Per-tile buffers and index-ordered merge |

## Open Questions

1. Is a transparent PNG acceptable for QQ delivery, or should the default be a
   light solid background?
2. Should multi-view output compose a contact sheet for QQ, or keep separate
   images?
3. Which real structure sizes and latency budget define the Phase 3C performance
   gate?
