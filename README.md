# Litematic-render-QQBot

Minecraft Litematic 无头渲染服务。当前处于 Phase 1：Rust parser + CLI。

## 文档

- [新版设计](.docs/qqbot-litematic-rust-renderer-engineering-design.md)
- [旧版设计（历史方案）](.docs/minecraft-render-service-engineering-design.md)
- [Phase 1 计划](tasks/plan.md)
- [Phase 1 任务清单](tasks/todo.md)
- [Phase 2 计划](tasks/phase-2-plan.md)
- [Phase 2 任务清单](tasks/phase-2-todo.md)
- [Litematic 格式笔记](docs/litematic-format.md)
- [Color-block 网格说明](docs/mesh-geometry.md)

## 当前范围

- 解析 `.litematic` v1-v7
- 规范 Scene IR、材料统计、诊断和哈希
- 默认色块 catalog、全方块 mesh、OBJ/PLY 导出
- `renderd inspect` / `validate` / `stats` / `mesh` CLI
- 资源包由部署者自行提供
- 暂不包含 Go QQBot 服务、CPU 光栅化和资源包加载

## CLI

```powershell
cargo run -p renderd-cli -- inspect path\to\model.litematic --json
cargo run -p renderd-cli -- validate path\to\model.litematic
cargo run -p renderd-cli -- stats path\to\model.litematic --json
cargo run -p renderd-cli -- mesh path\to\model.litematic --obj out.obj
cargo run -p renderd-cli -- mesh path\to\model.litematic --ply out.ply
```

`inspect --json` 输出稳定 manifest，包含 bounds、region、材料统计、诊断、
`inputHash` 和 `sceneHash`。

## 开发

需要 Rust stable 工具链：

```powershell
rustup default stable
cargo test --workspace
pwsh ./scripts/check.ps1
```

没有 PowerShell 7 时使用：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/check.ps1
```

Linux 环境使用 `bash ./scripts/check.sh`。真实 `.litematic` 样本放入
`fixtures/litematic/`，该目录默认不提交文件。

## IDE 提示

如果 RustRover 在测试文件里显示引用错误，但 `cargo test --workspace` 能通过，
通常是 IDE 的 `cargo check` 缓存滞后。依次尝试：

1. Cargo 工具窗口 → Reload Cargo Project。
2. File → Invalidate Caches / Restart。
3. 仍有问题时清理对应包的缓存：

```powershell
cargo clean -p litematic-nbt -p litematic-scene -p litematic-core -p renderd-cli
```

所有测试目标可以单独检查：

```powershell
cargo check -p litematic-nbt --tests
cargo check -p litematic-scene --tests
cargo check -p litematic-core --tests
cargo check -p renderd-cli --tests
```
