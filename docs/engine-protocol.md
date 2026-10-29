# Engine Protocol (v2)

Wire protocol between an upper-layer service/supervisor and a standalone renderer process. The goal is one transport that behaves the same on Linux and Windows, with a single job in flight per process.

**Status:** Implemented for `blockscope serve` and `blockscope supervise` (protocol v2).

## Transport

- Newline-delimited JSON over the worker's standard input and output.
- One UTF-8 JSON object per line, terminated by a single `\n`. Readers must accept `\r\n` but writers emit `\n` only.
- One line is limited to 1 MiB including its terminator. An overlong or invalid UTF-8 line returns `INVALID_REQUEST`; the worker/supervisor keeps running.
- No BOM, no pretty printing, no multi-line objects.
- `stdout` carries protocol messages only. Logs and diagnostics go to `stderr`.
- Standard input is read as raw bytes and decoded as UTF-8; do not route it through a console code page.
- v2 uses no sockets. stdio is identical across supported platforms and needs no port allocation, firewall rule, or named-pipe path.

## Lifecycle

```text
supervisor                          worker
    | ---- spawn (client jar, dirs) ---> |
    | <--- hello ----------------------- |
    | ---- render ----------------------> |
    | <--- result ----------------------- |
    | ---- render ----------------------> |
    | <--- result ----------------------- |
    | ---- shutdown ---------------------> |
    | <--- bye -------------------------- |
```

- The worker prints exactly one `hello` line after its material set is ready. The supervisor must not send jobs before `hello`.
- One job at a time. The worker processes lines sequentially; the supervisor owns the queue and the concurrency policy.
- Closing stdin is equivalent to `shutdown`. The worker finishes the current job, prints `bye`, and exits 0.

### Messages

```json
{"type":"hello","protocolVersion":2,"engineVersion":"0.4.1","platform":"windows","cores":12,
 "cpuQuota":null,"memoryLimitBytes":51401564160,"memoryAvailableBytes":21526179840,
 "suggestedWorkers":4,"suggestedThreads":3,"suggestedMaxQuads":54313181,
 "cacheBytes":10763089920,
 "backends":["cpu","auto"],
 "defaultTier":{"width":2048,"height":2048,"supersample":2}}
{"type":"render","id":"...","input":{...},...}
{"type":"result","id":"...","ok":true,...}
{"type":"result","id":"...","ok":false,...}
{"type":"shutdown"}
{"type":"bye"}
```

`hello` is printed only after the material set is ready, so the supervisor can size its pool from `suggestedWorkers` and `suggestedThreads` instead of guessing from the host. Values come from cgroup limits on Linux and `GlobalMemoryStatusEx` on Windows; see `blockscope capacity --json` for the same data from the command line. `cacheBytes` is the effective scene/mesh cache budget; `supervise` derives it from its per-worker memory ceiling and passes it to each child, while a standalone worker receiving `0` derives it from available memory.

## Render request

| Field           | Type     | Default        | Notes                                                                                                                                                  |
| --------------- | -------- | -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `type`          | string   | required       | `"render"`                                                                                                                                             |
| `id`            | string   | required       | 1-64 characters of `[A-Za-z0-9_-]`, echoed in the result                                                                                               |
| `input.path`    | string   | required       | Absolute path to a `.litematic`                                                                                                                        |
| `input.hash`    | string   | optional       | Caller-asserted SHA-256 identity; on a cache hit the worker trusts it and does not read the path, on a miss it verifies the file and fails on mismatch |
| `views`         | number[] | `[45]`         | Yaw angles in degrees, finite, `0..=360`; at most 64 entries                                                                                           |
| `width`         | integer  | `1024`         | `1..=4096`                                                                                                                                             |
| `height`        | integer  | `1024`         | `1..=4096`                                                                                                                                             |
| `supersample`   | integer  | `2`            | `1..=4`; internal pixels must stay under the limit                                                                                                     |
| `lighting`      | string   | `simple-ao-v1` | `flat-v1` or `simple-ao-v1`                                                                                                                            |
| `background`    | string   | `transparent`  | `transparent`, `#RRGGBB`, or `#RRGGBBAA`                                                                                                               |
| `textureFilter` | string   | `nearest`      | `nearest` or `linear`                                                                                                                                  |
| `format`        | string   | `png`          | `png` only in v2                                                                                                                                       |
| `outputDir`     | string   | required       | Absolute directory; created if missing                                                                                                                 |
| `threads`       | integer  | `0`            | Mesh and rasterizer threads; `0` uses all cores, capped at 256                                                                                         |
| `lod`           | integer  | `1`            | `1` disables; `2..=16` is a fixed factor; values above 1 lose detail                                                                                   |
| `elevation`     | number   | `30`           | Degrees, `-89.9..=89.9`                                                                                                                                |

