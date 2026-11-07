# Spec: Animated video export (`video-export`)

**Status:** Implemented (CLI, protocol v4, and hardware probing)
**Date:** 2026-11-04
**Owner:** CLI / rendering
**Depends on:** optional `gpu` feature, external `ffmpeg`

## Objective

Let a user turn a `.litematic` into a short camera animation without producing a
directory of PNGs. The renderer supplies RGBA frames; an external ffmpeg process
encodes them to MP4 or GIF. The CLI and the stdio service both expose the
feature.

Primary user stories:

```bash
blockscope animate build.litematic --output build.mp4 \
  --keyframe "0:45:30" --keyframe "6:405:30" --fps 30
```

```json
{"type":"render","id":"v1","input":{"path":"/data/build.litematic"},
 "format":"mp4","animation":{"keyframes":[{"time":0,"yaw":45,"elevation":30},
 {"time":6,"yaw":405,"elevation":30}],"fps":30},"outputDir":"/tmp/jobs/v1"}
```

Success means:

- one command produces a playable MP4 or GIF from keyframed yaw/elevation;
- MP4 frames stream to ffmpeg with no per-frame PNG or temporary image files;
- GIF uses a bounded temporary frame file and cleans it up on normal exits;
- PNG output and its byte-exact golden tests do not change;
- GPU rasterization and hardware video encoding are available, selectable, and
  report what was actually used;
- service protocol v4 carries the same options and result data as the CLI.

Out of scope:

- audio, subtitles, overlays, text, watermarks, or entity/texture animation;
- scene animation such as layer-by-layer reveal, block changes, or explosions;
- zoom, pan, depth of field, or lighting animation (v1 keyframes are yaw and
  elevation only);
- streaming/progressive output and mid-job cancellation beyond the supervisor's
  existing kill-and-retry policy;
- bundling ffmpeg or linking `libav*`;
- byte-exact video output across ffmpeg versions or hardware encoders;
- per-frame PNG export or contact sheets.

## Assumptions

1. A keyframe is `{time, yaw, elevation}`. `time` is seconds, `yaw` is any
   finite degree value, and `elevation` is within `-89.9..=89.9` because the
   existing camera rejects other values. The camera always auto-fits the scene,
   so there is no zoom or pan.
2. Yaw interpolates linearly without wrapping. `0 -> 360` is one full turn;
   `350 -> 10` rotates backward through 340 degrees. Users write `350 -> 370`
   for a short forward turn.
3. `--easing linear` is the default. `smooth` applies smoothstep
   (`u*u*(3-2u)`) to each segment. Easing is global, not per keyframe.
4. Frames are sampled at exact `1/fps` intervals. `duration` defaults to the
   last keyframe time. `frame_count` is `floor(duration*fps)` for a loop and
   `floor(duration*fps)+1` for a non-loop clip. `loop` defaults to true.
5. The first keyframe value is held before the first keyframe time; the last
   value is held after the last keyframe time. `duration` must be at least the
   last keyframe time.
6. Defaults: 30 fps for MP4, 15 fps for GIF when `--fps` is omitted; 1024x1024;
   supersample 2; `simple-ao-v1`; opaque black background; medium quality.
7. MP4 is H.264 with `yuv420p` for broad player compatibility. It cannot carry
   alpha, so an MP4 request with a transparent background is rejected. GIF
   supports 1-bit transparency.
8. `--encoder auto` is the default. It probes hardware encoders on first use and
   falls back to `libx264`. `--encoder software` forces `libx264`. Explicit
   hardware choices fail with `ENCODER_UNAVAILABLE` instead of silently falling
   back. `auto` only falls back during probing, not after frames have been
   encoded.
9. ffmpeg is an external executable, found on `PATH` or supplied with
   `--ffmpeg`. Minimum supported version is 4.4. The service never accepts an
   executable path or raw encoder arguments from a request.
10. Animation always uses the complete mesh. `--stream-mesh` is rejected for
    `animate`; streaming would rebuild the mesh for every frame. Service jobs
    already build a complete mesh.
11. Frames render sequentially, each using the full `threads` budget. Frame-level
    concurrency is not used because one frame already saturates the rasterizer.
12. Video output is visually deterministic for a pinned ffmpeg build and
    encoder, but it is not byte-exact and is not part of the golden suite.
13. Protocol v4 is used because video changes the result shape and `hello`
    capabilities. Existing PNG request fields remain accepted.

