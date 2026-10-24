# Spec: Phase 3C — Resource-Pack Materials and Models

**Status:** Implemented; validation in `tasks/phase-3c-textures-todo.md`.  
**Target:** Rust engine, Phase 3C (after Phase 3B)  
**Asset source for local validation:** `D:\hmcl\.minecraft\versions\26.2-Fabric\26.2-Fabric.jar`

## Objective

Replace the color-block appearance with runtime-loaded Minecraft materials and
full Java block models, without committing Minecraft assets.

Success means:

- `renderd render` can read a user-provided client jar or extracted assets.
- The resource pack's blockstate/model mapping is authoritative; there is no
  hand-maintained per-block texture table for model-backed blocks.
- Blockstates select the correct model variant or multipart model.
- Models resolve parent chains, texture variables, elements, UVs, rotations,
  culling, tint, and transparency.
- The CPU renderer samples textures, applies tint and lighting, and keeps the
  existing cutout/translucent behavior.
- Blocks without a usable pack model (fluids, block entities, unknown blocks)
  fall back to the Phase 2 geometry and colors, with diagnostics.
- Same input plus same assets produces the same PNG bytes.
- No Minecraft textures, models, or client jars are committed.

## Scope

### In scope

- Asset sources:
  - client jar read directly as a zip;
  - extracted assets directory;
  - one optional resource-pack overlay (directory or zip).
- `blockstates/<block>.json`:
  - `variants`;
  - `multipart` with `when`, `AND`, `OR`, and `a|b` alternatives;
  - variant/apply `x`, `y`, `uvlock`, `weight`.
- `models/**/*.json`:
  - `parent` inheritance with cycle detection;
  - `textures` strings and `{ "sprite", "force_translucent" }` objects;
  - texture variable resolution (`#all`, `#side`, ...);
  - `elements` with `from`, `to`, per-element `rotation`, `shade`;
  - faces with `uv`, `texture`, `cullface`, `rotation`, `tintindex`;
  - model-level `ambientocclusion`.
- Textures:
  - PNG loading from `assets/<ns>/textures/...`;
  - first frame of animated textures (`.mcmeta` recognized);
  - nearest-neighbor sampling by default, optional linear;
  - per-face tint for grass, foliage, water, and redstone;
  - texture alpha drives cutout and translucent behavior.
- Mesh and renderer integration:
  - explicit per-quad texture id and UVs;
  - per-quad render layer;
  - per-vertex tint, shade flag, and AO flag;
  - texture sampling in the CPU rasterizer.
- CLI:
  - `--client-jar <jar>`;
  - `--assets <dir>`;
  - `--resource-pack <dir|zip>`;
  - `--texture-filter nearest|linear`;
  - `renderd materials <file> ...` coverage report.
- Diagnostics and limits:
  - missing model/texture counters;
  - invalid JSON and cycle errors with stable codes;
  - zip path traversal protection;
  - JSON, model depth, element count, texture dimension, and total texture
    memory limits.
- Tests and documentation:
  - synthetic resource pack fixture owned by the project;
  - golden PNG for textured cube, staircase, cutout, and translucent cases;
  - ignored real-jar smoke test via `MINECRAFT_CLIENT_JAR`;
  - `docs/textures.md` and README updates.

### Out of scope

- Entity models and block-entity renderers (chest lids, sign text, banners,
  shulker boxes beyond a static box).
- Item models and `builtin/generated`.
- Animation playback; only the first frame is used.
- Biome-specific colormaps; a fixed plains tint is used.
- PBR, normal maps, emissive maps, shaders.
- GPU rendering.
- Automatic asset download.
- Multiple overlapping resource packs beyond one base plus one overlay.

## Assumptions

1. The user supplies assets explicitly; no auto-detection of launchers.
2. The client jar follows the vanilla layout
   `assets/minecraft/{blockstates,models,textures}`.
3. Resource packs use the same layout; a pack may be incomplete, so missing
   entries fall back to the jar and then to color blocks.
4. The renderer remains static and deterministic; random weighted variants use
   the first entry.
5. Fixed plains tint is acceptable for the first version:
   grass `#91BD59`, foliage `#77AB2F`, water `#3F76E4`.
6. The existing color palette remains the layer hint and the fallback color.
7. `--client-jar`, `--assets`, and `--resource-pack` are optional; without them
   the current color-block output is unchanged.

## Commands

