# Changelog

[中文](CHANGELOG.md)

User-facing changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Cloudflare Workers adapter (`blockscope-worker`) with a configurable HTTP
  endpoint and color-only PNG rendering from a `.litematic` request body.

### Fixed

- Fallback colors now inherit the base material for decorated block variants, so
  quartz pillars, stairs, and slabs render as quartz instead of the magenta
  unknown-block sentinel. Invisible blocks such as barriers are skipped instead
  of being painted.

- Terminal image output (`--terminal`) now fits the image to the visible grid
  instead of sending it at native pixel size, and prints the summary before the
  image so text can never scroll it out of view. Auto-detection falls back to
  half blocks inside `tmux`/`screen`, a warning is printed when stdout is not a
  terminal, and the iTerm2 encoder no longer sends invented `width=auto` /
  `height=auto` keys.

## [0.4.0] - 2026-10-28

### Added

- The CLI now has a shared terminal style layer: `--color auto|always|never`,
  `NO_COLOR` support, complete option lists, colored errors/warnings, and
  tidier human-readable output. Running `blockscope` without arguments shows a
  welcome screen with a text wordmark and exits `0`.
- `render`, `bench`, `mesh`, and `materials` show stderr progress on a terminal:
  mesh page counts and tile percentages for single-view rasterization, plus a
  view counter for multi-view renders. Progress turns off for pipes, `--json`,
  `CI`, or `--color never`, and never writes to stdout.
- The welcome wordmark steps down with the terminal width and height at print
  time, so narrow windows and split panes do not overflow; help text wraps to
  the terminal as well.
- Long-running progress bars reflow after a window resize instead of wrapping
  and damaging the terminal output.

### Fixed

- NBT lists, int arrays, and long arrays now check the remaining input before
  reserving memory. A tiny truncated file can no longer declare a huge length
  and force a multi-hundred-MiB allocation.
- Resource-pack directory and zip assets are bounded while reading or
  decompressing, so a zip bomb or oversized local file cannot be fully loaded
  before the size check.
- `Scene::new` rejects regions whose dimensions and packed state count disagree
  or whose palette is empty, preventing malformed embedder scenes from panicking
  in the mesh builder. `heap_bytes` now accounts for block entity NBT, so the
  serve cache budget no longer underestimates container data. Regions whose
  palette contains only air emit an `EMPTY_REGION` info diagnostic.
- `supervise` now uses a bounded input channel and a 1 MiB protocol line limit,
  providing real backpressure and rejecting overlong lines with
  `INVALID_REQUEST`.
- `supervise` derives its per-worker cache budget from the per-worker memory
  ceiling when `--cache-bytes` is not set, instead of letting every worker use
  half of the host's available memory.
- `supervise` hashes inputs on a dedicated thread, so queue admission returns
  `BUSY` before reading a file. A caller-supplied `input.hash` is reused for
  routing instead of hashing the file again.
- The streaming renderer no longer uses a barrier that could wait forever after
  a worker panic; projection workers are joined normally.
- Blockstate `x`/`y` rotations are normalized before negation, so `i32::MIN`
  no longer overflows in debug builds.
- Water and lava are no longer rendered as per-block staircase boxes. Meshing
  now reads the surrounding fluid columns and the cell above, computes the four
  top corner heights with vanilla's weighted averaging, hides internal faces
  between the same fluid, and promotes a column to a full block when the same
  fluid sits above it. Flowing surfaces are continuous slopes and no longer
  show seams from internal translucent faces.
- Waterlogged partial blocks now keep their own model and gain a matching water
  layer, including stairs, slabs, walls, leaves, signs, coral, and the
  always-water kelp/seagrass blocks. Waterlogged full opaque blocks skip the
  layer because it would be hidden; bubble columns use the fluid surface
  directly.
- Redstone wire no longer falls back to a magenta full cube in the built-in
  color catalog. It gets a flat 1/64-high dust plane and the same dark-red to
  bright-red `power=0..15` gradient as the textured path. The gradient is now
  square-root shaped so power 0 and 1 stay clearly distinct on thin dust lines.
  Resource-pack tint behavior is updated the same way.
- Regenerated the water golden PNG and added tests for fluid surfaces, shared
  fluid face culling, waterlogged blocks, redstone power, and builder-path
  consistency.

### Changed

- `serve` and `render --views` accept at most 64 angles. Multi-view batches are
  bounded by an output-byte budget and each view is written and released before
  the next batch, so memory no longer grows linearly with the view count.
- `serve` validates size, views, lighting, background, texture filter, and the
  output directory before parsing the input or building a mesh.
