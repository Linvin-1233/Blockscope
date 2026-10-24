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

- [ ] `render`, `materials`, and `bench` use the snapshot path.
- [ ] Golden PNGs remain byte-identical.
- [ ] CLI JSON output fields and values are unchanged.

**Verification:**

- [ ] `cargo test -p renderd-cli`
- [ ] `cargo test -p litematic-render --test golden_png --test golden_textures`
- [ ] `git diff --stat fixtures/golden` is empty after the run.

**Dependencies:** M2
**Files likely touched:** `crates/cli/src/main.rs`
**Estimated scope:** Small

## Task M4: Shared snapshot concurrency test

**Description:** Prove that one snapshot can serve several threads at once.

**Acceptance criteria:**

- [ ] One `Arc<MaterialSnapshot>` shared by 1, 4, and all-core threads.
- [ ] Every thread builds a mesh from the same scene.
- [ ] Mesh statistics and rendered PNG bytes are identical across thread counts.
- [ ] Test runs under the normal `cargo test` suite.

**Verification:**

- [ ] New test at `crates/material/tests/concurrency.rs`.
- [ ] Test fails if the snapshot is replaced with the live `MaterialSet`.

**Dependencies:** M3
**Files likely touched:** `crates/material/tests/concurrency.rs`
**Estimated scope:** Small

## Task M5: Performance guard

**Description:** Confirm the snapshot does not slow the pipeline and record the
numbers as the new baseline.

**Acceptance criteria:**

- [ ] Mesh building regresses by no more than 5 percent on the real fixture.
- [ ] Render time is unchanged within noise.
- [ ] Results recorded in the plan document.

**Verification:**

- [ ] `cargo run -p renderd-cli --release -- bench ... --iterations 5`
- [ ] Before and after numbers in the task notes.

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

- [ ] Request covers input path or hash, views, size, supersample, lighting,
      background, format, timeout, and job id.
- [ ] Response covers output paths, scene hash, statistics, timings, and error
      codes.
- [ ] Transport choice (stdio JSONL or local socket) is justified.
- [ ] Document lives in `docs/engine-protocol.md`.

**Verification:**

- [ ] Example request and response for a single job and a multi-view job.

**Dependencies:** None
**Files likely touched:** `docs/engine-protocol.md`
**Estimated scope:** Small

## Task P2: Single-job worker process

**Description:** Add a long-lived mode that loads materials once and processes
jobs sequentially.

**Acceptance criteria:**

- [ ] First job after start has no repeated material load cost.
- [ ] Job failures return a structured error and keep the process alive.
- [ ] Rasterizer threads are configurable per process.
- [ ] SIGINT or stdin close shuts down cleanly.

**Verification:**

- [ ] Manual run: two jobs in one process, second is faster.
- [ ] `cargo test -p renderd-cli` still passes.

**Dependencies:** P1, M3
**Files likely touched:** `crates/cli/src/main.rs`, `crates/cli/src/serve.rs`
**Estimated scope:** Medium

## Task P3: Supervisor and sticky pool

**Description:** Start a bounded pool of workers and route jobs to them.

**Acceptance criteria:**

- [ ] Pool size and per-worker threads are validated against core count.
- [ ] Routing by `inputHash` keeps repeated inputs on the same worker.
- [ ] Queue applies backpressure instead of unbounded growth.
- [ ] Outputs are written to a shared temp directory and returned by path.

**Verification:**

- [ ] Load test with concurrent jobs, report p50 and p95.
- [ ] Cache hit confirms stickiness (second request on the same input is faster).

**Dependencies:** P2
**Files likely touched:** new supervisor module or Go service
**Estimated scope:** Medium

## Task P4: Failure handling

**Description:** Keep the pool healthy when a worker dies or stalls.

**Acceptance criteria:**

- [ ] Per-job timeout kills and restarts the worker.
- [ ] A killed worker's job is retried once, then fails with a stable code.
- [ ] Zombie processes are reaped.
- [ ] Memory ceiling per worker is documented and enforced.

**Verification:**

- [ ] Kill a worker mid-job and confirm the pool recovers.
- [ ] Run for 30 minutes and confirm stable RSS.

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
