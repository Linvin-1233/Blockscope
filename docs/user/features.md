# 功能与限制

Blockscope 是一个把 Minecraft `.litematic` 投影文件渲染成 PNG 的
无头 Rust 图像渲染引擎，面向通用后端服务、批处理工具和自动化管线。

当前 workspace 版本为 `0.1.0`，支持 Windows 和 Linux。仓库包含解析、材质、
网格、CPU 光栅化、CLI 和 stdio 服务；不包含特定聊天平台或机器人适配层。

## 功能

- 解析 `.litematic` v1-v7：metadata、regions、palette、packed block states、
  block entities、entities 和 pending ticks
- 生成规范 Scene IR、材料统计、诊断、稳定 manifest 和 SHA-256 哈希
- 内置色块 catalog：无需资源包即可渲染常见方块、partial shapes 和流体
- 资源包材质：blockstate/model、parent 链、multipart、UV、旋转、tint、
  cutout/translucent、PNG 贴图和 overlay
- 内置箱子 body/lid/lock 模型和 entity 贴图
- 网格导出：OBJ、带顶点颜色的 PLY
- CPU 正交相机、tile 光栅化、`flat-v1` / `simple-ao-v1` 光照、PNG 输出
- 超采样 1-4、多视角、并行 tile、分页流式渲染
- LOD 固定因子 `2..=16` 或 `auto`，用于超大结构降级
- `serve` 长驻 worker 和 `supervise` 多进程调度，使用 newline-delimited JSON
- 容量检测、内存预算、mesh/render 上限和稳定错误码
- 黄金 manifest/PNG、并发和协议测试

## 当前限制

- 不包含特定聊天平台/机器人适配层；本仓库只提供渲染引擎和 stdio 协议
- 不渲染通用实体模型、实体动画、block entity 文本/图标、banner pattern 或
  skull skin
- 材质 tint 使用固定 plains 色，不读取 biome colormap
- 没有阴影、完整光照传播、emissive bloom 或 mipmap
- LOD 会丢弃 block entities、entities 和 pending ticks
- 资源包、客户端 jar 和私有 `.litematic` 不随仓库提交
- 没有网络端口；`serve`/`supervise` 只通过 stdio 与上层服务通信

## 项目结构

| 路径 | 内容 |
|---|---|
| `../../crates/nbt` | NBT 解码、压缩检测、资源限制 |
| `../../crates/litematic` | `.litematic` 容器解析和 Scene 构建 |
| `../../crates/scene` | Scene IR、统计、诊断、manifest、hash、LOD |
| `../../crates/material` | blockstate/model、资源包、PNG、tint、chest |
| `../../crates/mesh` | 色块/资源包网格、面剔除、AO、OBJ/PLY |
| `../../crates/render` | CPU 光栅化、tile、超采样、PNG |
| `../../crates/cli` | `blockscope` CLI、serve、supervise、容量检测 |
| `..` | 格式、渲染、材质、协议和部署文档 |
| `../../fixtures/golden` | 黄金 manifest 和 PNG |
| `../../scripts` | Windows/Linux 质量门禁 |

## 下一步

- [安装与快速开始](getting-started.md)
- [CLI 命令](cli.md)
- [限制与安全](limits.md)
- [English](en/features.md)
