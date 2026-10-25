# Deployment

The engine is a single static-ish binary with no runtime service dependencies.
It runs the same way on Linux, in containers, and on Windows.

## Prebuilt binaries

Tagged releases publish standalone binaries, so a host does not need a Rust
toolchain:

| Platform | Asset |
| --- | --- |
| Linux x86_64 | `blockscope-v<version>-x86_64-unknown-linux-musl.tar.gz` |
| Linux aarch64 | `blockscope-v<version>-aarch64-unknown-linux-musl.tar.gz` |
| Windows x86_64 | `blockscope-v<version>-x86_64-pc-windows-msvc.msi` (installer) |
| Windows x86_64 | `blockscope-v<version>-x86_64-pc-windows-msvc.zip` (portable) |
| macOS aarch64 | `blockscope-v<version>-aarch64-apple-darwin.tar.gz` |
| macOS x86_64 | `blockscope-v<version>-x86_64-apple-darwin.tar.gz` |

Each file ships with a `.sha256` file named after it without the extension
(`blockscope-v<version>-<target>.sha256`); verify it before deploying. Linux
artifacts are statically linked against musl and the Windows artifact uses the
static CRT. The MSI installs per user under `%LOCALAPPDATA%\Programs\Blockscope`,
maintains the user `PATH`, and removes both on uninstall; `msiexec /i <msi> /qn`
is the unattended form. The container image is still built from this repository
(see below); no prebuilt image is published.

## Docker

```bash
docker build -t blockscope:local .

docker run --rm --cpus=1 --memory=1g blockscope:local capacity --json
```

`capacity` reports what the engine would use inside that container. With
`--cpus=1 --memory=1g` it prints `cores: 1`, `memoryLimitBytes: 1073741824`,
`suggestedWorkers: 1`, `suggestedThreads: 1`, and a `1024x1024 supersample 1`
default tier.

Run a worker with a client jar mounted read-only and a writable data directory:

```bash
docker run --rm -i \
  --cpus=1 --memory=1g \
  -v "$PWD/assets/client.jar:/assets/client.jar:ro" \
  -v "$PWD/jobs:/data" \
  blockscope:local serve --client-jar /assets/client.jar
```

Run a supervisor with a pool instead of a single worker:

```bash
docker run --rm -i \
  --cpus=2 --memory=2g \
  -v "$PWD/assets/client.jar:/assets/client.jar:ro" \
  -v "$PWD/jobs:/data" \
  blockscope:local supervise --workers 1 --threads 2 --client-jar /assets/client.jar
```

On a 1 CPU / 1 GiB container the capacity suggestion is one worker and one
thread; raising `--workers` there only splits one core and doubles memory.

`blockscope supervise` also accepts:

- `--timeout <seconds>`: per-job deadline, default 60, `0` disables.
- `--memory <MiB>`: per-worker ceiling; defaults to 85 percent of available
  memory divided by the worker count. A worker that crosses it is killed and
  the job is retried once.
- `--queue <jobs>`: queued jobs before `BUSY`; defaults to `workers * 2`.
- `--cache <entries>`: parsed scenes cached per worker; defaults to 1.
- `--force-limits`: disables parser, mesh, and render caps for every worker.
  A warning is printed once at supervisor startup and once per worker.

For an oversized schematic on a large host:

```bash
blockscope render huge.litematic --output huge.png \
  --lod auto --force-limits --threads 0 \
  --client-jar /assets/client.jar
```

`--lod auto` keeps the exact mesh when it fits and only degrades when the
memory budget requires it. Use a fixed `--lod 2`/`4` when the output must be
reproducible across hosts with different memory.

Under the supervisor, raise the per-worker ceiling to match the chosen LOD:

```bash
blockscope supervise --workers 1 --threads 12 --memory 8000 \
  --force-limits --client-jar /assets/client.jar
```

The client sends `"lod": 4` in each render request; the worker cache separates
LOD levels, so switching LOD reparses and remeshes once.

The container image never ships Minecraft assets. Mount the jar or an extracted
`assets/` directory at runtime.

## Linux via WSL

WSL2 with Docker Desktop, or a plain Ubuntu distro, is enough for a local Linux
check:

```bash
cd /mnt/d/vscodeProj/Blockscope
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

Expected output: `cores: 1`, `cpuQuotaMilli: 1000`,
`memoryLimitBytes: 1073741824`, `suggestedWorkers: 1`,
`suggestedThreads: 1`, default tier `1024x1024 @ ss1`.

## CI

`.github/workflows/ci.yml` runs two jobs:

- `test` on `ubuntu-latest`, `windows-latest`, and `macos-latest`: fmt, clippy with
  `-D warnings`, and the full workspace test suite.
- `container` on `ubuntu-latest`: builds the image, then runs `capacity` and a
  protocol smoke test inside `--cpus=1 --memory=1g` so cgroup parsing is
  exercised on every push.

`.github/workflows/release.yml` runs on a `v*` tag: it checks that the tag
matches `[workspace.package] version`, creates the GitHub Release from
`CHANGELOG.md`, and uploads the five archives with sha256 files. Running the
workflow manually builds and packages the same targets with `dry-run`, so the
pipeline can be verified before the first tag exists.

## Capacity reference

`blockscope capacity` also reports `suggestedMaxQuads`: 80 percent of available
memory divided by roughly 400 bytes per quad. The default mesh cap is the lower
of 5,000,000 quads and that budget, so a 1 GiB container rejects oversized
schematics earlier than a 32 GiB host. `--force-limits` bypasses the cap;
`--lod auto` uses the budget to pick a factor.

| Machine | workers | threads | default tier | peak RSS per worker |
|---|---|---|---|---|
| 1 CPU / 1 GiB container | 1 | 1 | 1024 @ ss1 | ~200 MB |
| 1 CPU / 2 GiB VPS | 1 | 1 | 1024 @ ss1 | ~200 MB |
| 2 CPU / 2 GiB container | 1 | 2 | 1024 @ ss2 | ~250 MB |
| 4 CPU / 8 GiB | 2 | 2 | 2048 @ ss2 | ~350 MB |
| 12 CPU / 32 GiB | 4 | 3 | 2048 @ ss2 | ~350 MB |

These values come from the formulas in `blockscope capacity`; override them with
`--workers`, `--threads`, or `--memory-budget` when the automatic value is wrong
for a deployment.
