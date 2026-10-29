# Phase 5 Plan: Huge Structures

**Status:** C1-C2 and C4-C6 implemented. C3 (LOD) was removed by issue #19.
This plan covers the extreme fixtures.

## Measured baseline

`000001.litematic`, 211x211x211, 9.39M blocks, 211 cubed:

| Metric           | Value                                         |
| ---------------- | --------------------------------------------- |
| Mesh wall time   | 44.8 s (single-threaded, aborted at 5M quads) |
| Peak working set | 978 MB at the abort                           |
| Outcome          | `MESH_LIMIT_EXCEEDED` at 5,000,000 quads      |

### After C1 (parallel meshing, 12 threads, `--force-limits`)

| Metric                            | Value       |
| --------------------------------- | ----------- |
| Wall time                         | 27.1 s      |
| Peak working set                  | 13,493 MB   |
| Quads                             | 77,578,844  |
| Vertices                          | 310,315,376 |
| Pages                             | 2,744       |
| Blocks visited                    | 9,393,931   |
| Unknown blocks (no palette color) | 3,268,723   |

The fixture is pathological: its 9.4M blocks emit 77.6M quads with the
color-only catalog, roughly 8 quads per block, so face culling barely helps. The
client-jar material set used for the rendering measurements emits 118.6M quads
(474M vertices), so the mesh alone needs about 21 GB before the render tables.

Consequences:

- The default 5M cap is the correct behaviour on small machines; this file is
  far outside it.
- Even a 32 GB host is marginal without reduced resolution or streaming.
- 77.6M quads at roughly 0.5 to 1 second per million triangles cannot meet a
  10 minute budget without reducing the workload, regardless of mesh
  parallelization.

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
(about 200 bytes per quad at the time; C5 later reduced it to 128), forms the two
triangles while rasterizing a tile, and keeps only translucent triangles
separately for depth sorting.

Measured on `000002` at 1024x1024 and supersample 2:

| Metric           | Before      | After                   |
| ---------------- | ----------- | ----------------------- |
| Peak working set | 19,558 MB   | **13,514 MB**           |
| Wall time        | 38.7 s      | 32.4 s                  |
| Render time      | 13.5 s      | 6.6 s                   |
| PNG SHA-256      | `E861C6...` | `E861C6...` (identical) |

`000001` exact is estimated at roughly 29 GB (13.5 GB mesh plus 15.5 GB quad
table), still more free memory than the 48 GB test host had available. Streaming
and resolution controls are the remaining options for hosts below that.

### C4: Capacity-aware admission (implemented)

- `blockscope capacity` and both `hello` messages report `suggestedMaxQuads`
  (80 percent of available memory at roughly 400 bytes per quad).
- The default mesh cap is the lower of 5,000,000 quads and that budget, so a
  small container rejects oversized schematics before allocating render
  buffers. `--force-limits` remains the explicit override.

Measured on the test host: 27 GB free produced a 54.3M quad budget. `000002`
exact (19.9M quads) fits; `000001` (118.6M quads with the client jar) is
rejected unless `--force-limits` is used.

**Acceptance:** a 2 GiB worker rejects `000001` with the mesh cap and no OOM;
the same job succeeds with `--force-limits` on a larger machine.

### C5: Compact attributes (implemented)

- Mesh UVs are Q12 fixed point (`i16`) and ambient occlusion is a 2-bit level.
  Both reconstruct the exact previous float values on the measured fixtures.
- `Vertex` drops from 44 to 36 bytes and `Quad` from 180 to 148 bytes.
- The projected quad table stores one RGBA color, one shade factor, four AO
  levels, and Q12 UVs per quad: 200 to 128 bytes. Translucent triangles drop
  from 160 to 112 bytes.
- Memory: `000001` exact 23.1 to 18.9 GB, `000002` exact 7.5 to 6.3 GB.
- PNG SHA-256 is unchanged for `000001` exact and `000002` exact.

### C6: Container NBT is ignored for geometry (implemented)

The renderer never reads block entity contents. `mesh`, `bench`, and `render`
now skip the `TileEntities` payload during NBT decoding; `inspect`, `validate`,
and `stats` keep it unless `--ignore-block-entity-nbt` is passed. The skip does
not allocate the payload. If the stream ends inside `TileEntities`, the block
data parsed before it is kept.

`test-No2/12345.litematic` is the `all_ones_nbt_10GB` stress file: a 115x1x1
region of barrels with written books, stored as 9,538 gzip members and
10,000,526,997 decompressed bytes. The block data is in the first member. With
the skip, it parses in 6 ms and renders 462 triangles (115 barrels) in 0.32 s
with a 22 MB peak, instead of failing with `TRUNCATED_INPUT`.

## Order

C1, C2, C3, C4, C5, and C6 are implemented. Exact rendering covers scenes that
fit the budget, streaming keeps memory near the mesh size, compact attributes
shrink that mesh, and container NBT is skipped for geometry.

## Risks

| Risk                             | Impact | Mitigation                                    |
| -------------------------------- | ------ | --------------------------------------------- |
| Parallel mesh changes quad order | High   | Page merge in key order; byte-compare goldens |
| `BlockCatalog` not `Sync`        | Medium | Snapshot both catalogs before workers start   |
| Memory per quad still too high   | High   | C2 before claiming huge support               |
| Time budget on 1-core            | High   | C4 rejects early instead of swapping          |

## Open Questions

- What is the target machine for the extreme fixtures (RAM, cores)?

## Answers so far

- Extreme jobs may use all available device resources.
- The wall-clock budget for one huge image is 10 minutes.
