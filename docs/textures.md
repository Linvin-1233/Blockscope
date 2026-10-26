# Resource-Pack Materials

Phase 3C resolves block states through a Minecraft Java resource pack, loads the
referenced textures, and renders them on the CPU. The pack is supplied at
runtime; no Minecraft assets are stored in this repository.

## Overview

For end-user CLI workflows using `--client-jar`, `--assets`, and `--resource-pack`,
see [Materials and Resource Packs](./user/en/materials.md). Without resource pack
options, rendering defaults to the built-in color-block mode.

## Asset Layout

The CLI accepts these layouts and detects them automatically:

```text
<root>/assets/minecraft/blockstates/*.json
<root>/assets/minecraft/models/**/*.json
<root>/assets/minecraft/textures/**/*.png
```

The following roots work:

- a client jar or resource-pack zip (read directly, no extraction);
- a resource-pack directory;
- an extracted `assets` directory;
- an extracted `minecraft` namespace directory.

Lookup order:

```text
resource-pack overlay -> base jar/assets -> built-in name mapping -> color palette
```

## Blockstate Resolution

- `variants` keys match a subset of the block state properties; `""` matches
  all states. The most specific key wins.
- Variant arrays are weighted/random in Minecraft; static rendering uses the
  first entry so output stays deterministic.
- `multipart` supports `when`, `AND`, `OR`, `a|b` alternatives, and parts with
  no condition.
- Blockstate `x` and `y` rotations and `uvlock` are applied around the block
  center.

## Model Resolution

- `parent` chains merge textures and inherit elements when the child has none.
- Texture variables (`#all`, `#side`, ...) resolve recursively; `sprite`
  objects and `force_translucent` are supported.
- Elements support `from`, `to`, `rotation`, `rescale`, and `shade`.
- Faces support `uv`, `texture`, `cullface`, `rotation`, and `tintindex`.
- `ambientocclusion` disables vertex AO for a model.
- A model is treated as a full opaque cube only when an element covers the whole
  0..16 box and has all six faces.

## Textures

- PNGs are read from `assets/minecraft/textures/<name>.png`.
- Animated textures use the first square frame; `.mcmeta` animation playback is
  not implemented.
- Nearest-neighbor sampling is the default. `--texture-filter linear` enables
  bilinear sampling for HD packs.
- The rasterizer resolves a face's texture once and reuses it for every covered
  pixel. The sampler is inlined across the material/render crate boundary; a
  synthetic 16x16 microbenchmark measured 1.7-2.7x higher sampling throughput
  for nearest and bilinear filters.
- Texture alpha drives cutout and translucent rendering.
- Textures are indexed per face; there is no atlas, so no atlas bleeding.

Limits:

| Limit | Value |
|---|---|
| texture dimensions | 2048 x 2048 |
| decoded texture memory | 256 MiB |
| texture count | bounded by decoded texture memory |
| asset size | 8 MiB |
| model parent depth | 64 |

## Tint

The first version uses fixed plains colors:

| Surface | Color |
|---|---|
| grass | `#91BD59` |
| foliage | `#77AB2F` |
| water | `#3F76E4` |
| redstone wire | dark red to bright red by `power` |

Biome colormaps, `tintindex` 1 behavior, and world-specific tint are later
additive features.

## Render Layers

- The base layer comes from the original color palette.
- `force_translucent` texture objects make a quad translucent.
- Transparent texture pixels upgrade an opaque block to cutout.
- Per-quad layers keep grass overlays and redstone dust compositing correctly.

## Chests

Vanilla chest block models have no elements; chests are block entities. A
built-in closed-chest model is used:

- body, lid, and lock boxes;
- `entity/chest/<variant>.png` textures, including `_left` and `_right` for
  double chests;
- `facing` rotation and `type=single|left|right` geometry;
- trapped, ender, and copper chest variants;
- missing textures fall back to the block-name mapping or the color palette.

Other block entities keep the Phase 2 static box until their entity models are
implemented.

## Fallback

- If a blockstate or model is missing, the Phase 2 built-in geometry is used.
- If a model has textures but no elements, its `particle` texture is applied to
  the built-in geometry.
- Otherwise a block-name texture rule is tried, then the color palette.
- Unknown blocks stay visible in magenta.

## Mesh Export

`blockscope mesh` remains the color-block export path. OBJ/PLY files do not include
texture coordinates or materials yet; use `blockscope render` for textured output.

## Coverage Report

The `materials` command (documented in [CLI Reference](./user/en/cli.md))
reports per-block kinds (`pack-model`, `chest`, `fallback`, `unknown`), texture
status, and missing texture counts.

On a private large Fabric fixture with a matching client jar:

- 36,444 blocks use pack models;
- 4,400 chests use the built-in chest model;
- 283 water/shulker blocks use textured built-in fallback;
- 0 missing textures;
- 41,127 / 41,127 non-air blocks textured.

## Determinism

Given the same scene, pack, options, and renderer build, the RGBA buffer and PNG
bytes are deterministic. Weighted variants use the first entry, animated
textures use the first frame, and texture sampling does not depend on worker
count.

## Known Limitations

- Block entity models other than chests are approximate static boxes.
- No animation playback; only the first frame is sampled.
- No PBR, normal maps, or emissive maps.
- Fixed plains tint; no biome colormap.
- Weighted variants always choose the first entry.
- Custom item models and `builtin/generated` are out of scope.
- Only the `minecraft` asset namespace is resolved; modded block namespaces
  fall back to the built-in geometry and palette.
- Double-chest seams and latch placement are approximate.
- Texture alpha upgrades opaque pack models to cutout; a pack that needs a
  different layer can still be overridden by the built-in palette layer.
- UV lock and element rotations follow the common cases but are not validated
  against every resource pack feature.
- HD textures use nearest or bilinear sampling only; no mipmaps.

## Licensing

Never commit client jars, resource packs, or extracted Minecraft textures. The
repository contains only the loader, the block-name fallback rules, synthetic
test textures, and synthetic golden images.
