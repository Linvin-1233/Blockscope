# Phase 5 Plan: Huge Structures

**Status:** C1-C4 implemented. This plan covers the extreme fixtures.

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

### C2: Lower the render memory per quad (implemented)

The triangle table cost about 190 bytes per triangle and duplicated the mesh.
The rasterizer now projects each opaque or cutout quad once into a compact table
(about 200 bytes per quad), forms the two triangles while rasterizing a tile,
and keeps only translucent triangles separately for depth sorting.

Measured on `000002` at 1024x1024 and supersample 2:

| Metric | Before | After |
|---|---|---|
| Peak working set | 19,558 MB | **13,514 MB** |
| Wall time | 38.7 s | 32.4 s |
| Render time | 13.5 s | 6.6 s |
| PNG SHA-256 | `E861C6...` | `E861C6...` (identical) |

`000001` exact is estimated at roughly 29 GB (13.5 GB mesh plus 15.5 GB quad
table), still more free memory than the 48 GB test host had available; `--lod
auto` keeps choosing LOD 2.

### C3: Level of detail (implemented)

`000001` emits 77.6M quads, so exact rendering needs more than 13 GB before the
triangle table. `litematic_scene::aggregate` merges cells in a pure data layer,
and `--lod N` (2..=16) applies it after parsing:

- Each cell keeps the most frequent non-air state; ties use canonical order.
- Later regions win overlaps, matching the mesh builder.
- Output is an ordinary `Scene`, so the mesh builder and renderer are unchanged.
- Block entities, entities, and pending ticks are dropped.
- Off by default; opt-in for extreme schematics.

Wiring: `render`, `bench`, and `mesh` take `--lod`; the serve protocol carries
`lod` and keys its cache by `(inputHash, lod)`; `sceneHash` in the result is the
aggregated scene.

Measured with the client jar at 1024x1024, supersample 2, 12 threads:

| LOD | Quads | Triangles | Wall | Peak working set |
|---|---|---|---|---|
| 1 | 77.6M | 117M | 37.2 s | 23.1 GB |
| 2 | 10.5M | 14.6M | 9.4 s | 5.5 GB |
| 4 | 1.3M | 1.55M | 4.0 s | 683 MB |

With page streaming, exact `000001` now completes in 37.2 s and 23.1 GB, within
the 10 minute budget on a 48 GB host. LOD remains necessary for smaller hosts.

LOD 4 meets the 10 minute budget with a large margin. LOD 2 is viable on a
16 GB host but not on a small VPS. The render for LOD 4 reports 18 missing
models and 4 missing textures out of 9,190 resolved states; no magenta blocks
appear in the output.

### C4: Capacity-aware admission (implemented)

- `renderd capacity` and both `hello` messages report `suggestedMaxQuads`
  (80 percent of available memory at roughly 400 bytes per quad).
- The default mesh cap is the lower of 5,000,000 quads and that budget, so a
  small container rejects oversized schematics before allocating render
  buffers. `--force-limits` remains the explicit override.
- `--lod auto` uses the same budget to choose a factor.

Measured on the test host: 27 GB free produced a 54.3M quad budget. `000002`
exact (19.9M quads) is now chosen automatically; `000001` (77.6M quads) falls
back to LOD 2, and `000001` exact succeeds when forced.

**Acceptance:** a 2 GiB worker rejects `000001` with the mesh cap and no OOM;
the same job succeeds with `--force-limits` and `--lod` on a larger machine.

## Order

C1, C2, C3, and C4 are implemented. Exact rendering covers scenes that fit the
budget, streaming keeps memory near the mesh size, and LOD covers the rest.

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
