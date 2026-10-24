# Color-Block Mesh Geometry

Phase 2A turns the Phase 1 `Scene` into a deterministic mesh. It uses an
original color palette and built-in full-cube geometry. No Minecraft textures,
models, or resource packs are required.

## Pipeline

```text
Scene
  -> SceneBlockView
  -> ColorBlockCatalog
  -> MeshBuilder
  -> Mesh (16^3 pages)
  -> OBJ / PLY / statistics
```

## Coordinate System

- Scene world coordinates are signed `i64` block coordinates.
- A mesh page covers a 16x16x16 block region.
- `MeshPage.origin` is the page's world block origin as `i32`.
- Vertex positions are local to the page and stay in `0.0..=16.0`.
- This keeps `f32` precision stable even when the scene is far from the origin.
- Page assignment uses `div_euclid`, so negative coordinates work correctly.

## Mesh Contract

```text
Vertex {
  position: [f32; 3],   // page-local
  normal:   [f32; 3],
  color:    [u8; 4],    // straight alpha
  uv:       [f32; 2],   // [0,0] for color blocks
}

Quad {
  vertices: [Vertex; 4],
  layer:    opaque | cutout | translucent,
}

Mesh {
  pages: [MeshPage],
  statistics: MeshStatistics,
}
```

Vertices are ordered counter-clockwise when viewed from outside the block. The
quad normal must agree with the winding; tests assert this with a cross product.

## Face Culling

For a face that lies on the block boundary:

1. If the neighbor position is outside the scene, keep the face.
2. If the neighbor is air, keep the face.
3. If the neighbor model fully occludes (full cube + opaque), cull the face.
4. If the neighbor is the same full-cube state, cull the face. This removes
   internal faces between adjacent glass, water, ice, or leaves.
5. Otherwise keep the face. Different translucent materials keep their
   interface faces.

Partial models in Phase 2B will use per-face cull directions and voxelized
model occupancy.

## Color Catalog

`ColorBlockCatalog` resolves a canonical `BlockStateKey`:

- exact embedded palette entry;
- family heuristic (dye colors, wood types, ores, plants, glass, stairs/slab
  suffixes);
- magenta fallback with `known = false`.

The fallback is intentional: unknown blocks stay visible and are counted in
mesh statistics. Air resolves to an empty non-occluding model.

Default layers:

- `translucent`: water, glass, stained glass, ice, slime, honey;
- `cutout`: leaves, saplings, flowers, plants, rails, torches, ladders, panes,
  iron bars, chains, crops;
- `opaque`: everything else.

The palette is original approximate data under
`crates/mesh/data/color-palette.json`. It is not copied from Minecraft assets.

## Limits

`MeshLimits` defaults:

| Limit | Default |
|---|---|
| pages | 65,536 |
| quads | 5,000,000 |
| vertices | 20,000,000 |

Exceeding a limit returns `MESH_LIMIT_EXCEEDED` or `PAGE_LIMIT_EXCEEDED` and
exits with code 2 in the CLI.

## Exports

- **OBJ**: page-origin-adjusted vertices, normals, and quad faces.
- **PLY**: ASCII vertices with RGBA colors and quad faces. Useful for MeshLab,
  Blender, or other viewers.

Both exporters are deterministic: the same mesh produces identical bytes.

## CLI

```powershell
cargo run -p renderd-cli -- mesh path\to\model.litematic
cargo run -p renderd-cli -- mesh path\to\model.litematic --json
cargo run -p renderd-cli -- mesh path\to\model.litematic --obj out.obj
cargo run -p renderd-cli -- mesh path\to\model.litematic --ply out.ply
```

`--json` prints `MeshStatistics` with pages, quads, vertices, visited/rendered
blocks, unknown blocks, and layer counts.

## Partial Shapes

Phase 2B adds box-based shapes. Block state properties are read from the
canonical key:

| Family | Properties | Approximation |
|---|---|---|
| slab | `type=bottom/top/double` | exact box |
| stairs | `facing`, `half` | straight shape; corner `shape` values use straight geometry |
| carpet | none | 1/16 layer |
| snow | `layers=1..8` | 2/16 per layer |
| trapdoor | `half`, `open`, `facing` | closed panel or open side panel |
| door | `facing`, `open` | closed/open panel; hinge is ignored |
| fence | `north/east/south/west` | post + two bars per connection |
| fence gate | `facing`, `open` | post + gate panel |
| wall | `up`, connection sides | post + 8-wide arms |
| pane / iron bars | connection sides | center post + arms; isolated pane becomes a thin plane |
| button | `face`, `facing` | small floor/ceiling/wall box |
| pressure plate | none | 14x1x14 plate |
| torch | floor/wall name | small box, wall offset by `facing` |
| ladder | `facing` | thin side panel |
| rail | none | 16x1x16 plate |
| cactus | none | 14x16x14 box |
| cross plants | none | four quads (two crossed planes, double-sided) |
| water / lava | `level=0..15` | height `16-level`; level 0 is a full cube |
| chest / trapped / ender | none | 14x14x14 box |
| barrel / shulker box | none | 14x16x14 box |
| sign / banner | `facing` ignored | 2x16x16 panel |
| skull / head | none | 8x8x8 box |
| lectern | none | base plus top box |
| beacon | none | full cube, translucent layer |
| conduit | none | 8x8x8 box |

These are deliberately approximate. They preserve the silhouette and material
color; they do not try to reproduce every vanilla micro-detail. Block-entity
text, item contents, banner patterns, and skull skins are not rendered yet.

## Voxel Mesher

Model boxes are voxelized into a cached 16^3 occupancy grid. For each face
plane, exposed 1x1 cells are greedily merged into the largest rectangles. This:

- removes interior faces between overlapping or touching boxes;
- merges coplanar faces across box boundaries;
- emits `cullface` only for faces on the block boundary;
- caches the resulting model per canonical block state.

Full cubes bypass the voxel path and reuse the six-quad fast model. A model
whose boxes fill the complete 16^3 volume also collapses to the full cube.

## Same-State Culling

A boundary face is culled when the neighbor is:

- a full opaque cube;
- the same canonical block state, including partial shapes.

The second rule removes the touching sides of adjacent slabs, stairs, fences,
panes, glass, and water. Different partial states keep their interface faces.

## Resource Packs Deferred

- Resource-pack model/texture loading is not implemented.
- The `BlockCatalog` trait and `Mesh` contract are designed so a future
  resource-pack catalog can replace the color catalog without changing the CLI
  or scene format.
- Block-entity text/icons and dynamic item contents remain future work.
