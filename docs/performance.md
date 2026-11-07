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

## CPU streaming pipeline (2026-11-07)

The CPU streaming path now batches small mesh chunks before rendering, reuses
per-tile bin allocations across chunks, and bins quads page by page while they
are still hot in cache. `render` buffers small chunks until 100,000 quads and
passes larger chunks through; the mesh builder still emits 32-page chunks, so
the mesh peak does not grow just because the render batch is larger. The mesh
crate also exposes owned-chunk streaming variants, so the CLI can move pages
instead of cloning every `Quad`.

The translucent spill encodes each triangle once and only patches the tile
index per record. Per-tile translucent sorting uses `sort_unstable_by` with the
original bin position as the tie-break, so equal depth keys keep their previous
order while the sort avoids the stable merge-sort allocation.

Same-host A/B, baseline commit `219350b` versus the current build, release,
CPU, `--stream-mesh=on`, 1024x1024, supersample 2, 12 threads,
`--force-limits`, and `--no-file`. Runs alternate baseline and current binaries;
PNG hashes were checked in separate runs.

| Case | Baseline render | Current render | Render change | Baseline total | Current total |
| --- | ---: | ---: | ---: | ---: | ---: |
| `000002` color | 681.1 / 669.3 / 690.3 ms | 192.4 / 202.3 / 209.1 ms | median -70.3% | 1409.1 / 1426.2 / 1449.2 ms | 957.3 / 966.1 / 1099.0 ms |
| `000002` jar | 2494.6 / 2537.7 ms | 2302.2 / 2289.8 ms | median -8.8% | 4584.6 / 4630.3 ms | 4667.5 / 4395.3 ms |
| `000001` jar | 9927.2 ms | 9132.2 / 9615.2 ms | median -5.6% | 18682.1 ms | 16672.1 / 17536.5 ms |

`000002` color is the clearest case: the old streamed CLI path paid the
projection and tile barriers once per 32-page chunk; batching cuts 432 render
passes to roughly 23 at 100,000 quads. The jar cases are limited by the
translucent spill and sort, not by the per-chunk overhead.

The unstable sort was isolated with two binaries built from the same tree, one
with the stable sort and one with the tie-break sort. On `000001` jar:
stable 9,490.1 / 10,305.4 ms, tie-break unstable 9,132.2 / 9,615.2 ms, a 5.3%
median render reduction. PNG SHA-256 values are unchanged:

| Case | SHA-256 |
| --- | --- |
| `000002` color | `92F1244E88CF2F390B24A239419B5C8D4B732206303AEA22C525085D9D1C0BB3` |
| `000002` jar | `38250F5E3A96F6488CB48368AC373E24620B7339658FB4857C73D981AF56B328` |
| `000001` jar | `D143DA27D670E887AE25C4313CE331E1F22066F98D119659879764A0F5C28CCC` |

### Rejected CPU experiments (2026-11-07)

| Attempt | Result | Verdict | Why |
| --- | --- | --- | --- |
| Camera-depth buckets for opaque early-Z (`BLOCKSCOPE_CPU_DEPTH_BUCKETS=16/32/64`) | `000002` jar render 2,333-2,654 ms off versus 2,757-3,944 ms on | reverted | Extra bins and scatter cost outweighed any early-Z saving; depth testing was not the limiting fragment cost. |
| f32 edge/barycentric rasterizer for `--precision fast` | `000002` jar median 1,797.7 ms exact versus 1,757.1 ms fast (2.3%, within run noise); 0.779% pixels differ, max delta 21 | reverted | The tile path is memory and setup bound; the extra approximation did not buy a stable win. The existing fast point-storage mode remains. |

A small-bbox triangle fast path was also proposed, but a prior measurement
showed no gain and a slowdown, so it was not retried.

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

## GPU backend baseline (2026-11-05)

This is the pre-G2 baseline for `tasks/gpu-performance-spec.md`. It records why
the current GPU path is not yet a net win on a strong CPU. The host is the
12-core / 48 GB Windows machine with an NVIDIA GTX 1650 SUPER (4 GB), release
build, D3D12, color-only materials, 1024x1024 output, supersample 2, 12 threads,
`--force-limits`. The CLI default backend was still `cpu` when measured; the
spec changes it to `auto` in G6.

Build and run:

```powershell
cargo build --release -p blockscope-cli --features gpu

$exe = (Resolve-Path .\target\release\blockscope.exe).Path
& $exe bench fixtures\private\000002.litematic --backend cpu --iterations 3
& $exe bench fixtures\private\000002.litematic --backend gpu --iterations 3
& $exe bench fixtures\private\000001.litematic --backend cpu --iterations 1
& $exe bench fixtures\private\000001.litematic --backend gpu --iterations 2
```

