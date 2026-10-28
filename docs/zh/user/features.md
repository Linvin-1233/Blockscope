---
title: "功能与限制"
---

Blockscope 是一个把 Minecraft `.litematic` 投影文件渲染成 PNG 的
无头 Rust 图像渲染引擎，面向通用后端服务、批处理工具和自动化管线。

当前 workspace 版本为 `0.4.1`，支持 Windows、Linux 和 macOS。仓库包含解析、材质、
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
- 可选 `--greedy-mesh` 共面合并，降低大平面和重复结构的 quad 数
- CPU 正交相机、tile 光栅化、`flat-v1` / `simple-ao-v1` 光照、PNG 输出
- 可选离屏 `wgpu` 光栅化（`--features gpu`，`--backend gpu` 或 `auto`）；CPU
  仍是默认后端和逐字节参考
- 超采样 1-4、多视角、并行 tile、分页流式渲染
- LOD 固定因子 `2..=16`，必须显式选择，不再自动降级
- `serve` 长驻 worker 和 `supervise` 多进程调度，使用 newline-delimited JSON
- 容量检测、内存预算、mesh/render 上限和稳定错误码
- 黄金 manifest/PNG、并发和协议测试

## 当前限制

- 不包含特定聊天平台/机器人适配层；本仓库只提供渲染引擎和 stdio 协议
- 不渲染通用实体模型、实体动画、block entity 文本/图标、banner pattern 或
  skull skin
- 材质 tint 使用固定 plains 色，不读取 biome colormap
- 没有阴影、完整光照传播、emissive bloom 或 mipmap
- GPU 输出随厂商和驱动变化，不保证逐字节一致，也不包含在官方发布二进制中
- LOD 会丢弃 block entities、entities 和 pending ticks
- 资源包、客户端 jar 和私有 `.litematic` 不随仓库提交
- 没有网络端口；`serve`/`supervise` 只通过 stdio 与上层服务通信

## 下一步

- [安装与快速开始](./getting-started.md)
- [CLI 命令](./cli.md)
- [限制与安全](./limits.md)
- [English](/user/features)