Resolved by the user:

- first animation form: keyframed camera movement;
- ffmpeg dependency: external executable;
- first scope: CLI and service protocol;
- GPU scope: both wgpu rasterization and hardware video encoding.

## Commands

```bash
# Build and focused tests
cargo build --workspace
cargo build -p blockscope-cli --features gpu
cargo test -p litematic-video
cargo test -p blockscope-cli --test cli_animate
cargo test -p blockscope-cli --test serve_video

# Repository gate
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
just check
cargo xtask check
```

Manual smoke tests:

```bash
blockscope animate fixtures/cube.litematic --output out.mp4 \
  --keyframe "0:45:30" --keyframe "6:405:30" --fps 30 --encoder software
blockscope animate fixtures/cube.litematic --output out.gif \
  --keyframe "0:45:30" --keyframe "6:405:30" --fps 15 --background transparent
ffprobe -v error -show_streams -show_format out.mp4
```

Hardware checks are manual on machines with the relevant adapter; CI skips them.

## Project Structure

```text
crates/video/                         # new `litematic-video` crate
  src/lib.rs                          # Keyframe, Animation, FramePlan, Easing
  src/error.rs                        # VideoError and stable codes
  src/ffmpeg.rs                       # discovery, version, encoder list/probe
  src/encoder.rs                      # MP4 pipe and GIF two-pass encoder
  src/sink.rs                         # output temp file + atomic replace
  tests/timeline.rs
  tests/command_builder.rs
crates/cli/src/commands/animate.rs    # CLI orchestration; split if >800 lines
crates/cli/src/main.rs                # command registration
crates/cli/src/runtime.rs             # expose a single-frame render helper
crates/cli/src/serve.rs               # protocol v4 dispatch
crates/cli/src/serve/video.rs         # service video job adapter
crates/cli/src/supervise.rs           # video timeout routing
crates/cli/tests/cli_animate.rs
crates/cli/tests/serve_video.rs
docs/user/cli.md, docs/zh/user/cli.md
docs/rendering.md
docs/engine-protocol.md
docs/deployment.md
CHANGELOG.md, CHANGELOG.en.md
tasks/todo.md
```

`litematic-video` must not depend on `litematic-render` or the scene/mesh
crates. It accepts raw frames as `{width, height, rgba: &[u8]}` so it can be
unit-tested without a renderer and reused by the service.

## Code Style

- Rust 2024, `rustfmt`, 4 spaces, snake_case functions, PascalCase types, max
  width 100.
- Comments explain intent; identifiers stay English.
- No emoji.
- Stable error codes are string constants on `VideoError`; messages carry the
  actionable detail.

```rust
let plan = Animation::new(keyframes, duration, fps, looped, Easing::Linear)?
    .frame_plan()?;
for (index, time) in plan.sample_times().enumerate() {
    let (yaw, elevation) = plan.camera_at(time);
    let frame = render_frame(scene, mesh, snapshot, yaw, elevation, options)?;
    encoder.write_frame(&frame)?;
}
```

## CLI Contract

`blockscope animate <file>` reuses the render flags for size, background,
lighting, backend, supersample, threads, assets, mesh options, and limits.

| Flag | Default | Notes |
| --- | --- | --- |
| `--output PATH` | `animate.mp4` | Extension selects format when `--format` is absent |
| `--format mp4\|gif` | inferred | Must match the extension if both are present |
| `--keyframe TIME:YAW:ELEV` | required | Repeatable, 1..=64 entries, times strictly increasing |
| `--duration SECONDS` | last keyframe time | `0 < duration <= 60`; must cover the last keyframe |
| `--fps N` | 30 MP4 / 15 GIF | Integer `1..=60` |
| `--no-loop` | loop enabled | Excludes the duplicate endpoint and writes a one-shot GIF |
| `--easing linear\|smooth` | `linear` | Global interpolation mode |
| `--encoder auto\|software\|nvenc\|qsv\|amf\|videotoolbox\|vaapi` | `auto` | Platform-filtered; explicit failures do not fall back |
| `--quality low\|medium\|high` | `medium` | Maps to encoder-specific quality values |
| `--ffmpeg PATH` | `ffmpeg` | External executable |
| `--json` | off | Machine-readable result on stdout |
| `--terminal` | unsupported | Video has no terminal display path |

Validation rules:

- `--stream-mesh`, `--views`, `--yaw`, and `--elevation` are rejected with
  `INVALID_RENDER_OPTIONS`.
