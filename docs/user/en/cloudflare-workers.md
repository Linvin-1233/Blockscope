# Cloudflare Workers

The `blockscope-worker` crate exposes the CPU renderer as a stateless HTTP
endpoint. It accepts a `.litematic` body and renders a PNG in a single request.

A resource pack can be bundled as a Workers Static Asset and read through the
`ASSETS` binding. The pack ZIP is loaded in memory, so the Worker still does not
need a filesystem. Without a pack, the Worker falls back to the built-in color
catalog.

Static assets are bundled with each Worker version. To change the pack, rebuild
and redeploy; runtime pack selection needs R2 or another object store, which
this adapter does not support yet.

## Deploy

Prerequisites: Rust, the `wasm32-unknown-unknown` target, `worker-build` 0.8.7,
and an authenticated `cf` CLI.

```bash
rustup target add wasm32-unknown-unknown
cargo install worker-build --version 0.8.7 --locked
./scripts/build-worker.sh
cf deploy --prebuilt
```

`build-worker.sh` writes the Cloudflare Build Output to `.cloudflare/output/v0`.
If the account has more than one account, set `CLOUDFLARE_ACCOUNT_ID` before
deploying. `just worker` and `just worker-deploy` run the same commands.

### Bundle a resource pack

Put the pack at `pack.zip` in the repository root, or point at it explicitly:

```bash
RESOURCE_PACK=~/packs/vanilla.zip ./scripts/build-worker.sh
cf deploy --prebuilt
```

The build copies the pack to the Worker's `assets/` directory, enables the
`ASSETS` binding, and exposes it internally at `/pack.zip`. The committed
default is `RESOURCE_PACK_PATH=/pack.zip`; override the URL without moving the
file:

```bash
RESOURCE_PACK_PATH=/packs/vanilla.zip ./scripts/build-worker.sh
```

`pack.zip` is ignored by git. Do not commit Minecraft assets. If the file is
missing, the build produces a color-only Worker and logs a warning.

Limits: static assets allow 25 MiB per file and 20,000 files on Free
(100,000 on Paid). This adapter caps the pack at 24 MiB so the runtime check
fails before the isolate runs out of memory. A full client jar is usually too
large; use a resource-pack ZIP.

The Worker URL is printed by `cf deploy`:

```text
https://<worker>.<account-subdomain>.workers.dev
```

### Configure the endpoint

Edit `crates/worker/worker.config.json`:

```json
{
  "assets": { "runWorkerFirst": true },
  "env": {
    "ASSETS": { "type": "assets" },
    "RESOURCE_PACK_PATH": { "type": "text", "value": "/pack.zip" },
    "ENDPOINT": { "type": "text", "value": "/render" },
    "ALLOWED_ORIGIN": { "type": "text", "value": "*" }
  }
}
```

Or override at build time without editing the file:

```bash
ENDPOINT=/api/render ALLOWED_ORIGIN=https://example.com ./scripts/build-worker.sh
```

`ENDPOINT` must start with `/`. `ALLOWED_ORIGIN` controls the CORS header for
the render and health routes. Setting `RESOURCE_PACK_PATH` to an empty string
disables texture loading and forces color-only output. `runWorkerFirst` keeps
the pack off the public asset routes; the Worker reads it through `ASSETS`.

### CPU limits

The Workers Free plan uses a 10 ms CPU limit and rejects custom limits. Small
previews can finish, but larger structures need a paid plan. On a paid plan,
add a CPU limit at build time:

```bash
CPU_MS=30000 ./scripts/build-worker.sh
cf deploy --prebuilt
```

## API

`POST <endpoint>` with the raw `.litematic` bytes. The default endpoint is
`/render`.

| Query | Default | Notes |
|---|---|---|
| `width`, `height` | 512 | 1 to 1024; `width * height` at most 1,048,576 |
| `supersample` | 1 | 1 or 2 |
| `yaw`, `elevation` | 45, 30 | Degrees; elevation within ±89.9 |
| `lod` | 1 | 1 to 16 |
| `background` | transparent | `#RRGGBB` or `#RRGGBBAA` |
| `lighting` | `simple-ao-v1` | `simple-ao-v1` or `flat-v1` |

```bash
curl -X POST "https://<worker>/render?width=512&lod=1" \
  --data-binary @build.litematic \
  -o render.png
```

Successful responses are `image/png`. Errors are JSON:

```json
{
  "code": "MESH_LIMIT_EXCEEDED",
  "message": "...",
  "hint": "lower lod or request a smaller image"
}
```

`GET /health` returns engine status, including `textures: true|false`.
`OPTIONS` handles CORS preflight. When a pack is configured, block models and
textures are resolved from it; missing textures keep their fallback color.

Limits: 8 MiB input, 32 MiB decompressed NBT, 64,000,000 blocks, 200,000 mesh
quads, 24 MiB resource pack, and 1,048,576 output pixels. Decoding a pack and
its textures costs CPU and memory; keep the mesh cap low on the Free plan.

## Build a frontend

Host the UI separately (Cloudflare Pages, a static bucket, or another Worker)
and call the render endpoint. CORS is enabled, so no proxy is required.

```html
<input id="file" type="file" accept=".litematic">
<button id="render">Render</button>
<img id="out" alt="">

<script>
  const endpoint = "https://<worker>/render";

  document.querySelector("#render").addEventListener("click", async () => {
    const file = document.querySelector("#file").files[0];
    if (!file) return;

    const response = await fetch(`${endpoint}?width=512&height=512&lod=1`, {
      method: "POST",
      headers: { "content-type": "application/octet-stream" },
      body: file,
    });

    if (!response.ok) {
      const error = await response.json();
      throw new Error(`${error.code}: ${error.message}`);
    }

    const url = URL.createObjectURL(await response.blob());
    document.querySelector("#out").src = url;
  });
</script>
```

Keep the frontend out of this crate: it is a separate deployment artifact. Send
the file as the raw request body (not `multipart/form-data`) and read the
response as a blob. For large structures, expose the `lod` and `width`/`height`
controls so callers can stay within the CPU and pixel budgets.

## Non-goals

- Runtime pack switching without a redeploy (use R2 for that)
- Persistent caches, job queues, or cancellation
- Filesystem access
