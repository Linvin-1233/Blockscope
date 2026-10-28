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
or `{id}-{yaw}.png` for multiple views. When a complete mesh is available,
multiple views render concurrently within the `--threads` budget; output order
is preserved, and each view is written and released before the next batch.

Terminal images are fitted to the visible grid: when the rendered pixel size
would exceed the window, the next cell dimension is requested so the terminal
scales the image down while keeping its aspect ratio. The summary is printed
first and the image last, so text cannot scroll it out of view. Inside `tmux` or
`screen`, which drop graphics sequences unless passthrough is enabled,
auto-detection falls back to half blocks; pass `--terminal-protocol` to override
that. When stdout is not a terminal (a pipe, a log, a captured shell), a warning
is printed because the escape sequences cannot be displayed.

## Common Options

| Option | Description |
|---|---|
| `--client-jar <PATH>` | Read blockstates/models/textures directly from a Minecraft client jar |
| `--assets <DIR>` | Read from an extracted `assets/` directory |
| `--resource-pack <PATH>` | Overlay a directory, zip, or jar; takes precedence over the base |
| `--texture-filter nearest\|linear` | Texture sampling; default `nearest` |
| `--width` / `--height` | Output size; default `1024x1024`, maximum 4096 per side |
| `--supersample <1-4>` | Supersampling factor; default `2` |
| `--views <LIST>` | Comma-separated yaw angles, e.g. `45,225`; at most 64 |
| `--yaw` / `--elevation` | Single-view horizontal/elevation angles |
| `--background transparent\|#RRGGBB[AA]` | Background; default transparent |
| `--lighting flat-v1\|simple-ao-v1` | Lighting profile; default `simple-ao-v1` |
| `--threads <N>` | Mesh/rasterizer threads; `0` uses available parallelism, capped at 256 |
| `--lod 1\|2-16\|auto` | LOD factor; default `1` (disabled) |
| `--stream-mesh` | Build and render mesh in page chunks without retaining the full mesh; cannot be combined with `--lod auto` |
| `--greedy-mesh` | Merge coplanar unit faces with identical material and lighting; useful for large flat or repetitive surfaces, disabled by default |
| `--force-limits` | Raises input, scene, mesh, and pixel caps; can exhaust memory |
| `--ignore-block-entity-nbt` | Skip container NBT; always on for `mesh`/`render`/`bench` |
| `-t, --terminal` | Render and display the image directly in the terminal |
| `--terminal-protocol <auto\|kitty\|iterm2\|sixel\|halfblocks>` | Specify terminal graphics protocol; defaults to auto-detection |
| `--terminal-width <COLS>` / `--terminal-height <ROWS>` | Target display width in columns and height in rows |
| `--no-file` | Render only to the terminal without writing an output PNG file |
| `--color auto\|always\|never` | Color policy for help, errors, and human output; default `auto` |

`inspect`, `validate`, and `stats` keep `TileEntities` by default. Geometry
commands skip them by default to reduce parse time and memory for large
containers.

## Terminal Style and Progress

Running `blockscope` without arguments prints the static welcome screen and
exits immediately. The wordmark chooses its layout from the terminal width and
height at print time; redirected output uses the compact mark. After the
process exits, the terminal owns those characters and may reflow them on
resize; the program can no longer control them.

The wordmark steps down with the terminal width and height: an 82-column
banner, a 59-column compact mark, a 47-column small mark, and finally a plain
text title with only the most useful commands. Redirected output uses the
compact mark.

`--color` is accepted before or after the subcommand and controls help, errors,
and human-readable results:

| Value | Behavior |
|---|---|
| `auto` (default) | Color only on a terminal; `NO_COLOR` disables it |
| `always` | Emit color even through a pipe |
| `never` | Never emit color |

`--json` output is always plain JSON and is never affected by `--color`.

`render`, `bench`, `mesh`, and `materials` show progress on stderr when stderr
is a terminal: parsing, materials, mesh page counts, and tile percentages for
single-view rasterization. Multi-view renders show a view counter; streaming
compositing and `--lod auto` use indeterminate stages. Progress turns off when
stderr is not a terminal, with `--json`, when `CI` is set, with `NO_COLOR`, or
with `--color never`. It never writes to stdout or changes piped output. Bars
shrink or fall back to a bare counter on narrow terminals and reflow after a
window resize.

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
