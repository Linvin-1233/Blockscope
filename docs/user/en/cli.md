# CLI Reference

See [Installation and Quick Start](./getting-started.md) for building
`blockscope` and locating the binary.

## Commands

| Command | Purpose |
|---|---|
| `inspect` | Print the stable manifest, including bounds, regions, statistics, diagnostics, `inputHash`, and `sceneHash` |
| `validate` | Validate a file and print a summary |
| `stats` | Print block/material statistics |
| `mesh` | Build a mesh and optionally export OBJ or PLY |
| `render` | Render PNG output or display directly in the terminal |
| `bench` | Measure parse, LOD, mesh, render, and PNG timings |
| `materials` | Report resource-pack coverage for the scene |
| `capacity` | Print detected CPU/memory capacity and suggested worker/thread settings |
| `serve` | Long-lived worker over the stdio JSON protocol |
| `supervise` | Spawn and route jobs across multiple worker processes |

## Examples

```bash
# Manifest
blockscope inspect model.litematic --json
blockscope inspect model.litematic --json --compact

# Validation and statistics
blockscope validate model.litematic
blockscope stats model.litematic --json

# Mesh export
blockscope mesh model.litematic --obj out.obj
blockscope mesh model.litematic --ply out.ply

# Resolution, anti-aliasing, and multi-view
blockscope render model.litematic --output out.png \
  --width 2048 --height 2048 --supersample 2 --threads 4
blockscope render model.litematic --output out.png --views 45,225

# Huge structures
blockscope render huge.litematic --output huge.png --lod auto
blockscope render huge.litematic --output huge.png --lod 4
blockscope render huge.litematic --output huge.png --lod 1 --stream-mesh

# Material coverage
blockscope materials model.litematic --client-jar client.jar --json

# Capacity
blockscope capacity --json
# Terminal image rendering (Kitty, iTerm2, Sixel, Halfblocks/Truecolor)
blockscope render model.litematic --terminal
blockscope render model.litematic -t --terminal-protocol kitty
blockscope render model.litematic -t --no-file
blockscope preview model.litematic -t
```

A single view writes `out.png`. Multiple views write `out-45.png`,
`out-225.png`, and so on. In `serve` mode the names are `{id}.png` for one view
or `{id}-{yaw}.png` for multiple views.

## Common Options

| Option | Description |
|---|---|
| `--client-jar <PATH>` | Read blockstates/models/textures directly from a Minecraft client jar |
| `--assets <DIR>` | Read from an extracted `assets/` directory |
| `--resource-pack <PATH>` | Overlay a directory, zip, or jar; takes precedence over the base |
| `--texture-filter nearest\|linear` | Texture sampling; default `nearest` |
| `--width` / `--height` | Output size; default `1024x1024` |
| `--supersample <1-4>` | Supersampling factor; default `2` |
| `--views <LIST>` | Comma-separated yaw angles, e.g. `45,225` |
| `--yaw` / `--elevation` | Single-view horizontal/elevation angles |
| `--background transparent\|#RRGGBB[AA]` | Background; default transparent |
| `--lighting flat-v1\|simple-ao-v1` | Lighting profile; default `simple-ao-v1` |
| `--threads <N>` | Mesh/rasterizer threads; `0` uses available parallelism, capped at 256 |
| `--lod 1\|2-16\|auto` | LOD factor; default `1` (disabled) |
| `--stream-mesh` | Build and render mesh in page chunks without retaining the full mesh; cannot be combined with `--lod auto` |
| `--force-limits` | Raises input, scene, mesh, and pixel caps; can exhaust memory |
| `--ignore-block-entity-nbt` | Skip container NBT; always on for `mesh`/`render`/`bench` |
| `-t, --terminal` | Render and display the image directly in the terminal |
| `--terminal-protocol <auto\|kitty\|iterm2\|sixel\|halfblocks>` | Specify terminal graphics protocol; defaults to auto-detection |
| `--terminal-width <COLS>` / `--terminal-height <ROWS>` | Target display width in columns and height in rows |
| `--no-file` | Render only to the terminal without writing an output PNG file |

`inspect`, `validate`, and `stats` keep `TileEntities` by default. Geometry
commands skip them by default to reduce parse time and memory for large
containers.

## Exit Codes

- `0`: success
- `1`: internal error
- `2`: invalid input, parse failure, or resource limit

## See Also

- [Materials and Resource Packs](./materials.md)
- [LOD and Large Structures](./lod.md)
- [Limits and Security](./limits.md)
- [Service Mode](./service.md)
- [中文](../cli.md)