- MP4 requires even output width and height because of `yuv420p`; odd sizes are
  rejected before rendering.
- MP4 rejects `--background transparent`; GIF accepts it.
- `--encoder` other than `software`/`auto` is rejected for GIF because GIF
  encoding has no hardware path.
- A missing ffmpeg, an unsupported version, or an unusable explicit encoder is
  reported before the scene is parsed.
- Frame count is capped at 1800 and timeline duration at 60 seconds by default.
  `--force-limits` raises both caps.

Human output reports output path, format, size, frame count, fps, actual clip
duration, keyframe count, easing, loop, backend, encoder, render/encode/total
milliseconds, and output bytes. `--json` emits one object with the same fields.

## Frame Plan and Interpolation

```text
duration = explicit duration, else last keyframe time
frame_count = floor(duration * fps) + (loop ? 0 : 1)
sample i    = i / fps, i in 0..frame_count
```

- A loop clip with fewer than 2 frames is rejected; a non-loop clip needs at
  least 1 frame.
- If `duration*fps` is not integral, a non-loop clip ends at the last frame at
  or before `duration`. Document that users who need an exact endpoint should
  use a duration that is a multiple of `1/fps`.
- Interpolation clamps to the first/last keyframe outside the timeline.
- Elevation is validated after interpolation; linear and smoothstep between two
  valid values stay valid.
- The same plan yields the same sample times on every platform; frame order and
  timing are deterministic.

## Rendering and Encoding

### MP4

- Start ffmpeg before the first frame:
  `ffmpeg -hide_banner -loglevel error -f rawvideo -pix_fmt rgba -s WxH
  -framerate FPS -i - -an -c:v <encoder> <quality args> -pix_fmt yuv420p -r FPS
  -movflags +faststart -map_metadata -1 -threads N -f mp4 -y <temp>.mp4`.
- Write each frame's RGBA bytes to stdin. Backpressure from ffmpeg bounds memory
  to one frame buffer.
- Drain stderr on a separate thread into a bounded tail buffer so a noisy ffmpeg
  cannot deadlock the pipe; include the tail in `FFMPEG_FAILED`.
- Close stdin, wait for exit, then atomically replace the destination. A
  non-zero exit removes the temp file and fails the job.
- Encoder threads default to `max(1, threads / 4)` capped at 4, so encoding does
  not fully oversubscribe the worker's CPU budget. Hardware encoders may ignore
  the hint.
- Quality mapping is implemented per encoder and covered by unit tests. The
  exact flag sets are an implementation detail but must be recorded in docs.

### GIF

- GIF has no hardware encode path. Render frames once to a temporary raw RGBA
  file next to the output, then run two ffmpeg passes:
  1. `palettegen=stats_mode=diff` (with `reserve_transparent=1` when the
     background is transparent) writes a palette PNG;
  2. `paletteuse=dither=sierra2_4a` (with `alpha_threshold=128` for
     transparency) reads the raw file again and writes the GIF.
- `-loop 0` for looping, `-loop -1` for one-shot.
- The raw file size is exactly `frames * width * height * 4`; reject with
  `VIDEO_TOO_LARGE` above a 4 GiB default temp budget, before rendering.
- Temporary files are removed on success and on every normal error path. On a
  hard kill a stale file may remain; the next job with the same output path
  removes it before writing.
- A single-process `split`/`palettegen` graph is rejected because ffmpeg would
  buffer every frame in memory.

### GPU paths

- Rasterization reuses the existing `--backend cpu|gpu|auto` selection. Frames
  render sequentially; the mesh is built once and shared across frames.
- v1 reuses the mesh but keeps the existing per-frame CPU projection and GPU
  upload. If profiling shows projection/upload dominates, a follow-up
  `RenderSession` can keep vertex buffers resident and move only the camera;
  that optimization is not part of this spec.
- Hardware encoding is selected independently from rasterization. Both can run
  on the same GPU and may contend; `auto` reports the encoder it chose.
- Builds without the `gpu` feature still support video through the CPU
  rasterizer and software encoding.

## Service Protocol v4

`hello` gains a `video` object:

```json
"video" {
  "available": true,
  "formats": ["mp4", "gif"],
  "encoders": ["auto", "software", "nvenc", "qsv", "amf"],
  "ffmpegVersion": "6.1.1",
  "maxFrames": 1800,
  "maxDurationSeconds": 60,
  "hardwareProbe": "lazy"
}
```

