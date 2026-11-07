---
title: "Cloudflare Workers"
---

`blockscope-worker` crate 把 CPU 渲染器暴露为无状态 HTTP 端点。它接受请求体中的
`.litematic` 文件，并在单次请求中返回 PNG。

资源包可以作为 Workers 静态资源，通过 `ASSETS` 绑定加载。ZIP 包在内存中读取，
因此 Worker 不需要文件系统。没有资源包时，Worker 回退到内置色块 catalog。

静态资源随每个 Worker 版本一起打包。要更换资源包必须重新构建并部署；运行时选择
资源包需要 R2 或其他对象存储，本适配器暂不支持。

## 部署

前置条件：Rust、`wasm32-unknown-unknown` target，以及已登录的 `cf` CLI。
`cargo xtask worker` 会在首次使用时安装 `worker-build` 0.8.7。

```bash
rustup target add wasm32-unknown-unknown
cargo xtask worker
cf deploy --prebuilt
```

`cargo xtask worker` 把 Cloudflare Build Output 写入 `.cloudflare/output/v0`，
在 Windows、Linux 和 macOS 上行为一致。如果账号下有多个 account，部署前设置
`CLOUDFLARE_ACCOUNT_ID`。`just worker` 和 `just worker-deploy` 运行相同命令。

### 打包资源包

把资源包放在仓库根目录的 `pack.zip`，或显式指定路径：

```bash
RESOURCE_PACK=~/packs/vanilla.zip cargo xtask worker
cf deploy --prebuilt
```

Windows PowerShell 需要先设置环境变量：

```powershell
$env:RESOURCE_PACK = "C:\packs\vanilla.zip"
cargo xtask worker
cf deploy --prebuilt
```

构建会把资源包复制到 Worker 的 `assets/` 目录，启用 `ASSETS` 绑定，并在内部通过
`/pack.zip` 暴露。仓库默认 `RESOURCE_PACK_PATH=/pack.zip`；不移动文件也可以覆盖
URL：

```bash
RESOURCE_PACK_PATH=/packs/vanilla.zip cargo xtask worker
```

`pack.zip` 已被 git 忽略。不要提交 Minecraft 资产。文件缺失时构建会生成仅色块的
Worker 并输出警告。

限制：Free 方案静态资源单文件 25 MiB、最多 20,000 个文件（Paid 方案 100,000 个）。
本适配器把资源包限制为 24 MiB，让运行时检查在 isolate 耗尽内存前失败。完整客户端
jar 通常过大，请使用资源包 ZIP。

`cf deploy` 会输出 Worker URL：

```text
https://<worker>.<account-subdomain>.workers.dev
```

### 配置端点

编辑 `crates/worker/worker.config.json`：

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

也可以不编辑文件，在构建时覆盖：

```bash
ENDPOINT=/api/render ALLOWED_ORIGIN=https://example.com cargo xtask worker
```

`ENDPOINT` 必须以 `/` 开头。`ALLOWED_ORIGIN` 控制渲染和健康检查路由的 CORS
响应头。把 `RESOURCE_PACK_PATH` 设为空字符串会关闭纹理加载并强制仅色块输出。
`runWorkerFirst` 让资源包不出现在公开静态资源路由上；Worker 通过 `ASSETS` 读取。

### CPU 限制

Workers Free 方案限制 10 ms CPU 时间且不允许自定义上限。小预览可以完成，更大的
结构需要付费方案。付费方案可以在构建时设置 CPU 上限：

```bash
CPU_MS=30000 cargo xtask worker
cf deploy --prebuilt
```

## API

向 `<endpoint>` 发送 `POST`，请求体为原始 `.litematic` 字节。默认端点为 `/render`。

| 查询参数           | 默认值         | 说明                                          |
| ------------------ | -------------- | --------------------------------------------- |
| `width`, `height`  | 512            | 1 到 1024；`width * height` 最多 1,048,576    |
| `supersample`      | 1              | 1 或 2                                        |
| `yaw`, `elevation` | 45, 30         | 角度；elevation 在 ±89.9 内                   |
| `background`       | transparent    | `#RRGGBB` 或 `#RRGGBBAA`                      |
| `lighting`         | `simple-ao-v1` | `simple-ao-v1` 或 `flat-v1`                   |

```bash
curl -X POST "https://<worker>/render?width=512" \
  --data-binary @build.litematic \
  -o render.png
```

成功响应为 `image/png`。错误响应为 JSON：

```json
{
  "code": "MESH_LIMIT_EXCEEDED",
  "message": "...",
  "hint": null
}
```

`GET /health` 返回引擎状态，包括 `textures: true|false`。`OPTIONS` 处理 CORS
预检。配置资源包后，方块模型和纹理从包中解析；缺失纹理保留回退颜色。

限制：输入 8 MiB、解压后 NBT 32 MiB、64,000,000 个方块、200,000 个网格 quad、
资源包 24 MiB、输出 1,048,576 像素。请求体按上限流式读取，因此分块传输或低报
`Content-Length` 的请求会在 8 MiB 后拒绝，而不是完整缓冲。解码资源包和纹理会消耗
CPU 和内存；解析后的材质集缓存在 isolate 中，`RESOURCE_PACK_PATH` 变化时会替换
缓存条目。

## 构建前端

将 UI 单独托管（Cloudflare Pages、静态存储桶或其他 Worker）并调用渲染端点。
CORS 已启用，不需要代理。

```html
<input id="file" type="file" accept=".litematic" />
<button id="render">Render</button>
<img id="out" alt="" />

<script>
  const endpoint = "https://<worker>/render";

  document.querySelector("#render").addEventListener("click", async () => {
    const file = document.querySelector("#file").files[0];
    if (!file) return;

    const response = await fetch(`${endpoint}?width=512&height=512`, {
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

前端不要放进这个 crate：它是独立的部署产物。把文件作为原始请求体发送（不要用
`multipart/form-data`），并以 blob 读取响应。对于大结构，请暴露 `width`/`height`
控件，让调用方控制 CPU 和像素预算。

## 相关文档

- [服务模式](./service.md)
- [限制与安全](./limits.md)
- [部署说明](/deployment)
- [English](/user/cloudflare-workers)
