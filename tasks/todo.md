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

## Optional GPU backend

Plan: `tasks/gpu-feasibility.md`

- [x] Add `RenderBackend`, `RenderOptions.backend`, `backendUsed`, and
      `BACKEND_UNAVAILABLE`.
- [x] Implement the offscreen wgpu pipeline, atlas, vertex upload, readback,
      premultiplied resolve, and process-wide device/pipeline cache.
- [x] Dispatch CPU/GPU/auto and require a full mesh for GPU paths.
- [x] Add `--backend` to `render`/`bench`, service `backend`, and
      `hello.backends` / `backendUsed`.
- [x] Add parity and CLI/service tests; skip when no hardware adapter exists.
- [x] Document determinism, streaming, deployment, and measured performance.
- [x] Review the updated `tasks/gpu-performance-spec.md` and resolve the
      remaining calibration, parity-tolerance, and platform-matrix questions.
      The 0.1% parity target is documented as not met; bucket and WBOIT defaults
      are calibrated; musl/wasm stay CPU-only.
- [x] Decide whether release artifacts should ship a separate glibc GPU build.
      Linux x86_64 GNU is added as a GPU-enabled artifact; musl remains
      CPU-only.

## GPU performance path

Spec: `tasks/gpu-performance-spec.md`
Plan: `tasks/gpu-performance-plan.md`

- [ ] Prerequisite: CPU streaming chunk policy and multi-view reuse landed.
- [x] Phase 1: baseline and shared texture/atlas cache (Tasks 1-4).
  - [x] Task 1: GPU stage timings.
  - [x] Task 2: record baseline matrix and parity baseline.
  - [x] Task 3: shared texture storage.
  - [x] Task 4: GPU atlas cache.
- [x] Phase 2: GPU transform and compact instances (Tasks 5-9).
  - [x] Task 5: GpuQuad layout and CPU cull/encode.
  - [x] Task 6: camera-relative transform and precision guard.
  - [x] Task 7: opaque/cutout instanced draw.
  - [x] Task 8: textured/lit/cutout shader path.
  - [x] Task 9: remove the old opaque projection path.
- [x] Phase 3: GPU page streaming and multi-view (Tasks 10-14).
  - [x] Task 10: GpuRenderSession and staging ring.
  - [x] Task 11: wire GPU streaming through the CLI.
  - [x] Task 12: GPU allocation fallback and error paths.
  - [x] Task 13: multi-view session targets.
  - [x] Task 14: multi-view CLI and service wiring.
- [x] Phase 4: coarse depth buckets and WBOIT (Tasks 15-17).
  - [x] Task 15: translucent depth bucket spill and cleanup.
  - [x] Task 16: WBOIT targets, pipeline, and composite.
  - [x] Task 17: composite calibration and fallback (client-jar wall-time
        gate recorded as not met).
- [x] Task 18: hardware-aware `auto` policy.
- [x] Task 19: service policy and streaming documentation.
- [ ] Task 20: release matrix, CI, and final report. Workflows, docs, and the
      changelog are updated; the release workflow dry run and hardware matrix
      sign-off remain with the maintainer.
- [ ] Checkpoint F: success criteria are documented as not met for the 0.1%
      parity target and the two `000001` wall-time gates; GPU artifacts and
      human release approval remain pending.

## Video session slice

Spec: `tasks/video-export-spec.md`

- [x] Add `--sphere-fit` to `render`/`bench` and `RenderOptions`.
- [x] Add `GpuFrameSession`: upload opaque/cutout geometry once, then render
      frames by rewriting only the camera uniform and re-projecting
      translucent geometry.
- [x] Add `GpuFrameSession::render_frames` and a GPU parity/loop test.
- [x] Add the `litematic-video` timeline and ffmpeg encoder, plus
      `blockscope animate` with CPU fallback.
- [x] Service protocol v4: `hello.video`, video requests/results,
      `--ffmpeg`, and the supervisor `--video-timeout`.
- [x] Hardware encoder probing: lazy device probe at the target output size,
      `auto` fallback to libx264, explicit `ENCODER_UNAVAILABLE`.
