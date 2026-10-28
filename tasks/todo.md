# Outstanding Tasks

**Status:** Active

Only unfinished work is listed here. Completed task details live in
`CHANGELOG.md` and git history.

## CLI presentation

Plan: `tasks/cli-ux-plan.md`

- [x] Slice 1: UI foundation, color mode, banner, help/error styling.
- [x] Slice 2: human-readable output alignment and semantic color.
- [x] Slice 3: page/tile progress callbacks and CLI wiring.
- [x] Slice 4: docs, changelog, and full gate.
- [x] Slice 5: static banner width tiers.
- [x] Slice 6: live progress reflow on terminal resize.
- [x] Manual check of the welcome banner and live progress in a real
      Windows Terminal at 80 and 120 columns, plus 55/40/28/18-column tiers
      via a PTY.

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

## Fallback color coverage

- [x] Resolve decorated variants (`quartz_pillar`, `chiseled_*`, `smooth_*_slab`)
      through the base material entry instead of the magenta sentinel.
- [x] Skip invisible blocks (`barrier`, `light`, `structure_void`).
- [x] Add palette entries for anvils, hoppers, lanterns, end rods, cocoa, and
      weighted pressure plates.

## Terminal image display

- [x] Fit terminal images to the visible grid (kitty/iTerm2 cells, sixel pixels)
      and emit the image after the summary.
- [x] Fall back to half blocks inside `tmux`/`screen`; warn when stdout is not a
      terminal; stop sending `width=auto`/`height=auto` to iTerm2.
- [x] Update CLI docs and changelog.

## Release 0.4.1

- [x] Remove `--lod auto`; LOD requires an explicit `1..=16` factor.
- [x] Warn when LOD is enabled: `Enabling LOD will result in a significant loss
      of detail`.
- [x] Raise the `serve`/`supervise` protocol to v2 because `lod` no longer
      accepts `"auto"`.
- [x] Update CLI tests, docs, and changelog.
- [ ] Push `v0.4.1` and let the release workflow publish the artifacts
      (maintainer).
