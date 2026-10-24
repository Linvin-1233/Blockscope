# Phase 5 Plan: Huge Structures

**Status:** Planned. Phase 4 shipped (snapshot, capacity, serve, supervise,
timeouts). This plan covers the extreme fixtures.

## Measured baseline

`000001.litematic`, 211x211x211, 9.39M blocks, 211 cubed:

| Metric | Value |
|---|---|
| Mesh wall time | 44.8 s (single-threaded) |
| Peak working set | 978 MB |
| Outcome | `MESH_LIMIT_EXCEEDED` at 5,000,000 quads |

The other extreme fixtures are 5 to 7 times larger in volume, so default limits
reject them in minutes with multi-gigabyte memory. `all_ones_nbt_10GB.litematic`
is truncated and fails in 77 ms at the NBT layer.

## Goals

1. Make oversized schematics renderable on a machine with enough memory, with
   predictable time and memory rather than an arbitrary quad cap.
2. Keep the default safe profile: small machines reject huge inputs with a
   stable error unless `--force-limits` is passed.
3. Preserve byte-identical output for scenes that already fit the limits.

## Non-goals

- Rendering 20M+ quads on a 1 CPU / 2 GiB VPS. That needs level-of-detail and
  is covered by slice C3 only if required.
- Streaming to disk or progressive output.

## Slices

### C1: Parallel mesh building

Partition the scene into 16x16x16 pages. Each worker owns whole pages, builds
their quads in local y/z/x order, and returns them. The main thread merges pages
in `BTreeMap` key order, so output is byte-identical to the serial builder.

- `BlockCatalog` must be `Sync`. `MaterialSnapshot` already is; add a
  `ColorSnapshot` for the no-assets path so it stops using the `RefCell` cache.
- Owned-cell resolution replaces the per-region loop; a cell is visited once by
  whichever region owns it, matching current `blocks_visited`.
- `--threads` applies to mesh building as well as rendering; `0` uses all cores.
- Determinism test: parallel and serial meshes are equal for a scene with
  multiple pages and overlapping regions.

**Acceptance:** `000001` mesh time drops from 44.8 s to under 8 s on 12 threads;
existing mesh and golden manifest tests stay green.

### C2: Lower the render memory per quad

The triangle table costs about 190 bytes per triangle and duplicates the mesh.
Replace it with either compact triangles or direct quad rasterization:

- Precompute one projected quad (4 screen points + depth planes) per quad,
  split into two triangles during rasterization.
- Bin quad indices per tile instead of triangles.
- Keep colors and UVs in the mesh, not in a second table.

**Acceptance:** peak memory per quad drops by at least 30 percent at the same
output bytes; render time regresses by no more than 10 percent.

### C3: Optional level of detail

For inputs beyond the memory budget, `--lod 2` or `--lod 4` merges cells before
meshing:

- Aggregate rule: keep the highest-priority non-air state per LOD cell
  (opaque before cutout before translucent), majority wins ties.
- LOD is off by default and changes output; it is an explicit opt-in for
  extreme schematics on small machines.

**Acceptance:** `000003` renders at LOD 4 within the 2 GiB budget; document the
approximation.

### C4: Capacity-aware admission

- `renderd capacity` reports `suggestedMaxQuads` from available memory.
- The engine estimates quads after meshing; if the estimate exceeds the budget
  and `--force-limits` is absent, fail with `MEMORY_LIMIT` before allocating
  the render buffers.
- Supervisor hello exposes the budget.

**Acceptance:** a 2 GiB worker rejects `000001` with `MEMORY_LIMIT` and no OOM;
the same job succeeds with `--force-limits` on a larger machine.

## Order

C1, C2, C3, C4. C1 is the largest single win and has no output change. C2 is
required before C3 makes sense. C3 is conditional on the target machine and on
whether approximated output is acceptable.

## Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Parallel mesh changes quad order | High | Page merge in key order; byte-compare goldens |
| `BlockCatalog` not `Sync` | Medium | Snapshot both catalogs before workers start |
| Memory per quad still too high | High | C2 before claiming huge support |
| LOD changes visual output | Medium | Opt-in flag, documented |
| Time budget on 1-core | High | C4 rejects early instead of swapping |

## Open Questions

- What is the target machine for the extreme fixtures (RAM, cores)?
- Is approximated output (LOD) acceptable for huge schematics, or must every
  block be exact?
- What is the acceptable wall time for a huge render: seconds, a minute, more?
