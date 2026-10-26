# CPU Rendering and PNG Output

Phase 3 renders the Phase 2 mesh to an RGBA PNG on the CPU. It uses an
orthographic camera and a tile rasterizer. No GPU or Minecraft client is
required.

Textures are optional: without a resource pack the renderer uses flat block
colors; with one it samples the pack textures. See `docs/textures.md`.

## Pipeline

```text
Scene + Mesh
  -> Camera (orthographic fit, internal supersampled size)
  -> triangle build (page origin, lighting, AO, UV)
  -> tile rasterizer (depth, culling, alpha, texture sampling)
  -> RGBA8 buffer (supersampled)
  -> box-filter downsample
  -> PNG
```

## Camera

- Orthographic projection.
- Default preset: `isometric_preview_v1`, yaw 45°, elevation 30°.
- Yaw follows the reference Litematic viewer: yaw 0 looks from +Z (south),
  yaw 45 looks from the south-west, and yaw 225 looks from the north-east.
  The default view therefore shows the south and west faces of the structure.
- The camera looks at the center of the scene bounds.
- The scene is fitted with padding; default padding is 16 px.
- World positions are `MeshPage.origin + page-local vertex`.
- Screen Y is flipped so image coordinates follow PNG top-left origin.
- Smaller camera depth means closer to the camera.

The camera basis:

```text
forward = normalize([cos(elev)*sin(yaw), -sin(elev), -cos(elev)*cos(yaw)])
right   = normalize(cross(forward, world_up))
up      = cross(right, forward)
```

## Mesh Inputs

The mesh builder splits the scene into 16x16x16 pages and can build pages in
parallel (`--threads`, `0` uses all cores). Pages are merged in origin order, so
output is byte-identical to a single-threaded build. Both the asset-backed and
color-block paths resolve states into an immutable snapshot before workers
start, because the mutable catalogs are not `Sync`.

The rasterizer consumes the Phase 2 mesh:

- page-local positions,
- face normals,
- straight RGBA block colors or texture tint,
- texture id and per-vertex UVs,
- render layer and shade flag,
- per-vertex ambient occlusion.

Backface culling uses the camera forward vector:

```text
dot(quad_normal, camera_forward) >= 0 -> cull
```

Cross-plant quads are emitted as double-sided pairs, so one side survives the
cull.

## Rasterizer

- The image is split into square tiles; default 64x64.
- Each tile owns an RGBA f32 color buffer and an f64 depth buffer.
- Triangles are rasterized with edge functions and barycentric weights.
- Depth is interpolated linearly, which is correct for orthographic projection.
- Opaque and cutout triangles run first and write depth.
- Cutout fragments with alpha `< 0.5` are discarded.
- Textured fragments sample the texture, multiply by tint, face shade, and AO,
  then apply the same cutout/translucent rules.
- Translucent triangles are sorted far-to-near per tile and blended without
  writing depth.
- Solid backgrounds are filled before rasterization; transparent backgrounds
  start at `[0, 0, 0, 0]`.
- Final RGBA8 conversion clamps and rounds; fully transparent pixels are written
  as zero RGB.

### Row spans

Each edge function is affine in screen x on a fixed scanline, so the rasterizer
solves the three zero crossings to bound the row, then walks the two endpoints
in with the exact per-pixel edge test. A row only visits the pixels inside the
triangle instead of the whole bounding box. Barycentric weights, depth, and
color interpolation still use the same expressions in the same order, so
accepted fragments are bit-identical while most rejected pixels are never
visited. `pixelsTested` counts the fragments that remain after this clipping.

### Block entity NBT

The renderer never reads block entity contents, so `mesh`, `bench`, and `render`
skip the `TileEntities` payload while decoding. `inspect`, `validate`, and
`stats` keep it by default; pass `--ignore-block-entity-nbt` to skip it there
too. Skipping does not allocate the payload, and if the NBT stream ends inside
`TileEntities`, the block data parsed before it is kept and the missing block
entities become an empty list. This makes container-heavy schematics renderable
without decompressing gigabytes of book text.

