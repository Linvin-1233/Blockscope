# Phase 4 Task List - Shared Materials and Multi-Process Engine

**Status:** Planned.
**Plan:** `tasks/phase-4-engine-plan.md`

## Task M1: Snapshot type and builder

**Description:** Add an immutable, shareable material snapshot that pre-resolves
the block states used by a scene.

**Acceptance criteria:**

- [x] `MaterialSnapshot` holds resolved blocks, textures, info, and diagnostics.
- [x] `MaterialSet::snapshot` resolves every distinct non-air state once.
- [x] `MaterialSnapshot::resolve` performs no writes and needs no lock.
- [x] Snapshot results match `MaterialSet::resolve` for every fixture state.
- [x] `cargo test -p litematic-material` passes.

**Verification:**

- [x] New unit test compares snapshot and live resolution.
- [ ] `materials` report numbers are unchanged on the real fixture (moved to M3).

**Dependencies:** Baseline commit
**Files likely touched:** `crates/material/src/catalog.rs`, `crates/material/src/lib.rs`
**Estimated scope:** Medium

## Task M2: Snapshot implements the mesh catalog

**Description:** Let the mesh builder consume the snapshot through the existing
`BlockCatalog` trait.

**Acceptance criteria:**

- [x] `MaterialSnapshot` implements `BlockCatalog`.
- [x] `build_mesh` produces identical quads for snapshot and live set.
- [x] No change to `litematic-mesh` public signatures.
- [x] Mesh tests and golden manifests pass.

**Verification:**

- [x] `cargo test -p litematic-mesh -p litematic-core`
- [x] Snapshot and live catalogs build an identical `Mesh` for the same scene.

**Dependencies:** M1
**Files likely touched:** `crates/material/src/catalog.rs`, `crates/mesh/tests/*`
**Estimated scope:** Small

## Task M3: CLI pipeline uses the snapshot

**Description:** Warm up materials, build one snapshot per scene, then mesh and
render from it.

**Acceptance criteria:**

- [x] `render` and `bench` use the snapshot path.
- [ ] `materials` reports from the live set (single-threaded diagnostic path).
- [x] Golden PNGs remain byte-identical.
- [x] CLI JSON output fields and values are unchanged.

**Verification:**

- [x] `cargo test -p blockscope-cli`
- [x] `cargo test -p litematic-render --test golden_png --test golden_textures`
- [x] `git diff --stat fixtures/golden` is empty after the run.
- [x] Real fixture render is byte-identical before and after (SHA256 match).

**Dependencies:** M2
**Files likely touched:** `crates/cli/src/main.rs`
**Estimated scope:** Small

## Task M4: Shared snapshot concurrency test

**Description:** Prove that one snapshot can serve several threads at once.

**Acceptance criteria:**

- [x] One `Arc<MaterialSnapshot>` shared by 1, 4, and all-core threads.
- [x] Every thread builds a mesh from the same scene.
- [x] Mesh statistics and rendered PNG bytes are identical across thread counts.
- [x] Test runs under the normal `cargo test` suite.

**Verification:**

- [x] New tests in `crates/material/tests/snapshot.rs` and
      `crates/render/tests/concurrent.rs`.
- [x] Sharing the live `MaterialSet` instead fails at compile time (not `Sync`).

**Dependencies:** M3
**Files likely touched:** `crates/material/tests/snapshot.rs`,
`crates/render/tests/concurrent.rs`
**Estimated scope:** Small

## Task M5: Performance guard

**Description:** Confirm the snapshot does not slow the pipeline and record the
numbers as the new baseline.

**Acceptance criteria:**

- [x] Mesh building regresses by no more than 5 percent on the real fixture.
- [x] Render time is unchanged within noise.
- [x] Results recorded in the plan document.

**Verification:**

- [x] `cargo run -p blockscope-cli --release -- bench ... --iterations 5`
- [x] Before and after numbers in the task notes: mesh 146 ms before, 146-152 ms
      after (with 8 percent more quads); render 204 ms before, 207-237 ms after.

**Dependencies:** M4
**Files likely touched:** `tasks/phase-4-engine-plan.md`
**Estimated scope:** Extra small

