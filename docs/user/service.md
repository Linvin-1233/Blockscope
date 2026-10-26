# 服务模式

`serve` 在 stdin/stdout 上使用 newline-delimited JSON，不监听网络端口。它适合
由受信任的上层服务、批处理工具或自动化管线作为子进程启动。

## 启动 worker

```bash
blockscope serve --client-jar client.jar --threads 4 --cache 2
```

- `--threads`：请求未指定 `threads` 时使用的默认线程数；最多 256
- `--cache`：缓存的 scene/mesh 条数上限
- `--cache-bytes`：缓存预算（估算字节）；`0` 使用可用内存的一半。超出预算时
  按最旧优先淘汰，单个超大条目仍允许单独保留
- `--force-limits`：提高资源上限；只应对可信输入使用
- `--ignore-block-entity-nbt`：跳过容器 NBT

worker 在资源包准备完成后输出一行 `hello`，之后才接受 job。

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
- `input.hash` 可选，用于校验 SHA-256；命中缓存时 worker 不再读取或哈希文件
- `supervise` 会把为路由计算出的 hash 注入请求，因此经 supervisor 的重复输入
  也能走免读缓存路径
- `views` 为空时默认 `[45]`；有完整 mesh 时多视角并行渲染
- 输出文件为 `{id}.png` 或 `{id}-{yaw}.png`
- 完整字段、结果和错误码见 [引擎协议 v1](../engine-protocol.md)

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

| 选项 | 说明 |
|---|---|
| `--workers` | worker 进程数；`0` 使用 `capacity` 建议值 |
| `--threads` | 每个 worker 的线程数；`0` 使用 `capacity` 建议值 |
| `--timeout` | 每个 job 的超时秒数；`0` 关闭 |
| `--memory` | 每个 worker 的内存上限（MiB）；`0` 根据可用内存推导 |
| `--queue` | 等待队列长度；`0` 使用 `workers * 2` |
| `--cache` | 每个 worker 缓存的 scene/mesh 条数上限 |
| `--cache-bytes` | 每个 worker 的缓存预算（估算字节）；`0` 使用可用内存的一半 |

## 安全边界

`serve` 和 `supervise` 只应被可信调用方使用。输入路径、输出目录和资源包路径由
调用方控制；不要把 stdio 服务直接暴露给不可信网络客户端。

## 相关文档

- [引擎协议 v1](../engine-protocol.md)
- [Docker](./docker.md)
- [故障排查](./troubleshooting.md)
- [English](./en/service.md)