End-to-end:

| Case | CPU 12t | CPU 2t | GPU | Notes |
|---|---:|---:|---:|---|
| `000002` render | 190 ms | 341 ms | 201 ms | GPU 6% slower than CPU 12t |
| `000002` render + PNG | 235 ms | 385 ms | 245 ms | GPU 4% slower than CPU 12t |
| `000001` render, full mesh | 7,046 ms | - | 14,881 ms | GPU 2.1x slower |
| `000001` end to end | about 11.3 s | - | about 18.7 s | includes parse, mesh, render, PNG |

`000001` used 87.9M quads in this build. The GPU path requires the full mesh, so
it also chooses the 7.68 GB color-only memory peak instead of the 433 MB
streamed peak.

GPU stage timings from the last iteration of a three-iteration `000002` run:

| Stage | Time |
|---|---:|
| setup | 0.7 ms |
| atlas build/upload | 0.1 ms |
| CPU projection/cull | 81.0 ms |
| CPU vertex encode | 59.7 ms |
| buffer upload | 28.4 ms |
| opaque draw | 0.8 ms |
| translucent sort/draw | 0.6 ms |
| readback | 6.9 ms |
| resolve | 2.2 ms |
| total | 199.2 ms |

The GPU draw itself is under 1% of render time. CPU projection, vertex
expansion, and upload account for about 85%. This is the bottleneck G2 removes.

Current GPU parity on the 96x96 reference scenes:

| Scene | Differing pixels | Max channel delta | Alpha mismatch |
|---|---:|---:|---:|
| opaque/cutout/translucent | 1,021 / 9,216 (11.1%) | 1 | none |
| solid background | 815 / 9,216 (8.8%) | 1 | none |
| textured nearest | 0 | 0 | none |
| textured linear (M4 host) | 44 / 9,216 | 1 | none |

The 0.1% gate is a target for the second-generation path, not a property of the
current path. G2 and G5 own closing it or recording the measured exception.

### Shared texture storage (2026-11-05)

`TextureSet` now stores `Arc<Texture>` entries, so cloning it for a material
snapshot only bumps refcounts. The `000001` client-jar stream still peaks at
581.4 MB, within the 591 MB baseline's run-to-run variance; the live material
set is dropped before rendering, so the steady peak does not include a second
copy. The change removes the transient texture copy during snapshot creation and
is required for the process-wide GPU atlas cache. The measured command is the
same streamed client-jar render used for the 591 MB row in the summary table.

The GPU atlas cache is keyed by a weak texture-set identity. Its byte cap is
one quarter of the 60% VRAM working set, clamped to 64-512 MiB; the session
also includes the atlas in its admission check. On `000002 --client-jar`, the
first GPU render spent 1,037 ms building/uploading the atlas; the next render
reported `atlasCacheHit=true` and 4 microseconds. The second view of the same
material snapshot therefore reuses the atlas instead of repacking 227 MB of
textures.

### Instanced opaque/cutout path (2026-11-05)

The GPU path now uploads one 92-byte `GpuQuad` instance per opaque/cutout quad
instead of six projected 36-byte vertices. The camera transform, UV decode,
AO, and quad expansion run in the vertex shader; translucent geometry still
uses the CPU-projected path until G5.

`000002`, color-only, 1024x1024, ss2, 12 threads, median of 3:

| Path | Render | Render + PNG | Mesh |
|---|---:|---:|---:|
| CPU 12t | 197.0 ms | 242.7 ms | 235.2 ms |
| GPU instanced | 116.3 ms | 161.2 ms | 228.9 ms |

GPU stage timings for the same run: encode 70.4 ms, translucent projection
11.5 ms, upload 11.3 ms, draw 1.0 ms, readback 5.3 ms, resolve 2.1 ms, total
113.1 ms. The GPU is now faster than 12-thread CPU on this fixture.

`000001`, full mesh, color-only, one iteration:

| Path | Render | End to end |
|---|---:|---:|
| GPU instanced | 8,650 ms | about 12.5 s |
| Previous GPU path | 14,881 ms | about 18.7 s |
| CPU 12t | 7,046 ms | about 11.3 s |

On the large fixture the remaining GPU cost is dominated by CPU instance
encoding (2.94 s), translucent projection (1.13 s), and the old translucent
sort/draw (2.81 s). The full-mesh 7.68 GB peak is still the reason G3 streaming
is next.

### GPU streaming (2026-11-05)

`render --backend gpu --stream-mesh=on` now builds page chunks and feeds
`GpuRenderSession`; the full mesh is not retained. `000001`, color-only,
1024x1024, ss2, 12 threads, `--force-limits`:

