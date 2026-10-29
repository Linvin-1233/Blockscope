# Blockscope

[![CI](https://github.com/Linvin-1233/Blockscope/actions/workflows/ci.yml/badge.svg)](https://github.com/Linvin-1233/Blockscope/actions/workflows/ci.yml)

Blockscope is a headless Rust image-rendering engine that turns
Minecraft `.litematic` projections into PNG images. It is designed for general
backend services, batch tools, and automation pipelines. This repository
contains parsing, materials, meshing, CPU rendering (plus an optional offscreen
GPU backend), a CLI, and a stdio service.

It is not a Minecraft client screenshot tool: the engine parses `.litematic`
directly and renders deterministic PNGs on the CPU without a graphical
environment. Build with `--features gpu` to add the `wgpu` offscreen backend for
hosts with a weak CPU and a hardware adapter; the CPU remains the default and
the byte-exact golden reference.

- Version: `0.4.1`
- Platforms: Windows x86_64, Linux x86_64 / aarch64, macOS aarch64 / x86_64
- Download: prebuilt binaries on [GitHub Releases](https://github.com/Linvin-1233/Blockscope/releases)
- 中文: [README.md](README.md)

## Documentation

Read the documentation at [blockscope.linvin.net](https://blockscope.linvin.net). The source lives under `docs/` as a [Mintlify](https://mintlify.com) site; preview locally with `cd docs && mint dev`.

### User Guide

- [User documentation index](https://blockscope.linvin.net/user)
- [Features and limitations](https://blockscope.linvin.net/user/features)
- [Installation and quick start](https://blockscope.linvin.net/user/getting-started)
- [CLI reference](https://blockscope.linvin.net/user/cli)
- [Materials and resource packs](https://blockscope.linvin.net/user/materials)
- [LOD and large structures](https://blockscope.linvin.net/user/lod)
- [Service mode](https://blockscope.linvin.net/user/service)
- [Limits and security](https://blockscope.linvin.net/user/limits)
- [Docker](https://blockscope.linvin.net/user/docker)
- [Troubleshooting](https://blockscope.linvin.net/user/troubleshooting)

### Technical Documentation

- [Litematic format](https://blockscope.linvin.net/litematic-format)
- [Color-block mesh](https://blockscope.linvin.net/mesh-geometry)
- [Rendering and PNG output](https://blockscope.linvin.net/rendering)
- [Resource-pack materials](https://blockscope.linvin.net/textures)
- [Engine protocol v2](https://blockscope.linvin.net/engine-protocol)
- [Deployment](https://blockscope.linvin.net/deployment)

### Project

- [Contributing](CONTRIBUTING.en.md) ([中文](CONTRIBUTING.md))
- [Code of Conduct](CODE_OF_CONDUCT.en.md) ([中文](CODE_OF_CONDUCT.md))
- [Changelog](CHANGELOG.en.md) ([中文](CHANGELOG.md))

## Contributing

Bug reports, documentation improvements, and code contributions are welcome.
Read [CONTRIBUTING.en.md](CONTRIBUTING.en.md) and
[CODE_OF_CONDUCT.en.md](CODE_OF_CONDUCT.en.md) first.

Before submitting:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Or run `scripts/check.ps1` / `scripts/check.sh`. Do not commit Minecraft assets,
private `.litematic` files, or resource packs.

## License

MIT
