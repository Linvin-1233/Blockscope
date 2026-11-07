# Performance Baselines

This page records measured memory and timing baselines. The allocation test is the regression gate; the CLI numbers are diagnostics for changes that cannot be measured inside the test process.

## Allocation peak test

`crates/litematic/tests/memory.rs` measures peak heap allocation for `LitematicFile::parse_bytes` and `parse_bytes + into_scene`. It uses a process-global `GlobalAlloc` meter, so all scenarios live in one binary and run sequentially.

```bash
cargo test -p litematic-core --test memory -- --nocapture
```

The fixtures are synthetic and compressed before the measured window. The `512^3` sparse fixture uses custom volume limits because the default 100M per-region limit rejects it. This is intentional: it measures the representation cost, not the admission policy.

Baseline on 2026-10-28, Windows x86_64, 48 GB host, `cargo test` debug profile:

| Fixture                 |        Blocks |    Parse peak | Parse + scene peak |
| ----------------------- | ------------: | ------------: | -----------------: |
| tiny `2^3` dense        |             8 |      76,406 B |           76,406 B |
| sparse `256^3`, 1 stone |  16.8M volume |  12,588,227 B |       12,588,227 B |
| sparse `512^3`, 1 stone | 134.2M volume | 100,668,611 B |      100,668,611 B |
| dense `256^3` stone     |         16.8M |  12,588,226 B |       12,588,226 B |

Re-verified after the review fixes on 2026-10-28: all four rows are byte-identical. The new `crates/nbt/tests/memory.rs` also pins the decoder's pre-allocation guard: a truncated NBT document that declares a 64M-element int array or a 1M-element list must stay below 1 MiB of allocation instead of reserving the declared size.

The sparse peak scales with bounding-box volume: `512^3` is 8x `256^3`, which is exactly the volume ratio. `parse+scene` adds no measurable peak because the packed array is moved into the scene rather than copied.

## CLI diagnostics

Measured with the release binary on the same host. `000001` is the 9.39M-block, `211^3` fixture. `render` used a color-only catalog, 1024x1024, supersample 2, 12 threads, and `--force-limits` because the default 5M-quad cap rejects the exact mesh. The rows below were re-measured on 2026-10-28 after the review fixes; peak working set is sampled every 20 ms, so expect roughly 10-15% run-to-run variation on the short `validate` cases.

| Command                                              | Peak working set | Notes                                               |
| ---------------------------------------------------- | ---------------: | --------------------------------------------------- |
| `validate 000001`                                    |           103 MB | parse + scene                                       |
| `validate 000002 --force-limits`                     |           176 MB | 50.65M blocks; 176-178 MB observed                  |
| `validate 000003 --force-limits`                     |           145 MB | 67.1M blocks; 140-145 MB observed                   |
| `validate 000004 --force-limits`                     |           140 MB | 67.1M blocks; 122-149 MB observed                   |
| `render 000001 --stream-mesh --force-limits`         |       433-435 MB | color-only, 11.1-11.4 s wall, 91.5M triangles       |
| `render 000001 --stream-mesh=off --force-limits`     |          7.68 GB | color-only, 11.9 s wall, same PNG as streamed       |
| `render 000001 --force-limits --client-jar`          |       **591 MB** | exact translucent spill, 17.0 s wall, same PNG hash |
| `render 000002 --force-limits --client-jar`          |           216 MB | default auto streaming, 4.1 s wall                  |
| `bench 000001 --force-limits`                        |           6.8 GB | historical full-mesh run; not re-measured           |
| `bench 000002 --force-limits --client-jar`           |           3.8 GB | historical full-mesh run; not re-measured           |
| synthetic sparse `1024^3`, 1 block, `--force-limits` |           495 MB | historical `validate` run; not re-measured          |

The older table rows for `validate 000002`/`000003`/`000004` (132/130/115 MB) and the 407 MB / 8.8 s color stream were from an earlier build and host state. Both the current build and the pre-review `HEAD` binary measure 176-178 MB for `000002` and 11.6-11.8 s for the color stream on this host, so the differences are not review regressions.

