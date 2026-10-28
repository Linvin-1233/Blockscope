---
title: "LOD and Large Structures"
---

`--lod 1` is the default and keeps full detail. LOD is never selected
automatically: choose a fixed factor explicitly when a preview needs less
geometry.

```bash
blockscope render huge.litematic --output huge.png --lod 4
```

Enabling LOD prints a warning because data-layer aggregation cannot preserve
block-level mechanics:

```text
warning: Enabling LOD will result in a significant loss of detail
```

## Modes

- `--lod 1`: disable LOD and render the full mesh
- `--lod 2..=16`: fixed LOD factor, suitable for reproducible output across hosts

There is no `auto` mode. Choosing a factor from a memory budget silently changed
the output, and aggregated cells are functionally incorrect for redstone and
other mechanics. Prefer face culling, lazy meshing, or frustum pruning when a
scene is too large; use an explicit LOD only as a last-resort preview.

## Behavior

LOD aggregates each `N x N x N` cell into the most frequent non-air state; ties
use canonical state order. Overlapping regions use the same "later region wins"
policy as the mesh builder.

LOD drops:

- block entities
- entities
- pending ticks

Use LOD for previews, not for final output that depends on container contents,
redstone state, or entity detail.

## Memory and Reproducibility

A fixed `--lod` produces the same output on every host. `--force-limits` raises
resource caps but does not disable the fixed thread, texture, or NBT-depth
limits. Use it only with trusted input and enough memory.

See [CPU rendering](/rendering) for performance data and implementation details.

## See Also

- [CLI Reference](./cli.md)
- [Limits and Security](./limits.md)
- [中文](/zh/user/lod)