### Compact mesh attributes

The mesh stores fixed-point attributes instead of full floats. Texture
coordinates are Q12 (`i16`); the builder emits UVs on a 1/16 or 1/32 grid for the
tested material sets, so the round trip is exact. Ambient occlusion is one of
four levels. Quads also share their color, normal, texture, and shade, which the
builder already emits once per quad. Reconstruction uses the same float
expressions as before, so rendered output is bit-identical.

| Structure | Before | After |
|---|---|---|
| `Vertex` | 36 B | 16 B |
| `Quad` | 148 B | 88 B |
| projected quad table entry | 200 B | 128 B |
| translucent triangle | 160 B | 112 B |

### Geometry table

Opaque and cutout geometry is projected once per quad and stored in a compact
quad table (about 128 bytes per quad: four screen points, one color and shade
pair, four AO levels, four Q12 UVs, texture id, layer). Triangles are formed
only while a tile rasterizes a quad, so the render table is roughly half the
size of storing two full triangles per quad. Translucent triangles are kept
separately because they need a per-tile depth sort. Output stays byte-identical.

Measured on the 370x370 fixture at 1024x1024 and supersample 2: peak memory
dropped from 19.6 GB to 6.3 GB and render time from 13.5 s to 4.0 s; the PNG
hash did not change.

### Page streaming

When the projected quad table would be larger than the persistent tile
buffers, the rasterizer streams mesh pages instead. It projects a chunk of
pages, bins them to the tiles they touch, renders those tiles into buffers that
stay resident for the whole image, and drops the chunk. Only one chunk of
geometry is alive at a time. Output is byte-identical to the table path.

Chunks are sized by projected quad count rather than page count, because page
sizes differ by orders of magnitude between meshes; a page-count cap would spawn
a worker scope per handful of fine-grained pages. Within a chunk the same
scoped workers project contiguous page slices, bin the result, and rasterize the
touched tiles, so projection no longer serializes the main thread while the
workers wait. Slice and part order follow page order, which keeps the draw order
and therefore the output deterministic.

Measured on the test host at 1024x1024 and supersample 2:

| Scene | Peak working set | Wall | Render |
|---|---|---|---|
| `000002` exact | 6.3 GB | 30.4 s | 4.0 s |
| `000001` exact | 18.9 GB | 36.6 s | 10.1 s |

Both used to need the full per-quad table (13.5 GB and roughly 45 GB
respectively). A small-scene equality test forces streaming and compares RGBA
and statistics against the table path.

### Mesh page cache

Culling and ambient occlusion used to resolve the same block and its neighbours
through the region index and the material catalog for every model quad. The
builder now resolves a 16x16x16 page plus its one-block border once, then serves
culling and AO from an array. Each region palette is also resolved to an
immutable material table up front, so the inner loop indexes by palette id
instead of hashing a block-state key and cloning a model `Arc`.

The grid also caches one occlusion bit per cell and converts culling and AO
offsets to flat index deltas, so the inner loop does not re-check page bounds or
chase the palette table. A full-cube block whose six neighbours all occlude is
skipped without visiting its model quads. Equal states are assigned one
canonical id across regions, so the existing same-state culling still works
across region and page boundaries. Region order and y/z/x block order are
unchanged; mesh statistics and PNG output stay byte-identical for dense scenes.

Page discovery scans the packed block stream once and indexes only pages that
contain a non-air block. Sparse structures therefore no longer scan empty pages
or count them against `max_pages`; a scene with no air keeps the bounding-box
fast path. `blocksVisited` now counts cells in indexed pages, so an all-air mesh
reports zero visited blocks.

Measured on the test host with the color-only catalog, 1024x1024, supersample 2,
12 threads:

| Scene | Before mesh | After mesh | Before/after render | Peak working set |
|---|---|---|---|---|
| `000001` exact, 77.6M quads | 17.3-20.4 s | 5.7-5.9 s | 4.6 s / 4.0 s | 7.16 GB / 7.16 GB |
| `000002` exact, 50.7M blocks | 11.7 s | 0.24-0.25 s | 0.21 s / 0.19 s | 471 MB / 497 MB |

The page cache costs about 53 KB per worker and does not change the mesh
representation, so peak memory is still dominated by the resident `Mesh`. The
PNG SHA-256 values for both exact fixtures and the `000001` LOD 4 render were
unchanged.

### Streaming mesh construction

`--stream-mesh` builds 32-page chunks, renders them into the resident tile
buffers, and drops the chunk before building the next one. The full `Mesh` is
never materialized. `--lod auto` is rejected because choosing a factor needs a
complete mesh; multiple views rebuild the mesh once per view.

Measured end to end on the test host (parse + mesh + render + PNG, 1024x1024,
supersample 2, 12 threads, color-only materials):

| Scene | Peak working set (default / streamed) | Wall (default / streamed) |
|---|---|---|
| `000001` exact | 7.16 GB / **425 MB** | 10.2 s / 10.3 s |
| `000002` exact | 497 MB / **182 MB** | 0.9 s / 1.25 s |

The streamed PNG SHA-256 values are identical for both fixtures.

### Triangle binning

Before rasterization, each triangle is inserted into every tile its screen-space
bounding box covers. A tile only visits its own candidate list instead of the
whole mesh. Bins preserve triangle order, so rendering stays deterministic.

### Parallel tiles

Tiles are independent. The rasterizer takes work from a shared atomic cursor and
spawns up to `threads` scoped workers; each worker owns its tile color and depth
buffers and returns a finished tile. Results are sorted by tile index before
assembly, so pixel output and statistics do not depend on scheduling order.

`threads = 0` chooses `available_parallelism()`. The default is 0.

### Supersampling

`supersample` renders at `width * factor` by `height * factor`, then box-filters
down to the requested output size. The filter works in premultiplied alpha so
transparent edges do not darken. Supported factors are 1-4; default is 2.
`max_supersampled_pixels` limits the internal buffer separately from the output
pixel limit.

## Lighting

Two profiles are available:

### `flat-v1`

All face orientations use the same brightness. AO is ignored. This is the
fastest and most uniform profile.

### `simple-ao-v1`

```text
shade = 0.55 + 0.45 * max(dot(normal, light_dir), 0)
color = block_color * shade * vertex_ao
```

`light_dir` is a fixed normalized direction `[1, 2, 1] / sqrt(6)`. Vertex AO is
computed in the mesh builder from the three neighboring cells at boundary
corners. Interior and cross quads default to `ao = 1.0`.

This is an approximation. It does not run Minecraft sky/block light propagation,
shadows, or biome tinting. It is stable and deterministic.

## Options

| Option | Default |
|---|---|
| width | 1024 |
| height | 1024 |
| yaw | 45° |
| elevation | 30° |
| background | transparent |
| lighting | simple-ao-v1 |
| tile size | 64 |
| padding | 16 px |
| supersample | 2 |
| threads | 0 (auto) |
| texture filter | nearest |
| max pixels | 16,777,216 |
| max supersampled pixels | 33,554,432 |

Validation rejects zero dimensions, elevation beyond ±89.9°, non-finite values,
padding that leaves no drawable area, and outputs above the pixel limit.

For CLI rendering flags, multi-view output naming, and options reference, see
[CLI Reference - render](./user/en/cli.md). Resource limits and
`--force-limits` behavior are detailed in [Limits and Security](./user/en/limits.md).

## Benchmark

```powershell
cargo run -p blockscope-cli --release -- bench path\to\model.litematic --iterations 5
cargo run -p blockscope-cli --release -- bench path\to\model.litematic --width 2048 --height 2048 --supersample 2 --json
```

