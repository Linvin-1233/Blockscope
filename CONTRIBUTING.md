# 贡献指南

[English](CONTRIBUTING.en.md)

感谢你愿意改进 Blockscope。本文说明开发环境、代码规范、测试方式、
资产政策和提交 PR 的流程。

开始之前请先阅读：

- [README.md](README.md)
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- [CHANGELOG.md](CHANGELOG.md)

## 可以贡献什么

- 修复解析、渲染、材质、CLI、协议或文档问题
- 增加测试、基准和可复现的最小 fixture
- 改进错误信息、诊断和日志
- 优化大结构、LOD、内存和渲染性能
- 补充格式、协议和部署文档

如果你准备做较大的功能或接口变更，请先开 Issue 说明目标和范围，避免在未对齐
设计的情况下写大量代码。

## 开发环境

需要 Rust stable 工具链。项目使用 edition 2024，建议 Rust 1.85 或更新。

```bash
git clone https://github.com/Linvin-1233/Blockscope.git
cd Blockscope
cargo build --release -p blockscope-cli
```

Windows、Linux 和 macOS 都是受支持目标。真实资源包、客户端 jar 和 `.litematic`
样本由开发者本地提供，不进入版本库。

## 质量门禁

提交前至少运行：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

或直接运行仓库脚本或 `just`：

```bash
# 使用 just
just check

# Linux
bash ./scripts/check.sh

# Windows PowerShell 7+
pwsh ./scripts/check.ps1

# Windows PowerShell 5.1
powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/check.ps1
```
`check.ps1` 在 Windows 上还会用 `x86_64-unknown-linux-gnu` 目标再跑一遍 clippy，
用来捕获只在 Linux 编译的代码；首次运行会自动安装该 target，需要联网。

CI 的 test matrix 会在 Ubuntu、Windows 和 macOS 上运行同样的检查；container
和 MSI 任务分别在 Ubuntu 和 Windows 上运行。不要通过跳过测试、删除断言、
添加 `#[ignore]` 或放宽 lint 来让检查通过；如果确实需要调整规则，请在 PR
中说明原因。

## 仓库结构

| 目录 | 职责 |
|---|---|
| `crates/nbt` | NBT 解码、压缩检测、解码限制和错误码 |
| `crates/litematic` | `.litematic` 容器、region、palette、block entity 解析 |
| `crates/scene` | Scene IR、统计、诊断、manifest、hash、LOD |
| `crates/material` | blockstate/model、资源包、纹理、tint、chest |
| `crates/mesh` | 色块/资源包网格、面剔除、AO、OBJ/PLY 导出 |
| `crates/render` | CPU 光栅化、tile、超采样、PNG |
| `crates/cli` | `blockscope` 命令、serve、supervise、容量检测 |
| `crates/*/tests` | 集成测试、黄金测试和真实文件测试 |

依赖方向应保持单向：底层数据 crate 不依赖 CLI，渲染 crate 不依赖 CLI。
公共协议和错误码要版本化；不要把内部缓冲、临时文件或解析器内部结构直接塞进
协议 DTO。

## 代码规范

- Rust 使用 `rustfmt`，`max_width = 100`，4 空格缩进
- 函数和变量使用 `snake_case`，类型和 trait 使用 `PascalCase`
- 注释使用 `//`，解释“为什么”而不是复述代码；领域说明可以使用中文
- 单个 Rust 源文件保持在 800 行以内，超过时按模块拆分
- 优先复用现有 helper 和 crate，新增依赖需要说明必要性
- 错误使用稳定的错误码；不要只返回字符串
- 公共协议变更要同步更新 `docs/engine-protocol.md` 和 `MANIFEST_SCHEMA_VERSION`
  或 `PROTOCOL_VERSION`
- CLI 行为变更要同步更新 `docs/user`、`CHANGELOG.md` 和相关帮助文本
- 不要提交 `target/`、`renders/`、`cache/`、日志或 IDE 本地配置

项目内还有 `AGENTS.md`，其中包含针对编码代理的额外约定。使用 AI 工具时请一并
遵守。

## 测试

### 常规测试

```bash
cargo test --workspace
```

只跑某个 crate：

```bash
cargo test -p litematic-nbt
cargo test -p litematic-scene
cargo test -p litematic-core
cargo test -p litematic-material
cargo test -p litematic-mesh
cargo test -p litematic-render
cargo test -p blockscope-cli
```

