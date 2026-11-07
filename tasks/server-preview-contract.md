# Contract: `server-preview`

**Parent spec:** `tasks/server-preview-spec.md`
**Status:** Deferred (2026-11-07): the parent spec's textured latency gate
failed. This is a design record only; do not implement the textured stateless
contract.

This document is the detailed wire, CLI, cache, and test contract for the
server-preview feature. The parent spec owns the objective, assumptions,
latency gate, boundaries, and success criteria. If the two disagree, the parent
spec wins. The cache/memory policy remains useful for a resident-session or
draft-only follow-up; the stateless request and sample plan are not approved.
The spike found a viable **color-only blockout** variant for `000002`-class
structures; it needs a revised material/appearance contract before
implementation.

## RenderPreset v1

`RenderPreset` is a JSON object shared by preview, `render`, and `animate`. It
contains no paths and no performance settings.

```json
{
  "schemaVersion": 1,
  "framing": {
    "sphereFit": false
  },
  "camera": {
    "yaw": 45.0,
    "elevation": 30.0
  },
  "animation": {
    "keyframes": [
      { "time": 0.0, "yaw": 45.0, "elevation": 30.0 },
      { "time": 6.0, "yaw": 405.0, "elevation": 30.0 }
    ],
    "duration": 6.0,
    "fps": 30,
    "loop": true,
    "easing": "linear"
  },
  "appearance": {
    "background": "transparent",
    "lighting": "simple-ao-v1",
    "textureFilter": "nearest"
  }
}
```

Rules:

- `schemaVersion` is required and must be `1`. Unknown versions fail with
  `INVALID_RENDER_OPTIONS`.
- `framing`, `camera`, `animation`, and `appearance` are optional. Missing
  optional values use the same defaults as `RenderOptions` and `Animation`.
  `animation.keyframes` is required whenever `animation` is present.
- `camera.yaw` is finite; `camera.elevation` is within `-89.9..=89.9`.
- `animation.keyframes` follows the video contract: 1..=64 entries, times
  strictly increasing, elevation within `-89.9..=89.9`.
- `animation.duration` defaults to the last keyframe time, must be greater than
  zero and at most 60 seconds, and must cover the last keyframe.
- `animation.fps` is `1..=60`; it is part of the final video contract but does
  not decide preview sample count.
- `appearance.background` accepts `transparent`, `#RRGGBB`, or `#RRGGBBAA`;
  `lighting` accepts `flat-v1` or `simple-ao-v1`; `textureFilter` accepts
  `nearest` or `linear`.
- v1 rejects unknown fields with `INVALID_RENDER_OPTIONS`. Adding a field
  requires `schemaVersion: 2`; this keeps typos from being silently ignored in a
  versioned contract.
- `padding` is intentionally absent. `render` does not expose it publicly, so a
  preset value could not be reproduced by the final job. It can be added to
  `framing` when `render` gains a padding option.

The effective preset echoed by preview is fully populated: missing sections are
filled with defaults, and the output contains only the sections that apply
(static preview: `framing`, `camera`, `appearance`; animation preview:
`framing`, `animation`, `appearance`).

## Preview Request Contract

`preview` uses the same `input` and `outputDir` rules as `render`. Paths must be
absolute; the input must be a regular file; `outputDir` is created if missing.

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `type` | string | required | `"preview"` |
| `id` | string | required | 1-64 characters of `[A-Za-z0-9_-]`, echoed in the result |
| `input.path` | string | required | Absolute path to a `.litematic` |
| `input.hash` | string | optional | Same cache identity semantics as `render` |
| `width` | integer | `384` | `1..=512` |
| `height` | integer | `384` | `1..=512` |
| `yaw` | number | `45` | Static preview only; finite degrees |
| `elevation` | number | `30` | Static preview only; `-89.9..=89.9` |
| `sphereFit` | boolean | `false` | Shared framing; added to `render` and video too |
| `background` | string | `transparent` | Same as `render` |
| `lighting` | string | `simple-ao-v1` | `flat-v1` or `simple-ao-v1` |
| `textureFilter` | string | `nearest` | `nearest` or `linear` |
| `backend` | string | `auto` | `cpu`, `gpu`, or `auto`; resolved once per request; over-budget cold `auto` selects CPU streaming |
| `threads` | integer | worker default | `0` uses the worker default; same cap as `render` |
| `samples` | integer | `1` static, `6` animation | `1..=8` |
| `time` | number | `0` | Animation only; requires `samples: 1`; `0..=duration` |
| `animation` | object | absent | Same shape as the video `animation` object |
| `preset` | object | absent | `RenderPreset` v1; explicit fields override |
| `supersample` | integer | `1` | Must be `1`; values above 1 are rejected |
| `outputDir` | string | required | Absolute directory |