| Metric | Value |
|---|---:|
| Peak working set | 1,678 MB |
| Wall time | 11.27 s |
| Render | 5.10 s |
| Mesh | 5.04 s |
| PNG | 65.6 ms |
| Triangles | 91.6M |

The 2 GB memory gate passes. The 10 s wall gate is not met yet: the remaining
cost is the 32-page mesh chunk policy (P0) and the 9.5M retained translucent
triangles in the old GPU path (G5/WBOIT). `--stream-mesh=auto` and `on` produce
byte-identical PNGs to the full-mesh GPU path on the CLI fixture.

Multi-view GPU jobs use one `GpuRenderSession` and one mesh build:

| Case | Wall | Peak working set |
|---|---:|---:|
| `000002`, 1 view | 1.38 s | - |
| `000002`, 2 views | 1.40 s | - |
| `000002`, 12 views | 3.15 s | 943 MB |

Views are batched by the 60 percent VRAM target budget minus atlas and staging
headroom. On this host that permits the 12 views at 1024x1024 ss2 in two
sessions, with one mesh build and one upload per session.

### GPU translucent depth buckets (2026-11-05)

Translucent triangles above 262,144 entries spill into 128 camera-depth bucket
files instead of being retained in one vector. `000001` GPU streaming,
color-only, 1024x1024, ss2, 12 threads:

| Metric | Before buckets | After buckets |
|---|---:|---:|
| Peak working set | 1,678 MB | **499 MB** |
| Wall | 11.27 s | 11.35 s |
| Render | 5.10 s | 5.16 s |
| PNG SHA-256 | `42309C9F...` | unchanged |

The 9.5M translucent triangles are written to depth buckets and read
far-to-near in bounded sub-batches. The 10 s wall gate is still mesh-bound
(5.08 s); the bucket reader and per-bucket sort are not the bottleneck.

### WBOIT capture (2026-11-05)

Translucent buckets now draw into `Rgba16Float` accumulation and revealage
targets with depth test on and depth write off; a full-screen composite pass
resolves them over the opaque color target. Revealage is RGBA rather than
R16Float because WebGPU needs a per-fragment source alpha for
`OneMinusSrcAlpha`.

`000001` GPU streaming, color-only, 1024x1024, ss2, 12 threads:

| Metric | Old translucent path | WBOIT |
|---|---:|---:|
| Peak working set | 499 MB | **538 MB** |
| Wall | 11.35 s | 12.20 s |
| Render | 5.16 s | 5.34 s |
| PNG | identical to pre-bucket | visually similar, not byte-identical |

The 9,216-pixel parity scene keeps a maximum channel delta of 1 but still has
1,039 differing blended pixels, so the 0.1% tolerance remains a calibration
item for Task 17. The extra ~39 MB and ~0.2 s are the two half-float targets
plus the composite pass.

`000001` with the client jar, GPU streaming, 1024x1024, ss2, 12 threads:

| Metric | Value |
|---|---:|
| Peak working set | 656 MB |
| Wall | 22.64 s |
| Render | 12.21 s |
| Mesh | 6.32 s |
| Parse | 0.37 s |
| Triangles | 127.2M (24.1M translucent) |

The 3 GB memory gate passes; the 15 s wall gate does not. The gap is the
24.1M translucent triangles (projection + WBOIT fill) and the 6.3 s streamed
mesh. This is recorded as a target-not-met rather than hidden. Calibration
defaults are 128 depth buckets, a 262,144-triangle in-memory/read cap, and the
WBOIT weight `0.03 / (1e-5 + depth^4)` clamped to `1e-5..3000`.

## GPU performance path final results (2026-11-06)

`auto` is now the default backend for the CLI, `bench`, and `serve`. The policy
is deterministic for a machine description and chooses the GPU only when the
calibrated estimate predicts a win:

- CPU cost per quad and per internal pixel is scaled by
  `(12 / per_view_threads)^0.35` from the reference host; multi-view jobs split
  the thread budget across views.
- GPU cost includes device/target setup, readback, per-quad encode/upload/draw,
  and a 10 percent streaming overhead.
- The GPU is rejected without a hardware adapter, with a software adapter,
  above the f32-safe scene span, above the 60 percent VRAM working set, or for
  meshes below 10,000 quads.
- When the process-wide GPU context is not initialized, a single-view job on a
  CPU with eight or more threads is kept on the CPU. Shader compilation and
  device setup cost a few hundred milliseconds, which a one-shot CLI render
  cannot amortize; multi-view jobs amortize it across views, and a warm process
  that already paid setup can still choose the GPU.
