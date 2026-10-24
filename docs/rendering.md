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
- The camera looks at the center of the scene bounds.
- The scene is fitted with padding; default padding is 16 px.
- World positions are `MeshPage.origin + page-local vertex`.
- Screen Y is flipped so image coordinates follow PNG top-left origin.
- Smaller camera depth means closer to the camera.

The camera basis:

```text
forward = normalize([cos(elev)*sin(yaw), -sin(elev), cos(elev)*cos(yaw)])
right   = normalize(cross(forward, world_up))
up      = cross(right, forward)
```

## Mesh Inputs

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

## CLI

```powershell
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --width 2048 --height 2048
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --views 45,225
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --yaw 225 --elevation 35
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --supersample 4
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --threads 4
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --background "#202830" --lighting flat-v1
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png `
  --client-jar "D:\hmcl\.minecraft\versions\26.2-Fabric\26.2-Fabric.jar"
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --texture-filter linear
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --json
```

`--views` accepts comma-separated yaw angles. With one view the output path is
used as-is; with multiple views the renderer writes `out-45.png`,
`out-225.png`, and so on. `--background` accepts `transparent`, `#RRGGBB`, or
`#RRGGBBAA`. `--json` prints render statistics instead of the human-readable
summary.

The human-readable output includes parse, mesh, render, PNG, and total timings.

## Benchmark

```powershell
cargo run -p renderd-cli --release -- bench path\to\model.litematic --iterations 5
cargo run -p renderd-cli --release -- bench path\to\model.litematic --width 2048 --height 2048 --supersample 2 --json
```

`renderd bench` parses and meshes once, then renders and PNG-encodes
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
- pixels tested and pixels written.

Golden PNGs for cube, partial shapes, water, negative coordinates, AO, textured
cube, stairs, cutout, glass, and chest live in `fixtures/golden/render/`.
Regenerate them with `UPDATE_GOLDEN=1` after an intentional renderer change.
Textured goldens use project-owned synthetic assets only.

## Known Limitations

- Multi-view output writes separate PNGs; it does not compose a contact sheet.
- Textures require a user-provided resource pack or client jar.
- No mipmaps; nearest or bilinear sampling only.
- No shadows, full light propagation, or emissive bloom.
- Translucent sorting is per tile and approximate.
- CPU-only; tiles are parallel, but mesh construction and PNG encoding are not.
- Color management is approximate; output is treated as sRGB.
- Golden PNGs cover small synthetic scenes, not large real structures.
- Performance thresholds still need a real QQBot workload and deployment target.

## Determinism

Given the same Scene, Mesh, options, and renderer build, the RGBA buffer and PNG
bytes are deterministic. Triangle order follows page/quad order, translucent
sorting is stable, and the worker count does not change the output. Statistics
are accumulated in tile order after workers join.
