---
title: "限制与安全"
---

## 默认限制

| 项目                                        | 默认值                                               |
| ------------------------------------------- | ---------------------------------------------------- |
| 输入文件                                    | 64 MiB，必须是普通文件                               |
| 解压后 NBT                                  | 256 MiB                                              |
| NBT 深度 / tag 数                           | 64 / 4,000,000                                       |
| NBT string / list / array                   | 1 MiB / 4,000,000 / 64,000,000                       |
| NBT 总分配预算                              | 512 MiB                                              |
| regions                                     | 1,024                                                |
| 单 region 体积                              | 100,000,000 格                                       |
| 总 region 体积                              | 200,000,000 格                                       |
| palette / block entities / entities / ticks | 每个 region 1,000,000                                |
| 区域坐标范围                                | ±30,000,000                                          |
| 网格                                        | 5,000,000 quads / 20,000,000 vertices / 65,536 pages |
| 渲染像素                                    | 16,777,216；超采样后 33,554,432；单边 4096           |
| 单请求视角数                                | 最多 64                                              |
| 线程                                        | 最多 256                                             |
| 纹理                                        | 2048x2048，解码后 256 MiB，单个资产 8 MiB            |

网格上限还会按可用内存预算进一步收紧。

## `--force-limits`

`--force-limits` 会提高以下上限：

- 输入文件、解压后 NBT 和总 NBT 分配预算
- 场景/region 体积
- mesh quads、vertices、pages
- render 像素上限

它不会绕过线程、纹理、NBT 深度、NBT tag 数和单边 4096 像素等固定限制。该选项可能让进程耗尽内存，只应对可信输入使用。

## 安全边界

- `serve`/`supervise` 只通过 stdio 通信，不监听网络端口
- 输入必须是普通文件；目录、FIFO、设备和超限文件会被拒绝
- 输入路径和输出目录由调用方控制；请使用专用低权限用户运行
- 不要在未校验路径和资源包的情况下把服务暴露给不可信客户端
- 资源包和客户端 jar 由部署者提供，仓库不分发 Minecraft 资产

## 错误码

协议错误码、可重试性和 manifest 字段见 [引擎协议 v2](/engine-protocol)。常见错误：

- `INPUT_TOO_LARGE`：输入或资源超限
- `INVALID_REQUEST`：请求格式错误、相对路径或非普通文件
- `INVALID_RENDER_OPTIONS`：尺寸、光照、背景或线程数非法
- `MESH_LIMIT_EXCEEDED` / `PAGE_LIMIT_EXCEEDED`：网格上限
- `INVALID_NBT` / `INVALID_LITEMATIC`：文件损坏或格式不支持
- `ASSET_SOURCE_REQUIRED`：`materials` 缺少资源包或客户端 jar

## 相关文档

- [CLI 命令](./cli.md)
- [服务模式](./service.md)
- [故障排查](./troubleshooting.md)
- [English](/user/limits)
