---
title: "Service Mode"
---

`serve` speaks newline-delimited JSON over stdin/stdout. It does not listen on a network port. It is designed to be spawned as a child process by a trusted upper-layer service, batch tool, or automation pipeline.

## Start a Worker

```bash
blockscope serve --client-jar client.jar --threads 4 --cache 2
```

- `--threads`: default thread count for requests that omit `threads`; capped at 256
- `--cache`: maximum number of parsed scenes/meshes to keep
- `--cache-bytes`: estimated cache budget in bytes; `0` uses half of available memory. The oldest entries are evicted first; one oversized entry is still allowed to remain. When started by `supervise` without an explicit value, the supervisor derives it from the per-worker memory ceiling
- `--greedy-mesh`: merge coplanar unit faces with identical material and lighting; disabled by default
- `--ffmpeg`: external ffmpeg executable for MP4/GIF jobs; default is `ffmpeg` on `PATH`
- `--force-limits`: raise resource caps; trusted input only
- `--ignore-block-entity-nbt`: skip container NBT

The worker prints one `hello` line after materials are ready, then accepts jobs.
`hello.backends` lists `cpu`, `auto`, and (when compiled) `gpu`. Requests default
to `auto`: the worker uses the CPU for small scenes and the GPU only when the
hardware adapter, f32 precision guard, 60 percent VRAM budget, and measured cost
model predict a win. `cpu` is the byte-exact override; `gpu` forces hardware and
fails with `BACKEND_UNAVAILABLE` when none is usable. Every result reports the
backend that produced the image in `statistics.backendUsed`.

## Request Example

```json
{
  "type": "render",
  "id": "job-1",
  "input": {
    "path": "/data/model.litematic"
  },
  "views": [45, 225],
  "width": 1024,
  "height": 1024,
  "supersample": 2,
  "outputDir": "/data/out"
}
```

- `input.path` and `outputDir` must be absolute
- The input must be a regular file
- `input.hash` is optional and is a caller-asserted SHA-256 identity; on a cache hit the worker trusts it and does not read the file, on a miss it recomputes and verifies the digest
- `supervise` injects the hash it computed for routing, so repeated inputs also take the no-read cache path
- An empty `views` array defaults to `[45]`; at most 64 angles are accepted; multiple views render concurrently when a complete mesh is available
- `backend` is `cpu`, `gpu`, or `auto`; default `auto`. `cpu` is byte-exact, `gpu` returns `BACKEND_UNAVAILABLE` when no hardware adapter is usable, and `auto` may return approximate GPU output. Match the returned `statistics.backendUsed` when exactness matters
- `streamMesh` is `off` (default), `on`, or `auto`. It builds and renders page chunks without retaining the full mesh and supports one view per request. `off` keeps the cacheable full-mesh path; streaming requests do not populate the scene/mesh cache
- A GPU allocation, driver, precision, or VRAM-budget failure under `auto` falls back to the CPU before any output is written. A failure after the first GPU draw is fatal and never produces a partial PNG
- Output files are `{id}.png` or `{id}-{yaw}.png`; callers must keep `id` and `outputDir` unique across concurrent jobs. Duplicate ids or yaw values overwrite the same path
- Full fields, results, and error codes are in [Engine protocol v4](/engine-protocol)

## Video Jobs

Set `format` to `mp4` or `gif` and provide `animation`:

```json
{
  "type": "render",
  "id": "clip-1",
  "input": { "path": "/data/model.litematic" },
  "format": "mp4",
  "background": "#000000",
  "width": 1024,
  "height": 1024,
  "animation": {
    "keyframes": [
      { "time": 0, "yaw": 45, "elevation": 30 },
      { "time": 6, "yaw": 405, "elevation": 30 }
    ],
    "fps": 30,
    "loop": true,
    "easing": "linear"
  },
  "encoder": "auto",
  "quality": "medium",
  "outputDir": "/data/out"
}
```

- Video defaults to an opaque black background. MP4 also requires even dimensions; GIF accepts `transparent`.
- `views`, `streamMesh`, and explicit `elevation` are rejected for video jobs.
- `encoder` is `auto` (default), `software`, or a hardware family such as `nvenc`. `auto` probes at the requested size and falls back to `libx264`; explicit hardware requests fail with `ENCODER_UNAVAILABLE`.
- Output is `{id}.mp4` or `{id}.gif`. The result has `kind: "video"` and reports frames, fps, duration, bytes, the encoder actually used, and render/encode timings.
- A missing or too-old ffmpeg appears in `hello.video` with `available: false`; video requests then fail before the input is parsed.

## Supervisor

`supervise` manages a worker pool, queue, timeouts, and memory ceilings:

```bash
blockscope supervise \
  --client-jar client.jar \
  --workers 2 \
  --threads 4 \
  --timeout 120 \
  --memory 2048 \
  --queue 4 \
  --cache 2
```

| Option          | Description                                                                                   |
| --------------- | --------------------------------------------------------------------------------------------- |
| `--workers`     | Worker process count, at most 256; `0` uses the `capacity` suggestion                         |
| `--threads`     | Threads per worker; `0` uses the `capacity` suggestion                                        |
| `--timeout`     | Per-image-job timeout in seconds; `0` disables                                                |
| `--video-timeout` | Per-video-job timeout in seconds; default 600; `0` disables                                 |
| `--ffmpeg`      | External ffmpeg executable passed to every worker                                             |
| `--memory`      | Per-worker memory ceiling in MiB; `0` derives it from capacity                                |
| `--queue`       | Queue length; `0` uses `workers * 2`                                                          |
| `--cache`       | Maximum cached scenes/meshes per worker                                                       |
| `--cache-bytes` | Estimated cache budget per worker in bytes; `0` derives half of the per-worker memory ceiling |
| `--greedy-mesh` | Enable greedy mesh merging in every worker; disabled by default                               |

## Security Boundary

Use `serve` and `supervise` only with trusted callers. Input paths, output directories, and resource-pack paths are controlled by the caller. Do not expose the stdio service directly to untrusted network clients.

## See Also

- [Engine protocol v4](/engine-protocol)
- [Docker](./docker.md)
- [Troubleshooting](./troubleshooting.md)
- [中文](/zh/user/service)
