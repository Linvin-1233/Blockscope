---
title: "用户文档"
---

欢迎阅读 Blockscope 用户指南。Blockscope 是一个用 Rust 编写的高性能离线 Minecraft `.litematic` 投影文件解析、网格生成与 CPU 光栅化渲染引擎。

本指南提供在桌面和服务器环境中安装、配置、运行及排查 Blockscope 所需的全部说明。

## 指南总览

- **入门与概览**：了解引擎能力与边界 [功能与限制](./features.md)，按照 [安装与快速开始](./getting-started.md) 安装预编译程序，查阅 [CLI 命令](./cli.md) 了解所有子命令与参数。
- **材质与大结构**：在 [材质与资源包](./materials.md) 中配置客户端 jar 与材质包，在 [LOD 与大结构](./lod.md) 中了解大体量结构的分级渲染与降级策略。
- **服务与容器部署**：通过 [服务模式](./service.md) 使用 stdio JSONL 协议启动长驻 worker 与 supervisor，使用 [Docker](./docker.md) 进行轻量容器化部署。
- **参考与排查**：在 [限制与安全](./limits.md) 中了解各项安全上限与 `--force-limits`，遇到异常时查阅 [故障排查](./troubleshooting.md)。

## 项目链接

- [贡献指南](https://github.com/Linvin-1233/Blockscope/blob/main/CONTRIBUTING.md)
- [行为准则](https://github.com/Linvin-1233/Blockscope/blob/main/CODE_OF_CONDUCT.md)
- [变更日志](https://github.com/Linvin-1233/Blockscope/blob/main/CHANGELOG.md)
