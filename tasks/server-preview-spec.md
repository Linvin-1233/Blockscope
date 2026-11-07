# Spec: `server-preview`

**Status:** Deferred (2026-11-07): the textured stateless design is blocked;
a color-only blockout variant is the recommended future direction. See "Gate
Outcome" below.
**Date:** 2026-11-07
**Owner:** CLI / service protocol
**Depends on:** engine protocol v4, existing scene/mesh cache, optional `gpu` feature
**Idea:** `docs/ideas/3d-preview.md`
**Contract:** `tasks/server-preview-contract.md`

> **Superseded direction:** the original objective below assumes a textured
> preview that reuses the client-jar material set. The 2026-11-07 spike found
> that this is not viable at 384x384 on the 4 GB reference card. The recommended
> follow-up is a **color-only blockout preview** for `000002`-class structures,
> with a hard color-mesh geometry budget. A revised spec is required before
> implementation; the assumptions, request contract, and success criteria below
> describe the superseded textured design. This document is retained for the
> measured gate and the cache/memory policy.

## Objective

Let an upper-layer service render low-latency preview frames for camera and
keyframe tuning without turning the engine into a GUI, an HTTP server, or a
streaming media server. The engine remains a headless stdio child process; the
upper service owns the browser or desktop UI, throttling, authentication, and
user experience.

The preview request is a cheaper sibling of `render`, not a new renderer. It
uses the same parser, scene, mesh, material set, camera math, and PNG encoder as
the final job. A preview is explicitly approximate in sampling and backend, but
its camera framing and keyframe timing must match the final render.

Primary user story:

```text
Upper service UI: user drags yaw/elevation or edits a turntable timeline
  -> service sends one `preview` request over stdio
  -> engine returns one or more low-resolution PNG paths
  -> UI displays them and stores the echoed preset
  -> final `render` / video job uses the same preset
```

Success means:

- a warm-cache 384x384 preview of a `000002`-class fixture completes with p95
  under 300 ms on the reference host;
- the preview request is hard-capped and cannot become a full render by accident;
- preview framing matches the final render at the same camera and options;
- the existing `render`, `animate`, PNG goldens, and protocol v4 behavior do not
  change;
- the protocol change is additive and clients can detect it through
  `hello.preview`;
- no frame bytes travel over stdout; outputs are files in `outputDir`.

Out of scope:

- a desktop or browser viewer; the upper service builds it;
- an HTTP, WebSocket, or named-pipe server inside the engine;
- streaming frames over the stdio protocol;
- a persistent GPU frame session that keeps a scene resident between requests;
- video/GIF preview through ffmpeg;
- scene editing, block picking, material editing, or lighting animation;
- multi-angle contact sheets in the first slice;
- zoom, pan, depth of field, or padding controls that `render` does not expose;
- preview prioritization or preemption inside a worker;
- engine-side debounce, cancellation, or latest-only message handling;
- streaming an over-budget cold animation flipbook sample by sample;
- a separate preview cache class or priority eviction policy in this slice.

Resolved by the user:

- direction C from the 2026-11-07 discussion: server-side preview first;
- the engine stays headless; the upper service owns the UI;
- a separate viewer binary is acceptable in general, but is not part of this
  spec;
- full-mesh memory must be bounded: cold previews get a server-side budget and
  an over-budget fallback/rejection, never an unbounded build;
- the latency spike must measure the full path, not just a fully warmed
  ideal state;
- the upper service owns stale-preview handling; the engine does not preempt;
- write a spec before implementation.

## Problem

The engine accepts full render and video jobs. An upper service that wants to
let its users tune a camera has three bad choices:

1. run a full render for every drag and wait seconds;
2. reimplement the camera, mesh fitting, and lighting in the client and drift
   from the engine's output;
3. skip visual tuning and make users edit CLI flags or JSON by hand.

The first choice is too slow, the second defeats the purpose of a single engine,
and the third is the current workflow. The missing primitive is a cheap,
cache-friendly, low-resolution render that uses the exact same camera contract
as the final job.

The existing scene/mesh cache already makes repeated renders of the same input
skip parse and mesh work. The protocol already supports absolute paths and
multi-view rendering. What is missing is a request type with preview-specific
caps, a shared preset contract, and a latency budget.