- [x] Manual smoke test with NVENC, ffmpeg, and a client jar: MP4/GIF through
      `animate`, `serve`, and `supervise`. QSV/AMF/VAAPI/VideoToolbox still need
      testing on the matching hardware matrix.
- [x] Record the 600-frame 1024x1024 video memory, CPU/GPU, and
      software/hardware encoder results in `docs/performance.md`.
- [ ] Optional: repeat the encoder matrix on release hardware for every
      advertised hardware family.

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

## CPU streaming optimization

- [x] Buffer small streamed mesh chunks to 150,000 quads before rendering in
      `render` and the CPU `serve` path.
- [x] Add owned-chunk streaming mesh APIs so the CLI moves pages instead of
      cloning quads.
- [x] Reuse per-tile bin allocations across streamed chunks and shrink large
      retained lists to bound the peak.
- [x] Fuse projection and binning page by page.
- [x] Persist the page capacity hint across streamed chunks.
- [x] Encode each translucent spill triangle once and patch only the tile index.
- [x] Replace the per-tile stable translucent sort with an unstable sort that
      uses the original bin order as the tie-break.
- [x] Measure and reject camera-depth buckets for opaque early-Z: cache and bin
      costs outweighed any depth-test saving.
- [x] Measure and reject the f32 edge/barycentric rasterizer: within run noise
      and a higher pixel difference than the existing fast mode.
- [x] Measure and reject flat CSR bins: lower peak but about 1 s slower per
      `000001` jar render than the reused `Vec<Vec<u32>>` bins.
- [x] Measure and reject 64-page mesh chunks: faster mesh but about 250 MB more
      peak than 32 pages; the persisted hint recovers the mesh speed instead.
- [x] Measure and reject intra-chunk render splitting: extra tile passes made
      `000001` jar render much slower.
- [x] Add a nearest-neighbor texture fast path for in-range UVs.
- [x] Add stage timing diagnostics (`BLOCKSCOPE_STREAM_TIMING`) and a
      translucent-budget override (`BLOCKSCOPE_TRANSLUCENT_BUDGET`).
- [x] Measure the spill writer wait at about 0.04 s on `000001` jar; the writer
      is not the critical path, so parallel spill writers are not a priority.
- [x] Measure and reject part-outer tile scheduling: repeated thread scopes and
      tile-buffer setup made the tile stage roughly twice as slow.
- [x] Measure and reject two-bucket translucent grouping: bucket render
      improved but the larger read-ahead could not finish in time.
- [x] Measure and reject `BUCKET_TILES=8`: better bucket-render balance but
      about 190 MB more peak and no stable wall-time win.
- [x] Measure and reject 128px tiles: cache locality outweighed the lower
      per-tile overhead.
- [x] Measure and reject the 1 MB block bucket reader: the reader is not the
      critical path.
- [ ] Follow-ups: the `000001` jar stage split is project about 2.0 s, tile
      about 3.4 s, and translucent resolve about 2.8 s. The simple scheduling
      variants are exhausted; a persistent worker pool or a structural
      rasterizer change would be needed for the next step.

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
- [x] Push `v0.4.1` and let the release workflow publish the artifacts
      (maintainer).

## Server-side preview (deferred)

Spec: `tasks/server-preview-spec.md`
Contract: `tasks/server-preview-contract.md` (design record, do not implement)

- [x] Run the latency spike on the existing render paths for `000002` and
      `000001`. Textured stateless 384x384 warm p95 is 1.77-1.82 s and 10.5 s;
      the 300 ms gate failed. Lower resolution is geometry-bound; streaming
      bounds memory but is slower. Results in `docs/performance-preview.md`.
- [x] Run the resident-session spike. Textured `000002` is
      `BACKEND_UNAVAILABLE` on the 4 GB card (3.66 GB needed, 1.29 GB budget);
      `000001` needs more. Color-only `000002` 384x384 stateless meets the gate
      at p95 234-265 ms and 334 MB; color-only `000001` is 4.8-5.0 s. Color-only
      `000002` resident at 256x256 is about 27 ms/frame with 201.9 MB resident.
- [x] Defer to a future issue (2026-11-07). Color-only blockout preview is the
      recommended direction; textured preview is opt-in on larger GPUs and
      `000001`-class stays batch `--draft`.
