# Materials and Resource Packs

## Built-in Colors

Without a resource pack, the engine uses its built-in color catalog. This is
useful for quick previews, CI, and golden tests, but it does not reproduce real
textures.

```bash
blockscope render model.litematic --output out.png
```

## Minecraft Client Jar

```bash
blockscope render model.litematic --output out.png --client-jar client.jar
```

The jar may contain `assets/minecraft/blockstates`, `models`, and `textures`.
The engine reads the jar directly and does not extract it into the working
directory.

## Extracted Assets

```bash
blockscope render model.litematic --output out.png --assets ./assets
```

`--assets` points to a directory containing `assets/minecraft/...` (or a
directory whose root maps to the Minecraft asset tree).

## Resource-Pack Overlays

```bash
blockscope render model.litematic --output out.png \
  --client-jar client.jar \
  --resource-pack ./packs/my-pack
```

`--resource-pack` may be a directory, zip, or jar. Matching blockstates,
models, and textures in the overlay replace the base.

## Fallback Behavior

When a model or texture is missing, the engine falls back in this order:

1. Pack model
2. Built-in partial shape or color
3. Pack `particle` texture, if available
4. Magenta unknown block

Use `materials --json` to inspect coverage:

```bash
blockscope materials model.litematic --client-jar client.jar --json
```

Non-JSON `render` output also prints material diagnostics. See
[Resource-pack materials](../../textures.md) for the full contract.

## Asset Policy

Resource packs, client jars, and private `.litematic` files must be supplied by
the deployer. The repository does not commit `.jar` files, `resourcepacks/`,
Minecraft textures, or user uploads.

## See Also

- [CLI Reference](./cli.md)
- [Limits and Security](./limits.md)
- [Troubleshooting](./troubleshooting.md)
- [中文](../materials.md)
