# Changelog

[中文](CHANGELOG.md)

User-facing changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [0.1.2] - 2026-10-25

### Fixed

- The MSI now sets `ALLUSERS=2` and `MSIINSTALLPERUSER=1`, so it installs per
  user even when started from an elevated shell; the uninstall entry and `PATH`
  both belong to the current user and are removed on uninstall.
- CI now runs an MSI install/uninstall smoke test covering the install
  directory, uninstall entry, `PATH` entry, and file cleanup.

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
