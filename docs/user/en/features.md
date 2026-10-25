# Features and Limitations

Blockscope is a headless Rust image-rendering engine that turns
Minecraft `.litematic` projections into PNG images. It is designed for general
backend services, batch tools, and automation pipelines.

The current workspace version is `0.2.0` and supports Windows, Linux, and macOS. The
repository contains parsing, materials, meshing, CPU rasterization, a CLI, and a
stdio service. It does not contain a chat-platform or bot adapter.

## Features

- Parse `.litematic` v1-v7: metadata, regions, palettes, packed block states,
  block entities, entities, and pending ticks
- Build a canonical Scene IR, material statistics, diagnostics, deterministic
  manifests, and SHA-256 hashes
- Built-in color catalog for common blocks, partial shapes, and fluids without
  a resource pack
- Resource packs: blockstates, models, parent chains, multipart, UVs, rotation,
  tint, cutout/translucent layers, PNG textures, and overlays
- Built-in chest body/lid/lock model and entity textures
- Mesh export: OBJ and colored PLY
- CPU orthographic camera, tiled rasterization, `flat-v1` / `simple-ao-v1`
  lighting, and PNG output
- Supersampling 1-4, multi-view output, parallel tiles, and paged streaming
- LOD with a fixed `2..=16` factor or `auto`
- `serve` worker and `supervise` process pool over newline-delimited JSON
- Capacity detection, memory budgets, mesh/render limits, and stable error codes
- Golden manifest/PNG, concurrency, and protocol tests

## Current Limitations

- No chat-platform or bot adapter; this repository provides only the renderer
  and stdio protocol
- No general entity models, entity animations, block-entity text/icons, banner
  patterns, or skull skins
- Tint is a fixed plains color; biome colormaps are not read
- No shadows, full light propagation, emissive bloom, or mipmaps
- LOD drops block entities, entities, and pending ticks
- Resource packs, client jars, and private `.litematic` files are not committed
- No network listener; `serve`/`supervise` communicate over stdio only

## Next

- [Installation and Quick Start](./getting-started.md)
- [CLI Reference](./cli.md)
- [Limits and Security](./limits.md)
- [中文](../features.md)
