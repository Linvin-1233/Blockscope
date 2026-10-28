---
title: "LOD 与大结构"
---

默认 `--lod 1` 保留完整细节。超大结构可以使用固定 LOD 或自动预算：

```bash
blockscope render huge.litematic --output huge.png --lod 4
blockscope render huge.litematic --output huge.png --lod auto
```

## 模式

- `--lod 1`：关闭 LOD，渲染完整网格
- `--lod 2..=16`：固定 LOD 因子，适合需要跨机器复现的输出
- `--lod auto`：先尝试精确渲染，内存预算不足时依次尝试 `2`、`4`、`8`、`16`

## 行为

LOD 会把每个 `N x N x N` cell 聚合为最常见的非空气方块；平票时使用规范状态
顺序。重叠 region 使用与 mesh builder 相同的“后出现的 region 优先”策略。

LOD 场景会丢弃：

- block entities
- entities
- pending ticks

因此 LOD 只适合预览，不适合依赖容器内容或实体细节的最终展示。

## 内存与可复现性

`auto` 使用 `capacity` 计算出的 quad 预算；不同内存的机器可能选择不同因子。
如果需要跨机器得到相同输出，请使用固定 `--lod`。

`--force-limits` 会提高资源上限，但不会绕过线程、纹理和 NBT 深度等固定限制；
只应对可信输入使用，并确保机器有足够内存。

更多性能数据和实现细节见 [CPU 渲染说明](/rendering)。

## 相关文档

- [CLI 命令](./cli.md)
- [限制与安全](./limits.md)
- [English](/user/lod)