新增行为应有对应测试；修复 bug 应先写能复现问题的回归测试。测试应验证行为，
不要只断言内部实现细节。

### 真实文件测试

真实样本测试默认带 `#[ignore]`，需要本地环境变量：

```bash
# 解析真实 .litematic
LITEMATIC_FIXTURES_DIR=/path/to/fixtures \
  cargo test -p litematic-core --test real_files -- --ignored

# 真实 Minecraft 客户端 jar
MINECRAFT_CLIENT_JAR=/path/to/client.jar \
  cargo test -p litematic-material --test real_jar -- --ignored

# 真实材质渲染
MINECRAFT_CLIENT_JAR=/path/to/client.jar \
  cargo test -p litematic-render --test real_materials -- --ignored
```

Windows PowerShell：

```powershell
$env:LITEMATIC_FIXTURES_DIR = "D:\path\to\fixtures"
cargo test -p litematic-core --test real_files -- --ignored

$env:MINECRAFT_CLIENT_JAR = "D:\path\to\client.jar"
cargo test -p litematic-material --test real_jar -- --ignored
```

真实样本、客户端 jar 和渲染产物不要提交。

### 黄金文件

黄金 manifest 和 PNG 用于捕获确定性回归。只有在确认渲染/格式变更是有意的之后
才更新：

```bash
# Linux/macOS
UPDATE_GOLDEN=1 cargo test -p blockscope-cli --test golden
UPDATE_GOLDEN=1 cargo test -p litematic-render --test golden_png
```

```powershell
# Windows PowerShell
$env:UPDATE_GOLDEN = "1"
cargo test -p blockscope-cli --test golden
cargo test -p litematic-render --test golden_png
Remove-Item Env:UPDATE_GOLDEN
```

更新后请检查 `fixtures/golden/` 的 diff，确认变化符合预期；不要在没有说明的
情况下重建黄金文件。

## 提交规范

- 一个提交只做一个逻辑变更；重构、功能和修复分开提交
- 使用仓库现有的提交前缀：`feat:`、`fix:`、`perf:`、`refactor:`、`test:`、
  `docs:`、`chore:`
- 提交信息说明“改了什么”和“为什么”，不要只写 `fix bug`
- 新功能或行为变更同步更新 `docs/user`、`CHANGELOG.md`、协议文档和测试
- PR 描述包含：背景、改动、验证命令、已知限制
- 用户可见行为变更同步更新 `docs/user` 与根 README 的导航
- 如果改动影响输出，附上前后对比或黄金文件差异说明
- 保持 PR 可审查；过大改动请拆分为多个小 PR

提交前检查：

```bash
git status --short
git diff --check
```

## 资产与隐私

不要提交：

- Minecraft 客户端 jar、资源包、贴图或任何 Minecraft 资产
- 私有 `.litematic`、服务器存档或包含个人信息的世界数据
- 真实用户上传的文件、日志中的绝对路径或身份信息
- 从参考实现复制来的代码或资源

测试请使用 `tests/support/` 中的合成 fixture，或只在本机通过环境变量加载真实
文件。提交 issue 时请先移除文件名、服务器地址和其他敏感信息。

## 报告问题

提交 bug 时请包含：

- 操作系统、Rust 版本和 `blockscope --version`
- 完整命令或协议请求（删除敏感路径）
- 实际结果和预期结果
- 错误码和关键 stderr
- 如果可能，提供最小化、可公开的合成 fixture

如果问题涉及安全漏洞、路径穿越、拒绝服务或数据泄露，请不要先开公开 Issue，
通过维护者的 GitHub 资料页联系维护者，并提供影响范围、复现步骤和建议修复。

## 协议与兼容性

- `docs/engine-protocol.md` 是 serve/supervise 的协议契约
- manifest 变更需要评估 `MANIFEST_SCHEMA_VERSION`
- 错误码是稳定 API，不要随意重命名
- 新增协议字段优先保持向后兼容；删除或改变语义需要提升 `PROTOCOL_VERSION`
- 修改 CLI 默认值或限制时，同步更新 `docs/user`、帮助文本和测试

## 行为准则

参与本项目即表示你同意遵守 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。违反行为
准则的内容可能被删除，参与者可能被临时或永久移出项目空间。
