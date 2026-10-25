# 变更日志

[English](CHANGELOG.en.md)

本文件记录面向用户的变更。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [0.1.1] - 2026-10-25

### Added

- Windows 提供按用户安装的 MSI：安装到 `%LOCALAPPDATA%\Programs\Blockscope`，
  自动把目录加入用户 `PATH`，卸载时移除该条目；支持
  `msiexec /i <msi> /qn` 静默安装。
- CI 每次推送都会构建 MSI 作为校验产物；release 工作流为 `v*` 标签上传
  `blockscope-v<version>-x86_64-pc-windows-msvc.msi` 及 `.sha256`。

### Changed

- Windows 安装文档改为优先使用 MSI，便携 zip 保留为备选。

## [0.1.0] - 2026-10-25

当前 workspace 版本的首个公开快照。

### Added

- `.litematic` v1-v7 解析：metadata、regions、palette、packed block states、
  block entities、entities 和 pending ticks。
- 规范 Scene IR、材料统计、诊断、稳定 manifest、`inputHash` 和 `sceneHash`。
- 内置色块 catalog，支持常见方块族、partial shapes、流体和静态方块实体。
- 资源包材质：blockstate/model、parent 链、multipart、UV、旋转、tint、
  cutout/translucent、PNG 贴图和 overlay。
- 箱子 body/lid/lock 模型和 entity 贴图。
- 分页网格、面剔除、环境光遮蔽、OBJ/PLY 导出。
- CPU 正交相机、tile 光栅化、`flat-v1` / `simple-ao-v1` 光照、超采样 1-4、
  多视角和 PNG 输出。
- LOD 固定因子 `2..=16` 和 `--lod auto` 内存预算降级。
- `inspect`、`validate`、`stats`、`mesh`、`render`、`bench`、`materials`、
  `capacity`、`serve` 和 `supervise` 命令。
- stdio newline-delimited JSON 协议 v1，以及 supervisor 的队列、超时、内存
  上限和 worker 池。
- 容量检测、mesh/render 上限、稳定错误码和 Docker 镜像。
- 黄金 manifest/PNG 回归测试、并发测试、协议测试和真实文件可选测试。
- 面向用户的 README、贡献指南、行为准则和本变更日志。
- `docs/user` 中文用户文档和 `docs/user` 英文用户文档。
- 受限文件读取：输入必须是普通文件，并在读取前检查大小上限；目录、FIFO、
  设备和超限文件会直接返回错误。
- 线程上限回归测试、非 ASCII 背景回归测试和协议错误可重试性测试。
- `crates/cli/src/commands/` 命令模块，拆分 `main.rs`。
- `crates/render/src/rasterizer/stream.rs` 和 `downsample.rs`，拆分
  `rasterizer.rs`。
- `.github/workflows/release.yml`：推送 `v*` 标签时校验标签与 workspace
  版本一致，按 `CHANGELOG.md` 创建 GitHub Release，并上传 Windows x86_64、
  Linux x86_64/aarch64 预编译二进制及 `.sha256` 校验文件。

### Changed

- 项目更名为 `Blockscope`（此前为 `Litematic-image-render`），定位为通用
  Litematic 图像渲染引擎；用户文档不再以 QQBot 为产品前提。
- CLI 包更名为 `blockscope-cli`，二进制由 `renderd` 更名为 `blockscope`。
- 设计文档更名为 `blockscope-engineering-design.md`，并标注为历史稿。
- mesh、LOD、`serve` 和 `supervise` 的 worker/rasterizer 线程数统一限制为最多
  256；超限请求返回 `INVALID_RENDER_OPTIONS`。
- `supervise` 对 `INVALID_REQUEST` 返回 `retryable: false`，与协议文档一致。
- `supervise` 的输入哈希计算从 stdin 读取线程移到主调度循环，避免阻塞协议读取。
- 清理未使用的 `thiserror` 依赖和重复的 `flate2` dev-dependency。
- 对齐渲染并行度、同状态面剔除、纹理限制和协议错误码文档。
- `docs/engine-protocol.md` 标记协议 v1 已实现。
- 用户文档的安装章节优先提供预编译二进制下载，源码构建标记为开发用途。
- README 增加 GitHub Releases 下载入口，并写明支持的平台架构。
- 根 README 精简为文档导航、贡献入口和许可证说明。

### Fixed

- 修复 `Rgba::from_hex` 在收到多字节非 ASCII 颜色时按字节切片导致的 panic。
- 修复超大 `threads` 请求可能创建海量 scoped 线程并导致 worker 崩溃的问题。
- 修复 `serve`、CLI 和 `supervise` 在解析器限制之前读取整个文件的问题。
- 修复 `supervise` 在 FIFO、设备文件或超大文件上阻塞 stdin 读取线程的问题。
- 修复 `INVALID_REQUEST` 被 supervisor 错误标记为可重试的问题。

### Security

- 默认限制输入大小、解压大小、区域数量、体积、网格、像素和线程数。
- 资源包和 Minecraft 客户端资产不随仓库分发。
