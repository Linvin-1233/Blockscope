# Phase 3C Task List 鈥?Resource-Pack Materials

**Status:** Implementation and checks complete; human review pending.  
**Plan:** `tasks/phase-3c-textures-plan.md`  
**Spec:** `tasks/phase-3c-textures-spec.md`

## Task M1: Asset sources and JSON structs

**Description:** Add the `litematic-material` crate and read assets from a client
jar, an extracted directory, or a resource-pack overlay.

**Acceptance criteria:**

- [x] `Source` reads normalized paths from a jar, directory, and overlay.
- [x] Overlay wins, then base jar/dir.
- [x] Absolute paths, `..`, and missing entries are rejected or return `None`.
- [x] Zip entries are read without extracting the whole jar.
- [x] JSON structs cover variants, multipart, models, elements, and faces.
- [x] Stable error codes for unreadable assets and invalid JSON.
- [x] `cargo test -p litematic-material` passes.

**Verification:**

- [x] Synthetic pack directory source test.
- [x] Synthetic zip source test.
- [x] Overlay precedence test.

**Dependencies:** Phase 3B  
**Estimated scope:** Medium

## Task M2: Blockstate and model resolver

**Description:** Resolve a `BlockStateKey` to a pack model with elements and
texture references.

**Acceptance criteria:**

- [x] Variant keys match a subset of state properties; `""` matches all.
- [x] The most specific variant wins.
- [x] Variant arrays use the first entry deterministically.
- [x] Multipart supports `AND`, `OR`, and `a|b`.
- [x] Parent chains merge textures and inherit elements.
- [x] Texture variables resolve with cycle detection.
- [x] Elements produce quads with correct normals and UVs.
- [x] Face rotation, element rotation, blockstate x/y, and `uvlock` are applied.
- [x] `cullface`, `shade`, `tintindex`, and `ambientocclusion` are carried.
- [x] Missing models return fallback, not a panic.

**Verification:**

- [x] Synthetic pack covers variants, multipart, parent, and rotation.
- [x] A full cube renders with a quadrant texture in the expected orientation.
- [x] A staircase uses the expected model and UVs.

**Dependencies:** M1  
**Estimated scope:** Large

## Task M3: Texture loader and tint

**Description:** Load PNG textures and provide sampling plus tint.

**Acceptance criteria:**

- [x] PNGs decode from the asset source.
- [x] Animated textures use the first frame.
- [x] Texture dimensions and total memory are limited.
- [x] Missing textures return `None` and are counted.
- [x] Nearest and linear sampling are deterministic.
- [x] Grass, foliage, water, and redstone tints are applied.
- [x] `force_translucent` selects the translucent layer.

**Verification:**

- [x] Synthetic quadrant texture samples the expected corner colors.
- [x] Animated strip crops frame 0.
- [x] Oversized texture is rejected.

**Dependencies:** M2  
**Estimated scope:** Medium

## Task M4: Mesh and renderer texture integration

**Description:** Carry texture ids, UVs, tint, layer, and shade through the mesh
and sample them during rasterization.

**Acceptance criteria:**

- [x] `ModelQuad` carries UV, texture id, optional layer, and shade.
- [x] `Vertex` carries texture id and shade.
- [x] Built-in models receive sensible default UVs.
- [x] Mesh builder assigns tint/UV and suppresses AO when requested.
- [x] `render_mesh_with_textures` samples textures and respects alpha.
- [x] Existing `render_mesh` remains color-only.
- [x] Thread count does not change output.

**Verification:**

- [x] Color-block tests still pass.
- [x] Textured cube golden PNG.
- [x] Cutout and translucent texture tests.

**Dependencies:** M3  
**Estimated scope:** Large

## Task M5: Fallback and chest model

**Description:** Keep renders useful when the pack has no elements, and render
chests with body/lid/lock and entity textures.

**Acceptance criteria:**

- [x] Empty or missing models fall back to Phase 2 geometry.
- [x] `particle` texture is used as the fallback when present.
- [x] Block-name mapping covers common fallback blocks.
- [x] Chest models use body, lid, and lock boxes.
- [x] `facing` rotates the chest; `type=single|left|right` selects texture/half.
- [x] Missing chest texture falls back to the name mapping/color.
- [x] `materials` reports fallback reasons.

**Verification:**