- The protocol now documents `input.hash` as a caller-asserted cache identity:
  cache hits trust it, cache misses re-read and verify the file.
- `render` now defaults to `--stream-mesh=auto`: fixed-LOD renders stream mesh
  pages, while `--lod auto` keeps the full mesh because selecting a factor needs
  one. `--stream-mesh=off` restores the full-mesh path; `--stream-mesh` or `on`
  forces streaming. This avoids the full-mesh peak for large scenes by default.
- Translucent `Triangle` projected points are now f32 and the always-translucent
  layer tag is gone. The depth-sort key remains f64, shrinking the struct from
  112 B to 72 B. On `000001` with the client jar and streamed meshing, peak
  working set dropped from 2.28 GB to 1.52 GB; golden PNG tests are unchanged.
- Streamed rendering now spills exact translucent records to temporary bucket
  files above 262,144 triangles and renders one bucket at a time. `000001` with
  the client jar drops from 1.52 GB to **618 MB** with an identical PNG SHA-256;
  the temporary directory is removed when the render finishes. Adds the
  `TEMP_STORAGE_FAILED` error code.

### Performance

- Chest states resolve their block-entity model before the blockstate model, so
  a resource-pack model that would be discarded is no longer parsed.
- Material snapshots no longer build fallback geometry for states that a
  resource-pack model replaces. `ColorBlockCatalog` gains `resolve_style`, which
  returns only color, layer, and coverage; pack models and chests use it and the
  fallback model is built only when it is actually rendered. The `ModelChain`
  cache now shares `Arc`s, and texture transparency is computed and cached at
  decode time. On `000001` with the 26.2 client jar (23,813 states,
  512x512/supersample 1), material snapshot drops from 2.6-2.7 s to about 0.6 s,
  `materials` from 3.1 s to 1.3 s, and total render from 18.6 s to 14.8 s.
  Statistics and golden PNGs are unchanged.
- Waterlogged layers add translucent faces for every visible waterlogged block.
  On the `000001` random stress fixture with about 3.2 million
  `waterlogged=true` states, 512x512 at supersample 1 render time rose from
  about 3.1 s to 5.2 s. Ordinary builds stay effectively unchanged; the
  measured rectifier detail test is still about 0.2 s at 4096x4096.

## [0.3.0] - 2026-10-27

### Added

- Add `--greedy-mesh`, which merges coplanar unit faces inside a page when they
  share the same normal, plane, material, lighting, and uniform ambient
  occlusion. This reduces quad counts and mesh memory on flat or repetitive
  structures. It is disabled by default because random-palette scenes gain
  little and still pay the merge cost.

### Changed

- `render`, `bench`, `serve`, and library callers now reject output dimensions above
  4096 pixels per side, matching the engine protocol document; the total pixel
  budget is unchanged. Previously only the pixel count was checked, so extreme
  aspect ratios could bypass the protocol contract.
- Mesh building now caches each 16x16x16 page and its border, and pre-resolves
  every region palette into an immutable material table, so culling and ambient
  occlusion are array reads. Each cell also caches an occlusion bit and offsets
  resolve to flat index deltas, so the inner loop does not re-check page bounds.
  A full opaque block is skipped when all six neighbours occlude it. On the test
  fixtures (1024x1024, supersample 2, 12 threads, color-only materials) mesh
  time drops from 17.3-20.4 s to 5.7-5.9 s for `000001` exact and from 11.7 s to
  0.24-0.25 s for `000002` exact. Mesh statistics and PNG SHA-256 values are
  unchanged.
- Mesh page discovery now indexes only pages that contain a non-air block.
  Sparse structures no longer scan empty pages or fail `PAGE_LIMIT_EXCEEDED`
  because their bounding box spans too many pages; scenes without air keep the
  bounding-box fast path. `blocksVisited` now counts scanned blocks in indexed
  pages, so an all-air mesh reports zero.
- `render --stream-mesh` builds 32-page chunks and renders them immediately,
  without retaining a full `Mesh`. On the test fixtures at 1024x1024 with
  supersample 2, 12 threads, and color-only materials, peak working set drops
  from 7.16 GB to 425 MB for `000001` exact and from 497 MB to 182 MB for
  `000002` exact, with byte-identical PNG output. The mode cannot be combined
  with `--lod auto`; multiple views rebuild the mesh per view.
- Texture sampling resolves the face texture once and inlines the sampler; UVs
  inside `0..1` skip the repeat path. A synthetic 16x16 microbenchmark measured
  1.7-2.7x higher nearest/bilinear sampling throughput.
- Multiple views render concurrently within the `--threads` budget when a full
  mesh is available. CLI and `serve` preserve request order, while each
  concurrent view owns its tile buffers.