- A predicted win requires a 5 percent margin, so setup noise cannot flip the
  decision.

The 60 percent VRAM budget uses the platform probe where available and a
conservative floor otherwise: 2 GiB for discrete adapters and 1 GiB for
integrated or virtual adapters. `BLOCKSCOPE_GPU_VRAM_BYTES` overrides the probe.
Linux reads `/sys/class/drm/*/device/mem_info_vram_total`; Windows and macOS
currently use the floor. The session admits targets, readback, one 32 MiB
staging batch times three slots, and the atlas inside that budget. Larger page
chunks are split into bounded instance uploads.

R1 is the 12-thread Ryzen 5 5500 / GTX 1650 SUPER host, Windows D3D12, release
build, color-only, 1024x1024, supersample 2, `--force-limits`. Render and total
medians are from three iterations; total includes PNG.

| Case | CPU render | auto render | CPU total | auto total | backend used |
|---|---:|---:|---:|---:|---|
| `000002`, 12 threads | 259.0 ms | 224.6 ms | 312.7 ms | 279.4 ms | cpu |
| `000002`, 2 threads | 437.1 ms | 148.3 ms | 491.7 ms | 199.7 ms | gpu |

On 12 threads `auto` keeps the one-shot render exact on the CPU: the warm GPU
render is 190.2 ms, but the first GPU render in a fresh process is 535.5 ms
because it pays shader compilation and device setup. On 2 threads the GPU wins
even with setup. `auto` reports `backendUsed`, so the caller can always tell.

Commands:

```powershell
cargo build --release -p blockscope-cli --features gpu
$exe = (Resolve-Path .\target\release\blockscope.exe).Path
& $exe bench fixtures\private\000002.litematic --backend auto --threads 12 --iterations 3 --force-limits --json
& $exe bench fixtures\private\000002.litematic --backend cpu  --threads 12 --iterations 3 --force-limits --json
& $exe bench fixtures\private\000002.litematic --backend auto --threads 2  --iterations 3 --force-limits --json
& $exe bench fixtures\private\000002.litematic --backend cpu  --threads 2  --iterations 3 --force-limits --json
```

Explicit GPU full-mesh `render` finished a single view in 1.84 s and two views
in 1.71 s wall, inside the 1.3x two-view gate. Twelve `auto` views were batched
by the VRAM budget and finished in 3.42 s with a 1006.7 MB peak. Multi-view jobs
amortize setup across views, so the policy uses the GPU even on a strong CPU;
single-view `auto` keeps the exact CPU on 12 threads.

`000001` auto streaming (`--backend auto`, default `--stream-mesh=auto`) on the
12-thread host selects the CPU for exact one-shot output:

| Case | Wall | Peak working set | Render | Mesh |
|---|---:|---:|---:|---:|
| color-only | 15.03 s | 473 MB | 6.80 s | 5.45 s |
| client jar | 20.42 s | 619 MB | 12.08 s | 5.33 s |

Explicit `--backend gpu` measured 14.64 s / 502 MB for color-only and
19.83 s / 655 MB for the client jar. The difference is within run-to-run noise,
so `auto` keeps the byte-exact CPU on this strong host. The 2 GB and 3 GB memory
gates pass. The 10 s and 15 s wall gates do not; the remaining cost is page-chunk
meshing and CPU/WBOIT translucent work. These are recorded as not met.

Parity on the 96x96 GPU scenes, D3D12, max channel delta 1:

| Scene | Differing pixels | 0.1% target |
|---|---:|---|
| translucent hybrid | 1,039 / 9,216 (11.3%) | not met |
| opaque/cutout | 842 / 9,216 (9.1%) | not met |
| textured opaque | 245 / 9,216 (2.7%) | not met |

Alpha coverage matches exactly in all three. The spec's 0.1 percent
differing-pixel target is therefore not met on this driver; the maximum channel
delta target (2) is met. The parity tests keep a 15 percent regression cap and
record the measured values instead of hiding them. The CPU backend remains the
byte-exact reference and the explicit exactness override.

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

## Model UV cache (2026-11-07)

`PaletteTables` now caches the Q12-encoded UVs of every model quad once per
resolved palette entry. Mesh building still computes block-local positions, but
it no longer calls `encode_uv` for every emitted block quad.

Full-mesh `000002` with the client jar, CPU, 12 threads, 16 alternating `bench`
runs:

| Path | Mesh median | Range |
|---|---:|---:|
| Re-encode UVs per quad | 1,249.8 ms | 1,110.3-1,433.0 ms |
| Cached Q12 UVs | 1,133.1 ms | 1,019.5-1,318.6 ms |