```powershell
# Build
cargo build --workspace

# Fast checks
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Full Windows check
powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/check.ps1

# Texture render against the local client jar
cargo run -p renderd-cli --release -- render fixtures\private\四边形大宗.litematic `
  --client-jar "D:\hmcl\.minecraft\versions\26.2-Fabric\26.2-Fabric.jar" `
  --output renders\textured.png

# Material coverage
cargo run -p renderd-cli --release -- materials fixtures\private\四边形大宗.litematic `
  --client-jar "D:\hmcl\.minecraft\versions\26.2-Fabric\26.2-Fabric.jar" --json

# Extracted assets
cargo run -p renderd-cli --release -- render model.litematic --assets D:\mc-assets --output out.png

# Optional overlay
cargo run -p renderd-cli --release -- render model.litematic `
  --client-jar "D:\...\26.2-Fabric.jar" `
  --resource-pack "D:\...\resourcepacks\MyPack" `
  --output out.png
```

## Project Structure

```text
crates/material/                  new crate: assets, blockstates, models, textures
  src/source.rs                   jar, directory, and pack overlays
  src/blockstate.rs               variants, multipart, selectors
  src/model.rs                    parent chain, elements, faces, rotations
  src/texture.rs                  PNG, mcmeta, texture set, sampling metadata
  src/catalog.rs                  BlockCatalog implementation and fallback
  src/tint.rs                     fixed tint providers
  tests/                          synthetic pack and parser tests
  tests/fixtures/pack/            project-owned JSON and generated PNGs

crates/mesh/
  src/model.rs                    UV, texture id, layer, shade flags
  src/builder.rs                  texture/tint assignment, AO/shade
  src/catalog.rs                  BlockCatalog contract

crates/render/
  src/rasterizer.rs               texture sampling, per-quad layer
  src/texture.rs                  TextureView sampling helpers

crates/cli/
  src/main.rs                     flags, material loading, coverage report

docs/textures.md                  asset layout, model rules, limits, licensing
```

## Architecture

### Asset sources

```text
ResourcePack (optional overlay)
        ↓ miss
ClientJar / AssetDir
        ↓ miss
Built-in color palette
```

A `Source` trait exposes `read(&str) -> Result<Option<Vec<u8>>>` for normalized
paths such as `assets/minecraft/blockstates/stone.json`. Jar and zip sources
reject absolute paths and `..`. The overlay is queried first, then the base.

### Blockstate selection

1. Parse the canonical state name and sorted properties.
2. Load `blockstates/<name>.json` from the overlay/base.
3. If `variants` exists, choose the first variant whose key is a subset of the
   state properties; `""` matches any state. Prefer the key with the most
   matched properties.
4. If `multipart` exists, evaluate each part and concatenate the applied models.
5. Apply `x`/`y` rotation and `uvlock` around the block center.

Missing blockstate or model means fallback, not a hard failure.

### Model resolution

- Resolve `parent` recursively, merging textures and inheriting elements when
  the child has none.
- Resolve `#variable` textures recursively with cycle detection.
- Convert each element face into a CCW quad with an outward normal.
- Convert `uv` from 0..16 model units to normalized UVs; derive UVs from element
  bounds when omitted.
- Apply face `rotation` to UV corners.
- Apply element `rotation` around `origin`; support `x`, `y`, `z` and `rescale`.
- Carry `cullface`, `tintindex`, `shade`, and `ambientocclusion`.
- Mark a model as a full cube only when one element covers 0..16 and all six
  faces exist; only opaque full cubes occlude neighbors.

### Textures

- Paths normalize to `assets/minecraft/textures/<path>.png`.
- Animated textures use the first square frame. `.mcmeta` is optional.
- Missing texture: fallback to the built-in block texture map, then to the
  color palette.
- Texture limits: max 2048x2048 per image, max 256 MiB decoded total, max 4096
  textures.
- Sampling:
  - nearest: `floor(u * width)`, `floor(v * height)`, repeat;
  - linear: bilinear with repeat;
  - alpha = texture alpha * vertex tint alpha.
- Tint:
  - `tintindex` on grass/fern/tall grass uses grass tint;
  - leaves/vines use foliage tint;
  - water uses water tint;
  - redstone wire derives a red tint from `power`;
  - everything else is white.

### Layers

- Base layer comes from the existing color palette (`opaque`, `cutout`,
  `translucent`).
- `force_translucent` texture objects set the quad to translucent.
- A texture with transparent pixels upgrades an opaque block to cutout.
- Per-quad layers let grass side overlays and redstone dust composite correctly.
- Culling and AO use the block-level opaque/full-cube decision, not the per-quad
  layer.

### Fallback

- Pack blockstate/model mapping is tried first for every block state.
- If the selected model has no elements, use the Phase 2 built-in shape.
- Use the model's `particle` texture as the default texture for the built-in
  shape when present.