Full mesh memory is the main risk. The cache can hold a multi-gigabyte mesh for
`000002` or `000001`; a preview that builds and caches one would turn a 384x384
request into the exact memory spike this feature is supposed to avoid. The
preview policy must be explicit: use the cache when it is warm, build and cache
only within a small server-side budget on a cold miss, and fall back to
memory-bounded CPU streaming for a single large cold frame. It must never
silently build a multi-gigabyte full mesh because the output happens to be
small.

## Assumptions

1. A new request type `preview` is clearer than a `draft: true` flag on
   `render`. It gets its own caps, timeout, result shape, and routing without
   changing the semantics of existing `render` requests. Unknown types already
   fail with `INVALID_REQUEST`, so the addition is backwards-compatible.
2. The protocol stays at v4. `preview` is additive; existing clients never send
   it and never see a changed response. `hello.preview` advertises the
   capability. A protocol bump is only needed if the lifecycle or transport
   changes.
3. Preview output is PNG files in `outputDir`, named
   `{id}-preview-{index}.png`. No frame bytes, base64, or binary blobs travel
   over stdout. The 1 MiB line limit and the one-job-at-a-time lifecycle stay
   unchanged.
4. Preview caps: width and height `1..=512`, `supersample` fixed at `1`,
   `samples` `1..=8`, PNG only, one camera per request. The default size is
   384x384. The default sample count is 1 for a static preview and 6 for an
   animation preview.
5. A static preview uses `yaw` and `elevation`. An animation preview uses the
   same `animation` object as video and renders evenly sampled stills; it does
   not encode a video and does not use ffmpeg. `fps` is echoed in the preset but
   does not decide the sample count.
6. For a looping animation preview, sample times are
   `duration * i / samples` for `i in 0..samples`; the duplicate endpoint is
   excluded. For a non-looping preview, sample times are
   `duration * i / (samples - 1)` for `i in 0..samples`; the endpoint is
   included and `samples` must be at least 2. This mirrors the video frame
   plan's loop behavior without pretending to be a video.
7. Preview uses the existing scene/mesh cache when it can. On a cache miss, it
   may build and insert a full mesh only when the estimated mesh fits a
   server-side preview budget (`--preview-max-mesh-bytes`, default 256 MiB;
   advertised in `hello.preview`). Above the budget, a single-frame preview
   (static or `samples: 1` with `time`) uses controlled CPU streaming and is not
   inserted into the cache; an animation flipbook (`samples > 1`) is rejected
   with `RESOURCE_LIMIT` because every sample would re-stream. Cache hits are always allowed, even when the cached
   mesh is larger than the budget, because the memory is already resident.
   Clients cannot request `streamMesh` directly; the server chooses the path.
8. Preview resolves `backend: auto` once per request and uses the same backend
   for every sample. This keeps `statistics.backendUsed` meaningful and avoids
   mixing CPU and GPU output within one preview. `cpu` and `gpu` remain explicit
   overrides; `gpu` fails with `BACKEND_UNAVAILABLE` when no adapter is usable.
   On an over-budget cold miss, `auto` selects CPU streaming; an explicit `gpu`
   request fails with `RESOURCE_LIMIT` because GPU rendering requires a full
   mesh.
9. Preview works on CPU-only builds. The default `auto` may choose the CPU for
   small previews because GPU setup can cost more than a 384x384 CPU pass. The
   result reports what was used; no hardware is required for correctness.
10. `sphereFit` is added to `render` and video requests as an optional boolean.
    The CLI already exposes `--sphere-fit`, but the service protocol does not.
    Closing that gap is required for preview and final video to share framing.
    `padding` is not added in this slice because `render` does not expose it
    publicly; preview uses the engine default.
11. `RenderPreset` v1 is a portable JSON object with no input or output paths.
    It carries framing, camera, animation, and appearance. Preview echoes the
    effective preset; `render` and `animate` accept the same object so the upper
    service can persist exactly what it previewed.
12. Explicit request fields override preset values. `render` reads `camera` and
    `appearance`; `animate` reads `animation` and `appearance`; both read
    `framing`. A preset section that is not used by a command is ignored, not
    rejected. `render` falls back to the default camera when `camera` is absent.
    `animate` and an animated preview fail when neither explicit keyframes nor
    `preset.animation` provide a timeline; there are no default keyframes.
13. The supervisor gains `--preview-timeout SECONDS` (default 15; `0` disables).
    Preview jobs use it instead of `--timeout`. A preview timeout is not retried:
    a retry only adds latency and a timed-out preview is unlikely to finish on
    the second attempt. The supervisor still kills and restarts the worker.