The textured `000001` stream is 591 MB after the spill path was enabled; the earlier in-memory path peaked at 1.52 GB before spilling. The material set itself is about 227 MB; most of the remaining difference is the mesh chunk, resident tiles, and translucent triangles. The client-jar PNG SHA-256 is identical between `HEAD` and the review build.

The 18.9 GB figure in `docs/rendering.md` is the client-jar full-mesh path with more quads, not the streamed render path. The re-measured streamed color path is 433-435 MB and the current full color-only path is 7.68 GB. This is why the sparse parser is a conditional spike rather than a committed rewrite.

### CPU client-jar baseline (2026-11-06)

The color-only rows above are diagnostics, not a model of real use. On `000002`
the jar path has 17.3x more triangles (2.30M to 39.82M); on `000001` it has 2.5x
more translucent triangles (9.52M to 24.09M). It also spends about 1.7 s
decoding the 227 MB material set before rendering starts.

Measured on the R1 host with the release binary, CPU backend,
`--stream-mesh=on`, 1024x1024, supersample 2, 12 threads, `--force-limits`, and
`fixtures/private/texture/26.2-Fabric.jar`. Peak working set is sampled every
20 ms.

| Case            | Triangles | Parse    | Mesh      | Render    | PNG     | Total     | Peak     |
| --------------- | --------: | -------: | --------: | --------: | ------: | --------: | -------: |
| `000002` color  |     2.30M | 224.5 ms |   423.5 ms |   666.7 ms | 46.7 ms | 1,399.8 ms | 191.1 MB |
| `000002` jar    |    39.82M | 230.3 ms | 1,453.4 ms | 2,458.5 ms | 46.9 ms | 4,586.9 ms | 205.3 MB |
| `000001` color  |    91.57M | 203.5 ms | 5,472.5 ms | 6,933.9 ms | 64.6 ms | 12,714.7 ms | 433.7 MB |
| `000001` jar    |   128.63M | 198.3 ms | 5,349.3 ms | 12,421.6 ms | 64.5 ms | 19,736.8 ms | 580.6 MB |

`Total` is the CLI-reported total. For the jar rows it includes about 0.4 s
(`000002`) and 1.7 s (`000001`) of material and texture loading that is not part
of parse, mesh, render, or PNG. The peaks are 205 MB and 581 MB, not the
multi-gigabyte full-mesh path.

The jar triangle mix explains the render split:

| Case         |   Opaque | Cutout | Translucent |
| ------------ | -------: | -----: | ----------: |
| `000002` jar |  39.60M |  181k |         38k |
| `000001` jar |  84.68M | 19.87M |       24.09M |

Stage timing from a temporary instrumentation build. The timed run is about 9%
slower than the non-timed render on `000001`; use the ratios, not the absolute
sum.

| Case         | Project + bin |  Tile | Spill write | Translucent resolve | Assembly | Downsample |
| ------------ | ------------: | ----: | ----------: | ------------------: | -------: | ---------: |
| `000002` jar |        923 ms | 1,690 ms |      157 ms |                4 ms |    57 ms |       5 ms |
| `000001` jar |      2,586 ms | 3,866 ms |    2,853 ms |             4,171 ms |    50 ms |       5 ms |

`000002` is tile-bound. `000001` spends about 7.0 s in the translucent path,
more than half of its render time, and the spill write is serial on the main
thread. The library path behind the older full-mesh benchmark rows is not a
substitute for these streamed CLI numbers.

The streamed CLI color-only `000002` render is 667 ms here, not the roughly
214 ms measured through the four-chunk library path, because the streamed mesh
builder emits 432 32-page chunks for this fixture. That fixed per-chunk cost is
another reason not to use color-only bench results as a jar proxy.

`--greedy-mesh` with the jar was also measured: `000001` triangles changed by
-0.06% while mesh time grew 35% and render time grew 1%; `000002` triangles fell
1.2% while mesh time grew 73% and render time fell 3.5%. It stays opt-in.

Commands:

```powershell
$exe = (Resolve-Path .\target\release\blockscope.exe).Path
$jar = (Resolve-Path .\fixtures\private\texture\26.2-Fabric.jar).Path
& $exe render fixtures\private\000001.litematic --client-jar $jar `
  --backend cpu --stream-mesh=on --width 1024 --height 1024 `
  --supersample 2 --threads 12 --force-limits --output .\out.png
```

### P0 CPU streaming changes (2026-11-06)

The first CPU P0 slice targets the client-jar hot path:

- Translucent spill writes run on a FIFO writer thread with a 16-part queue, so
  spill I/O overlaps opaque rasterization.
- The spill bucket reader decodes the next bucket while the current bucket is
  sorted and rasterized, using one reader plus the normal tile workers.
- Quads and triangles whose bounds lie entirely outside the image are dropped
  before binning, and tile rasterization no longer repeats the bounding-box
  intersection test for every binned candidate.

A temporary `BLOCKSCOPE_SYNC_SPILL` build switch disabled the writer overlap
and bucket read-ahead so both paths could be timed on the same binary. It has
been removed.

`000001` jar, 1024x1024 ss2, 12 threads, two alternating runs per mode:

| Mode                                |                   Render |               Wall |                 Peak |
| ----------------------------------- | -----------------------: | -----------------: | -------------------: |
| serial spill + serial bucket read   | 11,698 / 11,690 ms       | 21.73 / 21.75 s    | 580.6 / 580.1 MB     |
| writer overlap + bucket read-ahead  | 11,114 / 11,133 ms       | 20.70 / 20.60 s    | 579.8 / 579.7 MB     |

The controlled difference is about 4.9% render and 5.2% wall time, with no
measurable peak change. `000002` jar measured 2,701 / 3,011 ms serial versus
2,788 / 2,775 ms overlapped; its translucent layer is too small for a stable
signal. The PNG SHA-256 values for both fixtures are unchanged.

A post-change spot check on the same host measured `000001` jar render
9.42-9.79 s, total 16.86-17.79 s, peak 578-580 MB, and `000002` jar render
2.35 s, total 4.52 s, peak 205 MB. Both PNG hashes still match the pre-change
files. Treat these as point-in-time diagnostics; the alternating A/B above is
the attribution.

Three experiments were measured and reverted:

- Quad-level triangle setup: no render win and a slower `000002`.
- Adaptive mesh chunks sized by a quad budget: neutral speed, but peak rose by
  26-90 MB depending on the target.
- Batch texture preload with parallel PNG decode: no wall-time win even after
  preserving first-use texture ids; on `000001` the prefetch runs were
  0.7-1.1 s slower than the clean run, because the model pre-scan offset the
  decode gain.

Absolute run times drift with host state; use the alternating A/B table for the
P0 claim rather than comparing runs from different times.

### Fast CPU projection (2026-11-07)

`--precision fast` stores the CPU screen-space quad table as f32 instead of f64.
`QuadRender` drops from 128 to 80 bytes, so projection writes and tile reads
shrink; the default `exact` path and its golden PNGs are unchanged.

Same-process A/B with the client jar, alternating exact and fast iterations on
one parsed scene and mesh:

| Case | Exact median | Fast median | Gain |
| --- | ---: | ---: | ---: |
| `000002` jar | 2,403.8 ms | 2,007.4 ms | 16.5% |
| `000001` jar | 10,781.2 ms | 10,203.4 ms | 5.4% |

Peak working set on the CLI path: `000001` exact 575-579 MB, fast 546-549 MB;
`000002` is about 206 MB in both modes.

Fast mode is explicitly approximate. On the same 1024x1024 jar renders it
changed 297 pixels (0.028%, max channel delta 21, no alpha changes) on `000002`,
and 66,978 pixels (6.388%, max delta 180, 75 alpha pixels) on `000001`. It is
opt-in; use `exact` for byte-for-byte comparisons.

A hybrid fast point (f32 x/y with f64 depth) was also measured. It reduced the
`000001` difference only from 6.39% to 5.94% while cutting the `000002` gain
from 16.5% to 10.7%, so the simpler f32 point is the shipped fast path.

## Review-fix performance check (2026-10-28)

