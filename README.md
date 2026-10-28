# Blockscope

[![CI](https://github.com/Linvin-1233/Blockscope/actions/workflows/ci.yml/badge.svg)](https://github.com/Linvin-1233/Blockscope/actions/workflows/ci.yml)

Blockscope 是一个把 Minecraft `.litematic` 投影文件渲染成 PNG 的
无头 Rust 图像渲染引擎，面向通用后端服务、批处理工具和自动化管线。仓库包含
解析、材质、网格、CPU 渲染、CLI 和 stdio 服务。

它不是 Minecraft 客户端截图工具：引擎直接解析 `.litematic`，在 CPU 上生成
确定性 PNG，并可在服务端无图形环境中运行。

- 当前版本：`0.4.0`
- 支持平台：Windows x86_64、Linux x86_64 / aarch64、macOS aarch64 / x86_64
- 下载：预编译二进制见 [GitHub Releases](https://github.com/Linvin-1233/Blockscope/releases)
- English: [README.en.md](README.en.md)

## 文档导航

文档位于 `docs/` 并配置为 [Mintlify](https://mintlify.com) 站点，本地可通过 `cd docs && mint dev` 启动实时预览。

### 用户文档

- [用户文档总览](docs/zh/user/index.md)
- [功能与限制](docs/zh/user/features.md)
- [安装与快速开始](docs/zh/user/getting-started.md)
- [CLI 命令](docs/zh/user/cli.md)
- [材质与资源包](docs/zh/user/materials.md)
- [LOD 与大结构](docs/zh/user/lod.md)
- [服务模式](docs/zh/user/service.md)
- [限制与安全](docs/zh/user/limits.md)
- [Docker](docs/zh/user/docker.md)
- [故障排查](docs/zh/user/troubleshooting.md)

### 技术文档

- [Litematic 格式说明](docs/litematic-format.md)
- [Color-block 网格说明](docs/mesh-geometry.md)
- [CPU 渲染说明](docs/rendering.md)
- [资源包材质说明](docs/textures.md)
- [引擎协议 v1](docs/engine-protocol.md)
- [部署说明](docs/deployment.md)

### 项目文档

- [贡献指南](CONTRIBUTING.md) ([English](CONTRIBUTING.en.md))
- [行为准则](CODE_OF_CONDUCT.md) ([English](CODE_OF_CONDUCT.en.md))
- [变更日志](CHANGELOG.md) ([English](CHANGELOG.en.md))

## 贡献

欢迎提交 bug、文档改进和代码贡献。开始前请阅读
[CONTRIBUTING.md](CONTRIBUTING.md) 和 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。

提交前请运行：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

或运行 `scripts/check.ps1` / `scripts/check.sh`。请勿提交 Minecraft 资产、
私有 `.litematic` 或资源包。

## 许可证

MIT