14. The supervisor does not add a second worker pool in this slice. Operators
    who need preview/final isolation run a dedicated `serve` or `supervise`
    process for preview traffic, with its own cache budget and thread budget.
    This preserves the one-job-at-a-time worker invariant and avoids a queueing
    redesign.
15. Preview requests obey the existing input, scene, mesh, memory, and asset
    limits. Preview is a performance feature, not a security boundary; the
    upper service must authorize callers and bound request rates.
16. Preview frames are written directly to their final paths, matching `render`.
    A failed or killed preview may leave earlier frames behind; the caller must
    treat an error result as a failed job and clean up the files. Output names
    are deterministic, so a retry overwrites them. Callers must keep `id` and
    `outputDir` unique across concurrent jobs, as with `render`.
17. Preview samples render sequentially. Each sample's RGBA buffer is encoded to
    PNG and released before the next sample starts; the result retains metadata,
    not frame pixels. At most one full RGBA buffer and one PNG encoder buffer are
    live per request. A multi-sample preview must not collect all frames in
    memory before writing.
18. The engine does not debounce, cancel, or preempt preview requests. The upper
    service owns latest-only behavior: debounce camera changes, keep at most one
    in-flight preview per session, coalesce pending changes to the latest state,
    and discard results whose generation is older than the latest displayed
    frame. The engine only guarantees that `id` is echoed and results are
    complete.
19. The preview cold-mesh budget is process-wide and operator-controlled, not
    per request. `serve` and `supervise` accept
    `--preview-max-mesh-bytes` and pass the effective value to workers; clients
    see it in `hello.preview` and cannot override it.

## Latency Spike (Go/No-Go)

Do this before writing production code. The numbers decide whether a stateless
preview is viable at all. Do not assume LOD; it was removed from the engine.
There is no level-of-detail fallback.

Measure the complete real path with the current release binary and a real client
jar. Do not report only the ideal state where every resource is already warm.

```bash
# Start one worker so the material set is loaded once.
blockscope serve --client-jar <client.jar> --threads 12 --cache 1

# Warm with a normal render first. A cold over-budget preview streams and does
# not populate the full mesh cache, so only a render can warm the cache for a
# large fixture. Use streamMesh=off and a cache budget large enough to retain
# the mesh; otherwise the warm-up render also streams and the cache stays cold.
# Use --force-limits for 000002/000001 when the default 5M-quad cap applies.
# Send render and preview requests as newline-delimited JSON on stdin.
```

Measure these paths. Use a small synthetic or small real fixture for the
under-budget case, and `000002` / `000001` for the large cases:

1. **Warm cache, static preview.** A normal `render` request warms the scene and
   mesh cache; the following `preview` request has `cacheHit: true`.
2. **Cold cache, under budget.** A small fixture builds and caches a full mesh
   within `--preview-max-mesh-bytes`.
3. **Cold cache, over budget, single-frame preview.** Test a static camera and
   a `samples: 1` + `time` animation frame on `000002` and `000001`; both use
   CPU streaming, `cacheHit: false`, `streamed: true`, and no full mesh cache
   entry.
4. **Cold cache, over budget, flipbook.** A `samples: 6` request on `000002` and
   `000001` must fail with `RESOURCE_LIMIT` without building a full mesh or
   starting a stream.

For each path, record:

| Field | Notes |
| --- | --- |
| Fixture | `000002`, `000001`, or a small under-budget fixture |
| Cache state | hit or miss; whether the material snapshot was already warm |
| Backend | CPU or GPU; streaming is always CPU |
| Samples | 1 or 6 |
| Width, height | 256, 384, and 512 for the warm path |
| `parseMs`, `meshEstimateMs`, `meshMs` | zero on a cache hit; estimate time on a cold miss |
| `renderMs`, `pngMs`, `totalMs` | report separately, not as one wall-clock number |
| `cacheHit`, `streamed`, `backendUsed` | from the result |
| Peak working set | process RSS |
| Peak VRAM | when GPU is used |
| Buffer retention | peak RSS for 1 sample vs 6 samples; verify it does not scale by 6 |

Material and texture warm-up:

- `hello` is printed after the material set is ready, but the first preview may
  still build a texture snapshot or atlas. Record the first request separately
  from steady state; do not call it "warm" unless both the material snapshot and
  the scene/mesh cache are warm.
- For the warm-cache gate, warm with a normal `render`, not with a preview,
  because a cold over-budget preview does not populate the full mesh cache.
  Report the worker's cache budget and whether it can actually hold the full
  mesh; if it cannot, the warm path is unavailable and the preview streams.