Rejected fields and combinations:

- `views`, `format` other than absent/`"png"`, `encoder`, `quality`, and
  `streamMesh` are rejected with `INVALID_RENDER_OPTIONS`. The server may use
  streaming internally for an over-budget cold single-frame preview; clients
  cannot select it, and it never inserts a mesh into the cache.
- `animation` and explicit `yaw`/`elevation` are mutually exclusive, matching
  video. A static preview omits `animation` and uses `preset.camera`.
- A static preview ignores `preset.animation`. An animation preview is requested
  by an explicit `animation` object or by `samples > 1`; if no explicit
  `animation` is given, `preset.animation` supplies the timeline. `samples > 1`
  with neither source is rejected.
- `samples` greater than 1 without an animation timeline is rejected.
- `animation` with `samples: 1` is allowed and renders `time`, which defaults to
  `0`. `time` without an animation timeline, or with `samples > 1`, is rejected.
- `supersample` above 1, width or height above 512, or samples above 8 fail
  before parsing or meshing.
- `preset.animation` and an explicit `animation` object: the explicit object
  wins. `preset.camera` and explicit `yaw`/`elevation`: explicit values win.

Sample plan:

```text
duration = animation.duration, else last keyframe time
samples = 1: t_0 = time (default 0)
loop=true:  t_i = duration * i / samples,          i in 0..samples
loop=false: t_i = duration * i / (samples - 1),    i in 0..samples, samples >= 2
camera at t_i = Animation::camera_at(t_i)
```

The interpolation, easing, clamping, and keyframe validation are shared with
the video crate; preview does not reimplement them.

## Cache and Mesh Budget

The preview budget is a server-side, process-wide limit:

```text
--preview-max-mesh-bytes BYTES    default 268435456 (256 MiB)
hello.preview.maxColdMeshBytes    the effective value
```

It is not a request field. The upper service cannot raise it, and a preview
request cannot turn a small output into an unbounded full-mesh allocation. The
budget covers mesh heap bytes only; the worker's existing memory ceiling still
applies to the scene, materials, caches, and frame buffers.

Decision tree for every request:

1. **Cache hit.** Use the cached scene and full mesh. Do not apply the
   cold-mesh budget; the memory is already resident. Report `cacheHit: true`,
   `streamed: false`. This is the fast path and the only path that can use the
   GPU for a large structure.
2. **Cache miss, estimated mesh within budget.** Convert the byte budget to a
   conservative quad limit using `size_of::<Quad>()` plus page and vector
   overhead (start with a 1.5x factor). Estimate the quad count with
   `estimate_mesh_quads`. If the estimate is clearly above the budget (for
   example more than 20% over), skip the full count and go to the streaming or
   rejection case. If it is near the boundary, confirm with `count_mesh_quads`
   and a preview-specific `MeshLimits`; the count pass does not allocate geometry
   and stops when the limit is exceeded. Build the full mesh with the hard quad
   limit, insert it into the cache, and render normally. Report
   `cacheHit: false`, `streamed: false`. Report `timings.meshEstimateMs`.
3. **Cache miss, over budget, single-frame preview (`samples: 1`),
   `backend: cpu` or `auto`.** This includes a static camera and a
   `samples: 1` + `time` animation frame. Render with the CPU streaming path
   (`streamMesh=on` internally) and do not insert a mesh into the cache. Report
   `cacheHit: false`, `streamed: true`, `backendUsed: "cpu"`. This bounds memory
   but rebuilds pages on every request.
