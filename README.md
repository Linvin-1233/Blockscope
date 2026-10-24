# Litematic-render-QQBot

Minecraft Litematic 无头渲染服务。Phase 3C 已完成：Rust 解析、色块/资源包
材质、CPU PNG 渲染和基准测试；下一步是 Go QQBot 服务。

## 文档

- [新版设计](.docs/qqbot-litematic-rust-renderer-engineering-design.md)
- [旧版设计（历史方案）](.docs/minecraft-render-service-engineering-design.md)
- [Phase 1 计划](tasks/plan.md)
- [Phase 1 任务清单](tasks/todo.md)
- [Phase 2 计划](tasks/phase-2-plan.md)
- [Phase 2 任务清单](tasks/phase-2-todo.md)
- [Phase 3 计划](tasks/phase-3-plan.md)
- [Phase 3 任务清单](tasks/phase-3-todo.md)
- [Litematic 格式笔记](docs/litematic-format.md)
- [Color-block 网格说明](docs/mesh-geometry.md)
- [CPU 渲染说明](docs/rendering.md)
- [资源包材质说明](docs/textures.md)
- [部署说明](docs/deployment.md)
- [引擎协议](docs/engine-protocol.md)
- [Phase 3C 材质 spec](tasks/phase-3c-textures-spec.md)
- [Phase 3C 材质计划](tasks/phase-3c-textures-plan.md)
- [Phase 3C 材质任务](tasks/phase-3c-textures-todo.md)

## 当前范围

- 解析 `.litematic` v1-v7
- 规范 Scene IR、材料统计、诊断和哈希
- 默认色块 catalog、常见 partial shape、流体高度和静态方块实体 box mesh
- 资源包 blockstate/model 解析、PNG 贴图、tint、cutout/translucent
- 箱子内置 body/lid/lock 模型和 entity 贴图
- OBJ/PLY 导出
- CPU 正交相机、tile 光栅化、`flat-v1` / `simple-ao-v1` 和 PNG 输出
- 超采样 1-4、多视角分别输出、并行 tile 和三角形分桶
- 黄金 PNG 回归测试、`renderd bench` 分阶段计时
- `renderd inspect` / `validate` / `stats` / `mesh` / `render` / `bench` / `materials` CLI
- 资源包由部署者自行提供，不提交任何 Minecraft 资产
- 暂不包含 Go QQBot 服务、通用实体模型和动画播放

## CLI

```powershell
cargo run -p renderd-cli -- inspect path\to\model.litematic --json
cargo run -p renderd-cli -- validate path\to\model.litematic
cargo run -p renderd-cli -- stats path\to\model.litematic --json
cargo run -p renderd-cli -- mesh path\to\model.litematic --obj out.obj
cargo run -p renderd-cli -- mesh path\to\model.litematic --ply out.ply
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --width 2048 --height 2048
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --views 45,225
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png --supersample 4 --threads 4
cargo run -p renderd-cli -- render path\to\model.litematic --output out.png `
  --client-jar "D:\hmcl\.minecraft\versions\26.2-Fabric\26.2-Fabric.jar"
cargo run -p renderd-cli --release -- materials path\to\model.litematic `
  --client-jar "D:\hmcl\.minecraft\versions\26.2-Fabric\26.2-Fabric.jar" --json
cargo run -p renderd-cli --release -- bench path\to\model.litematic --iterations 5
cargo run -p renderd-cli -- mesh path\to\huge.litematic --force-limits --json
```

`--force-limits` disables the parser, mesh, and render caps for one invocation
and prints a warning to stderr. It does not allocate anything up front, but a
large model can exhaust memory.

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
