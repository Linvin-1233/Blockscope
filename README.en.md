# Blockscope

[![CI](https://github.com/Linvin-1233/Blockscope/actions/workflows/ci.yml/badge.svg)](https://github.com/Linvin-1233/Blockscope/actions/workflows/ci.yml)

Blockscope is a headless Rust image-rendering engine that turns
Minecraft `.litematic` projections into PNG images. It is designed for general
backend services, batch tools, and automation pipelines. This repository
contains parsing, materials, meshing, CPU rendering, a CLI, and a stdio service.

It is not a Minecraft client screenshot tool: the engine parses `.litematic`
directly and renders deterministic PNGs on the CPU without a graphical
environment.

- Version: `0.1.2`
- Platforms: Windows x86_64, Linux x86_64 / aarch64, macOS aarch64 / x86_64
- Download: prebuilt binaries on [GitHub Releases](https://github.com/Linvin-1233/Blockscope/releases)
- 中文: [README.md](README.md)

## Documentation

### User Guide

- [User documentation index](docs/user/en/README.md)
- [Features and limitations](docs/user/en/features.md)
- [Installation and quick start](docs/user/en/getting-started.md)
- [CLI reference](docs/user/en/cli.md)
- [Materials and resource packs](docs/user/en/materials.md)
- [LOD and large structures](docs/user/en/lod.md)
- [Service mode](docs/user/en/service.md)
- [Limits and security](docs/user/en/limits.md)
- [Docker](docs/user/en/docker.md)
- [Troubleshooting](docs/user/en/troubleshooting.md)

### Technical Documentation

- [Litematic format](docs/litematic-format.md)
- [Color-block mesh](docs/mesh-geometry.md)
- [CPU rendering](docs/rendering.md)
- [Resource-pack materials](docs/textures.md)
- [Engine protocol v1](docs/engine-protocol.md)
- [Deployment](docs/deployment.md)

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

This repository does not currently contain a `LICENSE` file. Until the
maintainer adds one, the code is all rights reserved; contact the maintainer
before reusing it in another project.