4. **Cache miss, over budget, single-frame preview, `backend: gpu`.** Reject with
   `RESOURCE_LIMIT`; GPU rendering requires a full mesh.
5. **Cache miss, over budget, `samples > 1`.** Reject with `RESOURCE_LIMIT`.
   Every sample would re-stream the scene, and a six-sample flipbook would pay
   that cost six times. The upper service should use `samples: 1` with `time`
   for scrubbing; a warm cache or a smaller structure enables the flipbook.
6. **Estimate or build exceeds the hard limit.** Return `MESH_LIMIT_EXCEEDED`
   or `RESOURCE_LIMIT` with an actionable message. Do not fall back to an
   unbounded build.

Why this policy:

- Building and caching a multi-gigabyte mesh for a 384x384 preview is the
  failure mode this spec exists to prevent. `000002` is about 2.9 GB and
  `000001` about 11.3 GB in the measured full-mesh compact path; neither fits a
  preview budget.
- Streaming every preview would keep memory flat but would rebuild pages on
  every drag for small structures. The cache is worth keeping for the common
  case.
- A cache hit means the engine already paid the memory. A preview should not
  reject a structure that is already resident just because a cold build would
  exceed the budget.
- Cache entries created by preview use the same cache and may evict final-render
  entries. Run preview traffic in a dedicated `serve` or `supervise` process
  when mixed workloads must not evict each other. The spec does not add a
  separate cache class in this slice.

`streamMesh` remains a rejected request field. The server may choose the
streaming path internally only for case 3, and that path never inserts into the
full mesh cache. This keeps the client contract simple and the memory policy in
one place.

## Preview Result

```json
{
  "type": "result",
  "id": "p2",
  "ok": true,
  "kind": "preview",
  "inputHash": "578cffa3...",
  "sceneHash": "9a1c...",
  "cacheHit": true,
  "streamed": false,
  "frames": 6,
  "outputs": [
    {
      "kind": "image",
      "index": 0,
      "path": "/data/out/p2-preview-0.png",
      "yaw": 45.0,
      "elevation": 30.0,
      "time": 0.0,
      "width": 384,
      "height": 384,
      "bytes": 18234,
      "renderMs": 41.2,
      "pngMs": 2.8
    }
  ],
  "preset": {
    "schemaVersion": 1,
    "framing": { "sphereFit": false },
    "animation": {
      "keyframes": [
        { "time": 0.0, "yaw": 45.0, "elevation": 30.0 },
        { "time": 6.0, "yaw": 405.0, "elevation": 30.0 }
      ],
      "duration": 6.0,
      "fps": 30,
      "loop": true,
      "easing": "linear"
    },
    "appearance": {
      "background": "transparent",
      "lighting": "simple-ao-v1",
      "textureFilter": "nearest"
    }
  },
  "timings": {
    "parseMs": 0.0,
    "meshEstimateMs": 0.0,
    "meshMs": 0.0,
    "renderMs": 247.2,
    "pngMs": 16.8,
    "totalMs": 266.4
  },
  "statistics": {
    "triangles": 1234567,
    "culledBackfaces": 234567,
    "textures": 42,
    "backendUsed": "cpu"
  }
}
```

- `cacheHit` describes the scene/mesh cache for the request. `parseMs` and
  `meshMs` are zero on a hit, matching `render`. `streamed` is true only for
  the over-budget cold single-frame fallback. A streamed result has
  `cacheHit: false`, `streamed: true`, `backendUsed: "cpu"`, and includes
  `meshEstimateMs` when the budget decision required an estimate; it does not
  populate the cache, so a second identical request is also a streamed cold
  miss.
- `outputs` are sorted by `index`; clients must not rely on completion order.
- Static preview outputs omit `time`. Animation preview outputs include it.
- `kind: "preview"` distinguishes the result from a normal render. Each output
  keeps `kind: "image"` so existing image consumers can read it.