`blockscope bench` parses and meshes once, then renders and PNG-encodes
`--iterations` times. It reports min/median/mean/max for render, PNG, and their
sum, plus mesh and render counters, buffer sizes, and peak RSS on Linux
(`null` on other platforms).

For rasterizer-only scaling, use the synthetic example:

```powershell
cargo run --release -p litematic-render --example tile_bench -- --iterations 5
cargo run --release -p litematic-render --example tile_bench -- --width 2048 --height 2048 --supersample 1
```

It prints single-thread, two-thread, and auto-thread timings.

Reference run on a 12-core Windows host (`tile_bench` synthetic scene, 96,882
quads, 1024x1024 output, supersample 2, median of 3): 1 thread 300 ms, 2 threads
174 ms, auto threads 81 ms. Treat these as a scaling sanity check, not a CI
threshold.

## Render Statistics

`RenderStatistics` records:

- output width/height, supersample factor, and internal dimensions;
- tile count;
- total triangles and culled backfaces;
- opaque/cutout/translucent triangle counts;
- textured triangle count;
- pixels tested (fragments inside the clipped row spans) and pixels written.

Golden PNGs for cube, partial shapes, water, negative coordinates, AO, textured
cube, stairs, cutout, glass, and chest live in `fixtures/golden/render/`.
Regenerate them with `UPDATE_GOLDEN=1` after an intentional renderer change.
Textured goldens use project-owned synthetic assets only.

## Level of Detail

`--lod N` (N in 2..=16) aggregates the parsed scene into N-sized cells before
meshing. Each cell keeps the most frequent non-air state; ties use canonical
state order and overlapping regions keep the later region. The result is an
ordinary scene in LOD units, so meshing and rendering are unchanged.

LOD is an approximation: block entities, entities, and pending ticks are
dropped, and small details disappear. It is opt-in and off by default.

Measured on the 9.4M-block stress fixture (211x211x211, 1024x1024, supersample
2, 12 threads, client jar):

| LOD | Triangles | Wall | Peak working set |
|---|---|---|---|
| 1 (exact) | 117M | 36.6 s | 18.9 GB |
| 2 | 14.6M | 16.4 s | 9.6 GB |
| 4 | 1.55M | 4.0 s | 683 MB |

Aggregation is parallel over output rows and produces the same scene as the
single-threaded path. On the test host, the LOD 2 pass for `000001` dropped from
0.63 s to 0.21 s; the scan stage alone dropped from 0.54 s to 0.07 s on 12
threads. LOD 8 for `000002` dropped from 1.2 s to 0.20 s.

For oversized schematics, LOD 4 is the practical default; LOD 2 keeps more
detail at a large memory cost.

`--lod auto` starts with an exact mesh and falls back to 2, 4, 8, or 16 only
when the quad budget does not fit. The budget is 80 percent of available memory
divided by roughly 400 bytes per quad, so a file that can be rendered exactly
is never degraded. On the 48 GB test host, `000003` (solid, 67M blocks) renders
exactly in 7.5 s, while the porous `000001` falls back to LOD 2 in 16.4 s.

## Known Limitations

- Multi-view output writes separate PNGs; it does not compose a contact sheet.
- Textures require a user-provided resource pack or client jar.
- No mipmaps; nearest or bilinear sampling only.
- No shadows, full light propagation, or emissive bloom.
- Translucent sorting is per tile and approximate.
- CPU-only; tiles are parallel and mesh pages are built in parallel, but PNG
  encoding runs on the calling thread.
- Texture coordinates are Q12 fixed point; UVs outside [-8, 8) are clamped.
- Color management is approximate; output is treated as sRGB.
- Golden PNGs cover small synthetic scenes, not large real structures.
- Performance thresholds still need a real workload and deployment target.

## Determinism

Given the same Scene, Mesh, options, and renderer build, the RGBA buffer and PNG
bytes are deterministic. Triangle order follows page/quad order, translucent
sorting is stable, and the worker count does not change the output. Statistics
are accumulated in tile order after workers join.