- Otherwise use a built-in block-name-to-texture rule, then the color palette.
- Fluids keep the Phase 2 height model and use `water_still`/`lava_still`.
- Unknown block states use the magenta fallback and stay visible.

### Chest and block-entity special case

Chests are block entities with a special model in the client; vanilla
`block/chest.json` has no elements. A built-in closed-chest model is used instead:

- Detect `chest`, `trapped_chest`, `ender_chest`, and copper chest variants.
- Load `assets/minecraft/textures/entity/chest/<variant>.png` and the
  `_left`/`_right` variants for double chests.
- Build body, lid, and lock boxes using the standard Minecraft box unwrap at the
  vanilla texture offsets.
- Use `facing` to rotate the model and `type=single|left|right` to pick the texture
  and half width.
- If the entity texture is missing, fall back to the block-name mapping or the
  color palette.
- Other block entities keep the Phase 2 static box until their renderer models
  are implemented; the `materials` report lists them as `fallback-block-entity`.

### Renderer integration

- `ModelQuad` gains `uv`, `texture`, `layer`, and `shade`.
- `Vertex` carries the texture id, tint RGBA, and shade flag.
- `render_mesh` keeps the color-only signature; add
  `render_mesh_with_textures(..., &TextureSet)`.
- Shading and AO multiply texture RGB. Cutout uses the sampled alpha. Translucent
  quads blend with the sampled alpha.
- Texture sampling stays deterministic and does not depend on thread count.

## Code Style

Follow the workspace conventions:

- Rust 2021, `rustfmt`, 4 spaces, max width 100.
- English identifiers; comments explain intent.
- Data-driven mappings in JSON, not large `match` trees.
- No `unwrap` on untrusted asset data; errors use stable codes.
- No Minecraft assets in `fixtures/` or `crates/`; synthetic test textures only.

## Testing Strategy

- Unit tests:
  - variant subset matching and precedence;
  - multipart `AND`/`OR` and `a|b` values;
  - parent inheritance and texture variable resolution;
  - element/face UV conversion and face rotation;
  - blockstate rotation and `uvlock`;
  - element rotation and rescale;
  - first-frame animation cropping;
  - layer selection, force_translucent, missing texture fallback.
- Integration tests:
  - synthetic pack renders a textured cube and staircase;
  - cutout leaves discard transparent pixels;
  - translucent glass blends;
  - deterministic PNG bytes;
  - color-block mode unchanged when no assets are supplied.
- Golden PNGs:
  - `textured-cube`, `textured-stairs`, `textured-cutout`, `textured-glass`.
- Real asset smoke test:
  - ignored test reads `MINECRAFT_CLIENT_JAR`;
  - validates `stone`, `oak_stairs`, `grass_block`, `glass`, `water`, `observer`;
  - renders a small synthetic scene.
- Local validation:
  - run the commands above against `26.2-Fabric.jar`;
  - record material coverage and performance in `tasks/phase-3c-todo.md`.

## Boundaries

- Always:
  - keep color-block mode working without assets;
  - validate untrusted JSON/zip/PNG inputs;
  - report missing models/textures with counts;
  - run `fmt`, clippy, and tests before commits;
  - keep output deterministic.
- Ask first:
  - adding further asset formats or dependencies;
  - changing the default renderer behavior to require assets;
  - adding biome or animation playback scope.
- Never:
  - commit client jars, resource packs, or Minecraft textures;
  - download Minecraft assets automatically;
  - execute code from packs or resolve paths outside the asset root;
  - silently render a wrong model as opaque without a diagnostic.

## Success Criteria

1. `renderd render --client-jar <jar>` produces a textured PNG for the local
   `26.2-Fabric.jar`.
2. `renderd materials` reports pack/fallback/texture coverage for the user's
   fixture.
3. At least 90% of non-air block states in the fixture resolve to a pack model or
   a textured fallback; the rest are listed with reasons.
4. `oak_stairs`, `grass_block`, `glass`, `water`, `observer`, `redstone_wire`,
   `rail`, and `oak_fence` use pack geometry or a documented fallback.
5. Single and double chests render with the built-in chest model and entity
   texture, with facing and type respected.
6. Textured outputs are byte-deterministic across runs and thread counts.
7. Color-block output without assets is unchanged.
8. Windows and WSL checks pass; no Minecraft assets are committed.

## Resolved Decisions

1. Fixed plains tint for the first version; biome/colormap is a later additive
   option.
2. Missing textures fall back to the built-in mapping, then the color palette,
   with a warning and a counter.
3. One optional overlay resource pack is enough for the first version.
4. Chests use the built-in body/lid/lock model; other block entities stay
   approximate until a later phase.
5. The pure-Rust `zip` crate is approved for reading client jars and pack zips.
