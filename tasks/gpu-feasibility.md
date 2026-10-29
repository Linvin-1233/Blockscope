# GPU rendering (issue #16)

**Status:** implemented on branch `feat/gpu` as an opt-in backend. The CPU path
remains the default, the golden reference, and the only backend used by the
official static musl releases.

**Question:** can Blockscope render through `wgpu` (Vulkan / Metal / D3D12),
and should it?

## Verdict

Technically feasible, and useful for weak CPUs paired with a GPU, but not a
replacement for the CPU rasterizer. The implementation is offscreen and
opt-in for three reasons:

1. **Output is not byte-exact.** `docs/rendering.md` promises deterministic
   RGBA/PNG bytes on the CPU path, and golden tests assert byte equality. GPU
   rasterization cannot reproduce the f64 CPU rasterizer across vendors and
   drivers. On Metal the parity tests differ on at most 0.08% of pixels with a
   maximum channel delta of 1.
2. **The parallel CPU path is already fast.** On an M4 Max the 16-thread CPU
   rasterizer beats the integrated GPU; the GPU wins against one or two CPU
   threads by roughly 3-8x. The workload also spends significant time in mesh
   construction and projection, which stay on the CPU.
3. **Deployment is not universal.** Static musl binaries cannot load a Vulkan
   driver at runtime, headless containers need a GPU and driver packages, and
   hosted CI runners have no hardware adapter. Release artifacts therefore stay
   CPU-only.

## Implemented scope

- `litematic-render` feature `gpu` (`wgpu` 30, `pollster`, `bytemuck`),
  disabled by default.
- `RenderBackend::{Cpu, Gpu, Auto}` and `RenderOptions.backend`; statistics
  report `backendUsed`. `Auto` uses the GPU at 50,000 projected quads and falls
  back to the CPU when no hardware adapter is available. An explicit `gpu`
  request fails with `BACKEND_UNAVAILABLE`.
- Process-wide cached device and opaque/translucent pipelines. The first render
  pays adapter and shader setup; later renders do not.
- CPU `f64` projection and backface culling reused from the CPU path, followed
  by page-chunked vertex upload. Opaque/cutout geometry uses depth writes;
  translucent triangles use one global far-to-near sort and blended draws
  without depth writes.
- Shelf-packed RGBA8 atlas with WGSL nearest and bilinear sampling that matches
  the CPU wrap and half-texel rules.
- Premultiplied color target, source-over blending, and a parallel resolve pass
  that box-filters supersamples before dividing by alpha.
- Solid and transparent backgrounds, output texture/buffer limit checks, and
  rejection of software adapters.
- CLI `--backend cpu|gpu|auto` for `render` and `bench`; GPU paths require the
  full mesh and reject `--stream-mesh`. Service request field `backend`,
  `hello.backends`, and `statistics.backendUsed`. CLI output prints the
  effective backend.
- `tile_bench --backend gpu`, parity tests against the CPU image (opaque,
  cutout, translucent, both texture filters, solid/transparent backgrounds),
  and CLI smoke tests. Golden PNGs remain CPU-only.

## Measured performance

`tile_bench` synthetic scene: 96,882 quads, 48,441 after culling, supersample 1.
Apple M4 Max (16 logical cores), macOS 27, Metal, Rust 1.96, release build,
median of 5 iterations after warm-up:

| Internal resolution | CPU 1 thread | CPU 2 threads | CPU 16 threads | GPU |
|---:|---:|---:|---:|---:|
| 1024x1024 | 37.3 ms | 19.6 ms | 4.7 ms | 6.3 ms |
| 2048x2048 | 97.5 ms | 50.0 ms | 10.3 ms | 13.0 ms |
| 4096x4096 | 344.3 ms | 173.7 ms | 36.6 ms | 43.4 ms |

The first GPU render in a process adds about 15 ms for adapter, device, and
pipeline setup. GPU time includes readback and resolve; mesh building,
projection, vertex upload, and PNG encoding remain CPU work. The earlier
standalone probe measured only the draw pass and is superseded by these
end-to-end backend numbers.

On the larger private fixtures from `docs/performance.md`, rasterization is
25-35% of wall time, so even a much faster GPU pass caps end-to-end gains at
about 1.3-1.5x. The GPU is most useful when many views reuse one scene/mesh or
when the host has few fast CPU cores.

## Parity

Same geometry, lighting, textures, and options:

- Opaque/cutout/translucent scene: 259 of 9,216 pixels differ, max channel
  delta 1.
- Textured cube, nearest: bit-identical.
- Textured cube, linear: 44 of 9,216 pixels differ, max delta 1.
- Solid background (opaque and alpha 128): 261 of 9,216 pixels differ, max
  delta 1.

No alpha-coverage mismatches were observed. Edges and blending rounding move by
one channel value; other drivers may move more. Tests use a tolerance, never
byte equality.

## Remaining caveats

- Global translucent ordering differs from the CPU's per-tile ordering for
  intersecting translucent surfaces.
- Depth is f32 on the GPU versus f64 on the CPU, so coplanar ties may choose a
  different quad.
- `tiles`, `pixelsTested`, and `pixelsWritten` are CPU-only counters (zero on
  GPU).
- Multi-threaded GPU renders share one device; wgpu serializes internally. No
  performance guarantee is made for concurrent views.
- Linux GPU builds need a dynamically loadable Vulkan loader and drivers. The
  official musl artifacts do not enable the feature.
- CI has no hardware adapter, so GPU tests skip when `BACKEND_UNAVAILABLE` is
  returned. Run them on a developer machine or self-hosted GPU runner.

## Open deployment question

The implementation is ready to ship as an opt-in build, but enabling it in
release artifacts needs a decision from the issue author:

- Which deployment has a GPU, and what OS/architecture is it?
- Ship glibc GPU artifacts separately, or document a source build?
- Is vendor-dependent GPU output acceptable when the CPU backend remains
  available for exact output?
