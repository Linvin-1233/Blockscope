# Implementation Plan: Phase 3 — CPU PNG Renderer

**Status:** Phase 3A implemented; performance/golden corpus deferred.  
**Scope:** Render the Phase 2 mesh to a deterministic RGBA PNG with an
orthographic camera, CPU tile rasterizer, two lighting profiles, transparent or
solid backgrounds, and a `renderd render` CLI.

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
- One view per render in Phase 3A; multi-view output is additive later.
- PNG output only; GIF and multi-view composition are out of scope.

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

## Phase 3A Scope

- `litematic-render` crate.
- `RenderOptions`, `Background`, `LightingProfile`, `RenderError`.
- Orthographic `Camera` with fit and padding.
- Tile rasterizer with depth test, backface culling, cutout, translucent sort.
- `simple-ao-v1` and `flat-v1`.
- PNG encoder.
- `renderd render` CLI with width/height/yaw/elevation/background/lighting.
- Unit and CLI tests.

## Phase 3B Deferred

- Multi-view output and composited contact sheets.
- Anti-aliasing / supersampling.
- Texture sampling and resource-pack materials.
- Shadows, full light propagation, and emissive bloom.
- Parallel tile workers / SIMD.
- Golden PNG corpus and performance benchmarks.

## Task Index

| ID | Task | Depends on | Size |
|---|---|---|---|
| P3-1 | AO vertex attribute | Phase 2 | M |
| P3-2 | Render options and lighting profiles | P3-1 | S |
| P3-3 | Orthographic camera and fit | P3-2 | M |
| P3-4 | Tile rasterizer and compositing | P3-3 | L |
| P3-5 | PNG encoder and `renderd render` CLI | P3-4 | M |
| P3-6 | Renderer tests and documentation | P3-4, P3-5 | M |
| P3-7 | Golden PNG and performance corpus (deferred) | P3-6 | M |

Detailed acceptance criteria live in `tasks/phase-3-todo.md`.

## Checkpoints

### Checkpoint 3-1: Camera and triangle build

- [ ] Camera fit covers bounds and padding.
- [ ] Backface culling keeps camera-facing quads.
- [ ] AO and lighting attributes are applied.

### Checkpoint 3-2: Raster and PNG

- [ ] Transparent and solid backgrounds work.
- [ ] Opaque, cutout, and translucent layers render.
- [ ] PNG output is deterministic.
- [ ] `renderd render` writes a valid PNG.

### Checkpoint 3-3: Phase 3A acceptance

- [ ] `cargo fmt`, clippy, and tests pass on Windows and Linux.
- [ ] No Minecraft assets are committed.
- [ ] Renderer documentation is linked from README.
- [ ] Human review before Phase 4.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Orthographic fit clips geometry | Missing edges | Bounds padding and camera span tests |
| Translucent sorting is approximate | Local artifacts | Far-to-near tile sorting, no depth writes |
| AO cost on large scenes | Slow mesh build | Per-vertex boundary sampling; later cache/parallelize |
| No anti-aliasing | Jagged edges | Document limitation; add SSAA later |
| f32 color precision | Banding | Straight f32 tile buffers, u8 at final PNG |
| Large output memory | OOM | Pixel limit and tile buffers |

## Open Questions

1. Should the default output be one 45° view or two opposite views?
2. Is a transparent PNG acceptable for QQ delivery, or should the default be a
   light solid background?
3. Should Phase 3B prioritize anti-aliasing or performance before Phase 4?
