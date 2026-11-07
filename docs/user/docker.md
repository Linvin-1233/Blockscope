---
title: "Docker"
---

The image contains only `blockscope`; it does not include Minecraft assets. Mount the client jar, resource packs, and job directory at runtime.

## Build

```bash
docker build -t blockscope .
```

The default image is CPU-only. GPU builds need the host driver, device
passthrough, and the optional feature:

```bash
docker build --build-arg GPU=1 -t blockscope:gpu .
```

On Linux this requires a Vulkan or D3D12-capable host and the corresponding
device nodes (for example `--device /dev/dri`); the container does not bundle
drivers. GPU output is approximate and `auto` still falls back to the CPU when
no adapter is visible.

MP4/GIF jobs need an ffmpeg 4.4+ executable inside the container. The default
image does not bundle ffmpeg; install it in a derived image or mount a static
binary and pass `--ffmpeg` when it is not on `PATH`.

## Capacity

```bash
docker run --rm --cpus=1 --memory=1g blockscope capacity --json
```

## Run a Worker

```bash
docker run --rm -i \
  --cpus=2 --memory=2g \
  -v /host/client.jar:/assets/client.jar:ro \
  -v /host/jobs:/data \
  blockscope serve --client-jar /assets/client.jar
```

The container working directory is `/data`. Input paths and `outputDir` must be absolute paths visible inside the container.

## Run a Supervisor

```bash
docker run --rm -i \
  --cpus=4 --memory=4g \
  -v /host/client.jar:/assets/client.jar:ro \
  -v /host/jobs:/data \
  blockscope supervise \
    --workers 2 --threads 2 \
    --client-jar /assets/client.jar
```

## Smoke Test

```bash
printf '%s\n' '{"type":"shutdown"}' |
  docker run --rm -i blockscope serve
```

## Sizing

`capacity` reads cgroup CPU/memory limits and suggests worker, thread, and default-resolution settings. On a 1 CPU / 1 GiB host, prefer one worker, one thread, and a smaller resolution; increase concurrency only on larger hosts.

See [Deployment](/deployment) for the full deployment guide.

## See Also

- [Service Mode](./service.md)
- [Limits and Security](./limits.md)
- [中文](/zh/user/docker)
