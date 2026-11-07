---
title: "Features and Limitations"
---

Blockscope is a headless Rust image-rendering engine that turns Minecraft `.litematic` projections into PNG images, MP4 clips, and GIF animations. It is designed for general backend services, batch tools, and automation pipelines.

The current workspace version is `0.5.0` and supports Windows, Linux, and macOS. The repository contains parsing, materials, meshing, CPU rasterization (plus an optional GPU backend), external-ffmpeg video encoding, a CLI, and a stdio service. It does not contain a chat-platform or bot adapter.

## Features

- Parse `.litematic` v1-v7: metadata, regions, palettes, packed block states, block entities, entities, and pending ticks
- Build a canonical Scene IR, material statistics, diagnostics, deterministic manifests, and SHA-256 hashes
- Built-in color catalog for common blocks, partial shapes, and fluids without a resource pack
- Resource packs: blockstates, models, parent chains, multipart, UVs, rotation, tint, cutout/translucent layers, PNG textures, and overlays
- Built-in chest body/lid/lock model and entity textures
- Mesh export: OBJ and colored PLY
- Optional `--greedy-mesh` coplanar merging for large flat or repetitive surfaces
- CPU orthographic camera, tiled rasterization, `flat-v1` / `simple-ao-v1` lighting, and PNG output
- Optional offscreen `wgpu` GPU rasterization (`--features gpu`, `--backend gpu|auto`); Windows, macOS, and Linux GNU release archives enable it, while Linux musl and wasm builds stay CPU-only
- Keyframed camera animation to MP4 or GIF through an external ffmpeg 4.4+ executable, with optional hardware encoding
- Supersampling 1-4, multi-view output, parallel tiles, and paged streaming
- `serve` worker and `supervise` process pool over newline-delimited JSON, including video jobs in protocol v4
- Stateless [Cloudflare Workers](./cloudflare-workers.md) HTTP adapter for PNG rendering (CPU-only)
- Capacity detection, memory budgets, mesh/render limits, and stable error codes
- Golden manifest/PNG, concurrency, and protocol tests

## Current Limitations

- No chat-platform or bot adapter; this repository provides the renderer, stdio protocol, and a Cloudflare Workers adapter
- No general entity models, entity animations, block-entity text/icons, banner patterns, or skull skins
- Tint is a fixed plains color; biome colormaps are not read
- No shadows, full light propagation, emissive bloom, or mipmaps
- GPU output varies by vendor and driver and is not byte-exact; Windows, macOS, and Linux GNU release archives include it, while Linux musl and wasm builds are CPU-only
- Resource packs, client jars, and private `.litematic` files are not committed
- No network listener in `serve`/`supervise`; they communicate over stdio only. The Cloudflare Workers adapter is a separate HTTP endpoint

## Next

- [Installation and Quick Start](./getting-started.md)
- [CLI Reference](./cli.md)
- [Limits and Security](./limits.md)
- [中文](/zh/user/features)