`width * height` must not exceed 16,777,216 output pixels. Supersampling multiplies both sides, so the internal image must not exceed 33,554,432 pixels. The worker validates all render options and creates `outputDir` before parsing the input or building a mesh.

Paths must be absolute. The worker rejects relative paths with `INVALID_REQUEST`, so behaviour does not depend on the worker's working directory.

`timeoutMs` is deliberately absent: the supervisor enforces timeouts by killing and restarting the worker. The engine cannot reliably interrupt its own rasterizer mid-tile.

### Example: single view

```json
{
  "type": "render",
  "id": "a1",
  "input": { "path": "/data/model.litematic" },
  "views": [45],
  "width": 1024,
  "height": 1024,
  "supersample": 2,
  "lighting": "simple-ao-v1",
  "background": "transparent",
  "format": "png",
  "outputDir": "/tmp/jobs/a1"
}
```

### Example: multi-view

```json
{
  "type": "render",
  "id": "a2",
  "input": { "path": "/data/model.litematic", "hash": "578cffa3..." },
  "views": [45, 225],
  "width": 2048,
  "height": 2048,
  "supersample": 2,
  "background": "#202830",
  "textureFilter": "nearest",
  "outputDir": "/tmp/jobs/a2"
}
```

## Render result

```json
{
  "type": "result",
  "id": "0f8f...",
  "ok": true,
  "inputHash": "578cffa3...",
  "sceneHash": "fc8e1419...",
  "lod": 1,
  "cacheHit": false,
  "outputs": [
    {
      "path": "/tmp/jobs/0f8f/a-45.png",
      "yaw": 45,
      "width": 1024,
      "height": 1024,
      "bytes": 1484,
      "renderMs": 204.1,
      "pngMs": 5.2
    }
  ],
  "timings": {
    "parseMs": 24.1,
    "lodMs": 0.0,
    "meshMs": 148.2,
    "renderMs": 409.0,
    "pngMs": 10.4,
    "totalMs": 592.5
  },
  "statistics": {
    "triangles": 359110,
    "culledBackfaces": 146996,
    "textures": 206,
    "backendUsed": "cpu"
  }
}
```

- `outputs` preserves `views` order, one entry per view.
- Output files are `{id}.png` for a single view and `{id}-{yaw}.png` for multiple views. Callers must use a unique `id` (and `outputDir`) per job; duplicate ids or duplicate yaw values intentionally overwrite the same path.
- `cacheHit` is `true` when the worker reused a parsed scene and mesh for the same `inputHash` **and** `lod`; `parseMs`, `lodMs`, and `meshMs` are then `0`. When the request includes `input.hash`, a cache hit is resolved before reading the file. The supervisor fills `input.hash` with the hash it computed for routing when the caller omitted it.
- `sceneHash` describes the scene that was rendered. With `lod > 1` it is the aggregated scene, not the original projection.
- `lod` echoes the effective factor. The first request with `lod > 1` logs a detail-loss warning to stderr; the value itself is always explicit.
- `timings` are wall-clock aggregates for the whole job.
- `statistics` is advisory; consumers must not depend on fields beyond `triangles`.

