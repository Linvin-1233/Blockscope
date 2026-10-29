# Deployment

The engine is a single static-ish binary with no runtime service dependencies. It runs the same way on Linux, macOS, in containers, and on Windows.

## Release Binaries

Tagged releases publish standalone, statically linked binaries for Linux, macOS, and Windows (using musl on Linux and static CRT on Windows). For binary downloads, checksum verification, and desktop installer guides, see [Installation and Quick Start](./user/getting-started.md).

Release artifacts are CPU-only. GPU support is an optional source-build feature
(`--features gpu`) and needs a hardware adapter plus platform drivers. Static
musl binaries cannot load the Vulkan loader at runtime, so GPU deployments
should use a glibc build; containers additionally need the GPU device passthrough
and driver packages. See [Rendering and PNG Output](./rendering.md#gpu-backend).

## Cutting a Release

1. Write the user-facing notes under `## [Unreleased]` in `CHANGELOG.md` and `CHANGELOG.en.md`. `just bump` refuses to run when either section is empty, because the release body comes from the version section the recipe creates.
2. Run `just bump patch` (`minor` or `major` also work) on `main` in a clean tree. The recipe raises `[workspace.package] version` in `Cargo.toml`, refreshes `Cargo.lock`, updates the documents that state the current version, moves the unreleased notes under the new version heading, then commits `chore: bump to vX.Y.Z` and creates the annotated tag.
3. Push both: `git push origin main vX.Y.Z`. The tag starts `.github/workflows/release.yml`.

The versioned download examples in `docs/user/getting-started.md` and its Chinese counterpart in `docs/zh/user/getting-started.md` name the last published release; update them after the new release is available.

## Container Deployment

The container image contains only `blockscope` with no external runtime dependencies or assets. Client jars, resource packs, and job directories must be mounted at runtime.

For container build commands, volume mounting, worker execution, and supervisor setup, see the [Docker Guide](./user/docker.md) and [Service Mode](./user/service.md).

Always verify container sizing using `blockscope capacity` against the host cgroup limits before deploying.

## Cloudflare Workers

The `blockscope-worker` crate serves a stateless HTTP API from a Worker. It accepts a `.litematic` body and returns a PNG, using a bundled resource pack when one is configured. See [Cloudflare Workers](./user/cloudflare-workers.md) for deployment, endpoint configuration, CPU limits, and frontend integration.

## Linux via WSL

WSL2 with Docker Desktop, or a plain Ubuntu distro, is enough for a local Linux check:

```bash
cd /mnt/<drive>/path/to/Blockscope
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR=/tmp/litematic-target

cargo test --workspace
cargo run -p blockscope-cli --release -- capacity --json
```

To verify cgroup-aware sizing without Docker, use a transient systemd scope:

```bash
sudo systemd-run --scope -p MemoryMax=1G -p CPUQuota=100% -- \
  /tmp/litematic-target/release/blockscope capacity --json
```

Expected output: `cores: 1`, `cpuQuotaMilli: 1000`, `memoryLimitBytes: 1073741824`, `suggestedWorkers: 1`, `suggestedThreads: 1`, default tier `1024x1024 @ ss1`.

## CI

`.github/workflows/ci.yml` runs three jobs:

- `test` on `ubuntu-latest`, `windows-latest`, and `macos-latest`: fmt, clippy with `-D warnings`, and the full workspace test suite.
- `container` on `ubuntu-latest`: builds the image, then runs `capacity` and a protocol smoke test inside `--cpus=1 --memory=1g` so cgroup parsing is exercised on every push.
- `installer` on `windows-latest`: builds the static binary and MSI, runs the install/uninstall lifecycle test, and uploads the artifacts.

`.github/workflows/release.yml` runs on a `v*` tag: it checks that the tag matches `[workspace.package] version`, creates the GitHub Release from `CHANGELOG.md`, and uploads five platform archives with sha256 files plus the Windows MSI and its checksum. Running the workflow manually builds and packages the same five targets with `dry-run`; the MSI publish step runs only for tag pushes, so the pipeline can be verified before the first tag exists.

## Capacity reference

`blockscope capacity` also reports `suggestedMaxQuads`: 80 percent of available memory divided by roughly 400 bytes per quad. The default mesh cap is the lower of 5,000,000 quads and that budget, so a 1 GiB container rejects oversized schematics earlier than a 32 GiB host. `--force-limits` bypasses the cap; there is no automatic LOD, so choose an explicit `--lod` when a scene cannot fit.

| Machine                 | workers | threads | default tier | peak RSS per worker |
| ----------------------- | ------- | ------- | ------------ | ------------------- |
| 1 CPU / 1 GiB container | 1       | 1       | 1024 @ ss1   | ~200 MB             |
| 1 CPU / 2 GiB VPS       | 1       | 1       | 1024 @ ss1   | ~200 MB             |
| 2 CPU / 2 GiB container | 1       | 2       | 1024 @ ss2   | ~250 MB             |
| 4 CPU / 8 GiB           | 2       | 2       | 2048 @ ss2   | ~350 MB             |
| 12 CPU / 32 GiB         | 4       | 3       | 2048 @ ss2   | ~350 MB             |

These values come from the formulas in `blockscope capacity`; override them with `--workers`, `--threads`, or `--memory` when the automatic value is wrong for a deployment.
