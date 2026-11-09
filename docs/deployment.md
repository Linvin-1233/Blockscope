# Deployment

The engine is a single static-ish binary with no mandatory runtime service dependencies; MP4/GIF export optionally invokes an external ffmpeg. It runs the same way on Linux, macOS, in containers, and on Windows.

## Release Binaries

Tagged releases publish standalone binaries for Linux, macOS, and Windows: static musl archives for Linux x86_64/aarch64, a glibc GNU archive for Linux x86_64, macOS archives, and a Windows archive linked with the static CRT. For binary downloads, checksum verification, and desktop installer guides, see [Installation and Quick Start](./user/getting-started.md).

Release artifacts follow the platform capability matrix:

| Platform | Primary GPU backend | GPU feature | Fallback |
| --- | --- | --- | --- |
| Windows x64 (MSVC) | D3D12, Vulkan | enabled | CPU |
| macOS arm64 / x64 | Metal | enabled | CPU |
| Linux x86_64 GNU | Vulkan | enabled | CPU |
| Linux x86_64/aarch64 musl | none | disabled | CPU |
| wasm / Cloudflare Workers | none | disabled | CPU |

`auto` is the default backend in every artifact. It selects a hardware GPU only
when the policy predicts a win and falls back to the CPU before producing
output when the adapter, driver, precision guard, or VRAM budget rejects the
GPU. `--backend cpu` is the byte-exact reference and explicit override.

Static musl binaries cannot load the Vulkan loader at runtime, and a Worker
isolate has no GPU. Those are platform constraints, not missing code paths.
Linux containers need GPU device passthrough and driver packages; build the
image with `--build-arg GPU=1` to compile the feature. Windows, macOS, and Linux
GNU users can also build from source with `--features gpu`.

