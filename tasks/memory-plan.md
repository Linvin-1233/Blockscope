# Implementation Plan: Memory Baseline and Sparse Spike

**Status:** Baseline and spike complete; integration not started
**Date:** 2026-10-28
**Scope:** Option 4 (allocation-peak regression tests) plus a time-boxed S0
spike for option 2 (sparse litematic). No production parser rewrite until the
spike proves a win.

## Overview

The 2026-10-28 measurements changed the priority:

- parse + scene is 108-132 MB for the 9.4M-67M-block fixtures;
- streamed render of `000001` is 407 MB;
- full-mesh `bench` is 6.8 GB with the color catalog (18.9 GB with the client
  jar in the existing docs).

So the first deliverable is a repeatable memory oracle, not a parser rewrite.
The sparse parser is a spike: implement enough to measure a streaming
`BlockStates` scan, compare peak and wall time against the dense parser, and
stop if the win is not clear.

## Architecture Decisions

- **Allocation meter, not OS RSS, for parse/scene.** A `GlobalAlloc` wrapper in
  a dedicated `litematic-core` integration test gives deterministic byte
  counts on both Windows and Linux. RSS polling stays a diagnostic because
  working-set accounting differs by platform.
- **Separate test binaries.** The meter is process-global. Keep measured
  scenarios in one integration test binary and run them serially behind a
  mutex; other test binaries are unaffected.
- **Generate fixtures without measuring setup.** Write compressed NBT bytes
  before the measured window. Stream zero long words instead of building a
  dense `Vec<i64>` in the test process.
- **Two measured phases.** `parse_bytes` and `parse_bytes + into_scene` are
  separate windows. A change that only moves cost between them is visible.
- **Streamed render is a diagnostic, not a default CI gate.** The existing
  `--stream-mesh` path needs a real fixture and a multi-second run. Record it
  in `docs/performance.md`; add an ignored/opt-in test if it stays stable.
- **Sparse spike stays standalone.** Do not change `Region`, `Scene`, the
  parser, or the CLI until the spike shows at least a 50 percent peak win on a
  target the project cares about, with parse time under 2x. The spike may live
  in an example or an ignored test.
- **Do not overwrite the historical plan.** `tasks/plan.md` is Phase 1 history
  and `tasks/todo.md` has unrelated pending verification. This plan lives in
  `tasks/memory-plan.md`; new tasks are appended to `tasks/todo.md` under a
  separate section.

## Task List

### Phase 1: Parse and scene baseline

- [ ] **Task 1: Add the allocation meter and synthetic fixtures**
  - Acceptance: `crates/litematic/tests/memory.rs` measures `parse_bytes` and
    `parse_bytes + into_scene` for tiny dense, sparse, dense, and multi-region
    fixtures; prints peak bytes with `--nocapture`; passes with generous
    initial budgets.
  - Verify: `cargo test -p litematic-core --test memory -- --nocapture`.
  - Files: `crates/litematic/tests/memory.rs`,
    `crates/litematic/tests/support/` (fixture helper),
    `crates/litematic/Cargo.toml` (dev-dependency only if needed).
  - Dependencies: none. Size: M.

- [x] **Task 2: Record the baseline and commands**
  - Acceptance: `docs/performance.md` contains the date, host, command, and
    peak table for parse/scene plus the streamed/full-mesh diagnostic; no
    production behavior changes.
  - Verify: numbers match Task 1 output; `cargo test --workspace` still passes.
  - Files: `docs/performance.md`, `README.md` or `docs/rendering.md` link.
  - Dependencies: Task 1. Size: S.

- [x] **Task 3: Add the streamed/full-mesh diagnostic**
  - Acceptance: a reproducible command records peak working set for
    `render --stream-mesh` and `bench` on a real fixture; the result is
    documented, not a default CI assertion.
  - Verify: run the command on `fixtures/private/000001.litematic` when
    available; record the numbers.
  - Files: `docs/performance.md`, optionally an ignored test or example.
  - Dependencies: Task 2. Size: S.

### Checkpoint: Baseline
- [x] `cargo fmt --all --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] Baseline table reviewed before the spike starts

### Phase 2: Sparse spike

- [x] **Task 4: Implement the streaming `BlockStates` probe**
  - Acceptance: a standalone example or ignored test scans a synthetic sparse
    `1024^3` litematic in bounded chunks, counts occupied values, and reports
    peak allocated bytes and wall time. It does not change production APIs.
  - Verify: run it against the synthetic sparse fixture and the dense
     `000001` fixture; compare with `validate`.
  - Files: `crates/litematic/examples/` or `crates/litematic/tests/`,
    `crates/litematic/Cargo.toml` (dev-dependency only if needed).
  - Dependencies: Task 1. Size: M.

- [x] **Task 5: Decide and record**
  - Acceptance: `docs/performance.md` records the spike result and a go/no-go
    decision. Go requires at least a 50 percent peak reduction on the target
    sparse case and parse time under 2x; otherwise the module stops here.
  - Verify: numbers and decision reviewed; no production code changed.
  - Files: `docs/performance.md`, `tasks/memory-plan.md`,
    `tasks/sparse-litematic-spec.md`.
  - Dependencies: Task 4. Size: XS.

### Checkpoint: Spike decision
- [x] Full gate passes
- [x] Spike numbers and decision recorded
- [ ] Next: write a new implementation plan for `sparse-litematic` before
      touching production code

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Global allocator sees parallel test allocations | High | One test binary, serial measured window behind a mutex |
| Fixture generation dominates the measured peak | High | Compress before the window; stream zero words |
| Absolute budgets vary by allocator/platform | Medium | Assert ratios where possible; use generous absolute tripwires |
| Streamed render test is slow or host-dependent | Medium | Keep it diagnostic/ignored; do not gate default CI |
| Sparse spike becomes a rewrite | High | Standalone probe only; stop at Task 5 unless the win is clear |
| Existing plans are overwritten | High | New plan file; append new todo section only |

## Open Questions

1. Should the streamed render diagnostic be an ignored test, an example, or
   only a documented command? Proposed: documented command first; promote to an
   ignored test if it stays stable.
2. Which sparse fixture is the go/no-go target: synthetic `1024^3` with one
   block, or a real user file? Proposed: synthetic for determinism, plus any
   real sparse file the user can provide.
3. Is a 50 percent peak reduction the right bar when the default limits reject
   `512^3`? Proposed: yes, but only if `--force-limits` sparse inputs are a
   supported use case; otherwise no-go and focus on streamed render defaults.
