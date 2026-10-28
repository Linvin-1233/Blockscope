---
title: "LOD and Large Structures"
---

`--lod 1` is the default and keeps full detail. For large structures, use a
fixed LOD or the automatic budget:

```bash
blockscope render huge.litematic --output huge.png --lod 4
blockscope render huge.litematic --output huge.png --lod auto
```

## Modes

- `--lod 1`: disable LOD and render the full mesh
- `--lod 2..=16`: fixed LOD factor, suitable for reproducible output across hosts
- `--lod auto`: try exact rendering first, then `2`, `4`, `8`, and `16` as the
  memory budget requires

## Behavior

LOD aggregates each `N x N x N` cell into the most frequent non-air state; ties
use canonical state order. Overlapping regions use the same "later region wins"
policy as the mesh builder.

LOD drops:

- block entities
- entities
- pending ticks

Use LOD for previews, not for final output that depends on container contents or
entity detail.

## Memory and Reproducibility

`auto` uses the quad budget derived from `capacity`; hosts with different
memory may choose different factors. Use a fixed `--lod` when output must be
reproducible across machines.

`--force-limits` raises resource caps but does not disable the fixed thread,
texture, or NBT-depth limits. Use it only with trusted input and enough memory.

See [CPU rendering](/rendering) for performance data and
implementation details.

## See Also

- [CLI Reference](./cli.md)
- [Limits and Security](./limits.md)
- [中文](/zh/user/lod)
