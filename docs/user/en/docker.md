# Docker

The image contains only `blockscope`; it does not include Minecraft assets. Mount
the client jar, resource packs, and job directory at runtime.

## Build

```bash
docker build -t blockscope .
```

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

The container working directory is `/data`. Input paths and `outputDir` must be
absolute paths visible inside the container.

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

`capacity` reads cgroup CPU/memory limits and suggests worker, thread, and
default-resolution settings. On a 1 CPU / 1 GiB host, prefer one worker, one
thread, and a smaller resolution; increase concurrency only on larger hosts.

See [Deployment](../../deployment.md) for the full deployment guide.

## See Also

- [Service Mode](./service.md)
- [Limits and Security](./limits.md)
- [中文](../docker.md)
