# Implementation Plan: Phase 4 - Shared Materials and Multi-Process Engine

**Status:** M1-M5 and P1-P4 implemented; P5 pending.
**Baseline:** Phase 3C plus the in-flight chest/piston work (verified with
`scripts/check.ps1`, exit 0).
**Related:** `tasks/phase-4-engine-todo.md`

## Goal

Make the material pipeline safe to share across concurrent work, then expose the
renderer as a long-lived, multi-process engine:

1. Replace the `RefCell`-based material caches with a read-only snapshot that any
   thread can share without locking the hot path.
2. Run N engine processes behind a supervisor, each with its own material state,
   so a crash is isolated and independent jobs can run in parallel.

## Why a snapshot instead of swapping `RefCell` for `RwLock`

`build_mesh` calls `BlockCatalog::resolve` once per block and, through face
culling and ambient occlusion, up to 18 times per quad. The current 41k-block
fixture produces 313k quads, so resolution runs in the millions of calls per
mesh. A lock per call would add tens to hundreds of milliseconds and could
regress mesh building by an order of magnitude.

A `RefCell` to `RwLock` swap makes the type `Sync` but keeps the hot path inside
a lock. The plan therefore separates construction from use:

```text
MaterialSet (single-threaded, may keep RefCell)
  -> resolve every state used by the scene
  -> MaterialSnapshot (immutable, Send + Sync)
       - HashMap<BlockStateKey, Arc<ResolvedBlock>>
       - TextureSet
       - diagnostics and per-state info
  -> shared as Arc<MaterialSnapshot>
  -> build_mesh reads it with zero locking
```

Locking the builder remains an option only if incremental warmup across requests
is needed later.

## Architecture Decisions

- **Snapshot over lock swap.** The render and mesh hot paths stay lock-free;
  `RefCell` remains acceptable inside the single-threaded builder.
- **`BlockCatalog` stays the mesh interface.** `MaterialSnapshot` implements the
  existing trait, so `litematic-mesh` needs no API change.
- **Textures are owned by the snapshot.** `render_mesh_with_textures` keeps
  taking `&TextureSet`, sourced from the snapshot, so rendering stays read-only.
- **Process pool size is bounded by cores.** One job already uses every core by
  default, so `processes x threads <= physical cores`; two workers with six
  threads each is the reference configuration.
- **Sticky routing by `inputHash`.** Processes do not share caches, so routing
  the same input to the same worker preserves mesh and snapshot hits.
- **Results travel by path, not by pipe.** Encoded images land in a shared temp
  directory; the protocol returns paths plus statistics.

## Baseline Reference

Measured before the chest/piston changes on the 41k-block fixture, release build:

| Case | Total | Render | Mesh | Material load | PNG |
|---|---|---|---|---|---|
| 1024x1024, ss2 | ~380 ms | 204 ms | 146 ms | ~185 ms | 5 ms |
| 2048x2048, ss2 | ~1.49 s | 1100 ms | 157 ms | ~185 ms | 23 ms |
| 4096x4096, ss1 | ~1.51 s | 1053 ms | 157 ms | ~185 ms | 83 ms |

`crates/render/src/camera.rs` and `options.rs` changed in the chest/piston work,
so these numbers must be re-measured before they are used as a regression gate.

## Slices

### Slice 1: Material snapshot

- New `MaterialSnapshot` type in `litematic-material`.
- `MaterialSet::snapshot(states)` pre-resolves a set of block states.
- Snapshot implements `BlockCatalog`; resolution is a hash lookup.
- Snapshot owns the `TextureSet` and diagnostics.

### Slice 2: Pipeline integration

- CLI warms up materials and builds a snapshot per scene.
- `render` and `materials` use the snapshot instead of the live set.
- No change to public mesh or render signatures.

### Slice 3: Concurrency proof

- Shared `Arc<MaterialSnapshot>` across 1, 4, and available-parallelism threads.
- Same mesh statistics, same render statistics, byte-identical PNG.

