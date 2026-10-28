# Service Mode

`serve` speaks newline-delimited JSON over stdin/stdout. It does not listen on a
network port. It is designed to be spawned as a child process by a trusted
upper-layer service, batch tool, or automation pipeline.

## Start a Worker

```bash
blockscope serve --client-jar client.jar --threads 4 --cache 2
```

- `--threads`: default thread count for requests that omit `threads`; capped at
  256
- `--cache`: maximum number of parsed scenes/meshes to keep
- `--cache-bytes`: estimated cache budget in bytes; `0` uses half of available
  memory. The oldest entries are evicted first; one oversized entry is still
  allowed to remain. When started by `supervise` without an explicit value, the
  supervisor derives it from the per-worker memory ceiling
- `--greedy-mesh`: merge coplanar unit faces with identical material and
  lighting; disabled by default
- `--force-limits`: raise resource caps; trusted input only
- `--ignore-block-entity-nbt`: skip container NBT

The worker prints one `hello` line after materials are ready, then accepts jobs.

## Request Example

```json
{
  "type": "render",
  "id": "job-1",
  "input": {
    "path": "/data/model.litematic"
  },
  "views": [45, 225],
  "width": 1024,
  "height": 1024,
  "supersample": 2,
  "outputDir": "/data/out"
}
```

- `input.path` and `outputDir` must be absolute
- The input must be a regular file
- `input.hash` is optional and is a caller-asserted SHA-256 identity; on a cache
  hit the worker trusts it and does not read the file, on a miss it recomputes
  and verifies the digest
- `supervise` injects the hash it computed for routing, so repeated inputs also
  take the no-read cache path
- An empty `views` array defaults to `[45]`; at most 64 angles are accepted;
  multiple views render concurrently when a complete mesh is available
- Output files are `{id}.png` or `{id}-{yaw}.png`; callers must keep `id` and
  `outputDir` unique across concurrent jobs. Duplicate ids or yaw values
  overwrite the same path
- Full fields, results, and error codes are in
  [Engine protocol v1](../../engine-protocol.md)

## Supervisor

`supervise` manages a worker pool, queue, timeouts, and memory ceilings:

```bash
blockscope supervise \
  --client-jar client.jar \
  --workers 2 \
  --threads 4 \
  --timeout 120 \
  --memory 2048 \
  --queue 4 \
  --cache 2
```

| Option | Description |
|---|---|
| `--workers` | Worker process count, at most 256; `0` uses the `capacity` suggestion |
| `--threads` | Threads per worker; `0` uses the `capacity` suggestion |
| `--timeout` | Per-job timeout in seconds; `0` disables |
| `--memory` | Per-worker memory ceiling in MiB; `0` derives it from capacity |
| `--queue` | Queue length; `0` uses `workers * 2` |
| `--cache` | Maximum cached scenes/meshes per worker |
| `--cache-bytes` | Estimated cache budget per worker in bytes; `0` derives half of the per-worker memory ceiling |
| `--greedy-mesh` | Enable greedy mesh merging in every worker; disabled by default |

## Security Boundary

Use `serve` and `supervise` only with trusted callers. Input paths, output
directories, and resource-pack paths are controlled by the caller. Do not expose
the stdio service directly to untrusted network clients.

## See Also

- [Engine protocol v1](../../engine-protocol.md)
- [Docker](./docker.md)
- [Troubleshooting](./troubleshooting.md)
- [中文](../service.md)