## Checkpoint: Shared materials

- [ ] `scripts/check.ps1` exits 0.
- [ ] Golden files unchanged.
- [ ] Concurrency test green.
- [ ] Human review before starting the engine work.

## Task P1: Engine protocol document

**Description:** Define the worker and supervisor protocol before writing any
service code.

**Acceptance criteria:**

- [x] Request covers input path or hash, views, size, supersample, lighting,
      background, format, and job id. Job timeouts are supervisor-enforced.
- [x] Response covers output paths, scene hash, statistics, timings, and error
      codes.
- [x] Transport choice (stdio JSONL or local socket) is justified.
- [x] Document lives in `docs/engine-protocol.md`.

**Verification:**

- [x] Example request and response for a single job and a multi-view job.

**Dependencies:** None
**Files likely touched:** `docs/engine-protocol.md`
**Estimated scope:** Small

## Task P2: Single-job worker process

**Description:** Add a long-lived mode that loads materials once and processes
jobs sequentially.

**Acceptance criteria:**

- [x] First job after start has no repeated material load cost.
- [x] Job failures return a structured error and keep the process alive.
- [x] Rasterizer threads are configurable per process and per request.
- [x] Closing stdin or sending `shutdown` ends the process with `bye` and exit 0.
- [ ] SIGINT is left to the supervisor kill path (no custom handler in v1).

**Verification:**

- [x] Manual run: two jobs in one process took 412 ms and 394 ms with no reload.
- [x] `cargo test -p blockscope-cli` passes, including protocol round-trip tests.

**Dependencies:** P1, M3
**Files likely touched:** `crates/cli/src/main.rs`, `crates/cli/src/serve.rs`
**Estimated scope:** Medium

## Task P3: Supervisor and sticky pool

**Description:** Start a bounded pool of workers and route jobs to them.

**Acceptance criteria:**

- [x] Pool size and per-worker threads default to detected capacity.
- [x] Oversized `workers x threads` prints a warning instead of failing.
- [x] Routing by `inputHash` keeps repeated inputs on the cached worker.
- [x] Queue applies backpressure with a `BUSY` result instead of unbounded growth.
- [x] Outputs are written to the requested directory and returned by path.

**Verification:**

- [x] Two concurrent real-fixture jobs: 1.15 s wall, 652 ms each.
- [x] Sequential repeats report `cacheHit: true` and `meshMs: 0`.
- [ ] p50 and p95 under sustained load (moved to P5).

**Dependencies:** P2
**Files likely touched:** new supervisor module or Go service
**Estimated scope:** Medium

## Task P4: Failure handling

**Description:** Keep the pool healthy when a worker dies or stalls.

**Acceptance criteria:**

- [x] Per-job timeout kills and restarts the worker.
- [x] A killed worker's job is retried once, then fails with `TIMEOUT`,
      `MEMORY_LIMIT`, or `INTERNAL_ERROR`.
- [x] Killed workers are reaped with `wait`; no zombies accumulate.
- [x] Memory ceiling per worker is documented and enforced (default 85 percent
      of available memory divided by the pool size).

**Verification:**

- [x] Kill a worker mid-job and confirm the pool recovers (timeout test).
- [x] Memory ceiling test kills an over-budget worker and reports
      `MEMORY_LIMIT`.
- [ ] 30 minute soak with stable RSS (moved to P5).

**Dependencies:** P3
**Files likely touched:** supervisor module
**Estimated scope:** Medium

## Task P5: Capacity report

**Description:** Record measured capacity for the two reference topologies.

**Acceptance criteria:**

- [ ] 1 process x 12 threads and 2 processes x 6 threads measured.
- [ ] p50, p95, images per second, and peak RSS reported.
- [ ] Recommendation written into the plan document.

**Verification:**

- [ ] Repeatable script or documented command sequence.

**Dependencies:** P4
**Files likely touched:** `tasks/phase-4-engine-plan.md`
**Estimated scope:** Extra small

## Checkpoint: Engine

- [ ] Pool survives a worker kill.
- [ ] Peak RSS within budget.
- [ ] Capacity numbers documented.
- [ ] Human review before deployment work.