### Slice 4: Engine worker

- Long-lived `renderd serve` mode with warm materials.
- One job at a time per process; rasterizer threads configurable.
- Request/response protocol documented before implementation.

### Slice 5: Supervisor and pool

- Process pool with bounded size and health checks.
- Sticky routing by `inputHash`.
- Timeout, kill, restart, and zombie reaping.
- Shared temp directory for outputs.

## Phase 4 Measurements

Shared snapshot, release build, 41k-block fixture, 1024x1024 at supersample 2:

| Run | Mesh (snapshot + build) | Render median | Triangles |
|---|---|---|---|
| Baseline (live set) | 146 ms | 204 ms | 332,710 |
| Snapshot run 1 | 151.7 ms | 207.5 ms | 359,110 |
| Snapshot run 2 | 148.5 ms | 207.2 ms | 359,110 |
| Snapshot run 3 | 145.9 ms | 237.0 ms | 359,110 |

Triangle count grew 8 percent because of the chest and piston models. Mesh cost
per quad is unchanged, and render time stays inside run-to-run noise. A
1024x1024 supersample-1 render of the real fixture is byte-identical before and
after the pipeline switch (SHA256 `9CAFA199...`).

### Serve mode

Two jobs in one worker with the real fixture, 1024x1024 at supersample 2:

| Job | Total | Parse | Mesh | Render | PNG |
|---|---|---|---|---|---|
| `two-1` | 412 ms | 22 ms | 147 ms | 222 ms | 14 ms |
| `two-2` | 394 ms | 22 ms | 131 ms | 226 ms | 6 ms |

No material reload between jobs. `renderd capacity --json` on this host reports
12 cores, 4 suggested workers, 3 threads, and a 2048 supersample-2 default tier.

### Linux container verification

WSL2 Ubuntu 24.04, release build, inside a systemd scope with
`MemoryMax=1G` and `CPUQuota=100%`:

```json
{"platform":"linux","cores":1,"cpuQuotaMilli":1000,
 "memoryLimitBytes":1073741824,"memoryAvailableBytes":1073233920,
 "suggestedWorkers":1,"suggestedThreads":1,
 "defaultWidth":1024,"defaultHeight":1024,"defaultSupersample":1}
```

The cgroup v2 limit and CPU quota are picked up without configuration, and the
pool collapses to one worker on one thread as intended.

### Supervisor

Two workers, three threads each, real fixture, 1024x1024 at supersample 2:

| Run | Wall | Per job | Cache |
|---|---|---|---|
| Two concurrent jobs | 1.15 s | 652 ms each | miss |
| Sequential repeat | - | skips parse and mesh | hit (`cacheHit: true`) |

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Uncommitted chest/piston work mixed with refactor | High | Commit the verified baseline before task M1 |
| Lock inside the resolve hot path | High | Snapshot; perf check in M5 is a hard gate |
| Golden PNG drift | High | Byte-compare `fixtures/golden/render/` after every task |
| Oversubscribed threads across processes | Medium | Enforce `processes x threads <= cores` in config validation |
| Memory growth per process | Medium | Bounded LRU, explicit `max_cached_meshes`, measure RSS |
| Windows/Linux process differences | Medium | Long-lived pool, no spawn-per-job; confirm deploy target |
| Worker crash loses a job | Medium | Supervisor retries once, then reports a failure |

## Open Questions

- What is the target concurrent render count?
- Is multi-process for crash isolation, throughput, or both?
- Is incremental warmup across requests required, or is a per-scene snapshot
  enough?

## Decisions

- **Platforms:** Linux and Windows, both supported from the start.
- **Transport:** newline-delimited JSON over stdio (`docs/engine-protocol.md`).
  No sockets in v1, so there is no named-pipe/Unix-socket split.
- **Shutdown:** stdin EOF or a `shutdown` message, with a platform-specific
  forced kill only after a grace period.