The engine uses 60 percent of adapter VRAM as its working budget. Linux reads
the value from sysfs; Windows and macOS use a conservative floor unless the
operator sets `BLOCKSCOPE_GPU_VRAM_BYTES`. `WGPU_BACKEND` restricts the wgpu
backend (for example `vulkan` or `dx12`). GPU output is vendor-dependent and
not byte-exact; see [Rendering and PNG Output](./rendering.md#gpu-backend).

## Cutting a Release

1. Write the user-facing notes under `## [Unreleased]` in `CHANGELOG.md` and `CHANGELOG.en.md`. `just bump` refuses to run when either section is empty, because the release body comes from the version section the recipe creates.
2. Run `just bump patch` (`minor` or `major` also work) on `main` in a clean tree. The recipe raises `[workspace.package] version` in `Cargo.toml`, refreshes `Cargo.lock`, updates the documents that state the current version, moves the unreleased notes under the new version heading, then commits `chore: bump to vX.Y.Z` and creates the annotated tag.
3. Push both: `git push origin main vX.Y.Z`. The tag starts `.github/workflows/release.yml` for the GitHub Release binaries and `.github/workflows/publish-crates.yml` for crates.io.

The versioned download examples in `docs/user/getting-started.md` and its Chinese counterpart in `docs/zh/user/getting-started.md` name the last published release; update them after the new release is available.

## Video Encoding

MP4 and GIF export uses an external ffmpeg executable; release binaries do not
bundle ffmpeg or link `libav*`. Install ffmpeg 4.4 or newer on the host and make
it available on `PATH`, or pass `--ffmpeg /path/to/ffmpeg` to `animate`, `serve`,
or `supervise`. A supervisor resolves the path once and passes it to every
worker. Requests cannot override the executable path.

`--encoder auto` is the default. It probes hardware encoders at the requested
output size and falls back to libx264. Explicit hardware choices require the
matching driver and device:

- NVENC needs a supported NVIDIA driver and GPU.
- Quick Sync needs the Intel media driver and a usable device.
- AMF needs the AMD driver and runtime DLLs.
- VideoToolbox is the native macOS path.
- VAAPI uses `/dev/dri/renderD128`; pass the render node into containers and
  install the vendor driver.

Hardware encoding is optional. The CPU rasterizer plus libx264 is the
reproducible configuration and the reference for quality comparisons.

## Container Deployment

The container image contains only `blockscope` with no external runtime dependencies or assets. Client jars, resource packs, and job directories must be mounted at runtime. For video jobs, install ffmpeg in a derived image or mount it at runtime; the default image stays slim and does not include ffmpeg.

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

`.github/workflows/ci.yml` runs four jobs:

- `test` on `ubuntu-latest`, `windows-latest`, and `macos-latest`: fmt, clippy with `-D warnings`, the full workspace test suite, and a GPU-feature compile (`just check-gpu`).
- `wasm` on `ubuntu-latest`: clippy for the Worker target, which stays CPU-only.
- `container` on `ubuntu-latest`: builds the CPU image, then runs `capacity` and a protocol smoke test inside `--cpus=1 --memory=1g` so cgroup parsing is exercised on every push.
- `installer` on `windows-latest`: builds the GPU-enabled static binary and MSI, runs the install/uninstall lifecycle test, and uploads the artifacts.

Hardware parity and performance tests are not part of the default hosted-runner
gate because those runners have no GPU. Run `cargo test -p litematic-render
--features gpu --test gpu` on a machine with an adapter; the tests print
`BACKEND_UNAVAILABLE` and skip when none exists. Use a self-hosted GPU runner
for release validation.

`.github/workflows/release.yml` runs on a `v*` tag: it checks that the tag matches `[workspace.package] version`, creates the GitHub Release from `CHANGELOG.md`, and uploads six platform archives with sha256 files plus the Windows MSI and its checksum. Windows, macOS, and Linux GNU archives enable the GPU feature; musl archives stay CPU-only. Running the workflow manually builds and packages the same targets with `dry-run`; the MSI publish step runs only for tag pushes, so the pipeline can be verified before the first tag exists.

## crates.io

`.github/workflows/publish-crates.yml` publishes the workspace crates from an immutable release tag, never from a local working tree. Before uploading anything it checks out the exact tag and requires all of the following:

- the working tree is clean;
- the tag exists on `origin` and points at the checked-out commit;
- the tag commit is an ancestor of `origin/main`;
- `cargo xtask verify-tag` confirms that the tag matches `[workspace.package] version`;
- `just check` passes on that commit.

The publish script, `.github/scripts/publish-crates.sh`, then publishes the crates in dependency order with `cargo publish --locked`. It skips versions that are already on crates.io, so a partial or failed run can be re-run safely. It also checks that every publishable workspace member is in its explicit allowlist, which prevents a newly added crate from being published accidentally. A workspace member that is not ready for crates.io must set `publish = false` or stay out of the release tag; otherwise the allowlist check stops the workflow.

For the first publish of each crate name, create a crates.io API token with the `publish-new` and `publish-update` scopes and store it as the `CARGO_REGISTRY_TOKEN` secret on the `crates-io` GitHub environment. The environment can require reviewers so the token is only used after approval. crates.io allows a burst of five brand-new crate names and then one every ten minutes; the script sleeps through a rate-limit response and the job has a 180-minute budget. The existing `v0.5.0` tag predates this workflow's publish metadata, so cut a new patch tag before the first crates.io release.

After every crate has its first version, the long-lived token can be replaced with [Trusted Publishing](https://crates.io/docs/trusted-publishing/): configure the repository, the `publish-crates.yml` workflow, and the `crates-io` environment on crates.io, then exchange an OIDC token with `rust-lang/crates-io-auth-action`. Trusted publishing cannot create a brand-new crate, which is why the token is still needed for the bootstrap.

For a local packaging check from a clean checkout of the tag, run `TAG=vX.Y.Z DRY_RUN=true bash .github/scripts/publish-crates.sh`; it performs the same preflight and lists the package contents for every crate without uploading.

## Capacity reference

`blockscope capacity` also reports `suggestedMaxQuads`: 80 percent of available memory divided by roughly 400 bytes per quad. The default mesh cap is the lower of 5,000,000 quads and that budget, so a 1 GiB container rejects oversized schematics earlier than a 32 GiB host. `--force-limits` bypasses the cap and is only appropriate for trusted input on a host with enough memory.

| Machine                 | workers | threads | default tier | peak RSS per worker |
| ----------------------- | ------- | ------- | ------------ | ------------------- |
| 1 CPU / 1 GiB container | 1       | 1       | 1024 @ ss1   | ~200 MB             |
| 1 CPU / 2 GiB VPS       | 1       | 1       | 1024 @ ss1   | ~200 MB             |
| 2 CPU / 2 GiB container | 1       | 2       | 1024 @ ss2   | ~250 MB             |
| 4 CPU / 8 GiB           | 2       | 2       | 2048 @ ss2   | ~350 MB             |
| 12 CPU / 32 GiB         | 4       | 3       | 2048 @ ss2   | ~350 MB             |

These values come from the formulas in `blockscope capacity`; override them with `--workers`, `--threads`, or `--memory` when the automatic value is wrong for a deployment.
