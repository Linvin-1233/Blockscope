# Implementation Plan: Phase 2 — Color-Block Geometry

**Status:** Phase 2A/2B/2C implemented; resource packs deferred.  
**Scope:** Build a deterministic mesh from the Phase 1 Scene IR using an
original color-block catalog. No Minecraft assets, textures, or rasterization.

## Overview

Phase 1 produces a validated `Scene` with packed block states, statistics, and
diagnostics. Phase 2 converts that scene into a renderer-ready mesh:

```text
Scene
  -> SceneBlockView (region overlap resolution + neighbor lookup)
  -> BlockCatalog (block state -> color, layer, model)
  -> MeshBuilder (face culling + page assignment)
  -> Mesh (pages of quads)
  -> OBJ/PLY/stats for inspection
```

Phase 2 does not produce PNGs. The CPU rasterizer and lighting profiles remain
in Phase 3.

## Confirmed Decisions

- Default rendering uses an original color palette and built-in geometry.
- No Minecraft textures, models, or fonts are committed.
- Resource packs are optional and user-provided; the resource-pack loader is
  deferred until after the color-block path is stable.
- Production targets Linux and Windows.
- The mesh format is versioned and deterministic.

## Architecture Decisions

### ADR-201: Separate mesh crate

Add `crates/mesh` (`litematic-mesh`) depending on `litematic-scene`. The scene
crate stays data-only; geometry, catalog, and export logic live in the mesh
crate.

### ADR-202: Chunk-aligned mesh pages

Meshes are split into 16x16x16 pages keyed by page origin. Vertex positions are
local to the page, keeping f32 precision high for large worlds. Page order is
deterministic.

### ADR-203: World view resolves overlaps

`SceneBlockView` indexes regions per page and resolves every world position by
later region order. It provides both the winning state and its region index.
Meshing uses it for overlap filtering and neighbor culling.

### ADR-204: Full cubes first, voxelized boxes later

Phase 2A supports full cubes only. Phase 2B adds box-based partial shapes and a
16^3 voxelization pass with greedy face merging, cached per block state.

### ADR-205: Explicit face culling rules

A face is culled when its neighbor is a full opaque cube, or when the neighbor
is the same full-cube state. Different translucent full cubes keep their
interface faces. Partial models are added in Phase 2B.

### ADR-206: Unknown blocks are visible

Unknown block states use a magenta fallback cube, are counted in mesh
statistics, and produce a diagnostic count. They are never silently omitted.

### ADR-207: No textures in the mesh contract

Vertices carry position, normal, RGBA color, and UV. Color-block meshes use
UV `[0,0]` and a flat color. Resource-pack geometry can fill UVs later without
changing the mesh container.

## Phase 2A Scope

- New `litematic-mesh` crate.
- `Rgba`, `RenderLayer`, `BlockModel`, `ModelQuad`, `FaceDirection`.
- Embedded original color palette for common vanilla blocks + family heuristics.
- Full-cube model, opaque/cutout/translucent classification.
- `SceneBlockView` with page index and later-region-wins resolution.
- Mesh builder with face culling, page assignment, limits, statistics.
- OBJ and PLY exporters.
- `renderd mesh <file>` with `--json`, `--obj`, `--ply`.
- Unit tests, CLI tests, and `docs/mesh-geometry.md`.

## Phase 2B Scope

- Box-based block shapes: slab, stairs, fence, wall, pane, door, trapdoor,
  carpet, snow layer, button, pressure plate, torch, ladder, rail.
- Voxelized 16^3 model occupancy + greedy face merging, cached per state.
- Cross plants and other non-box models via explicit quads.
- Simple fluid height/flow approximation.
- Shape-family tests and golden mesh statistics.

## Phase 2C Scope

- Static block-entity boxes: chest, sign, banner, skull, lectern, beacon,
  conduit, copper golem statue.
- Block-entity text/icons where the data is available.
- Optional resource-pack catalog behind a separate feature or module.

## Task Index

| ID | Task | Depends on | Size |
|---|---|---|---|
| P2-1 | Mesh types and error/limits | None | M |
| P2-2 | Color palette and catalog | P2-1 | M |
| P2-3 | Scene block view | P2-1 | M |
| P2-4 | Full-cube mesh builder | P2-2, P2-3 | M |
| P2-5 | OBJ/PLY exporters | P2-4 | S |
| P2-6 | `renderd mesh` CLI | P2-4, P2-5 | M |
| P2-7 | Phase 2A tests and docs | P2-4, P2-6 | M |
| P2-8 | Box shapes + voxel mesher | P2-4 | L |
| P2-9 | Shape families and properties | P2-8 | L |
| P2-10 | Fluids and static block entities | P2-9 | L |

Detailed acceptance criteria live in `tasks/phase-2-todo.md`.

## Checkpoints

### Checkpoint 2A-1: Catalog and view

- [ ] Color catalog resolves exact, family, and unknown states.
- [ ] Scene block view resolves overlaps and page neighbors.
- [ ] Unit tests pass on Windows and Linux.

### Checkpoint 2A-2: Mesh and export

- [ ] Single and adjacent cubes produce expected face counts.
- [ ] Stone/glass and glass/glass culling rules pass.
- [ ] Page boundaries and limits are covered.
- [ ] OBJ/PLY output is deterministic.
- [ ] `renderd mesh` runs on a synthetic fixture.

### Checkpoint 2A-3: Phase 2A acceptance

- [ ] `cargo fmt --check`, `clippy -D warnings`, and tests pass.
- [ ] No Minecraft assets are committed.
- [ ] Mesh statistics are documented and linked from README.
- [ ] Human review before starting Phase 2B.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Large scenes exceed mesh memory | OOM | Page limits, quad/vertex limits, sparse page map |
| Overlap order differs from renderer | Wrong geometry | Single `SceneBlockView` used by builder and culling |
| Translucent culling looks wrong | Visual artifacts | Explicit same-state + opaque rules; golden cases |
| Color table looks arbitrary | Product mismatch | Family heuristics + documented fallback; user can tune later |
| Partial shapes balloon scope | Delay | Split 2A/2B/2C; 2A ships full cubes first |
| f32 precision at world scale | Geometry cracks | Page-local f32 positions; page origin in i32 |

## Open Questions

1. Should Phase 2B include all listed shape families at once, or split fences,
   walls, and panes into a follow-up?
2. Should unknown blocks use magenta, a hash color, or a checker pattern?
   Current plan uses magenta for visibility.
3. Is OBJ + PLY enough for manual review, or is a simple wireframe preview
   needed before Phase 3?