- [x] Synthetic chest-like model test.
- [x] Real jar renders single and double chests recognizably.
- [x] Missing assets keep color-block output valid.

**Dependencies:** M4  
**Estimated scope:** Medium

## Task M6: CLI and materials report

**Description:** Wire assets into the CLI and report coverage.

**Acceptance criteria:**

- [x] `--client-jar`, `--assets`, `--resource-pack`, and `--texture-filter` exist.
- [x] `render` and `bench` accept assets.
- [x] `materials` prints per-block model/texture source and counts.
- [x] `materials --json` is stable and parseable.
- [x] Invalid assets exit with code 2 and a stable code.
- [x] Without assets, existing behavior is unchanged.

**Verification:**

- [x] CLI test with a synthetic pack.
- [x] CLI test for a missing jar.
- [x] `materials --json` parses.

**Dependencies:** M5  
**Estimated scope:** Medium

## Task M7: Golden tests and real-jar validation

**Description:** Lock down textured output and validate against a real client jar.

**Acceptance criteria:**

- [x] Golden PNGs cover textured cube, stairs, cutout, translucent, and chest.
- [x] Golden PNGs use only project-owned synthetic assets.
- [x] Ignored test reads `MINECRAFT_CLIENT_JAR` and validates key models.
- [x] Local fixture renders with at least 90% pack/fallback texture coverage.
- [x] Textured output is byte-deterministic.
- [x] No Minecraft assets are committed.

**Verification:**

- [x] `cargo test -p litematic-material`
- [x] `cargo test -p litematic-render`
- [x] `$env:MINECRAFT_CLIENT_JAR=...; cargo test -p litematic-material --test real_jar -- --ignored --nocapture`
- [x] Render the local fixture and inspect the PNG.

**Dependencies:** M6  
**Estimated scope:** Medium

## Task M8: Documentation and final checks

**Description:** Document asset layout, resolution, fallbacks, limits, and
licensing.

**Acceptance criteria:**

- [x] `docs/textures.md` covers jar/dir/pack layout and CLI usage.
- [x] README links the document and lists the new flags.
- [x] Limits and known approximations are documented.
- [x] Windows and WSL checks pass.
- [x] Task status and plan checkpoints are updated.
- [ ] Human review before Phase 4.

**Verification:**

- [x] `powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/check.ps1`
- [x] WSL check command from the README.
- [x] `blockscope materials` on the local fixture.

**Dependencies:** M7  
**Estimated scope:** Small

## Follow-up: Camera yaw orientation (2026-10-24)

**Description:** Align the default view with the reference Litematic viewer and
remove the apparent piston/observer direction flip.

- [x] Yaw now uses the reference viewer convention: 0 looks from the south,
      45 from the south-west, and 225 from the north-east.
- [x] `Camera` unit tests pin the default orientation and screen-right axis.
- [x] Golden PNGs regenerated for the new orientation.
- [x] `docs/rendering.md` documents the convention and camera basis.

**Verification:**

- [x] Default render matches the reference viewer's rotation 45 layout.
- [x] `cargo test --workspace` passes.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes.

## Follow-up: Double chest UV orientation (2026-10-24)

**Description:** Fix large chest halves showing their outer border at the
middle seam. The entity box unwrap now follows `ModelPart.Cube` UV
orientation.

- [x] Front, back, side, and lid-top UVs match vanilla entity model unwrap.
- [x] Left/right halves keep their outer borders and meet at the middle.
- [x] Regression tests assert the front and lid-top UV corners.
- [x] `textured-materials.png` golden regenerated.

**Verification:**

- [x] `cargo test -p litematic-material`
- [x] `cargo test -p litematic-render --test golden_textures`
- [x] `powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/check.ps1`

## Follow-up: Chest lock and piston face UVs (2026-10-24)

**Description:** Restore the large chest center lock and fix rotated face UVs
used by piston models.

- [x] Double chest halves include the inner lock slivers from `ChestModel`.
- [x] JSON face rotation permutes UV corners instead of rotating coordinates.
- [x] Regression tests cover the center lock and 90/270 degree face rotations.
- [x] Piston bases, rods, and heads sample the correct texture strips.

**Verification:**

- [x] `cargo test -p litematic-material`
- [x] `cargo test -p litematic-render --test golden_textures`
- [x] `powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/check.ps1`
