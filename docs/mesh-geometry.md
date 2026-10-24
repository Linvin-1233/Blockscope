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

## Phase 2B/2C Deferred

- Partial shapes: slab, stairs, fence, wall, pane, door, trapdoor, carpet,
  snow, button, plate, torch, ladder, rail.
- Voxelized box models and greedy face merging.
- Fluids with height/flow.
- Static block-entity boxes.
- Resource-pack model/texture loading.

The `BlockCatalog` trait and `Mesh` contract are designed so those additions do
not change the CLI or scene format.
