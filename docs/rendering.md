# CPU Rendering and PNG Output

Phase 3 renders the Phase 2 mesh to an RGBA PNG on the CPU. It uses an
orthographic camera and a tile rasterizer. No GPU, Minecraft client, textures,
or resource packs are required.

## Pipeline

```text
Scene + Mesh
  -> Camera (orthographic fit)
  -> triangle build (page origin, lighting, AO)
  -> tile rasterizer (depth, culling, alpha)
  -> RGBA8 buffer
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
- straight RGBA block colors,
- render layer,
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
- Translucent triangles are sorted far-to-near per tile and blended without
  writing depth.
- Solid backgrounds are filled before rasterization; transparent backgrounds
  start at `[0, 0, 0, 0]`.
- Final RGBA8 conversion clamps and rounds; fully transparent pixels are written
  as zero RGB.

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
| max pixels | 16,777,216 |

Validation rejects zero dimensions, elevation beyond ±89.9°, non-finite values,
padding that leaves no drawable area, and outputs above the pixel limit.

## CLI

```powershell
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --width 2048 --height 2048
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --yaw 225 --elevation 35
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --background "#202830" --lighting flat-v1
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --json
```

`--background` accepts `transparent`, `#RRGGBB`, or `#RRGGBBAA`. `--json`
prints render statistics instead of the human-readable summary.

## Render Statistics

`RenderStatistics` records:

- output width/height and tile count;
- total triangles and culled backfaces;
- opaque/cutout/translucent triangle counts;
- pixels tested and pixels written.

## Known Limitations

- One view per render; multi-view output is deferred.
- No anti-aliasing or supersampling yet.
- No textures, resource packs, or mipmaps.
- No shadows, full light propagation, or emissive bloom.
- Translucent sorting is per tile and approximate.
- CPU-only and single-threaded in Phase 3A.
- Color management is approximate; output is treated as sRGB.

## Determinism

Given the same Scene, Mesh, options, and renderer build, the RGBA buffer and PNG
bytes are deterministic. Triangle order follows page/quad order, and
translucent sorting is stable.