The cached path is 9.3% lower at the median. The two fixtures checked (`000002`
and `000001`, client jar, 1024x1024, supersample 2) produce byte-identical PNG
SHA-256 values before and after the change.

Caching the scaled corner offsets as well was also measured. It added 48 bytes
per model quad to the working set and reduced the combined gain to 1.3%, so only
the UV cache is kept.

## SIMD investigation (2026-11-07)

The `perf/simd-rasterizer` branch measured whether explicit AVX2 kernels help
the CPU render path. The answer for the current workload is no: the hot loops
are dominated by per-triangle and per-quad setup, not by wide arithmetic over
long contiguous runs. The experimental kernels were reverted rather than kept
as neutral complexity.

### Where the time goes

`000002` with the client jar, CPU backend, `--stream-mesh=on`, 1024x1024,
supersample 2, 12 threads, and `--force-limits`. Stage timings came from a
temporary instrumentation build; the render column is the CLI total.

| Stage | Wall |
|---|---:|
| Project + bin | 804-844 ms |
| Tile rasterization | 1,336-1,343 ms |
| Assembly (encode + copy) | 43-47 ms |
| Translucent resolve | 3 ms |
| Render total | 2,331-2,384 ms |

Mesh building in the same run is about 1.4 s wall. The sampled page breakdown
shows the inner block loop is dominated by model iteration and culling (41%),
quad push and allocation (19%), ambient occlusion (17%), and vertex creation
(12.5%). Each of those is branch-heavy, and the largest SIMD-able share is
well under half of the mesh stage.

### Why per-pixel SIMD does not fill

The rasterizer was instrumented to count triangle invocations, rows, and
non-empty rows on `000001` with the client jar:

| Counter | Value |
|---|---:|
| Triangle rasterize calls | 142,936,956 |
| Rows visited | 735,406,071 |
| Non-empty rows | 334,038,619 |
| Pixels tested | 597,506,052 |

That is 1.79 tested pixels per non-empty row. A four-lane row kernel almost
always falls through to its scalar tail, so it cannot repay its setup cost.
The same distribution holds on `000002` (133.0M tested pixels over 33.7M
triangles). SIMD across triangles would need a batched setup that preserves
draw order; that is a rasterizer redesign, not a local kernel change.

### Rejected attempts

| Attempt | Result | Verdict | Why |
|---|---|---|---|
| AVX2 four-pixel row rasterizer | `000002` render 2,110-2,201 ms vs baseline 2,057-2,273 ms | reverted | No stable win; rows average under two pixels. |
| AVX2 four-vertex quad projection | Kernel 2.897 ms -> 2.773 ms per 200k quads (4.5%); project CPU 3,612 ms -> 3,672 ms end to end | reverted | The kernel is a small part of projection, and the target-feature call boundary erased the gain. |
| Batched two-quad / four-triangle setup | `000002` tile 1,800-2,033 ms vs baseline 1,356-1,622 ms; render 2,807-3,148 ms vs 2,374-2,745 ms | reverted | Copying triangle inputs and prepared setups between passes cost more than the shared bounds/edge work; the single-triangle path was already inlined and kept inputs in registers. |
| AVX2 four-triangle setup kernel | 400k batches: scalar 27.6 ms, AVX2 40.2 ms | reverted | Lane assembly, stores, and scalar clipping outweighed the four-wide arithmetic on the reference Zen 2 host. |
| Six-face cull mask in `PageGrid::fill` | Full-mesh median 1,152.6 ms vs 1,144.1 ms over 8 alternating `bench` runs | reverted | Six face rules per cell is more neighbour work than the ~3.75 model quads per cell consume; the existing `occludes` byte check already covers most culls. |
| Cached Q12 UVs plus scaled corner offsets | Full-mesh median 1,253.4 ms vs 1,269.4 ms over 16 alternating runs | offset half reverted | The extra 48 bytes per quad offset most of the UV-only win; keep the UV cache alone. |
| SIMD ambient occlusion / vertex creation | Not implemented | rejected by measurement | Sampled shares are 17% and 12.5% of the block loop; gather-based AO is not faster than L1 scalar loads on the reference Zen 2 host. |

### Follow-up options

The measurements point at algorithmic work rather than wider vectors:

- Keep explicit SIMD for bulk image paths only if they grow beyond the current
  assembly and downsample costs, which are under 50 ms per render.
- Do not retry four-triangle batching without a copy-free design that keeps the
  scalar setup inlined; the measured version regressed tile time by 33-47%.
- Do not retry a full six-face cull mask without a vectorized mask build; the
  scalar version does more neighbour work than the block loop saves.
