---
title: "CLI 命令"
---

`blockscope` 的构建和路径说明见 [安装与快速开始](./getting-started.md)。

## 命令总览

| 命令        | 用途                                                                            |
| ----------- | ------------------------------------------------------------------------------- |
| `inspect`   | 输出稳定 manifest，包含 bounds、regions、统计、诊断、`inputHash` 和 `sceneHash` |
| `validate`  | 快速验证文件并输出摘要                                                          |
| `stats`     | 输出方块/材料统计                                                               |
| `mesh`      | 构建网格，可导出 OBJ 或 PLY                                                     |
| `render`    | 渲染 PNG 或直接在终端中显示图像                                                 |
| `bench`     | 测量 parse、mesh、render、PNG 各阶段耗时                                        |
| `animate`   | 按关键帧相机渲染 MP4 或 GIF 动画                                                |
| `materials` | 检查资源包对当前模型的覆盖率                                                    |
| `capacity`  | 输出 CPU/内存容量和建议的 worker/thread 配置                                    |
| `serve`     | 长驻 worker，使用 stdio JSON 协议                                               |
| `supervise` | 启动并调度多个 worker 进程                                                      |

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
blockscope render huge.litematic --output huge.png --stream-mesh
blockscope render huge.litematic --output huge.png --stream-mesh --greedy-mesh

# 材质覆盖率
blockscope materials model.litematic --client-jar client.jar --json

# 动画（需要 ffmpeg 4.4 或更新版本）
blockscope animate model.litematic --output out.mp4 \
  --keyframe 0:45:30 --keyframe 6:405:30 --fps 30
blockscope animate model.litematic --output out.gif \
  --keyframe 0:45:30 --keyframe 4:405:30 --background transparent

# 容器容量
blockscope capacity --json
# 终端内直接预览图像（Kitty、iTerm2、Sixel、Halfblocks/Truecolor）
blockscope render model.litematic --terminal
blockscope render model.litematic -t --terminal-protocol kitty
blockscope render model.litematic -t --no-file
blockscope preview model.litematic -t
```

多视角渲染时，单视角输出 `out.png`，多视角输出 `out-45.png`、`out-225.png` 等。`serve` 模式的输出命名为 `{id}.png`（单视角）或 `{id}-{yaw}.png`（多视角）。有完整 mesh 时，多视角会按 `--threads` 预算并行渲染；输出顺序不变，并且每个视角会在下一批开始前写出并释放缓冲，内存不会随视角数量无限增长。

`animate` 将关键帧相机采样为视频。关键帧格式为 `TIME:YAW:ELEVATION`，单位为秒和角度；每个关键帧重复一次 `--keyframe`。MP4 使用 H.264，需要不透明背景和偶数尺寸；GIF 支持 1 位透明。`--ffmpeg` 可指定外部可执行文件，最低支持 4.4。`--encoder auto` 会在首次使用时按输出尺寸探测硬件编码器，失败时回退到 libx264；`software` 强制使用 libx264，显式硬件编码器未编译或不可用时返回 `ENCODER_UNAVAILABLE`，不会静默回退。使用 GPU 后端时，`--json` 会在 `gpuStages` 中报告聚合的 GPU 阶段耗时。

未使用 `--force-limits` 时，单个视频限制为 60 秒和 1,800 帧。

| 选项 | 说明 |
| --- | --- |
| `--output <PATH>` | 输出路径，默认 `animate.mp4`；未指定 `--format` 时由扩展名选择 MP4 或 GIF |
| `--format mp4\|gif` | 强制指定容器格式 |
| `--keyframe TIME:YAW:ELEV` | 每个关键帧重复一次；时间必须递增 |
| `--duration <SECONDS>` | 视频时长，默认使用最后一个关键帧的时间 |
| `--fps <N>` | 帧率，1-60；MP4 默认 30，GIF 默认 15 |
| `--no-loop` | 循环视频不重复末尾端点 |
| `--easing linear\|smooth` | 相机插值方式，默认 `linear` |
| `--encoder auto\|software\|nvenc\|qsv\|amf\|videotoolbox\|vaapi` | 编码器，默认 `auto` |
| `--quality low\|medium\|high` | 编码质量，默认 `medium` |
| `--ffmpeg <PATH>` | 外部 ffmpeg 可执行文件（4.4 或更新） |
| `--json` | 以 JSON 输出视频报告 |

终端图像会先适配可见网格：渲染尺寸超过窗口时只向终端指定一个字符维度，由终端按原始宽高比缩小图像。摘要先输出、图像最后输出，后续文字不会把它顶出屏幕。在 `tmux` 或 `screen` 内（默认会丢弃图形序列，除非开启 passthrough），自动检测会退化为半块字符；需要时用 `--terminal-protocol` 覆盖。当 stdout 不是终端（管道、日志、被捕获的 shell）时会给出警告，因为转义序列无法显示。

## 常用选项

| 选项                                                           | 说明                                                                  |
| -------------------------------------------------------------- | --------------------------------------------------------------------- |
| `--client-jar <PATH>`                                          | 直接读取 Minecraft 客户端 jar                                         |
| `--assets <DIR>`                                               | 读取已解压的 `assets/` 目录                                           |
| `--resource-pack <PATH>`                                       | 叠加资源包目录、zip 或 jar；优先级高于 base                           |
| `--texture-filter nearest\|linear`                             | 纹理采样方式，默认 `nearest`                                          |
| `--width` / `--height`                                         | 输出尺寸，默认 `1024x1024`，单边最大 4096                             |
| `--supersample <1-4>`                                          | 超采样倍数，默认 `2`                                                  |
| `--views <LIST>`                                               | 逗号分隔的 yaw 角度，例如 `45,225`，最多 64 个                        |
| `--yaw` / `--elevation`                                        | 单视角水平/俯仰角                                                     |
| `--sphere-fit`                                                 | 按旋转不变的包围球取景，而不是随 yaw 变化的投影轮廓；适合转台动画     |
| `--background transparent\|#RRGGBB[AA]`                        | 背景，默认透明                                                        |
| `--lighting flat-v1\|simple-ao-v1`                             | 光照配置，默认 `simple-ao-v1`                                         |
| `--threads <N>`                                                | mesh/rasterizer 线程数，`0` 使用可用并行度，最多 256                  |
| `--backend cpu\|gpu\|auto`                                     | 光栅化后端，默认 `auto`；`cpu` 为逐字节参考，`gpu` 强制硬件、不可用即失败 |
| `--precision exact\|fast`                                      | CPU 投影精度，默认 `exact`；`fast` 用 f32 存储屏幕点，以少量像素差异换取速度和内存 |
| `--stream-mesh [auto\|on\|off]`                                | 按页构建并渲染网格，不保留完整 Mesh；默认 `auto`，`off` 保留完整 Mesh |
| `--greedy-mesh`                                                | 合并同材质、同光照的共面单位面；适合大平面和重复结构，默认关闭        |
| `--quantize-mesh`                                             | 将流体和植物顶点量化到 1/1440 方块网格；降低 mesh 内存，代价是少量像素差异，默认关闭 |
| `--force-limits`                                               | 提高输入、场景、网格和像素上限；可能耗尽内存                          |
| `--ignore-block-entity-nbt`                                    | 跳过容器 NBT；`mesh`/`render`/`bench` 默认开启                        |
| `-t, --terminal`                                               | 在终端中直接渲染并显示图像                                            |
| `--terminal-protocol <auto\|kitty\|iterm2\|sixel\|halfblocks>` | 指定终端图像协议，默认自动检测                                        |
| `--terminal-width <COLS>` / `--terminal-height <ROWS>`         | 指定终端显示字符列数与行数                                            |
| `--no-file`                                                    | 仅在终端显示图像，跳过生成磁盘 PNG 文件                               |
| `--color auto\|always\|never`                                  | 终端颜色策略，默认 `auto`；对帮助、错误和结果输出生效                 |

