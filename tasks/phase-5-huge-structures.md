# Phase 5 Plan: Huge Structures

**Status:** Planned. Phase 4 shipped (snapshot, capacity, serve, supervise,
timeouts). This plan covers the extreme fixtures.

## Measured baseline

`000001.litematic`, 211x211x211, 9.39M blocks, 211 cubed:

| Metric | Value |
|---|---|
| Mesh wall time | 44.8 s (single-threaded, aborted at 5M quads) |
| Peak working set | 978 MB at the abort |
| Outcome | `MESH_LIMIT_EXCEEDED` at 5,000,000 quads |

### After C1 (parallel meshing, 12 threads, `--force-limits`)

| Metric | Value |
|---|---|
| Wall time | 27.1 s |
| Peak working set | 13,493 MB |
| Quads | 77,578,844 |
| Vertices | 310,315,376 |
| Pages | 2,744 |
| Blocks visited | 9,393,931 |
| Unknown blocks (no palette color) | 3,268,723 |

The fixture is pathological: its 9.4M blocks emit 77.6M quads, roughly 8 quads
per block, so face culling barely helps. The mesh alone needs about 13 GB, and
a triangle table would add another 20 to 30 GB.

Consequences:

- The default 5M cap is the correct behaviour on small machines; this file is
  far outside it.
- Even a 32 GB host is marginal without level of detail.
- 77.6M quads at roughly 0.5 to 1 second per million triangles cannot meet a
  10 minute budget without LOD, regardless of mesh parallelization.

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

### C1: Parallel mesh building (done)

Partition the scene into 16x16x16 pages. Each worker owns whole pages, builds
their quads in local y/z/x order, and returns them. The main thread merges pages
in `BTreeMap` key order, so output is byte-identical to the serial builder.

- `BlockCatalog` must be `Sync`; `MaterialSnapshot` and the new `ColorSnapshot`
  provide that.
- Owned-cell resolution replaces the per-region loop; a cell is visited once by
  whichever region owns it, matching current `blocks_visited`.
- `--threads` applies to mesh building as well as rendering; `0` uses all cores.
- Verified: a real-fixture render with `--threads 1` and `--threads 12` has the
  same SHA-256, and a multi-region multi-page test compares serial against 2, 4,
  and 8 threads.
- Measured: `000001` builds 77.6M quads in 27.1 s on 12 threads, about 2.9M
  quads/s. The original "under 8 s" target assumed a far smaller mesh; the
  fixture is 15 times over the default quad cap, so the useful metric is
  throughput and memory, not an absolute wall time.

### C2: Lower the render memory per quad

The triangle table costs about 190 bytes per triangle and duplicates the mesh.
Replace it with either compact triangles or direct quad rasterization:

- Precompute one projected quad (4 screen points + depth planes) per quad,
  split into two triangles during rasterization.
- Bin quad indices per tile instead of triangles.
- Keep colors and UVs in the mesh, not in a second table.

**Acceptance:** peak memory per quad drops by at least 30 percent at the same
output bytes; render time regresses by no more than 10 percent.

### C3: Level of detail (required for the extreme fixtures)

`000001` emits 77.6M quads, so exact rendering needs more than 13 GB before the
triangle table. `--lod 2` or `--lod 4` merges cells before meshing:

- Aggregate rule: keep the highest-priority non-air state per LOD cell
  (opaque before cutout before translucent), majority wins ties.
- LOD is off by default and changes output; it is an explicit opt-in for
  extreme schematics.
- Target: LOD 4 should cut `000001` to well under 5M quads and fit in a few GB.

**Acceptance:** `000001` renders at LOD 4 within 2 GB of mesh memory and inside
the 10 minute budget.

### C4: Capacity-aware admission

- `renderd capacity` reports `suggestedMaxQuads` from available memory.
- The engine estimates quads after meshing; if the estimate exceeds the budget
  and `--force-limits` is absent, fail with `MEMORY_LIMIT` before allocating
  the render buffers.
- Supervisor hello exposes the budget.

**Acceptance:** a 2 GiB worker rejects `000001` with `MEMORY_LIMIT` and no OOM;
the same job succeeds with `--force-limits` and `--lod` on a larger machine.

## Order

C1 is done. Next C2, then C3, then C4. C3 is no longer optional: `000001` cannot
meet the 10 minute budget exactly on any realistic host.

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
- What minimum LOD quality is acceptable: block majority, or "any non-air wins"?
- Should LOD be allowed automatically when a job would exceed the memory
  budget, or always be explicit?

## Answers so far

- Extreme jobs may use all available device resources.
- LOD is allowed.
- The wall-clock budget for one huge image is 10 minutes.