- `statistics` describes the first frame and remains advisory.
- `backendUsed` is the same for every sample in the request.

Error behavior:

- All validation errors use the existing codes: `INVALID_REQUEST`,
  `INVALID_RENDER_OPTIONS`, `ANIMATION_INVALID`, `OUTPUT_TOO_LARGE`,
  `BACKEND_UNAVAILABLE`, `RESOURCE_LIMIT`, and `MESH_LIMIT_EXCEEDED`. No new
  error code is introduced for preview caps.
- A malformed preset, unsupported `schemaVersion`, or unknown preset field is
  `INVALID_RENDER_OPTIONS`.
- An over-budget cold flipbook or an explicit `gpu` request on an over-budget
  cold miss is `RESOURCE_LIMIT`.
- A build that exceeds the hard preview mesh limit is `MESH_LIMIT_EXCEEDED`.
  Streaming failures pass through `TEMP_STORAGE_FAILED` or
  `TRANSLUCENT_BUDGET_EXCEEDED`.
- Output write failures are `IO_ERROR`.
- A failed or killed preview may leave earlier output files; the result is still
  an error and the caller owns cleanup.

## CLI Contract

`blockscope render <file> --draft` is the CLI smoke path for a single preview
frame. It is not a new command and it does not reuse the `preview` subcommand
alias, which already means terminal image display.

| Flag | Default | Notes |
| --- | --- | --- |
| `--draft` | off | Applies preview caps: default 384x384, max 512 per side, supersample 1 |
| `--preset PATH` | absent | Load a `RenderPreset` v1 JSON file |
| `--yaw`, `--elevation`, `--sphere-fit` | existing | Explicit flags override preset values |
| `--output PATH` | existing | Same naming as `render` |

Rules:

- `--draft` with `--width` or `--height` above 512 is rejected.
- `--draft` with `--supersample` above 1 is rejected.
- `--draft` with more than one `--views` angle is rejected.
- `--draft` does not change stream-mesh selection, backend, lighting, or
  background; it only caps size and supersampling.
- `--draft` is a one-shot render and does not use the service preview cache or
  the `--preview-max-mesh-bytes` budget. The existing `--stream-mesh` policy
  applies; the default `auto` remains memory-aware. A user who forces
  `--stream-mesh=off` accepts the full render memory cost.
- A draft PNG is byte-identical to a normal `render` at the same options. This
  is the CLI half of the framing-parity test.

`blockscope render --preset` and `blockscope animate --preset` accept the same
JSON object. `render` reads `framing`, `camera`, and `appearance`; `animate`
reads `framing`, `animation`, and `appearance`. Explicit flags override preset
values. The `--json` report includes the effective preset so a UI or script can
round-trip it.

## Service and Supervisor Contract

`hello` gains a `preview` object in both `serve` and `supervise`:

```json
"preview": {
  "available": true,
  "formats": ["png"],
  "maxWidth": 512,
  "maxHeight": 512,
  "maxPixels": 262144,
  "maxSamples": 8,
  "supersample": 1,
  "presetSchemaVersion": 1,
  "maxColdMeshBytes": 268435456,
  "maxStreamSamples": 1,
  "coldMeshFallback": "cpu-stream"
}
```

- `available` is always true for this build; the object exists so clients can
  detect support without guessing from the engine version.
- The size, sample, and supersample caps are static. `maxColdMeshBytes` is the
  effective operator setting; `0` disables cold full-mesh builds and forces all
  cold previews to stream (or reject if they need a full mesh).
- `maxStreamSamples` is `1`: an over-budget cold animation flipbook is rejected,
  and the client should use single-sample `time` requests for scrubbing.
- `coldMeshFallback` is `"cpu-stream"`; a client that sees it knows a large cold
  single-frame preview will be slower and CPU-only, not an error.
- `hello.preview` is additive. `protocolVersion` stays `4`.
- The supervisor's hello includes the same object and a top-level
  `"previewTimeoutSeconds": 15`, matching the existing `timeoutSeconds` and
  `videoTimeoutSeconds` fields.

