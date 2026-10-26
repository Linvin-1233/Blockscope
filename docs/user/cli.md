# CLI 命令

`blockscope` 的构建和路径说明见 [安装与快速开始](./getting-started.md)。

## 命令总览

| 命令 | 用途 |
|---|---|
| `inspect` | 输出稳定 manifest，包含 bounds、regions、统计、诊断、`inputHash` 和 `sceneHash` |
| `validate` | 快速验证文件并输出摘要 |
| `stats` | 输出方块/材料统计 |
| `mesh` | 构建网格，可导出 OBJ 或 PLY |
| `render` | 渲染 PNG 或直接在终端中显示图像 |
| `bench` | 测量 parse、LOD、mesh、render、PNG 各阶段耗时 |
| `materials` | 检查资源包对当前模型的覆盖率 |
| `capacity` | 输出 CPU/内存容量和建议的 worker/thread 配置 |
| `serve` | 长驻 worker，使用 stdio JSON 协议 |
| `supervise` | 启动并调度多个 worker 进程 |

## 常用示例

```bash
# manifest
blockscope inspect model.litematic --json
blockscope inspect model.litematic --json --compact

# 验证和统计
blockscope validate model.litematic
blockscope stats model.litematic --json

# 网格导出
blockscope mesh model.litematic --obj out.obj
blockscope mesh model.litematic --ply out.ply

# 高分辨率、抗锯齿、多视角
blockscope render model.litematic --output out.png \
  --width 2048 --height 2048 --supersample 2 --threads 4
blockscope render model.litematic --output out.png --views 45,225

# 超大结构
blockscope render huge.litematic --output huge.png --lod auto
blockscope render huge.litematic --output huge.png --lod 4
blockscope render huge.litematic --output huge.png --lod 1 --stream-mesh

# 材质覆盖率
blockscope materials model.litematic --client-jar client.jar --json

# 容器容量
blockscope capacity --json
# 终端内直接预览图像（Kitty、iTerm2、Sixel、Halfblocks/Truecolor）
blockscope render model.litematic --terminal
blockscope render model.litematic -t --terminal-protocol kitty
blockscope render model.litematic -t --no-file
blockscope preview model.litematic -t
```

多视角渲染时，单视角输出 `out.png`，多视角输出 `out-45.png`、`out-225.png`
等。`serve` 模式的输出命名为 `{id}.png`（单视角）或 `{id}-{yaw}.png`（多视角）。
有完整 mesh 时，多视角会按 `--threads` 预算并行渲染；输出顺序不变，但每个
并发视角各自持有 tile buffer，内存会相应增加。

## 常用选项

| 选项 | 说明 |
|---|---|
| `--client-jar <PATH>` | 直接读取 Minecraft 客户端 jar |
| `--assets <DIR>` | 读取已解压的 `assets/` 目录 |
| `--resource-pack <PATH>` | 叠加资源包目录、zip 或 jar；优先级高于 base |
| `--texture-filter nearest\|linear` | 纹理采样方式，默认 `nearest` |
| `--width` / `--height` | 输出尺寸，默认 `1024x1024` |
| `--supersample <1-4>` | 超采样倍数，默认 `2` |
| `--views <LIST>` | 逗号分隔的 yaw 角度，例如 `45,225` |
| `--yaw` / `--elevation` | 单视角水平/俯仰角 |
| `--background transparent\|#RRGGBB[AA]` | 背景，默认透明 |
| `--lighting flat-v1\|simple-ao-v1` | 光照配置，默认 `simple-ao-v1` |
| `--threads <N>` | mesh/rasterizer 线程数，`0` 使用可用并行度，最多 256 |
| `--lod 1\|2-16\|auto` | LOD 因子，默认 `1`（关闭） |
| `--stream-mesh` | 按页构建并渲染网格，不保留完整 Mesh；不能与 `--lod auto` 同用 |
| `--force-limits` | 提高输入、场景、网格和像素上限；可能耗尽内存 |
| `--ignore-block-entity-nbt` | 跳过容器 NBT；`mesh`/`render`/`bench` 默认开启 |
| `-t, --terminal` | 在终端中直接渲染并显示图像 |
| `--terminal-protocol <auto\|kitty\|iterm2\|sixel\|halfblocks>` | 指定终端图像协议，默认自动检测 |
| `--terminal-width <COLS>` / `--terminal-height <ROWS>` | 指定终端显示字符列数与行数 |
| `--no-file` | 仅在终端显示图像，跳过生成磁盘 PNG 文件 |

`inspect`、`validate` 和 `stats` 默认保留 `TileEntities` 内容；几何命令默认跳过，
以减少大容器文件的解析时间和内存。

## 退出码

- `0`：成功
- `1`：内部错误
- `2`：输入无效、解析失败或资源超限

## 相关文档

- [材质与资源包](./materials.md)
- [LOD 与大结构](./lod.md)
- [限制与安全](./limits.md)
- [服务模式](./service.md)
- [English](./en/cli.md)
