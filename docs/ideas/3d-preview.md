# Server-Side 3D Preview

**Status:** Deferred (2026-11-07): the textured stateless design failed the
latency gate; color-only blockout is the recommended future direction.
**Date:** 2026-11-07
**Decision:** Direction C from the 2026-11-07 discussion: server-side preview first. The engine remains headless; the upper-layer service owns the UI.

## Problem Statement

How might we let an upper-layer service offer low-latency camera and keyframe
tuning without turning the engine into a GUI, an HTTP server, or a streaming
media server?

The engine currently accepts full render and video jobs. The upper service has no
cheap way to show a user what a new yaw, elevation, or keyframe timeline looks
like before committing to a final render. Reusing `render` at full quality is
too slow for dragging, and reimplementing the camera in the client would drift
from the engine's framing.

## Recommended Direction

Add a **stateless preview request** to the existing stdio protocol. A preview
returns one or more low-resolution PNGs written to `outputDir`, reuses the same
scene/mesh cache and camera math as `render`, and accepts the same camera and
keyframe fields plus a small sample count for animation. The upper service
builds its own UI and sends a preview request when the user releases a drag or
changes a keyframe. This keeps the engine headless, cross-platform, and free of
sockets while still making angle and rotation-speed tuning feel interactive.

Preview is explicitly approximate and hard-capped: at most 512x512, supersample
fixed at 1, at most eight sampled frames, and a fixed wall-time budget. It must
not change final render output, consume video encoders, or compete with final
jobs on the same worker. Advertise the capability in `hello.preview` so clients
can detect it without a protocol-version bump. Preview jobs should run in a
dedicated worker or supervisor pool; the one-job-at-a-time worker invariant
stays intact.

Introduce a shared `RenderPreset` v1 that carries camera, keyframes, framing,
background, and lighting without file paths. Preview echoes the effective preset
in its result, and `render` / `animate` accept the same object. This is the piece
that prevents the upper UI, preview, and final render from drifting. A
persistent GPU frame session is phase 2, only if the stateless path misses the
latency target.

**Trade-off:** this direction does not remove CLI typing for the maintainer's own
angle tuning; it enables the integrator's users to tune angles in the
integrator's UI. If the primary goal were the maintainer's own workflow, the
preset-plus-viewer direction would be cheaper. Direction C is the most expensive
of the three and is justified only by the service-integration use case.

## Spike Outcome (2026-11-07)

The textured stateless path was measured before implementation. `000002` at
384x384, warm cache, p95 is 1.77-1.82 s, about 6x over the 300 ms gate;
`000001` is 10.5 s. Dropping `000002` to 128x128 changes render time by about
5%, so lower resolution does not help. The path is geometry-bound. Streaming
bounds memory (216 MB / 545 MB versus 5.96 GB / 17.5 GB for full meshes) but is
slower (3.5 s / 13.6 s). The ordinary GPU path is slower than CPU at these
sizes.

A textured resident session also fails on the 4 GB reference card: `000002`
needs 3.66 GB of resident instance data against a 1.29 GB VRAM budget. A larger
GPU is required; `000001` needs more still.

The **color-only blockout** variant is viable: `000002` at 384x384, warm cache,
p95 is 234-265 ms with 334 MB peak on the CPU, meeting the 300 ms gate.
Its color mesh heap is 182 MiB, within the 256 MiB cold-mesh budget; `000001`
is 6.48 GiB, far above it. The built-in catalog still uses the magenta fallback
for unknown states (8.96M blocks on `000002`), so this is a geometry/framing
preview, not a color-accurate one. `000001` color-only is still 4.8-5.0 s. A
color-only resident session at 256x256 is about 27 ms/frame with 201.9 MB
resident.

Recommended direction: a **color-only blockout preview** with a hard color-mesh
geometry budget (`000002`-class works, `000001`-class rejects), optionally a
256x256 resident session for lower latency, and textured preview only as an
opt-in on GPUs with enough VRAM. `000001`-class stays batch `--draft`. Full
measurements are in `docs/performance-preview.md`; the spec is
`tasks/server-preview-spec.md`.

The MVP and Not Doing sections below describe the original textured design and
are retained as context. A future issue should use the color-only blockout
scope; the measurements are in `docs/performance-preview.md`.

## Key Assumptions to Validate

- [x] **Warm-cache preview latency (textured failed, color-only passed
      2026-11-07).** At 384x384, supersample 1, the p95 preview latency is
      under 300 ms on the reference host after the scene and mesh are cached.
      Textured `000002` measured 1.77-1.82 s and `000001` 10.5 s; lower
      resolution does not help because the path is geometry-bound. Color-only
      `000002` measured 234-265 ms and meets the gate; color-only `000001` is
      4.8-5.0 s.
- [ ] **Cache hits or bounded streaming are acceptable.** Small structures
      should hit the cache after the first preview. Large cold structures either
      stream (textured) or are rejected by the color-only geometry budget; they
      do not become cache hits unless a final render warms the cache. The UI
      must accept the slower path or run a warmed dedicated process. Test a
      sequence of 20 yaw changes and report hit rate, stream count, and stage
      times.
