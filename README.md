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

在线文档见 [blockscope.linvin.net](https://blockscope.linvin.net)，源码位于 `docs/`，配置为 [Mintlify](https://mintlify.com) 站点；本地可通过 `cd docs && mint dev` 启动实时预览。

### 用户文档

- [用户文档总览](https://blockscope.linvin.net/zh/user)
- [功能与限制](https://blockscope.linvin.net/zh/user/features)
- [安装与快速开始](https://blockscope.linvin.net/zh/user/getting-started)
- [CLI 命令](https://blockscope.linvin.net/zh/user/cli)
- [材质与资源包](https://blockscope.linvin.net/zh/user/materials)
- [服务模式](https://blockscope.linvin.net/zh/user/service)
- [限制与安全](https://blockscope.linvin.net/zh/user/limits)
- [Docker](https://blockscope.linvin.net/zh/user/docker)
- [故障排查](https://blockscope.linvin.net/zh/user/troubleshooting)

### 技术文档

- [Litematic 格式说明](https://blockscope.linvin.net/litematic-format)
- [Color-block 网格说明](https://blockscope.linvin.net/mesh-geometry)
- [CPU 渲染说明](https://blockscope.linvin.net/rendering)
- [资源包材质说明](https://blockscope.linvin.net/textures)
- [引擎协议 v3](https://blockscope.linvin.net/engine-protocol)
- [部署说明](https://blockscope.linvin.net/deployment)

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