- `available` is false when ffmpeg is missing or older than 4.4; the object then
  carries a `reason` and empty `formats`/`encoders`.
- `encoders` lists names compiled into the local ffmpeg plus `auto` and
  `software`. It does not promise that a hardware device is usable;
  `encoderUsed` in the result is authoritative. Hardware probes are cached for
  the process lifetime.
- `serve` and `supervise` accept `--ffmpeg PATH`. The supervisor passes the
  resolved path to its children. Requests cannot override it.

Request additions:

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `format` | string | `png` | `png`, `mp4`, or `gif` |
| `animation` | object | absent | Required for `mp4`/`gif`; rejected for `png` |
| `animation.keyframes` | object[] | required | 1..=64 `{time, yaw, elevation}` |
| `animation.duration` | number | last keyframe time | Same limits as CLI |
| `animation.fps` | integer | 30 MP4 / 15 GIF | `1..=60` |
| `animation.loop` | boolean | `true` | |
| `animation.easing` | string | `linear` | `linear` or `smooth` |
| `encoder` | string | `auto` | Enum only; never raw ffmpeg flags |
| `quality` | string | `medium` | `low`, `medium`, or `high` |

- `views` and `animation` are mutually exclusive.
- `format: "png"` keeps the v3 behavior and rejects `animation`.
- Video output is `{id}.mp4` or `{id}.gif` in `outputDir`.
- The result has one output entry with
  `kind: "video"`, `format`, `width`, `height`, `frames`, `fps`,
  `durationSeconds`, `bytes`, `encoder`, `renderMs`, and `encodeMs`.
  Image entries keep `kind: "image"` and their existing `yaw` field.
- `timings` adds `encodeMs`; `renderMs` is the sum of frame render times.
  `statistics` is the first frame's statistics and remains advisory.
- New error codes: `ANIMATION_INVALID`, `FFMPEG_NOT_FOUND`,
  `FFMPEG_VERSION_UNSUPPORTED`, `ENCODER_UNAVAILABLE`, `VIDEO_TOO_LARGE`,
  `VIDEO_TEMP_FAILED`, and `FFMPEG_FAILED`. Only `FFMPEG_FAILED` and
  `VIDEO_TEMP_FAILED` are retryable.
- `supervise` gains `--video-timeout SECONDS` (default 600; `0` disables).
  Requests with `format` `mp4` or `gif` use it instead of the normal
  `--timeout`; PNG jobs keep the existing default of 60 seconds.

## Error Handling and Cleanup

- Every ffmpeg child gets a guard that kills and waits on drop unless it has
  already exited.
- The worker closes ffmpeg's stdin on all paths, so a killed worker makes ffmpeg
  see EOF and finish. The supervisor may still leave a short-lived encoder; the
  next retry overwrites the temp output.
- Output is written to a sibling temp file and renamed on success. On Windows,
  an existing destination is removed before the rename.
- GIF temp files use a name derived from the validated job id or output file and
  never escape `outputDir`.
- A broken pipe is converted to `FFMPEG_FAILED` with the captured stderr tail,
  not a panic or a raw OS error.

## Testing Strategy

Unit tests in `litematic-video`:

- keyframe validation: duplicate times, decreasing times, out-of-range
  elevation, duration shorter than the last keyframe, keyframe count limits;
- frame plan: loop/non-loop counts, exact sampling, fractional duration,
  minimum frames, maximum frames;
- interpolation: linear, smoothstep, clamp before/after, `0 -> 360`, no wrap;
- ffmpeg argument builders for every format/encoder/quality combination;
- ffmpeg version parsing, encoder-list parsing, and probe command construction;
- temp file naming and cleanup guards.

CLI integration tests without ffmpeg:

- missing executable returns `FFMPEG_NOT_FOUND` before parsing;
- invalid keyframes, odd MP4 dimensions, transparent MP4, GIF with a hardware
  encoder, `--stream-mesh`, and `--views` fail with the documented codes;
- `--json` stays machine-readable and progress stays on stderr.

Integration tests with ffmpeg (skipped when unavailable, run in a dedicated CI
job and locally with `BLOCKSCOPE_FFMPEG_TESTS=1`):

- render a tiny fixture to MP4 and GIF with `--encoder software`;
- `ffprobe` reports the expected codec, dimensions, fps, frame count, and
  duration;
