# Changelog

[中文](CHANGELOG.md)

User-facing changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

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