The review fixes do not change the single-view render hot path. The pre-review `HEAD` binary (`6b5a680`) and the current build were compared on the same host with the same commands:

| Case                                             |                                           `HEAD` |                 Current |
| ------------------------------------------------ | -----------------------------------------------: | ----------------------: |
| `000001` color stream, 1024x1024 ss2, 12 threads |                        11.65-11.75 s, 434-438 MB | 11.1-11.4 s, 432-435 MB |
| `000001` client jar stream, same settings        |                                16.98 s, 591.0 MB |       17.00 s, 590.8 MB |
| PNG SHA-256, client jar                          |                              `7D7E583F...C5D687` |     `7D7E583F...C5D687` |
| NBT allocation test rows                         | 76,406 / 12,588,227 / 100,668,611 / 12,588,226 B |               identical |

The color-stream difference is within host noise; no path regressed. The `000001` client-jar image is byte-identical. The current full-mesh color path measured 7.68 GB / 11.9 s and produced the same PNG SHA-256 as the streamed path; it was not re-run on `HEAD` because the full-mesh path is unchanged and costs about 8 GB per run.

Multi-view rendering now batches by a 256 MiB output-byte budget and writes each view before starting the next batch. At 1024x1024 (4 MiB RGBA) and 2048x2048 (16 MiB RGBA), the thread budget remains the limiting factor, so normal multi-view parallelism is unchanged. At 4096x4096 (64 MiB RGBA), a batch is capped at four views instead of retaining every output; this is the intended memory tradeoff and is covered by `view_batch_budget` unit tests.

The supervisor hashes inputs on a dedicated thread instead of its event loop, and reuses a caller-supplied `input.hash` for routing. This is a latency improvement; no end-to-end render throughput table is claimed for it because the renderer remains the bottleneck.

## Textured render memory

Texture memory is not the dominant cost. For `000001` with the client jar:

- the material set is about 227 MB;
- 12.59M translucent triangles were retained for the per-tile depth sort;
- each `Triangle` was 112 bytes, so the retained triangles were about 1.41 GB;
- streamed rendering peaked at 2.28 GB before the compact layout.

`Triangle` now stores projected screen points as f32 and no longer stores the always-translucent layer tag. The depth key stays f64, so the sort order is unchanged; the struct is 72 bytes and the regression test in `crates/render/src/rasterizer/project.rs` pins that ceiling. The in-memory translucent path measured 1.52 GB before spilling was implemented.

Above 262,144 retained triangles, the streamed renderer spills exact records to temporary bucket files and processes one bucket at a time. Re-measured after the review fixes, `000001` with the client jar peaks at **591 MB** in 17.0 s with the same PNG SHA-256 as `HEAD` (the previous 618 MB / 16.2 s row was from an earlier host/build state). The temporary directory is removed when the render finishes.

`render` defaults to `--stream-mesh=auto`, which streams like `on`; `--stream-mesh=off` restores the full-mesh path. This is what avoids the 18.9 GB full-mesh peak without requiring a flag.

## Reproducing the CLI peak

Windows PowerShell, sampling the child working set every 20 ms:

```powershell
$exe = (Resolve-Path .\target\release\blockscope.exe).Path
$p = Start-Process -FilePath $exe -ArgumentList @(
  'render', 'fixtures\private\000001.litematic',
  '--output', "$env:TEMP\peak.png",
  '--width', '1024', '--height', '1024', '--supersample', '2',
  '--threads', '12', '--stream-mesh', '--force-limits'
) -PassThru -NoNewWindow
$peak = 0
while (-not $p.HasExited) {
  try {
    $p.Refresh()
    if ($p.WorkingSet64 -gt $peak) { $peak = $p.WorkingSet64 }
  } catch {}
  Start-Sleep -Milliseconds 20
}
$p.WaitForExit()
"exit=$($p.ExitCode) peakMB=$([math]::Round($peak / 1MB, 1))"
```

Use `bench` with the same file and without `--stream-mesh` to record the full-mesh path. That path is intentionally not part of the default CI gate; it is a diagnostic for deciding whether streaming should become the default.

## Sparse spike

