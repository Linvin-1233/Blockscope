---
title: "服务模式"
---

`serve` 在 stdin/stdout 上使用 newline-delimited JSON，不监听网络端口。它适合由受信任的上层服务、批处理工具或自动化管线作为子进程启动。

## 启动 worker

```bash
blockscope serve --client-jar client.jar --threads 4 --cache 2
```

- `--threads`：请求未指定 `threads` 时使用的默认线程数；最多 256
- `--cache`：缓存的 scene/mesh 条数上限
- `--cache-bytes`：缓存预算（估算字节）；`0` 使用可用内存的一半。超出预算时按最旧优先淘汰，单个超大条目仍允许单独保留。由 `supervise` 启动时，若未显式指定，supervisor 会按每个 worker 的内存上限派生并传入该值
- `--greedy-mesh`：合并同材质、同光照的共面单位面；默认关闭
- `--ffmpeg`：MP4/GIF 任务使用的外部 ffmpeg 可执行文件；默认为 `PATH` 中的 `ffmpeg`
- `--force-limits`：提高资源上限；只应对可信输入使用
- `--ignore-block-entity-nbt`：跳过容器 NBT

worker 在资源包准备完成后输出一行 `hello`，之后才接受 job。`hello.backends`
列出 `cpu`、`auto`，以及编译了 `gpu` 特性时的 `gpu`。请求默认使用 `auto`：
小场景走 CPU，只有硬件适配器、f32 精度保护、60% 显存预算和成本模型都
预测有收益时才使用 GPU。`cpu` 是逐字节精确的显式覆盖；`gpu` 强制硬件，
不可用时返回 `BACKEND_UNAVAILABLE`。每个结果都通过
`statistics.backendUsed` 报告实际后端。

## 请求示例

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

- `input.path` 和 `outputDir` 必须是绝对路径
- 输入必须是普通文件
- `input.hash` 可选，是调用方声明的 SHA-256 身份；命中缓存时 worker 信任该值并不再读取文件，未命中时才重新计算并校验
- `supervise` 会把为路由计算出的 hash 注入请求，因此经 supervisor 的重复输入也能走免读缓存路径
- `views` 为空时默认 `[45]`，最多 64 个角度；有完整 mesh 时多视角并行渲染
- `backend` 为 `cpu`、`gpu` 或 `auto`，默认 `auto`。`cpu` 逐字节精确；`gpu` 在无可用硬件适配器时返回 `BACKEND_UNAVAILABLE`；`auto` 可能返回近似 GPU 输出，需要精确结果时请检查 `statistics.backendUsed`
- `streamMesh` 为 `off`（默认）、`on` 或 `auto`，按页构建并渲染且不保留完整 mesh，每个请求只支持一个视角。`off` 保留可缓存的完整 mesh 路径；流式请求不会写入 scene/mesh 缓存
- `auto` 下的 GPU 分配、驱动、精度或显存预算失败会在写出任何输出前回退到 CPU；第一次 GPU draw 之后的失败是致命错误，不会留下部分 PNG
- 输出文件为 `{id}.png` 或 `{id}-{yaw}.png`；调用方必须保证 `id` 和 `outputDir` 在并发任务中唯一，重复 `id` 或重复视角会覆盖同一路径
- 完整字段、结果和错误码见 [引擎协议 v4](/engine-protocol)

## 视频任务

把 `format` 设为 `mp4` 或 `gif`，并提供 `animation`：

```json
{
  "type": "render",
  "id": "clip-1",
  "input": { "path": "/data/model.litematic" },
  "format": "mp4",
  "background": "#000000",
  "width": 1024,
  "height": 1024,
  "animation": {
    "keyframes": [
      { "time": 0, "yaw": 45, "elevation": 30 },
      { "time": 6, "yaw": 405, "elevation": 30 }
    ],
    "fps": 30,
    "loop": true,
    "easing": "linear"
  },
  "encoder": "auto",
  "quality": "medium",
  "outputDir": "/data/out"
}
```

- 视频默认使用不透明黑色背景。MP4 还要求宽高为偶数；GIF 支持 `transparent`
- 视频任务会拒绝 `views`、`streamMesh` 和显式 `elevation`
- `encoder` 可为 `auto`（默认）、`software` 或 `nvenc` 等硬件编码器。`auto` 会按目标尺寸探测硬件，失败时回退到 `libx264`；显式硬件编码器不可用时返回 `ENCODER_UNAVAILABLE`
- 输出为 `{id}.mp4` 或 `{id}.gif`。结果中的输出项带 `kind: "video"`，并报告帧数、fps、时长、字节数、实际使用的编码器以及渲染/编码耗时
- ffmpeg 缺失或版本过低时，`hello.video` 的 `available` 为 `false`；视频请求会在解析输入前失败

## Supervisor

`supervise` 管理 worker 池、队列、超时和内存上限：

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

| 选项            | 说明                                                                 |
| --------------- | -------------------------------------------------------------------- |
| `--workers`     | worker 进程数，最多 256；`0` 使用 `capacity` 建议值                  |
| `--threads`     | 每个 worker 的线程数；`0` 使用 `capacity` 建议值                     |
| `--timeout`     | 每个图片 job 的超时秒数；`0` 关闭                                    |
| `--video-timeout` | 每个视频 job 的超时秒数；默认 600；`0` 关闭                        |
| `--ffmpeg`      | 传给每个 worker 的外部 ffmpeg 可执行文件                             |
| `--memory`      | 每个 worker 的内存上限（MiB）；`0` 根据可用内存推导                  |
| `--queue`       | 等待队列长度；`0` 使用 `workers * 2`                                 |
| `--cache`       | 每个 worker 缓存的 scene/mesh 条数上限                               |
| `--cache-bytes` | 每个 worker 的缓存预算（估算字节）；`0` 使用该 worker 内存上限的一半 |
| `--greedy-mesh` | 在每个 worker 中启用贪心网格合并；默认关闭                           |

## 安全边界

`serve` 和 `supervise` 只应被可信调用方使用。输入路径、输出目录和资源包路径由调用方控制；不要把 stdio 服务直接暴露给不可信网络客户端。

## 相关文档

- [引擎协议 v4](/engine-protocol)
- [Docker](./docker.md)
- [故障排查](./troubleshooting.md)
- [English](/user/service)
