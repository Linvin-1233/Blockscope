# Performance Baselines

This page records measured memory and timing baselines. The allocation test is
the regression gate; the CLI numbers are diagnostics for changes that cannot be
measured inside the test process.

## Allocation peak test

`crates/litematic/tests/memory.rs` measures peak heap allocation for
`LitematicFile::parse_bytes` and `parse_bytes + into_scene`. It uses a
process-global `GlobalAlloc` meter, so all scenarios live in one binary and run
sequentially.

```bash
cargo test -p litematic-core --test memory -- --nocapture
```

The fixtures are synthetic and compressed before the measured window. The
`512^3` sparse fixture uses custom volume limits because the default 100M
per-region limit rejects it. This is intentional: it measures the representation
cost, not the admission policy.

Baseline on 2026-10-28, Windows x86_64, 48 GB host, `cargo test` debug profile:

| Fixture | Blocks | Parse peak | Parse + scene peak |
|---|---:|---:|---:|
| tiny `2^3` dense | 8 | 76,406 B | 76,406 B |
| sparse `256^3`, 1 stone | 16.8M volume | 12,588,227 B | 12,588,227 B |
| sparse `512^3`, 1 stone | 134.2M volume | 100,668,611 B | 100,668,611 B |
| dense `256^3` stone | 16.8M | 12,588,226 B | 12,588,226 B |

Re-verified after the review fixes on 2026-10-28: all four rows are
byte-identical. The new `crates/nbt/tests/memory.rs` also pins the decoder's
pre-allocation guard: a truncated NBT document that declares a 64M-element int
array or a 1M-element list must stay below 1 MiB of allocation instead of
reserving the declared size.

The sparse peak scales with bounding-box volume: `512^3` is 8x `256^3`, which
is exactly the volume ratio. `parse+scene` adds no measurable peak because the
packed array is moved into the scene rather than copied.

## CLI diagnostics

Measured with the release binary on the same host. `000001` is the 9.39M-block,
`211^3` fixture. `render` used a color-only catalog, 1024x1024, supersample 2,
12 threads, and `--force-limits` because the default 5M-quad cap rejects the
exact mesh. The rows below were re-measured on 2026-10-28 after the review
fixes; peak working set is sampled every 20 ms, so expect roughly 10-15%
run-to-run variation on the short `validate` cases.

| Command | Peak working set | Notes |
|---|---:|---|
| `validate 000001` | 103 MB | parse + scene |
| `validate 000002 --force-limits` | 176 MB | 50.65M blocks; 176-178 MB observed |
| `validate 000003 --force-limits` | 145 MB | 67.1M blocks; 140-145 MB observed |
| `validate 000004 --force-limits` | 140 MB | 67.1M blocks; 122-149 MB observed |
| `render 000001 --stream-mesh --force-limits` | 433-435 MB | color-only, 11.1-11.4 s wall, 91.5M triangles |
| `render 000001 --stream-mesh=off --force-limits` | 7.68 GB | color-only, 11.9 s wall, same PNG as streamed |
| `render 000001 --force-limits --client-jar` | **591 MB** | exact translucent spill, 17.0 s wall, same PNG hash |
| `render 000002 --force-limits --client-jar` | 216 MB | default auto streaming, 4.1 s wall |
| `bench 000001 --force-limits` | 6.8 GB | historical full-mesh run; not re-measured |
| `bench 000002 --force-limits --client-jar` | 3.8 GB | historical full-mesh run; not re-measured |
| synthetic sparse `1024^3`, 1 block, `--force-limits` | 495 MB | historical `validate` run; not re-measured |

The older table rows for `validate 000002`/`000003`/`000004` (132/130/115 MB)
and the 407 MB / 8.8 s color stream were from an earlier build and host state.
Both the current build and the pre-review `HEAD` binary measure 176-178 MB for
`000002` and 11.6-11.8 s for the color stream on this host, so the differences
are not review regressions.

The textured `000001` stream is 591 MB after the spill path was enabled; the
earlier in-memory path peaked at 1.52 GB before spilling. The material set
itself is about 227 MB; most of the remaining difference is the mesh chunk,
resident tiles, and translucent triangles. The client-jar PNG SHA-256 is
identical between `HEAD` and the review build.

The 18.9 GB figure in `docs/rendering.md` is the client-jar full-mesh path with
more quads, not the streamed render path. The re-measured streamed color path is
433-435 MB and the current full color-only path is 7.68 GB. This is why the
sparse parser is a conditional spike rather than a committed rewrite.

## Review-fix performance check (2026-10-28)

The review fixes do not change the single-view render hot path. The pre-review
`HEAD` binary (`6b5a680`) and the current build were compared on the same host
with the same commands:

| Case | `HEAD` | Current |
|---|---:|---:|
| `000001` color stream, 1024x1024 ss2, 12 threads | 11.65-11.75 s, 434-438 MB | 11.1-11.4 s, 432-435 MB |
| `000001` client jar stream, same settings | 16.98 s, 591.0 MB | 17.00 s, 590.8 MB |
| PNG SHA-256, client jar | `7D7E583F...C5D687` | `7D7E583F...C5D687` |
| NBT allocation test rows | 76,406 / 12,588,227 / 100,668,611 / 12,588,226 B | identical |

The color-stream difference is within host noise; no path regressed. The
`000001` client-jar image is byte-identical. The current full-mesh color path
measured 7.68 GB / 11.9 s and produced the same PNG SHA-256 as the streamed
path; it was not re-run on `HEAD` because the full-mesh path is unchanged and
costs about 8 GB per run.

Multi-view rendering now batches by a 256 MiB output-byte budget and writes each
view before starting the next batch. At 1024x1024 (4 MiB RGBA) and 2048x2048
(16 MiB RGBA), the thread budget remains the limiting factor, so normal
multi-view parallelism is unchanged. At 4096x4096 (64 MiB RGBA), a batch is
capped at four views instead of retaining every output; this is the intended
memory tradeoff and is covered by `view_batch_budget` unit tests.

The supervisor hashes inputs on a dedicated thread instead of its event loop,
and reuses a caller-supplied `input.hash` for routing. This is a latency
improvement; no end-to-end render throughput table is claimed for it because the
renderer remains the bottleneck.

## Textured render memory

Texture memory is not the dominant cost. For `000001` with the client jar:

- the material set is about 227 MB;
- 12.59M translucent triangles were retained for the per-tile depth sort;
- each `Triangle` was 112 bytes, so the retained triangles were about 1.41 GB;
- streamed rendering peaked at 2.28 GB before the compact layout.

`Triangle` now stores projected screen points as f32 and no longer stores the
always-translucent layer tag. The depth key stays f64, so the sort order is
unchanged; the struct is 72 bytes and the regression test in
`crates/render/src/rasterizer/project.rs` pins that ceiling. The in-memory
translucent path measured 1.52 GB before spilling was implemented.

Above 262,144 retained triangles, the streamed renderer spills exact records to
temporary bucket files and processes one bucket at a time. Re-measured after the
review fixes, `000001` with the client jar peaks at **591 MB** in 17.0 s with
the same PNG SHA-256 as `HEAD` (the previous 618 MB / 16.2 s row was from an
earlier host/build state). The temporary directory is removed when the render
finishes.

`render` defaults to `--stream-mesh=auto`, which streams like `on`;
`--stream-mesh=off` restores the full-mesh path. This is what avoids the
18.9 GB full-mesh peak without requiring a flag.

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

Use `bench` with the same file and without `--stream-mesh` to record the
full-mesh path. That path is intentionally not part of the default CI gate; it
is a diagnostic for deciding whether streaming should become the default.

## Sparse spike

The prototype is `crates/litematic/examples/sparse_probe.rs`. It scans the gzip
stream twice: the first pass records region headers and `BlockStates` offsets,
and the second pass decodes the packed words without retaining the decompressed
document or the long array. It counts non-zero palette values; it does not build
a scene or validate palette indices yet.

```bash
cargo run --release -p litematic-core --example sparse_probe -- path\to\file.litematic --dense
```

Measured on 2026-10-28 with the release example:

| File | Dense parse peak | Stream probe peak | Dense parse | Stream decode |
|---|---:|---:|---:|---:|
| synthetic sparse `512^3`, 1 block | 100,668,608 B | 141,739 B | 48.4 ms | 97.7 ms |
| synthetic sparse `1024^3`, 1 block | 805,311,680 B | 141,739 B | 354.6 ms | 689.5 ms |
| `000001` (9.39M blocks) | 83,846,504 B | 141,711 B | 120.4 ms | 20.3 ms |

The probe peak is constant at about 142 KB. The sparse case is about 2x slower
because it decodes every value from a second gzip pass; the dense fixture is
faster because it avoids building the NBT tree.

**Decision: go for the integration spike follow-up.** The peak reduction is far
above the 50 percent bar and the sparse parse time is under 2x. This is not yet
a production parser: the next step is a scoped implementation plan for
`RegionStates`, canonical scene hashing, and parity tests. The probe's
`occupied_nonzero` counter is not the same as `non_air` when the palette has a
non-zero default entry; the production path must use the palette default and
preserve diagnostics.