## Errors

```json
{
  "type": "result",
  "id": "0f8f...",
  "ok": false,
  "error": { "code": "OUTPUT_TOO_LARGE", "message": "...", "retryable": false }
}
```

| Code                          | Retryable | Cause                                                                                           |
| ----------------------------- | --------- | ----------------------------------------------------------------------------------------------- |
| `INVALID_REQUEST`             | no        | Malformed JSON, missing field, relative path, non-regular input                                 |
| `INVALID_RENDER_OPTIONS`      | no        | Bad size, elevation, lighting, background, or thread count                                      |
| `OUTPUT_TOO_LARGE`            | no        | Output pixel limit, supersampled limit, or 4096-per-side limit exceeded                         |
| `INPUT_TOO_LARGE`             | no        | Raw input exceeds the parser input cap                                                          |
| `INVALID_NBT`                 | no        | NBT decode failure                                                                              |
| `RESOURCE_LIMIT`              | no        | Parser or scene resource limit                                                                  |
| `INVALID_LITEMATIC`           | no        | Corrupt or unsupported file                                                                     |
| `ASSET_SOURCE_REQUIRED`       | no        | `materials` needs a pack or client jar                                                          |
| `ASSET_READ_FAILED`           | no        | Pack or jar asset cannot be read                                                                |
| `ASSET_LIMIT`                 | no        | Asset exceeds its size limit                                                                    |
| `ASSET_SOURCE_UNSUPPORTED`    | no        | Pack or jar format is unsupported                                                               |
| `INPUT_HASH_MISMATCH`         | no        | `input.hash` does not match the file                                                            |
| `MESH_LIMIT_EXCEEDED`         | no        | Mesh limits exceeded                                                                            |
| `PAGE_LIMIT_EXCEEDED`         | no        | Mesh page limit exceeded                                                                        |
| `BUSY`                        | yes       | Supervisor queue is full                                                                        |
| `SHUTDOWN`                    | yes       | Supervisor is draining and stopped accepting work                                               |
| `MEMORY_LIMIT`                | yes       | Worker exceeded its per-worker memory ceiling                                                   |
| `PNG_ENCODE_FAILED`           | maybe     | Encoder failure; retry once                                                                     |
| `TEMP_STORAGE_FAILED`         | yes       | Translucent spill could not create, write, or read its temporary bucket files                   |
| `TRANSLUCENT_BUDGET_EXCEEDED` | no        | Translucent geometry exceeded the in-memory budget on a target without spill storage; lower LOD |
| `IO_ERROR`                    | maybe     | Input or output file access failed                                                              |
| `INTERNAL_ERROR`              | maybe     | Bug; retry once, then report                                                                    |
| `TIMEOUT`                     | yes       | Emitted by the supervisor, not the engine                                                       |

Parser errors also pass through their stable `LitematicErrorCode` names, such as `INVALID_PALETTE`, `INVALID_REGION_BOUNDS`, and `BLOCK_STATE_DATA_LENGTH_MISMATCH`.

`retryable` is advisory. A retried job must be idempotent: outputs are written to a fresh directory or overwritten in place, and no state survives a job.

## Supervisor

`blockscope supervise` speaks the same protocol on its own stdio and owns a pool of `blockscope serve` children:

```json
{
  "type": "hello",
  "protocolVersion": 2,
  "engineVersion": "0.4.1",
  "platform": "linux",
  "cores": 4,
  "memoryLimitBytes": 8589934592,
  "workers": 2,
  "threadsPerWorker": 2,
  "queueLimit": 4,
  "cacheEntries": 1,
  "timeoutSeconds": 60,
  "memoryLimitPerWorkerBytes": 3650722201,
  "suggestedMaxQuads": 17000000,
  "defaultLod": 1,
  "defaultTier": { "width": 2048, "height": 2048, "supersample": 2 }
}
```

