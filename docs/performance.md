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

The sparse peak scales with bounding-box volume: `512^3` is 8x `256^3`, which
is exactly the volume ratio. `parse+scene` adds no measurable peak because the
packed array is moved into the scene rather than copied.

## CLI diagnostics

Measured with the release binary on the same host. `000001` is the 9.39M-block,
`211^3` fixture. `render` used a color-only catalog, 1024x1024, supersample 2,
12 threads, and `--force-limits` because the default 5M-quad cap rejects the
exact mesh.

| Command | Peak working set | Notes |
|---|---:|---|
| `validate 000001` | 108 MB | parse + scene |
| `validate 000002 --force-limits` | 132 MB | 50.65M blocks |
| `validate 000003 --force-limits` | 130 MB | 67.1M blocks |
| `validate 000004 --force-limits` | 115 MB | 67.1M blocks |
| `render 000001 --stream-mesh --force-limits` | 407 MB | color-only, 8.8 s wall, 77.6M quads |
| `render 000001 --force-limits --client-jar` | 1.52 GB | in-memory translucent path, 117.1M triangles |
| `render 000001 --force-limits --client-jar` | **618 MB** | exact translucent spill, same PNG hash |
| `render 000002 --force-limits --client-jar` | 216 MB | default auto streaming |
| `bench 000001 --force-limits` | 6.8 GB | full mesh, color-only, 4.2 s mesh |
| `bench 000002 --force-limits --client-jar` | 3.8 GB | full mesh, 4.09 GB mesh bytes |
| synthetic sparse `1024^3`, 1 block, `--force-limits` | 495 MB | `validate`; 1.07B volume |

The textured `000001` stream dropped from 2.28 GB to 1.52 GB after compacting
translucent triangles. The material set itself is about 227 MB; most of the
remaining difference is the mesh chunk, resident tiles, and 12.6M translucent
triangles. On the previous build, 000001 differed from the f32-point build by
2 pixels out of 1,048,576 (maximum channel delta 4); all golden fixtures remain
byte-identical.

The 18.9 GB figure in `docs/rendering.md` is the client-jar full-mesh path with
more quads, not the streamed render path. The measured streamed path is about
407 MB; the full color-only mesh is 6.8 GB. This is why the sparse parser is a
conditional spike rather than a committed rewrite.

## Textured render memory

Texture memory is not the dominant cost. For `000001` with the client jar:

- the material set is about 227 MB;
- 12.59M translucent triangles were retained for the per-tile depth sort;
- each `Triangle` was 112 bytes, so the retained triangles were about 1.41 GB;
- streamed rendering peaked at 2.28 GB before the compact layout.

`Triangle` now stores projected screen points as f32 and no longer stores the
always-translucent layer tag. The depth key stays f64, so the sort order is
unchanged; the struct is 72 bytes and the regression test in
`crates/render/src/rasterizer/project.rs` pins that ceiling. Streamed `000001`
with the client jar dropped to 1.52 GB.

Above 262,144 retained triangles, the streamed renderer spills exact records to
temporary bucket files and processes one bucket at a time. `000001` with the
client jar drops to **618 MB** with the same PNG SHA-256; the render adds about
1.6 seconds of temporary-file I/O. The temporary directory is removed when the
render finishes.

`render` now defaults to `--stream-mesh=auto`: fixed-LOD renders stream, and
`--lod auto` keeps the full mesh because selecting a factor needs a complete
mesh. `--stream-mesh=off` restores the full-mesh path. This is what avoids the
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