- [ ] **Sampled stills are enough for rotation speed.** Integrators can judge a
      turntable from six to eight evenly sampled frames. If they need motion,
      the scope grows to short video preview and ffmpeg latency.
- [ ] **Upper-layer UI ownership.** Integrators accept building the UI,
      debouncing camera changes, keeping one in-flight preview, discarding stale
      results, and throttling requests; they do not expect the engine to serve
      HTTP or WebSocket or preempt jobs. Confirm with two or three integrators
      before committing.
- [ ] **Dedicated preview capacity is acceptable.** A separate preview worker or
      supervisor pool is operationally acceptable. If not, preview traffic will
      starve final renders because the worker processes one job at a time.
- [ ] **Framing consistency is sufficient.** Preview may differ from final output
      in sampling and backend, but the camera framing and keyframe timing must
      match. Validate with a projected-bounds test, not a pixel diff.

## MVP Scope

In:

- `RenderPreset` v1: schema version, `framing` (`sphereFit`), camera (`yaw`,
  `elevation`), optional keyframes (`time`, `yaw`, `elevation`), duration, fps,
  loop, easing, background, lighting. No input or output paths. `padding` is
  deferred until `render` exposes it.
- Protocol request `type: "preview"` with the same `input` and appearance fields
  as `render`, plus `samples` for animation timelines. Limits: width and height
  at most 512, supersample fixed at 1, samples 1..=8, PNG only.
- Response `kind: "preview"` with an `outputs` list (`path`, `yaw`, `elevation`,
  `time`, `width`, `height`), the effective `preset`, cache statistics, stage
  timings, and `backendUsed`.
- `hello.preview` capability object with the caps and supported fields. Additive;
  no protocol-version bump unless the lifecycle changes.
- Preview uses the scene/mesh cache when warm. On a cold miss it builds and
  caches a full mesh only within a server-side budget
  (`--preview-max-mesh-bytes`, default 256 MiB). Above the budget, a static
  preview streams on the CPU without caching; a flipbook is rejected. Clients
  cannot request `streamMesh` directly.
- A dedicated preview worker or supervisor pool, plus a `--preview-timeout`
  setting. Preview jobs cannot request video encoders.
- CLI smoke path: `blockscope render --draft` uses the same caps and writes
  PNGs. Do not reuse the `preview` subcommand name; it is already an alias for
  terminal rendering.
- Tests: framing equality between preview and final render at the same camera,
  cache-hit stage times, cap rejection, line-size safety, and unchanged
  render/video output.
- Docs: engine protocol, service mode, CLI, changelog.

Out:

- Streaming raw or base64 frames over stdio.
- A persistent `GpuFrameSession` preview session.
- An HTTP or WebSocket server inside the engine.
- A desktop viewer binary.
- Video or GIF preview through ffmpeg.
- Multi-angle contact sheets in the first slice.
- Scene editing, material editing, or block picking.

## Not Doing (and Why)

- **Streaming frames over the line protocol** — the protocol is newline-delimited
  JSON with a 1 MiB line limit and no backpressure story. Files in `outputDir`
  are simpler, keep the transport unchanged, and match existing render results.
- **A persistent preview session in the MVP** — it would pin a worker, require
  sticky routing and idle timeouts, and complicate the supervisor. Do it only if
  the stateless p95 misses the target.
- **HTTP/WebSocket in the engine** — the documented deployment model is stdio
  child processes with no sockets. The upper service owns transport, auth, and
  rate limiting.
- **A GUI viewer** — it is a second product surface with cross-platform
  packaging and driver support. The chosen direction is service integration.
- **Video or GIF preview** — ffmpeg startup and encoder probing dominate a
  sub-second interaction. Sampled PNG frames are enough to judge rotation speed.
- **Full-resolution preview** — a 4K preview defeats the latency goal and can
  evict cache entries needed by final jobs.
- **Priority preemption** — the worker processes one job at a time and cannot
  interrupt the rasterizer. Use a separate preview pool instead.
- **Client-side camera reimplementation** — it would drift from the engine's
  framing and defeat the purpose of previewing.

## Open Questions

- Should preview be a new request `type` or a `mode: "preview"` field on
  `render`? A new type is easier to cap, meter, and route, but adds protocol
  surface.
- Does `samples` mean evenly spaced times across `duration`, or should the
  client send explicit `times`? Explicit times are more flexible but larger.
- Should previews populate the same scene/mesh cache as final jobs, or use a
  smaller preview-only cache to avoid evicting expensive final-render entries?
- Is `--preview-timeout` a supervisor flag, a worker flag, or both? The
  supervisor owns timeouts today and kills workers on expiry.
- Do integrators need multi-angle preview (several yaws in one request) or is a
  single camera per request enough? A contact sheet would need a separate frame
  cap.
- Should preview be allowed on CPU-only deployments? If yes, what is the
  minimum acceptable latency, and does the cap need to shrink further?
