---
title: "LOD 与大结构"
---

默认 `--lod 1` 保留完整细节。LOD 永远不会自动选择：需要减少几何量时，必须显式指定固定因子。

```bash
blockscope render huge.litematic --output huge.png --lod 4
```

启用 LOD 会打印警告，因为数据层聚合无法保留方块级机制：

```text
warning: Enabling LOD will result in a significant loss of detail
```

## 模式

- `--lod 1`：关闭 LOD，渲染完整网格
- `--lod 2..=16`：固定 LOD 因子，适合需要跨机器复现的输出

不再提供 `auto` 模式。按内存预算自动选择因子会静默改变输出，而聚合后的 cell 对红石等机制而言在功能上是不正确的。结构过大时优先考虑面剔除、惰性网格构建或视锥裁剪；只有在必须生成预览时才显式启用 LOD。

## 行为

LOD 会把每个 `N x N x N` cell 聚合为最常见的非空气方块；平票时使用规范状态顺序。重叠 region 使用与 mesh builder 相同的“后出现的 region 优先”策略。

LOD 场景会丢弃：

- block entities
- entities
- pending ticks

因此 LOD 只适合预览，不适合依赖容器内容、红石状态或实体细节的最终展示。

## 内存与可复现性

固定 `--lod` 在所有机器上产生相同输出。`--force-limits` 会提高资源上限，但不会绕过线程、纹理和 NBT 深度等固定限制；只应对可信输入使用，并确保机器有足够内存。

更多性能数据和实现细节见 [CPU 渲染说明](/rendering)。

## 相关文档

- [CLI 命令](./cli.md)
- [限制与安全](./limits.md)
- [English](/user/lod)