Queueing:

- Measure one request against an idle worker.
- Measure four concurrent requests through a two-worker `supervise`; report
  queue wait, p50/p95 completion, and whether any result was stale relative to
  a newer request. The engine processes one job per worker and does not
  preempt.
- Set `--queue 1` in the spike for the upper-layer latest-only pattern; a larger
  queue only measures how long stale requests can wait.

Gate:

- `000002` at 384x384, warm cache, static preview, p95 under 300 ms. This is the
  release latency gate.
- `000001` at 384x384 warm cache is reported but is not a latency gate. If it
  exceeds 1 s, document that the upper service must use 256x256 or accept a
  slower preview for structures of that size.
- Cold under-budget preview reports parse and mesh time and is expected to be
  seconds, not milliseconds.
- Cold over-budget static streaming is reported. If `000001` exceeds 3 s at
  384x384, document that only 256x256 single-frame scrubbing is practical.
- Cold over-budget `samples: 6` must be rejected before mesh building.
- No cold preview may build a full mesh above `--preview-max-mesh-bytes`. If it
  does, stop: that is a memory bug, not a latency trade-off.

If the `000002` latency gate fails, stop this spec and revisit the direction.
The spike outcome below records the measured failure; a lower resolution cap is
not a viable fallback because the path is geometry-bound.

Record the spike results in `docs/performance-preview.md` before implementation.

## Gate Outcome (2026-11-07)

The spike is complete. The textured stateless 384x384 design fails the latency
gate:

| Case | Warm p95 | Peak working set | Verdict |
|---|---:|---:|---|
| textured small fixture | 14.7 ms | 22.7 MB | viable |
| textured `000002` full mesh | 1,817 ms | 5,961 MB | 6x over the 300 ms gate |
| textured `000002` streamed | 3,547 ms | 216 MB | memory-safe, too slow |
| textured `000001` full mesh | 10,543 ms | 17,506 MB | not interactive |
| textured `000001` streamed | 13,836 ms | 545 MB | memory-safe, too slow |

The textured resident session also fails on the 4 GB reference card:
`000002` at 128x128 needs 3.66 GB and the 60% VRAM budget is 1.29 GB, so it
returns `BACKEND_UNAVAILABLE`. The resident instance table is
resolution-independent, so 192x192 and 256x256 fail the same way. `000001`
textured needs more still.

The **color-only blockout** variant changes the result:

| Case | Warm p50 | Warm p95 | Peak WS | Verdict |
|---|---:|---:|---:|---|
| color `000002`, 384x384 | 208-213 ms | 234-265 ms | 334 MB | meets the 300 ms gate |
| color `000001`, 384x384 | 4,820-4,986 ms | 4.8-5.0 s | not recorded | not interactive |

The `000002` color mesh has 2.30M quads (182 MiB heap) versus 39.84M textured
quads; `000001` still has 87.88M color quads (6.48 GiB). The 256 MiB cold-mesh
budget separates the two classes. The built-in catalog still renders unknown
states with the magenta fallback; `000002` has 8.96M unknown blocks, so the
blockout preview is a geometry/framing aid, not a final-color preview. A
color-only resident session at 256x256 measured about 27 ms/frame with 201.9 MB
resident, so it is an optional lower-latency mode, not a requirement for
`000002`-class.

Do not implement the textured stateless contract in
`tasks/server-preview-contract.md`. The recommended direction is a **color-only
blockout preview**:

1. **Color-only blockout preview.** Use the built-in color catalog, not the
   client jar. Stateless 384x384 on the CPU meets the gate for `000002`-class.
   It shows geometry and framing, not textures; the upper service must label it
   as a blockout preview. A hard color-mesh geometry budget rejects
   `000001`-class structures.
2. **Optional resident session.** Keep the color-only mesh resident at
   128x128-256x256 for lower latency on `000002`-class structures. This adds
   session lifecycle and routing work; it is not needed for the 300 ms gate.
3. **Optional textured preview on larger GPUs.** Textured `000002` needs about
   3.66 GB resident and therefore roughly an 8 GB card; `000001` needs more.
   This is opt-in by hardware, not the default.
4. **Batch draft for `000001`-class.** Keep the stateless `--draft` preflight
   (9-10 s color/textured) without an interactive claim.

The memory policy in the contract is still useful: cache hits are fast but
expensive; streaming bounds memory but is slower; a cold preview must never
build an unbounded full mesh. A revised spec is required because the color-only
variant changes the material and appearance assumptions. Full details and raw
results are in `docs/performance-preview.md`.