- the GIF loops and its temp files are gone;
- a hard ffmpeg failure surfaces `FFMPEG_FAILED` and leaves no temp output;
- an explicit unavailable hardware encoder surfaces `ENCODER_UNAVAILABLE`;
- `auto` falls back to software when no hardware encoder is usable.

Service tests:

- v3 PNG requests still succeed against a v4 worker;
- video requests produce the documented output entry and `timings.encodeMs`;
- `views` + `animation`, `png` + `animation`, and missing `animation` are
  rejected;
- `hello.video` is null/unavailable when ffmpeg is missing;
- supervisor routes video jobs to the longer timeout.

No video bytes enter the golden suite. The CPU PNG goldens must be unchanged.

## Performance and Documentation

Record a benchmark after implementation: 1024x1024, 180 frames, one real
fixture, CPU vs GPU rasterization, software vs one available hardware encoder.
Report render, encode, total, peak RSS, and output size in
`docs/performance.md`. Do not add a CI threshold until the numbers are stable.

Document:

- `docs/user/cli.md` and `docs/zh/user/cli.md`: `animate` flags, examples,
  keyframe syntax, and encoder/quality behavior;
- `docs/rendering.md`: frame pipeline, GPU reuse, hardware encoding, determinism
  caveats;
- `docs/engine-protocol.md`: v4 fields, result shape, error codes, supervisor
  timeout;
- `docs/deployment.md`: ffmpeg installation, container images, hardware device
  passthrough, and the fact that release binaries remain CPU-only and do not
  bundle ffmpeg;
- `README.md`/`README.en.md`, `CHANGELOG.md`/`CHANGELOG.en.md`, and
  `tasks/todo.md`.

## Boundaries

Always:

- keep the PNG path and goldens byte-identical;
- validate all request fields before parsing or meshing;
- use `Command` arguments, never a shell, and never a request-controlled
  executable path;
- clean up ffmpeg children and temp files on every normal error path;
- report the backend and encoder actually used;
- update docs, changelog, and `tasks/todo.md`.

Ask first:

- adding the `litematic-video` workspace crate and its production dependencies;
- bumping the engine protocol from v3 to v4;
- changing the default image to include ffmpeg, or adding a video-specific image;
- enabling hardware encoding by default (`--encoder auto` becomes the default);
- changing the supervisor's default timeout for non-video jobs.

Never:

- bundle GPL ffmpeg/libx264 binaries without a license decision;
- link `libav*` or add `ffmpeg-next`;
- put video bytes in the golden suite;
- accept raw encoder arguments or executable paths from service requests;
- commit generated videos, private `.litematic` files, or resource packs;
- weaken PNG determinism or existing limits to make video pass.

## Success Criteria

- `blockscope animate` produces a playable MP4 and GIF from keyframes on Windows
  and Linux with an external ffmpeg.
- MP4 writes no per-frame PNG or temporary image files; GIF temp usage is
  checked against the budget and cleaned up.
- Keyframe interpolation and frame planning are unit-tested and invalid inputs
  fail before rendering.
- `--backend gpu` works with video, and `--encoder auto` uses a working hardware
  encoder when one exists, otherwise falls back to software with a clear report.
- Explicit unavailable encoders fail with `ENCODER_UNAVAILABLE`.
- Service protocol v4 handles video jobs, reports `hello.video`, and still
  serves v3-style PNG jobs.
- PNG output, PNG statistics, and all goldens are unchanged.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, and `cargo test --workspace` pass.

## Open Questions

1. Container packaging: keep the default slim image without ffmpeg and add a
   `video` build arg/target, or install ffmpeg in the default image? Proposed:
   keep the default slim and add an opt-in video image.
2. Protocol version: bump to v4 as proposed, or keep v3 additive? The result
   shape and `hello` capabilities change, so v4 is clearer but requires client
   upgrades.
3. Linux VAAPI: include it in v1 as a best-effort encoder when `/dev/dri`
   exists, or defer it because device/filter setup is vendor-specific? Proposed:
   include it behind the normal probe, but do not block the release on hardware
   testing.
4. Should `duration` be allowed to truncate before the last keyframe? Proposed:
   no; require the timeline to cover every keyframe.
5. Should hardware encoding be the default (`auto`) for service jobs, or should
   servers opt in to avoid contaminating output quality and determinism?
   Proposed: keep `auto` as the default for both CLI and service, with
   `software` documented as the reproducible choice.