`render` and video requests gain optional `sphereFit`:

- `false` is the default and preserves existing output.
- `true` matches the CLI `--sphere-fit` behavior.
- Video uses it as the base framing for every frame.
- Existing clients that omit it are unaffected.

Supervisor behavior:

- `serve` and `supervise` gain `--preview-max-mesh-bytes BYTES` (default
  268435456; `0` disables cold full-mesh builds). The supervisor passes the
  effective value to every worker and reports it in `hello.preview`.
- A request with `type: "preview"` uses `--preview-timeout` (default 15
  seconds; `0` disables).
- A preview timeout kills the worker, returns `TIMEOUT`, and does not retry.
  The worker is restarted for subsequent jobs.
- Preview jobs share the queue, memory ceiling, and cache with other jobs. For
  isolation, run a dedicated preview `serve` or `supervise` process with its own
  `--preview-max-mesh-bytes` and cache budget.
- `serve` does not enforce a timeout; that remains the supervisor's job.

## Upper-Layer Integration Contract

The engine processes one job at a time per worker and never preempts a running
job. A camera drag can easily produce dozens of requests per second; if the
upper service forwards every one, the queue fills with stale frames and the UI
shows a late result for an old camera angle. This is a client responsibility,
not an engine feature.

The upper service must implement latest-only behavior:

- debounce camera changes (50-100 ms is a reasonable starting point) and
  coalesce all changes during the debounce window into one request;
- keep at most one in-flight preview per UI session;
- if a change arrives while a request is in flight, store only the latest
  pending state, replacing any earlier pending state;
- when the in-flight result arrives, discard it if its generation is older than
  the latest displayed state, then send the pending state immediately;
- encode a monotonically increasing generation in `id` (for example
  `session-000042`); the result echoes the id, and completion order must not be
  trusted;
- on `BUSY`, back off and retry only the latest state, never a stale queued
  state;
- use `--queue 1` on a dedicated preview supervisor when the UI is the only
  producer, so stale requests cannot accumulate;
- run a dedicated preview worker process when final renders must not be delayed
  by preview traffic.

Sketch:

```text
in_flight = none
pending = none
generation = 0

on_camera_change(state):
    pending = state
    if in_flight is none:
        send(latest(state))

on_send(state):
    generation += 1
    in_flight = generation
    request.id = "session-" + generation

on_result(result):
    if result.id != "session-" + generation:
        discard
    else:
        display(result)
    in_flight = none
    if pending is not none:
        send(latest(pending))
        pending = none
```

The engine does not add a `cancel`, `latest`, or priority message in this slice.
A queued stale preview still consumes a worker until it finishes. Document this
pattern in `docs/user/service.md` and `docs/zh/user/service.md`; the engine
protocol documents only the one-job-at-a-time and id-echo guarantees.

## Testing Strategy

Unit tests in `blockscope-cli`:

- `RenderPreset` parse/serialize round-trip, defaults, schema version, unknown
  fields, invalid ranges, and explicit-over-preset precedence;
- preview cap validation: width, height, supersample, samples, rejected fields;
- sample plan: static, looping, non-looping, one sample, eight samples,
  fractional duration, clamp before/after keyframes;
- cache policy unit tests with a low `--preview-max-mesh-bytes`: under-budget
  builds, over-budget static streaming, over-budget flipbook rejection, and
  over-budget explicit GPU rejection;
- `sphereFit` mapping to `RenderOptions` and video base options;
- result serialization: output order, `time` presence, `frames`, `streamed`,
  `cacheHit`, `meshEstimateMs`, preset echo.

Integration tests with `serve`:

- a static preview writes one PNG and reports `kind: "preview"`, `frames: 1`,
  timings, and the effective preset;
- a six-sample animation preview writes six sorted PNGs and each output's
  `time` matches the documented formula;
- a second preview of the same input reports `cacheHit: true`, `parseMs: 0`,
  and `meshMs: 0`;