## Tech Stack

- Rust 2024, existing workspace crates: `litematic-core`, `litematic-scene`,
  `litematic-mesh`, `litematic-render`, `litematic-video`, `blockscope-cli`.
- `serde` / `serde_json` for the request and preset contracts; already present.
- No new runtime dependency. No `winit`, `egui`, HTTP server, WebSocket, or
  async runtime.
- The optional `gpu` feature remains optional; preview must build and work
  without it.

## Commands

```bash
# Focused tests
cargo test -p blockscope-cli --test serve_preview
cargo test -p blockscope-cli --test cli_draft
cargo test -p blockscope-cli --test serve
cargo test -p blockscope-cli

# Full gate
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
just check
cargo xtask check
```

Manual smoke tests:

```bash
# CLI draft path (single frame, preview caps)
blockscope render <fixture>.litematic --draft --output draft.png \
  --yaw 45 --elevation 30 --backend cpu

# Service preview path (one static frame)
echo '{"type":"preview","id":"p1","input":{"path":"<absolute fixture>"},
  "yaw":45,"elevation":30,"width":384,"height":384,"outputDir":"<absolute dir>"}' \
  | blockscope serve --client-jar <client.jar>

# Service preview path (six turntable samples)
echo '{"type":"preview","id":"p2","input":{"path":"<absolute fixture>"},
  "width":384,"height":384,"samples":6,
  "animation":{"keyframes":[{"time":0,"yaw":45,"elevation":30},
  {"time":6,"yaw":405,"elevation":30}],"duration":6,"fps":30,"loop":true},
  "outputDir":"<absolute dir>"}' | blockscope serve
```

## Project Structure

```text
crates/cli/src/preview.rs              # caps, sample plan, shared preview validation
crates/cli/src/preset.rs               # RenderPreset v1, serde, apply/echo, validation
crates/cli/src/serve.rs                # Message enum, hello.preview, dispatch
crates/cli/src/serve/preview.rs        # protocol request adapter and result shape
crates/cli/src/serve/video.rs          # accept sphereFit for video
crates/cli/src/supervise.rs            # preview timeout routing, hello.preview
crates/cli/src/commands/render.rs      # --draft and --preset
crates/cli/src/commands/animate.rs     # --preset
crates/cli/src/main.rs                 # flag registration if needed
crates/cli/tests/serve_preview.rs      # protocol, caps, cache, framing, cleanup
crates/cli/tests/cli_draft.rs          # CLI draft and preset round-trip
docs/engine-protocol.md                # request/result/hello, error behavior
docs/user/service.md, docs/zh/user/service.md
docs/user/cli.md, docs/zh/user/cli.md
docs/performance.md                    # existing performance baselines
docs/performance-preview.md            # preview latency spike and post-implementation numbers
CHANGELOG.md, CHANGELOG.en.md
tasks/server-preview-spec.md, tasks/server-preview-contract.md
tasks/todo.md
```

No new workspace crate. `preview.rs` and `preset.rs` stay under the 800-line
file limit; split them if the request/result adapter grows.

## Code Style

- Rust 2024, `rustfmt`, 4 spaces, snake_case functions, PascalCase types, max
  width 100.
- Comments explain intent; identifiers stay English. No emoji.
- Validate every request field before parsing or meshing.
- Stable error codes come from the existing `CliError` / protocol set; do not
  add a preview-only code unless the existing codes cannot express the failure.

```rust
let caps = PreviewCaps::default();
let request = PreviewRequest::parse(value, &caps)?;
let preset = request.effective_preset()?;
let options = preset.render_options(request.width, request.height)?;
let output = render_preview_frame(&scene, &mesh, snapshot.as_deref(), &options)?;
```

## Boundaries

Always:

- keep existing `render`, `animate`, PNG, and golden output unchanged;
- validate caps, paths, and preset fields before parsing or meshing;
- enforce `--preview-max-mesh-bytes` before a cold full-mesh build;
- use CPU streaming for an over-budget cold single-frame preview and keep it
  out of the full mesh cache;
- reject an over-budget cold flipbook before building a mesh;
- reuse the scene/mesh cache on a hit and the video crate's interpolation math;
- render samples sequentially and release each frame buffer after PNG encoding;
- write only inside `outputDir` and use deterministic output names;
- report the backend, cache state, stream state, timings, and effective preset;
- update docs, changelog, and `tasks/todo.md`.