The prototype is `crates/litematic/examples/sparse_probe.rs`. It scans the gzip stream twice: the first pass records region headers and `BlockStates` offsets, and the second pass decodes the packed words without retaining the decompressed document or the long array. It counts non-zero palette values; it does not build a scene or validate palette indices yet.

```bash
cargo run --release -p litematic-core --example sparse_probe -- path\to\file.litematic --dense
```

Measured on 2026-10-28 with the release example:

| File                               | Dense parse peak | Stream probe peak | Dense parse | Stream decode |
| ---------------------------------- | ---------------: | ----------------: | ----------: | ------------: |
| synthetic sparse `512^3`, 1 block  |    100,668,608 B |         141,739 B |     48.4 ms |       97.7 ms |
| synthetic sparse `1024^3`, 1 block |    805,311,680 B |         141,739 B |    354.6 ms |      689.5 ms |
| `000001` (9.39M blocks)            |     83,846,504 B |         141,711 B |    120.4 ms |       20.3 ms |

The probe peak is constant at about 142 KB. The sparse case is about 2x slower because it decodes every value from a second gzip pass; the dense fixture is faster because it avoids building the NBT tree.

**Decision: go for the integration spike follow-up.** The peak reduction is far above the 50 percent bar and the sparse parse time is under 2x. This is not yet a production parser: the next step is a scoped implementation plan for `RegionStates`, canonical scene hashing, and parity tests. The probe's `occupied_nonzero` counter is not the same as `non_air` when the palette has a non-zero default entry; the production path must use the palette default and preserve diagnostics.

## GPU frame session (2026-11-06)

`GpuFrameSession` is the first resident-geometry slice for video export. It
uploads every opaque/cutout quad once and keeps the vertex buffers for the life
of the session; each frame rewrites only the camera uniform and re-projects the
translucent layer. This is the session shape that avoids re-uploading the full
mesh for every video frame.

Measured with the release build, 12-thread host, GTX 1650 SUPER, D3D12,
color-only materials, 256x256, supersample 1, 12 yaw frames, sphere fit on:

| Input | Per-frame full renders | Resident session | Speedup | Resident bytes |
|---|---:|---:|---:|---:|
| `000002` | 2,567.9 ms | 413.5 ms | 6.21x | 201.9 MB |

The resident geometry is a hard VRAM constraint: `000001` color-only needs
about 7.27 GB for the instance table plus targets, above the 1.29 GB working-set
budget on this host, so the session returns `BACKEND_UNAVAILABLE` instead of
attempting an allocation. Large or textured scenes keep using the streaming
`GpuRenderSession` (or a future bounded frame batch) rather than a resident
session.

Run the diagnostic with:

```powershell
cargo run --release -p blockscope-cli --features gpu --example frame_session_bench -- `
  fixtures/private/000002.litematic
```

This is a diagnostic, not a CI gate. The existing GPU parity and frame-loop
tests protect correctness.

### Animate CLI (2026-11-06)

`blockscope animate` drives the resident frame session end to end. Measured on
the same host and release build, `000002` color-only, 128x128, supersample 1,
12 threads, 60 frames at 4 fps, sphere fit on, ffmpeg 9.0.2:

| Backend | Mesh | Render | Encode | Total |
|---|---:|---:|---:|---:|
| CPU | 328.3 ms | 5,553.0 ms | 37.6 ms | 6,248.9 ms |
| GPU resident session | 460.2 ms | 2,989.1 ms | 68.0 ms | 4,856.4 ms |

The GPU session renders the frame loop 1.86x faster and the complete command is
1.29x faster. Short clips can still be faster on the CPU because the resident
session pays its upload/setup cost up front: 12 frames at the same size measured
1,861 ms on CPU versus 2,089 ms on GPU. `000001` color-only cannot hold the
resident instance table and `--backend auto` falls back to the CPU; explicit
`--backend gpu` reports `BACKEND_UNAVAILABLE`.

The command uses an external ffmpeg. MP4 is H.264/yuv420p with an opaque
background and even dimensions; GIF is a two-pass palette encode with optional
1-bit transparency.