`inspect`、`validate` 和 `stats` 默认保留 `TileEntities` 内容；几何命令默认跳过，以减少大容器文件的解析时间和内存。

默认后端为 `auto`：小场景使用 CPU，只有在存在硬件适配器、通过 f32 精度
保护、满足 60% 显存预算且成本模型预测 GPU 更快时才使用 GPU。单视角 `gpu`
支持分页流式渲染；多视角 GPU 任务保留完整 Mesh，以便一次上传服务所有视角。
若二进制未启用 `gpu` 特性、没有硬件适配器或只有软件适配器，`auto` 使用 CPU；
显式 `gpu` 会以 `BACKEND_UNAVAILABLE` 失败。GPU 输出与 CPU 参考视觉接近，
但跨驱动不保证逐字节一致，因此 golden PNG 和确定性保证仍以 `cpu` 为准。

`--precision fast` 只影响 CPU 投影。默认 `exact` 路径不变；`fast` 用 f32
存储屏幕点，缩小编译后的投影表并加快投影和 tile 读取，但可能改变少量像素的
边缘覆盖和最近邻纹理采样。它适合预览和吞吐优先的场景，不适合逐字节比较。
GPU 后端忽略该设置。

## 终端样式与进度

在没有参数时，`blockscope` 打印静态欢迎屏并立即退出。字标会按启动时的终端宽度和高度选择布局；输出被重定向时使用紧凑字标。静态输出退出后由终端负责重排，程序无法继续控制已经打印的字符。

字标会按终端宽度和高度逐级降级：82 列宽字标、59 列紧凑字标、47 列小字标，更窄时改用纯文字标题并只保留最有用的命令；输出被重定向时使用紧凑字标。

`--color` 可放在子命令前后，控制帮助、错误和人类可读结果的颜色：

| 值             | 行为                                    |
| -------------- | --------------------------------------- |
| `auto`（默认） | 仅在终端输出颜色；`NO_COLOR` 会关闭颜色 |
| `always`       | 即使通过管道也输出颜色                  |
| `never`        | 不输出颜色                              |

`--json` 输出始终是纯 JSON，不受 `--color` 影响。

`render`、`bench`、`mesh` 和 `materials` 在 stderr 是终端时显示进度：解析、材质、网格页数和单视角栅格化的 tile 百分比。多视角显示视角计数；流式渲染的最终合成使用不定进度。以下情况自动关闭：stderr 不是终端、`--json`、设置了 `CI` 环境变量、`NO_COLOR`，或 `--color never`。进度只写入 stderr，不会影响 stdout 和管道输出；进度条会按终端宽度缩短或改为只显示计数，窗口缩放后会自动重排。

## 退出码

- `0`：成功
- `1`：内部错误
- `2`：输入无效、解析失败或资源超限

## 相关文档

- [材质与资源包](./materials.md)
- [限制与安全](./limits.md)
- [服务模式](./service.md)
- [English](/user/cli)
