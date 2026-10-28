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
  uv:       [i16; 2],   // Q12 fixed point
}

Quad {
  vertices: [Vertex; 4],
  normal:   [f32; 3],
  color:    [u8; 4],    // straight alpha
  ao:       [u8; 4],    // 2-bit levels
  texture:  u16,
  layer:    opaque | cutout | translucent,
  shade:    bool,
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
4. If the neighbor is the same canonical block state, including partial
   shapes, cull the face. This removes internal faces between adjacent glass,
   water, ice, leaves, and matching partial shapes. The check is state-based,
   not a per-face coplanarity test.
5. Otherwise keep the face. Different translucent materials keep their
   interface faces.

Partial models use per-face cull directions and voxelized model occupancy.

## Color Catalog

`ColorBlockCatalog` resolves a canonical `BlockStateKey`:

- exact embedded palette entry;
- family heuristic (dye colors, wood types, ores, plants, glass, stairs/slab
  suffixes);
- material inheritance for decorated variants, so `quartz_pillar`,
  `chiseled_quartz_block`, `smooth_quartz_slab`, and `cut_sandstone` take the
  plain material entry's color;
- magenta fallback with `known = false`.

The fallback is intentional: unknown blocks stay visible and are counted in
mesh statistics. Air and invisible blocks (`barrier`, `light`,
`structure_void`) resolve to an empty non-occluding model.

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

For CLI invocation and mesh export options (`--obj`, `--ply`, `--json`), see
[CLI Reference - mesh](./user/en/cli.md).

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
| water / lava | `level=0..15` | vanilla corner-averaged surface; shared fluid faces are hidden |
| waterlogged blocks | `waterlogged=true` | keeps its own model and adds a water layer |
| redstone wire | `power=0..15` | flat dust plane, tinted from dark red to bright red |
| chest / trapped / ender | none | 14x14x14 body plus a lock box reaching z=16 |
| barrel / shulker box | none | 14x16x14 box |
| sign / banner | `facing` ignored | 2x16x16 panel |
| skull / head | none | 8x8x8 box |
| lectern | none | base plus top box |
| beacon | none | full cube, translucent layer |
| conduit | none | 8x8x8 box |

These are deliberately approximate. They preserve the silhouette and material
color; they do not try to reproduce every vanilla micro-detail. Block-entity
text, item contents, banner patterns, and skull skins are not rendered yet.

### Fluid surfaces

Water and lava are not emitted as one flat box per block. The mesher reads the
eight horizontal columns around a fluid cell plus the cell above, then computes
the four top corner heights with vanilla's weighted averaging:

- source and falling levels rest at `8/9` of a block, levels 1-7 at
  `(8-level)/9`;
- a column with the same fluid above becomes a full block and hides its top;
- source-like columns get weight 10 in the corner average, lower flowing
  columns get weight 1, and solid neighbours are excluded;
- faces shared with the same fluid are hidden, including faces between
  different levels.

This makes a flowing channel a continuous slope instead of a staircase and
removes the internal translucent faces that previously caused seams.

Waterlogged blocks keep their own stairs, slabs, walls, leaves, or plant model
and gain the same shaped water layer inside the cell. Waterlogged full opaque
blocks are skipped because the water would be invisible. Kelp, seagrass, and
bubble columns are treated as water-filled; bubble columns use the fluid
surface directly instead of a separate block model.

Redstone wire without a resource pack uses a built-in flat plane at the vanilla
dust height. The color catalog tints it with the same `power=0..15` gradient as
the textured path, so low-power dust is visibly darker. The gradient is
square-root shaped so power 0 and 1 stay clearly distinct on thin dust lines.

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

The second rule removes shared boundary faces of adjacent slabs, stairs,
fences, panes, and glass. It is state-based: stacked partial models can lose an
exposed face when their geometry does not meet at the same plane. Different
partial states keep their interface faces.

Fluids use a separate rule: any face shared with the same fluid is hidden,
even when the two cells have different `level` values. Their surfaces are
merged by the corner averaging described above instead.

## Resource Packs

The `BlockCatalog` trait is implemented by both the original color catalog and
the resource-pack catalog (`litematic-material`). Pack models replace the
built-in shape when their blockstate/model resolves to elements; otherwise the
Phase 2 shape, the model `particle` texture, or the color palette is used.

Pack quads carry explicit UVs, a texture id, a tint, an optional per-quad layer,
and a shade flag. Texture coordinates follow Minecraft's face convention; chest
entity textures use the standard box unwrap. See `docs/textures.md` for the full
asset and fallback contract.

Block-entity text/icons, entity models beyond the built-in chest, and dynamic
item contents remain future work.