Ask first:

- bumping the protocol version instead of adding `hello.preview`;
- adding a second worker pool, preview priority, or cancellation to `supervise`;
- adding a separate preview cache class or changing cache eviction policy;
- changing the default 256 MiB cold-mesh budget after the spike;
- adding `padding` or any new appearance control to `render`;
- adding a new workspace crate or dependency.

Never:

- build a full mesh above the preview budget on a cold preview;
- insert a streamed mesh into the full mesh cache;
- let a client raise the cold-mesh budget or force `streamMesh` directly;
- add sockets, HTTP, WebSocket, or a GUI to the engine;
- add engine-side debounce, cancellation, or preemption for preview;
- stream frame bytes or base64 through the protocol;
- use ffmpeg or video encoders for preview;
- bypass input, memory, or mesh limits for preview;
- change golden PNGs or weaken existing render determinism;
- commit private `.litematic` files, resource packs, or generated previews.

## Success Criteria

- The latency spike records `000002` at 384x384, warm cache, p95 under 300 ms on
  the reference host; `000001` is measured and documented.
- `preview` returns one to eight PNG files, sorted by index, with the effective
  preset and stage timings.
- Static and animated previews match `render` byte-for-byte at the same camera,
  size, supersample, backend, and appearance options.
- All caps are enforced before parse/mesh, and the maximum response line stays
  under 1 MiB.
- No cold `preview` request builds a full mesh above
  `--preview-max-mesh-bytes`. An over-budget cold single-frame preview streams
  on the CPU and does not populate the cache; an over-budget cold flipbook or
  explicit GPU request is rejected. The CLI `--draft` path is a one-shot render and keeps
  the existing `--stream-mesh` policy instead.
- A warm-cache preview of a large structure that is already resident is allowed
  and reports `cacheHit: true`.
- Peak working set for a six-sample preview stays framebuffer-bounded and does
  not scale with six accumulated RGBA frames.
- `hello.preview` is advertised by `serve` and `supervise`, including the
  effective `maxColdMeshBytes`; protocol v4 clients continue to work unchanged.
- `sphereFit` works on render and video requests without changing existing
  output when omitted.
- `render --draft` and `--preset` work on the CLI; the terminal `preview` alias
  is unchanged.
- A preview timeout is enforced by the supervisor and does not retry.
- The service docs document the upper-layer debounce/latest-only pattern, and
  the engine makes no preemption or cancellation promise.
- No ffmpeg, HTTP, GUI, or new dependency is required.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, and `cargo test --workspace` pass.

## Open Questions

1. Should `preview` be a new request type or a `mode: "preview"` field on
   `render`? Proposed: new type, for separate caps and timeout routing.
2. Is an additive protocol v4 with `hello.preview` sufficient, or should the
   version bump because a new request type changes the lifecycle surface?
   Proposed: additive; clients that do not send `preview` are unaffected.
3. Should preview allow a hash-only cache hit without `input.path`? Proposed:
   no; match `render` and require `input.path` for now.
4. Should `samples` be a count of frames or intervals? Proposed: frames, with
   the loop/non-loop formulas above.
5. Should the upper service be able to request a preview-only worker pool from
   `supervise`? Proposed: not in this slice; run a separate process.
6. Should previews share the final-render cache or use a separate preview cache
   class that evicts before final-render entries? Proposed: share within a
   process for this slice; isolate by process when eviction matters.
7. Should `padding` become a public render option so presets can control frame
   margin? Proposed: defer until a concrete user request.
8. Should preview support multiple yaw angles in one request (a contact sheet)?
   Proposed: defer; the UI can send multiple preview requests, and the caps
   stay simple.
9. Should scrubbing use a single-sample preview with `time`, or should the UI
   always request a flipbook? Proposed: support both; `time` requires
   `samples: 1`, and a flipbook uses `samples > 1`.
10. Is 256 MiB the right default cold-mesh budget? Proposed: start at 256 MiB
    and calibrate against peak RSS in the spike. Operators can lower or raise
    it; clients cannot.
11. Should an over-budget cold flipbook stream each sample instead of rejecting?
    Proposed: no; use single-frame `time` requests for scrubbing. A six-sample
    stream would pay the page-build cost six times and still miss the latency
    target.
12. Should the engine add a `cancel`, `latest`, or preview-priority message?
    Proposed: no in this slice. The upper service owns debounce and latest-only;
    the engine only guarantees one job per worker and id echo.