- a cold under-budget preview reports `cacheHit: false`, `streamed: false`,
  `meshEstimateMs > 0` (when measured), and the next preview hits the cache;
- a cold over-budget single-frame preview reports `cacheHit: false`,
  `streamed: true`, `backendUsed: "cpu"`, and does not populate the cache; a
  second identical request is still a cold streamed miss;
- a cold over-budget `samples: 1` + `time` animation frame uses the same
  streaming path and reports the requested `time`;
- a `samples: 1` + `time` request picks the interpolated camera at that time,
  and a time outside `0..=duration` is rejected;
- a cold over-budget `samples: 6` request returns `RESOURCE_LIMIT` before
  building a mesh; a warm cache makes the same request succeed;
- a cold over-budget explicit `gpu` request returns `RESOURCE_LIMIT`;
- a synthetic fixture with a low preview budget never builds a full mesh above
  the budget; assert the limit is enforced rather than silently ignored;
- every cap violation fails before parsing or meshing;
- `views`, `streamMesh`, `format: "mp4"`, `encoder`, and `quality` are rejected;
- a response line stays under 1 MiB for the maximum eight outputs and long
  absolute paths;
- `hello.preview` is present in `serve` and `supervise`, including the effective
  `maxColdMeshBytes`;
- a preview timeout returns `TIMEOUT`, is not retried, and the next job
  succeeds on a fresh worker;
- `id` is echoed exactly, and results for different generations are not assumed
  to arrive in request order;
- sample rendering is sequential and the result holds metadata only; the
  latency spike records peak RSS for one sample versus six samples to confirm
  that frame buffers are not accumulated.

Framing parity tests:

- render the same fixture at 512x512, supersample 1, CPU, with explicit
  `backend: "cpu"` through both `preview` and `render` at the same
  `yaw`/`elevation`/`sphereFit`/appearance; assert identical PNG bytes;
- for animation, assert each sampled preview frame is byte-identical to a
  `render` at the same interpolated camera and options;
- repeat with `sphereFit: true` to cover the new field.

CLI tests:

- `render --draft` writes a PNG with 384x384 default dimensions;
- `--draft` rejects 1024 width, supersample 2, and multiple views;
- `render --preset` and `animate --preset` round-trip a preset file;
- explicit flags override preset values;
- existing `render`, `animate`, and terminal `preview` alias tests still pass.

Existing goldens and `cargo test --workspace` must remain unchanged. GPU parity
tests are optional and skip when no hardware adapter is available; the CPU path
is the reference.

## Performance and Documentation

Before implementation, record the latency spike in
`docs/performance-preview.md` as described above. After implementation, record
the same matrix again:

- warm-cache static and six-sample previews at 256x256, 384x384, and 512x512;
- cold under-budget build, cold over-budget static stream, and cold over-budget
  flipbook rejection;
- `000002` and `000001`, CPU and GPU when available;
- material snapshot warm and cold, `parseMs`, `meshEstimateMs`, `meshMs`,
  `renderMs`, and `pngMs` separately;
- one request against an idle worker versus four queued requests through
  `supervise`;
- peak working set and peak VRAM;
- peak working set for one sample versus six samples; the difference should be
  framebuffer-sized, not six full RGBA frames.

Document:

- `docs/engine-protocol.md`: `preview` request/result, `hello.preview`,
  `sphereFit`, cache/stream results, error behavior, supervisor timeout;
- `docs/user/service.md` and `docs/zh/user/service.md`: preview examples,
  caching, `--preview-max-mesh-bytes`, streaming fallback, flipbook rejection,
  isolation guidance, and the upper-layer latest-only/debounce pattern;
- `docs/user/cli.md` and `docs/zh/user/cli.md`: `--draft`, `--preset`, and the
  distinction from the terminal `preview` alias;
- `docs/rendering.md`: the preview/final parity contract, the cold mesh budget,
  the CPU streaming fallback, and the approximate nature of preview sampling;
- `CHANGELOG.md`, `CHANGELOG.en.md`, and `tasks/todo.md`.
