# Implementation Plan: Phase 3C 鈥?Resource-Pack Materials

**Status:** Implemented and checked; human review pending.  
**Spec:** `tasks/phase-3c-textures-spec.md`  
**Input fixture:** `fixtures/private/鍥涜竟褰㈠ぇ瀹?litematic`  
**Local assets:** `D:\hmcl\.minecraft\versions\26.2-Fabric\26.2-Fabric.jar`

## Goal

Resolve block states through the resource pack's blockstate/model mapping, load
the referenced textures, and render textured geometry on the CPU. Fall back to
the Phase 2 built-in geometry and the color palette when the pack has no usable
model. Chests get a dedicated built-in model.

## Pipeline

```text
Scene block state
  -> AssetSource (overlay pack -> client jar/assets dir)
  -> blockstates/<name>.json
  -> variant or multipart selection
  -> model parent chain + texture variables
  -> elements / faces / UV / rotations
  -> mesh quads with texture id, tint, layer
  -> CPU rasterizer samples TextureSet
  -> PNG
```

## Slices

### Slice 1: Asset source and serialization

- New crate `litematic-material`.
- `Source` trait for client jar, extracted assets, and resource-pack overlay.
- Normalized asset paths and zip path safety.
- JSON structs for blockstates and models.
- Stable errors and limits.
- Synthetic pack fixture builder for tests.

### Slice 2: Blockstate and model resolution

- Variant subset matching and deterministic first selection.
- Multipart `when`, `AND`, `OR`, and `a|b`.
- Parent inheritance and texture variable resolution.
- Elements, faces, default/explicit UVs, face rotation.
- Blockstate x/y rotation and `uvlock`.
- Element rotation and rescale handling.

### Slice 3: Texture pipeline

- PNG decode and animated first-frame crop.
- Texture registry with nearest/linear sampling.
- Texture limits and missing-texture diagnostics.
- Tint providers for grass, foliage, water, and redstone.
- `force_translucent` and texture-alpha layer selection.

### Slice 4: Mesh and renderer integration

- `ModelQuad` UV, texture id, layer, and shade.
- `Vertex` texture id and shade.
- Mesh builder tint/UV assignment and AO suppression.
- `render_mesh_with_textures` and fragment sampling.
- Deterministic output across thread counts.

### Slice 5: Fallback and chest model

- Built-in model fallback via `ColorBlockCatalog`.
- Block-name-to-texture fallback rules.
- `particle` texture fallback for empty models.
- Chest body/lid/lock model with entity textures.
- Single/left/right chest type and facing.

### Slice 6: CLI and coverage

- `--client-jar`, `--assets`, `--resource-pack`, `--texture-filter`.
- `blockscope materials` report.
- JSON and human output.
- Asset errors use exit code 2.

### Slice 7: Validation and documentation

- Synthetic pack tests and golden PNGs.
- Ignored real-jar test via `MINECRAFT_CLIENT_JAR`.
- Render the local fixture and inspect visual quality.
- `docs/textures.md`, README, task updates.
- Windows and WSL checks.

## Task Index

| ID  | Task                                 | Depends on | Size |
| --- | ------------------------------------ | ---------- | ---- |
| M1  | Asset sources and JSON structs       | Phase 3B   | M    |
| M2  | Blockstate and model resolver        | M1         | L    |
| M3  | Texture loader and tint              | M2         | M    |
| M4  | Mesh/render texture integration      | M3         | L    |
| M5  | Fallback and chest model             | M4         | M    |
| M6  | CLI and materials report             | M5         | M    |
| M7  | Golden tests and real-jar validation | M6         | M    |
| M8  | Documentation and final checks       | M7         | S    |

## Risks

| Risk                                       | Impact                       | Mitigation                                                     |
| ------------------------------------------ | ---------------------------- | -------------------------------------------------------------- |
| UV/normal orientation differs from vanilla | Textures rotated or mirrored | Synthetic quadrant texture, golden tests, real-jar visual pass |
| Model parser rejects valid packs           | Missing geometry             | Tolerant parser, fallback, `materials` diagnostics             |
| Empty block-entity models                  | Invisible chests             | Dedicated chest model and fallback                             |
| Texture memory/zip bombs                   | OOM                          | Per-image and total limits, zip path checks                    |
| Large model count slows mesh build         | Latency                      | Lazy per-state cache, parse only used blocks                   |
| Cross-platform trig differences            | Golden drift                 | Golden only uses 0/90 rotations; real assets not byte-golden   |
| Resource pack overlay misses textures      | Patchy output                | Base jar fallback, then name mapping/color                     |

## Checkpoints

### Checkpoint M-1: Parser

- [x] A synthetic blockstate selects the expected model.
- [x] Parent textures and variables resolve.
- [x] A synthetic full cube renders with the correct UV orientation.

### Checkpoint M-2: Real assets

- [x] The local jar opens and resolves stone, stairs, grass, glass, water,
      observer, redstone wire, rail, and fence.
- [x] `blockscope materials` reports coverage for the local fixture.

### Checkpoint M-3: Acceptance

- [x] Textured PNG is deterministic.
- [x] Color-block mode without assets is unchanged.
- [x] Chests render with body/lid/lock and entity texture.
- [x] Windows and WSL checks pass.
- [x] No Minecraft assets are committed.

Detailed acceptance criteria live in `tasks/phase-3c-textures-todo.md`.

## Local Validation

- Client jar: `D:\hmcl\.minecraft\versions\26.2-Fabric\26.2-Fabric.jar`.
- Fixture: `fixtures/private/鍥涜竟褰㈠ぇ瀹?litematic`.
- Coverage: 41,127 / 41,127 non-air blocks textured; 36,444 pack models,
  4,400 chests, 283 textured fallbacks, 0 missing textures.
- Render: 1024x1024, SSAA 2, 45/30, about 575 ms total in release mode.
- Facing checks passed for north/south/east/west and up/down observers and for
  single/double chests.
