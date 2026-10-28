# Outstanding Tasks

**Status:** Active

Only unfinished work is listed here. Completed task details live in
`CHANGELOG.md` and git history.

## Pending verification

- [ ] Run the real-file smoke tests with
      `LITEMATIC_FIXTURES_DIR=... cargo test --workspace -- --ignored`.
- [ ] Review `docs/litematic-format.md` with a human.
- [ ] Measure `--greedy-mesh` on a real flat or repetitive build and decide
      whether it should become the default. It stays opt-in while random-palette
      scenes pay extra mesh time for little reduction.

## Rendering fidelity

- [x] Shape water and lava surfaces from neighboring fluid levels, hide shared
      fluid faces, and fill columns under the same fluid.
- [x] Give redstone wire a built-in flat dust model and the `power=0..15` tint
      in the color catalog, with a square-root ramp so power 0 and 1 differ.
- [x] Add water layers to waterlogged partial blocks while keeping their own
      models; treat bubble columns and always-water plants correctly.
- [x] Add fidelity tests and regenerate the water golden PNG.

## Memory baseline and sparse spike

Plan: `tasks/memory-plan.md`

- [x] Task 1: Add the allocation meter and synthetic fixtures
      (`crates/litematic/tests/memory.rs`).
- [x] Task 2: Record the baseline and commands in `docs/performance.md`.
- [x] Task 3: Add the streamed/full-mesh diagnostic on a real fixture.
- [x] Checkpoint: baseline gate (`fmt`, `clippy`, `cargo test --workspace`).
- [x] Task 4: Implement the standalone streaming `BlockStates` probe.
- [x] Task 5: Record the spike result and go/no-go decision.
- [ ] Checkpoint: write a new implementation plan for `sparse-litematic`
      before touching production code.

## Textured render memory

Plan: `tasks/render-memory-plan.md`

- [x] A: default `render` to `--stream-mesh=auto`; keep `off`/`on` overrides.
- [x] B: compact translucent `Triangle` from 112 B to 72 B.
- [x] C1: exact translucent spill to temporary bucket files; 618 MB measured.
- [x] Measure streamed/full textured memory and update `docs/performance.md`.
- [x] Update CLI/render tests and changelog.
- [ ] Optional: `serve` stream-mesh protocol support; separate spec.