- `serve` skips input reads and re-hashing when `input.hash` is present and the
  cache hits. `supervise` injects the hash it already computed for routing, so
  repeated inputs take the no-read cache path.
- `serve` and `supervise` add an estimated cache byte budget via
  `--cache-bytes`, defaulting to half of available memory. The oldest entries
  are evicted first; a single oversized entry is still allowed to remain, so
  two huge meshes do not stay resident together.
- Rasterizer tiles target about 1024 tiles: a 1024x1024 internal image uses
  32px tiles and a 2048x2048 image keeps 64px, instead of leaving 12 workers to
  finish 16-64 tiles in two straggling rounds. On `000001` (77.6M quads, 12
  threads) 256x256/ss1 drops from 5.7-6.4 s to 3.1-3.2 s, 512x512/ss1 from
  4.5-4.6 s to 3.1-3.2 s, 512x512/ss2 from 3.8 s to 3.2 s, and 1024x1024/ss1
  from 3.7 s to 3.2 s. RGBA stays identical and `tiles` no longer depends on
  the worker count.
- The rasterizer monomorphizes the triangle loop for textured and untextured
  geometry. The untextured path drops the per-pixel texture branch and UV
  interpolation; on `000001` (no textured triangles) the 1024x1024/ss2 render
  drops from 4.1 s to 3.9 s with identical output.
- Mesh page capacity now follows the median quads-per-non-air-block ratio of the
  last few pages with a 15% margin. Growth adds a quarter of the current
  capacity instead of doubling, and pages far below their reservation shrink at
  the end. `000001` mesh time drops from 4.0-4.3 s to 3.1-3.2 s and resident
  mesh from 10.7 GB to 7.9 GB; `000002` drops from 310 MB to 261 MB while its
  mesh pass stays at about 0.22 s.
- Release builds enable thin LTO and a single codegen unit. On the test host
  `000001` mesh and render are about 4-5% faster and `000002` parse is about 20%
  faster; release build time grows accordingly.
- `--lod auto` samples 128 evenly spaced pages before building a factor and
  only runs an exact count when the estimate is clearly above the budget. The
  count reuses the page cache and culling but allocates no vertices, ambient
  occlusion, or quads, so an oversized exact mesh is no longer built before
  falling back. On `000001` the auto mesh pass drops from 5.0-5.2 s to
  3.6-3.7 s with the same factor and output; a scene that fits, such as
  `000002`, pays about 30 ms for the sample.

## [0.2.0] - 2026-10-25

### Added

- macOS support for Apple Silicon (`aarch64-apple-darwin`) and Intel (`x86_64-apple-darwin`).
- macOS memory and capacity detection in CLI using `sysctl` and Mach VM statistics.
- Supervisor process RSS monitoring on macOS via `proc_pidinfo` for memory limit enforcement.
- `bench` peak RSS reporting on macOS via `getrusage`.
- CI test matrix and release distribution workflow expanded to include macOS runner and targets.
- Terminal image rendering for `render`: Kitty, iTerm2, Sixel, and half-block protocols via `--terminal`, with automatic protocol detection.

### Changed

- The CPU rasterizer clips each scanline to the triangle's row span, and streamed
  chunks now project and bin on the same worker pool that rasterizes them,
  chunking by projected quad count instead of page count. Render time on the
  large fixtures (1024x1024, supersample 2) drops from 271 ms to 147 ms
  (`DenseNonSolid`), 77 ms to 40 ms (`Atrium`), and 73 ms to 30 ms
  (`Valkyrie`). PNG output stays byte-identical; `pixelsTested` now reports the
  span fragments actually tested after row clipping.
- Parallel rendering now scales across SSAA downsampling, mesh page projection,
  LOD aggregation, and the parallel mesh builder, improving core utilization on
  large and flat scenes.
- Mesh memory use is reduced: `Vertex` shrinks from 36 to 16 bytes and `Quad`
  from 148 to 88 bytes (about 40% less geometry memory); `normal`, `color`,
  `texture`, `shade`, and `ao` move from the vertex to the quad. This is a
  public API change: code that constructs `Vertex` or reads those fields must
  read them from `Quad` instead.

### Fixed

- The supervisor samples worker RSS before enforcing the memory ceiling, so a
  worker is stopped in the same sampling cycle instead of one cycle later.

## [0.1.2] - 2026-10-25

### Fixed

- The MSI now sets `ALLUSERS=2` and `MSIINSTALLPERUSER=1`, so it uses the
  per-user context even when started from an elevated shell; files go to the
  user profile, the `PATH` entry belongs to the current user, and uninstall
  removes both.
