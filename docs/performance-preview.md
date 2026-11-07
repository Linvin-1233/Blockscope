# Server preview latency spike (2026-11-07)

This page records the server-preview latency spike. The older full-render and
GPU frame session baselines are in [performance.md](performance.md).

This spike measured the existing `render` paths that a future `preview` request
would reuse. It did not implement preview. The goal was the `server-preview`
gate: `000002` at 384x384, warm cache, p95 under 300 ms.

Environment: Windows 11, Ryzen 5 5500 (6C/12T), 47.9 GB RAM, GTX 1650 SUPER
4 GB, release `blockscope 0.4.1`, client jar `26.2-Fabric.jar`, 12 threads,
384x384, supersample 1, `views=[45]`, `elevation=30`. `serve` used
`--cache 1 --force-limits`; warm cases used `streamMesh=off`, streaming cases
used `streamMesh=on`. Peak working set was read from
`GetProcessMemoryInfo(PeakWorkingSetSize)`.

### Warm, cold, and streamed paths

| Case | Cold total | Steady p50 | Steady p95 | Render p50 | PNG p50 | Peak WS |
|---|---:|---:|---:|---:|---:|---:|
| small fixture (textured), warm | 115.0 ms | 13.7 ms | 14.7 ms | 5.0 ms | 7.1 ms | 22.7 MB |
| `000002`, full mesh warm | 3,270.6 ms | 1,602.2 ms | 1,817.0 ms | 1,474.7 ms | 8.6 ms | 5,961 MB |
| `000002`, streamed | 3,900.2 ms | 3,494.5 ms | 3,546.7 ms | 1,655.8 ms | 7.6 ms | 216 MB |
| `000001`, full mesh warm | 14,416.4 ms | 9,390.3 ms | 10,543.4 ms | 9,326.1 ms | 10.0 ms | 17,506 MB |
| `000001`, streamed | 16,027.9 ms | 13,646.7 ms | 13,836.5 ms | 8,617.0 ms | 8.9 ms | 545 MB |

`000002` has 39.82M triangles; `000001` has 128.63M. The small fixture has
11,478 triangles and 2,079 blocks. Warm p50/p95 use 20 requests for `000002`
and the small fixture, and 10 requests for `000001`; streamed rows use 5
requests. The cold column is the first request in a fresh worker; it includes
parse, mesh build, and the first texture snapshot.

### Resolution scaling on `000002`

Warm cache, full mesh, 5 requests per size and backend:

| Size | CPU wall p50/p95 | CPU render p50 | GPU wall p50/p95 | GPU render p50 |
|---:|---:|---:|---:|---:|
| 128x128 | 1,602.9 / 1,804.2 ms | 1,479.4 ms | 2,616.8 / 3,195.3 ms | 2,494.7 ms |
| 192x192 | 1,674.4 / 1,707.5 ms | 1,506.9 ms | 2,765.7 / 2,787.7 ms | 2,607.7 ms |
| 256x256 | 1,650.5 / 1,711.4 ms | 1,523.8 ms | 2,666.9 / 2,757.1 ms | 2,544.9 ms |
| 384x384 | 1,701.1 / 1,769.5 ms | 1,561.9 ms | 2,632.2 / 2,650.9 ms | 2,494.4 ms |

The render is geometry-bound, not fill-bound: dropping from 384x384 to
128x128 changes CPU render time by about 5%. Lowering the preview resolution
does not rescue the stateless design.

### GPU and resident-session context

The ordinary GPU render path is slower than CPU at these sizes: `000002` took
2.52-3.16 s and `000001` took 16.0-17.2 s wall on the GTX 1650 SUPER. It
repeats setup/upload work for each request.

A textured `GpuFrameSession` was measured with the client jar. `000002` at
128x128 failed before rendering:

```text
BACKEND_UNAVAILABLE: resident frame session needs 3662199664 bytes,
60% VRAM budget is 1288490188
```

The resident instance table dominates and is resolution-independent, so higher
resolutions cannot fit either. `000001` textured was not attempted; its
color-only instance table already needs about 7.27 GB, above the same 1.29 GB
budget. Textured `000002` therefore needs roughly a 6-8 GB card, with 8 GB
safe; `000001` needs more still. The 4 GB reference card cannot do it.

Run the diagnostic with:

```powershell
cargo run --release -p blockscope-cli --features gpu \
  --example frame_session_texture_bench -- \
  fixtures/private/000002.litematic fixtures/private/texture/26.2-Fabric.jar 128 128
```

The diagnostic uses `sphere_fit: true`, 60 yaw frames, and two warm-up frames.

The color-only resident session is much smaller: a fresh 12-frame 256x256 run
measured 325 ms (about 27 ms/frame) with 201.9 MB resident, and the `animate`
CLI measured 60 color-only frames at 128x128 in 2,989 ms (about 50 ms/frame).
The 12-frame table in [performance.md](performance.md) reports loop totals
for both columns; 413.5 ms is not a per-frame value.

### Color-only blockout preview

The color-block catalog removes the client-jar model detail. `000002` drops
from 39.82M triangles to 2.30M; `000001` drops from 128.63M to 91.57M.

| Case | Cold total | Warm p50 | Warm max | Render p50 | Peak WS |
|---|---:|---:|---:|---:|---:|
| `000002` color, 384x384 | 651-733 ms | 208-213 ms | 265 ms (20 reps) | 80-83 ms | 333 MB |
| `000001` color, 384x384 | 9,554 ms | 4,820-4,986 ms | 4,986 ms (3 reps) | 4,756-4,921 ms | not recorded |

`000002` color-only meets the 300 ms gate at 384x384 on the CPU, with no client
jar and no GPU. `000001` does not. The color-only session is an optional
latency optimization for `000002`-class structures; it is not required to meet
the gate.

### Verdict

- Textured preview is not viable at 384x384 on the 4 GB reference host:
  stateless p95 is 1.77-1.82 s; the resident session returns
  `BACKEND_UNAVAILABLE` for `000002` because the textured instance table needs
  3.66 GB against a 1.29 GB budget. A larger GPU is required for textured
  `000002`; `000001` needs more still.
- Color-only blockout preview **meets** the 300 ms gate for `000002` at
  384x384 (p95 236-265 ms, 333 MB peak) on the CPU. It does not show textures,
  so it is a framing/rotation preview, not a final-look preview.
- `000001` remains non-interactive in every mode measured: 4.8-5.0 s
  color-only, 9.4-10.5 s textured.
- Recommended direction: a color-only blockout preview with a hard color-mesh
  geometry budget. `000002`-class structures work; `000001`-class structures
  are rejected. A resident session at 256x256 is an optional lower-latency
  mode; textured preview is opt-in only on hardware with enough VRAM, and
  `000001`-class structures stay batch `--draft`.

Raw results: `out/preview-spike/results.json` and
`out/preview-spike/resolution.json`. Reproduction scripts:
`out/preview-spike/measure.py`, `resolution.py`, `color_preview.py`, and
`color_000001.py`.
