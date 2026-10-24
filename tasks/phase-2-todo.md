# Phase 2 Task List 鈥?Color-Block Geometry

**Status:** Active  
**Plan:** `tasks/phase-2-plan.md`

## Task P2-1: Mesh types, limits, and errors

**Description:** Define the renderer-facing mesh contract before adding
geometry.

**Acceptance criteria:**

- [x] `Rgba` and `RenderLayer` (`opaque`, `cutout`, `translucent`) exist.
- [x] `Vertex` carries position, normal, RGBA color, and UV.
- [x] `Quad` carries four vertices and a render layer.
- [x] `MeshPage` carries a page origin and quads.
- [x] `Mesh` carries sorted pages and statistics.
- [x] `MeshLimits` covers pages, quads, and vertices.
- [x] Stable `MeshErrorCode` and `MeshError` exist.

**Verification:**

- [x] `cargo test -p litematic-mesh` passes.
- [x] Size/ordering tests cover page and quad contracts.

**Dependencies:** None  
**Estimated scope:** Medium

## Task P2-2: Color palette and catalog

**Description:** Resolve a block state to an original RGBA color, a render
layer, and a full-cube model.

**Acceptance criteria:**

- [x] Embedded JSON palette covers common vanilla blocks.
- [x] Family heuristics cover wool, concrete, terracotta, glass, planks, logs,
      leaves, and stairs/slab suffixes.
- [x] Unknown states use a magenta fallback and `known = false`.
- [x] Layer overrides and keyword heuristics work.
- [x] `BlockCatalog` is a trait with `ColorBlockCatalog` as the default.
- [x] `ColorBlockCatalog::from_json_str` supports tests and future packs.

**Verification:**

- [x] Exact, family, and unknown lookups are tested.
- [x] Opaque/cutout/translucent classification is tested.
- [x] Air is not resolved as a solid full cube.

**Dependencies:** P2-1  
**Estimated scope:** Medium

## Task P2-3: Scene block view

**Description:** Resolve world positions across overlapping regions and index
regions by 16^3 page.

**Acceptance criteria:**

- [x] View builds a page -> region index map.
- [x] `resolve` returns the winning state and region index.
- [x] Later region order wins, matching Phase 1.
- [x] Local index calculation matches x-fastest, z-then-y order.
- [x] Out-of-scene positions return `None`.
- [x] Page boundaries are handled with `div_euclid`/`rem_euclid`.

**Verification:**

- [x] Single-region and overlap tests pass.
- [x] Negative coordinates and page boundaries are covered.
- [x] Neighbor lookups return the expected region.

**Dependencies:** P2-1  
**Estimated scope:** Medium

## Task P2-4: Full-cube mesh builder

**Description:** Emit page-local quads for full cubes with face culling and
limits.

**Acceptance criteria:**

- [x] Air blocks are skipped.
- [x] Full-cube faces have correct normals and CCW winding.
- [x] Face culling follows ADR-205.
- [x] Page-local vertex positions are in `0..16`.
- [x] Overlapping regions emit only the winning block.
- [x] `MeshStatistics` counts visited/rendered blocks, quads, vertices, pages,
      layers, and unknown blocks.
- [x] `MeshLimits` is enforced during build.

**Verification:**

- [x] One cube -> 6 quads.
- [x] Two adjacent stone cubes -> 10 quads.
- [x] Stone + glass -> 11 quads.
- [x] Two glass cubes -> 10 quads.
- [x] Page-boundary cubes are in separate pages.
- [x] Quad limit test returns `MESH_LIMIT_EXCEEDED`.

**Dependencies:** P2-2, P2-3  
**Estimated scope:** Medium

## Task P2-5: OBJ and PLY exporters

**Description:** Export the mesh for manual inspection.

**Acceptance criteria:**

- [x] OBJ writes page-origin-adjusted vertices and quad faces.
- [x] PLY writes ASCII vertices with RGBA colors and quad faces.
- [x] Output order is deterministic.
- [x] Export does not mutate the mesh.
- [x] Empty mesh exports a valid header.

**Verification:**

- [x] OBJ/PLY snapshot tests pass.
- [x] Re-export produces identical bytes.
- [x] Vertex and face counts match mesh statistics.

**Dependencies:** P2-4  
**Estimated scope:** Small

## Task P2-6: `renderd mesh` CLI

**Description:** Add a mesh command for statistics and export.

**Acceptance criteria:**

- [x] `renderd mesh <file>` prints mesh statistics.
- [x] `--json` prints statistics as JSON.
- [x] `--obj <path>` writes OBJ.
- [x] `--ply <path>` writes PLY.
- [x] Invalid input and mesh limits return exit code 2.
- [x] `--help` documents the command.

**Verification:**

- [x] CLI tests cover stats, JSON, OBJ, and PLY.
- [x] Missing/invalid file errors are stable.
- [x] `cargo run -p renderd-cli -- mesh --help` works.

**Dependencies:** P2-4, P2-5  
**Estimated scope:** Medium

## Task P2-7: Phase 2A tests and documentation

**Description:** Consolidate tests, golden stats, and geometry documentation.

**Acceptance criteria:**

- [x] `docs/mesh-geometry.md` documents coordinates, pages, winding, culling,
      color fallback, layers, limits, and deferred features.
- [x] README links the mesh command and document.
- [x] Windows and Linux checks pass.
- [x] No Minecraft assets, resource packs, or private fixtures are committed.
- [x] Phase 2A checkpoint reviewed by a human.

**Verification:**

- [x] `cargo fmt --all --check` passes.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [x] `cargo test --workspace` passes on Windows and WSL.
- [x] `renderd mesh` demo succeeds on a synthetic fixture.

**Dependencies:** P2-4, P2-6  
**Estimated scope:** Medium

## Task P2-8: Box shapes and voxel mesher (Phase 2B)

**Description:** Add box-based partial models and a cached 16^3 voxelization
pass with greedy face merging.

**Acceptance criteria:**

- [x] `ModelBox`/`ModelPart` representation exists.
- [x] Voxelizer removes internal faces and merges coplanar cells.
- [x] Model cache is keyed by canonical block state.
- [x] Slab and stairs produce the expected visible faces.
- [x] Full cubes still use the fast path.

**Dependencies:** P2-4  
**Estimated scope:** Large

## Task P2-9: Shape families and properties (Phase 2B)

**Description:** Implement fence, wall, pane, door, trapdoor, carpet, snow,
button, plate, torch, ladder, rail, and cross-plant geometry.

**Acceptance criteria:**

- [x] Block properties drive connection and orientation.
- [x] Each family has bounds/face-count tests.
- [x] Cutout and translucent layers are respected.
- [x] Unknown properties fall back safely with diagnostics.

**Dependencies:** P2-8  
**Estimated scope:** Large

## Task P2-10: Fluids and static block entities (Phase 2C)

**Description:** Add simple fluid height/flow and static block-entity boxes.

**Acceptance criteria:**

- [x] Water/lava source and flowing heights are approximated.
- [x] Chest/sign/banner/skull/lectern/beacon/conduit boxes are supported.
- [x] Block-entity text/icons are optional and documented.
- [x] Resource-pack catalog remains a separate future module.

**Dependencies:** P2-9  
**Estimated scope:** Large