- Pool size and threads default to `blockscope capacity`; `--workers`, `--threads`, `--queue`, and `--cache` override them.
- Jobs are queued and dispatched to idle workers. The queue is bounded; a full queue returns `BUSY`.
- Repeated inputs are routed to the worker that already cached them, so a second render of the same file skips parsing and meshing.
- A worker that exits mid-job produces `INTERNAL_ERROR` and is restarted.
- A job that exceeds `--timeout` (default 60 seconds, `0` disables) is killed, retried once on a fresh worker, then answered with `TIMEOUT`.
- A worker that exceeds its memory ceiling (85 percent of the available memory divided by the pool size, or `--memory` in MiB) is killed the same way with `MEMORY_LIMIT`.
- Results may arrive out of order because workers run in parallel; match them by `id`.
- Killed workers are reaped before replacement, so the pool does not accumulate zombies. A supervisor that dies closes the worker pipes, which makes the workers exit on stdin EOF.

## Cross-platform rules

| Concern             | Linux                                    | Windows                                           | macOS                                    |
| ------------------- | ---------------------------------------- | ------------------------------------------------- | ---------------------------------------- |
| Protocol I/O        | pipe bytes                               | pipe bytes                                        | pipe bytes                               |
| Graceful stop       | stdin EOF or `shutdown`                  | stdin EOF or `shutdown`                           | stdin EOF or `shutdown`                  |
| Kill policy         | `SIGKILL` after a 10 s grace             | `TerminateProcess` after a 10 s grace             | `SIGKILL` after a 10 s grace             |
| Child cleanup       | reaped by supervisor; exit on pipe close | reaped by supervisor; exit on pipe close          | reaped by supervisor; exit on pipe close |
| RSS source          | `/proc/<pid>/status` `VmRSS`             | `K32GetProcessMemoryInfo` working set             | `proc_pidinfo` `pti_resident_size`       |
| Temp directory      | `$TMPDIR`                                | `%TEMP%`                                          | `$TMPDIR`                                |
| Path separator      | `/`                                      | `/` or `\`; absolute drive paths required         | `/`                                      |
| Non-UTF-8 paths     | rejected                                 | not possible; all paths are UTF-16, sent as UTF-8 | rejected                                 |
| Atomic file replace | rename                                   | delete then rename, or `ReplaceFile`              | rename                                   |

Chinese and other non-ASCII file names are supported because every path crosses the pipe as UTF-8. Supervisors on Windows must read the pipe as UTF-8, not through the console code page.

The engine must never depend on a console: no colour codes, no interactive prompts, no reading from a TTY.

## Supervisor contract

- Start `N` workers, each with `T` rasterizer threads, such that `N * T <= physical cores`. Reference: `2 x 6` on a 12-thread machine.
- Route jobs by `inputHash` so a repeated input lands on the worker that already holds its snapshot and mesh cache. A cold worker pays the material load once.
- Queue depth is bounded. When full, reject with a busy response rather than growing memory.
- Timeout per job, kill and restart the worker, retry once, then surface `TIMEOUT`.
- On worker crash, restart it; an in-flight job is retried once.
- Never write protocol data to a worker's stdin while a job is in flight.

## Versioning

- `protocolVersion` is an integer, incremented for breaking changes.
- New optional request fields and new response fields are not breaking.
- Removing fields, renaming fields, or changing defaults is breaking.
- Requests do not carry `protocolVersion`; clients compare the value in `hello` before sending jobs. An unknown request `type` returns `INVALID_REQUEST` and leaves the worker running.

## Non-goals for v2

- Cancellation of a running job.
- Streaming or progressive output.
- Non-PNG formats.
- In-process job queues or multi-job batching.

## Overrides

`serve` and `supervise` accept `--force-limits`, which raises the parser, mesh, and render caps for that process. It prints a warning on stderr and is meant for offline work on oversized schematics, not for the default service profile.
