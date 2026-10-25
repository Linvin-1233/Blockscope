# Deployment

The engine is a single static-ish binary with no runtime service dependencies.
It runs the same way on Linux, in containers, and on Windows.

## Docker

```bash
docker build -t renderd:local .

docker run --rm --cpus=1 --memory=1g renderd:local capacity --json
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
  renderd:local serve --client-jar /assets/client.jar
```

Run a supervisor with a pool instead of a single worker:

```bash
docker run --rm -i \
  --cpus=2 --memory=2g \
  -v "$PWD/assets/client.jar:/assets/client.jar:ro" \
  -v "$PWD/jobs:/data" \
  renderd:local supervise --workers 1 --threads 2 --client-jar /assets/client.jar
```

On a 1 CPU / 1 GiB container the capacity suggestion is one worker and one
thread; raising `--workers` there only splits one core and doubles memory.

`renderd supervise` also accepts:

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
renderd render huge.litematic --output huge.png \
  --lod 4 --force-limits --threads 0 \
  --client-jar /assets/client.jar
```

Under the supervisor, raise the per-worker ceiling to match the chosen LOD:

```bash
renderd supervise --workers 1 --threads 12 --memory 8000 \
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
cd /mnt/d/vscodeProj/Litematic-render-QQBot
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR=/tmp/litematic-target

cargo test --workspace
cargo run -p renderd-cli --release -- capacity --json
```

To verify cgroup-aware sizing without Docker, use a transient systemd scope:

```bash
sudo systemd-run --scope -p MemoryMax=1G -p CPUQuota=100% -- \
  /tmp/litematic-target/release/renderd capacity --json
```

Expected output: `cores: 1`, `cpuQuotaMilli: 1000`,
`memoryLimitBytes: 1073741824`, `suggestedWorkers: 1`,
`suggestedThreads: 1`, default tier `1024x1024 @ ss1`.

## CI

`.github/workflows/ci.yml` runs two jobs:

- `test` on `ubuntu-latest` and `windows-latest`: fmt, clippy with
  `-D warnings`, and the full workspace test suite.
- `container` on `ubuntu-latest`: builds the image, then runs `capacity` and a
  protocol smoke test inside `--cpus=1 --memory=1g` so cgroup parsing is
  exercised on every push.

## Capacity reference

| Machine | workers | threads | default tier | peak RSS per worker |
|---|---|---|---|---|
| 1 CPU / 1 GiB container | 1 | 1 | 1024 @ ss1 | ~200 MB |
| 1 CPU / 2 GiB VPS | 1 | 1 | 1024 @ ss1 | ~200 MB |
| 2 CPU / 2 GiB container | 1 | 2 | 1024 @ ss2 | ~250 MB |
| 4 CPU / 8 GiB | 2 | 2 | 2048 @ ss2 | ~350 MB |
| 12 CPU / 32 GiB | 4 | 3 | 2048 @ ss2 | ~350 MB |

These values come from the formulas in `renderd capacity`; override them with
`--workers`, `--threads`, or `--memory-budget` when the automatic value is wrong
for a deployment.