- CI now runs an MSI install/uninstall smoke test covering the install
  directory, per-user registration, `PATH` entry, and file cleanup.

## [0.1.1] - 2026-10-25

### Added

- A per-user Windows MSI that installs to
  `%LOCALAPPDATA%\Programs\Blockscope`, appends the directory to the user
  `PATH`, removes both on uninstall, and supports
  `msiexec /i <msi> /qn` for unattended installs.
- CI builds the MSI on every push as a validation artifact; the release workflow
  uploads `blockscope-v<version>-x86_64-pc-windows-msvc.msi` and its `.sha256`
  for `v*` tags.

### Changed

- Windows installation docs now prefer the MSI and keep the portable zip as an
  alternative.

## [0.1.0] - 2026-10-25

First public snapshot of the current workspace version.

### Added

- `.litematic` v1-v7 parsing: metadata, regions, palettes, packed block states,
  block entities, entities, and pending ticks.
- Canonical Scene IR, material statistics, diagnostics, deterministic manifests,
  `inputHash`, and `sceneHash`.
- Built-in color catalog for common block families, partial shapes, fluids, and
  static block entities.
- Resource-pack materials: blockstates/models, parent chains, multipart, UVs,
  rotation, tint, cutout/translucent layers, PNG textures, and overlays.
- Chest body/lid/lock model and entity textures.
- Paged meshes, face culling, ambient occlusion, and OBJ/PLY export.
- CPU orthographic camera, tiled rasterization, `flat-v1` / `simple-ao-v1`
  lighting, supersampling 1-4, multi-view output, and PNG.
- Fixed LOD `2..=16` and `--lod auto` memory-budget degradation.
- `inspect`, `validate`, `stats`, `mesh`, `render`, `bench`, `materials`,
  `capacity`, `serve`, and `supervise` commands.
- Stdio newline-delimited JSON protocol v1, plus supervisor queues, timeouts,
  memory ceilings, and worker pools.
- Capacity detection, mesh/render limits, stable error codes, and a Docker image.
- Golden manifest/PNG, concurrency, protocol, and optional real-file tests.
- User-facing README, contributing guide, code of conduct, and this changelog.
- Chinese and English user documentation under `docs/user`.
- Bounded file reader: inputs must be regular files and are checked against the
  size limit before reading. Directories, FIFOs, devices, and oversized files are
  rejected.
- Regression tests for the thread cap, non-ASCII backgrounds, and protocol
  retryability.
- Split CLI command implementations into `crates/cli/src/commands/`.
- Split the streaming rasterizer into `crates/render/src/rasterizer/stream.rs`
  and `downsample.rs`.
- `.github/workflows/release.yml`: a `v*` tag is checked against the workspace
  version, the GitHub Release is created from `CHANGELOG.md`, and prebuilt
  Windows x86_64 and Linux x86_64/aarch64 binaries are uploaded with `.sha256`
  checksum files.

### Changed

- Renamed the project to `Blockscope` (previously `Litematic-image-render`) and
  positioned it as a general Litematic image-rendering engine; user
  documentation no longer assumes QQBot.
- Renamed the CLI package to `blockscope-cli` and the binary from `renderd` to
  `blockscope`.
- Renamed the design document to `blockscope-engineering-design.md` and marked it
  as historical.
- Worker/rasterizer threads for mesh, LOD, `serve`, and `supervise` are capped at
  256; oversized requests return `INVALID_RENDER_OPTIONS`.
- `supervise` returns `retryable: false` for `INVALID_REQUEST`, matching the
  protocol documentation.
- Supervisor routing hashes are computed in the dispatch loop instead of the
  stdin reader thread.
- Removed the unused `thiserror` dependency and the duplicate `flate2`
  dev-dependency.
- Aligned rendering parallelism, same-state culling, texture limits, and
  protocol error-code documentation.
- Marked the v1 engine protocol as implemented.
- The installation chapter now points at prebuilt binaries first and marks
  building from source as a development path.
- The README now links GitHub Releases and lists the supported architectures.
- Slimmed the root README to navigation, contribution, and license information.

### Fixed

- Fixed a panic when `Rgba::from_hex` received a multi-byte non-ASCII color.
- Fixed oversized `threads` requests creating excessive scoped threads and
  crashing a worker.
- Fixed `serve`, the CLI, and `supervise` reading entire files before parser
  limits were applied.
- Fixed `supervise` blocking its stdin reader on FIFOs, devices, or oversized
  files.
- Fixed `INVALID_REQUEST` being marked retryable by the supervisor.

### Security

- Default limits cover input size, decompressed size, regions, volume, meshes,
  pixels, and threads.
- Resource packs and Minecraft client assets are not distributed with the
  repository.
