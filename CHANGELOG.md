# 变更日志

[English](CHANGELOG.en.md)

本文件记录面向用户的变更。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增

- 新增 Cloudflare Workers 适配器（`blockscope-worker`），支持可配置 HTTP 端点和
  CORS，直接从 `.litematic` 请求体渲染 PNG，并可从 Workers 静态资源加载资源包。

### 修复

- 回退配色现在会沿材质变体继承基础颜色：石英柱、楼梯、台阶按石英渲染，不再落入
  品红“未知方块”占位色；屏障等不可见方块不再被绘制。
- 终端图像输出（`--terminal`）现在会先适配可见网格，而不是按原始像素尺寸发送；
  摘要在图像之前输出，后续文字不会把图像顶出屏幕。在 `tmux`/`screen` 内自动检测
  退化为半块字符；stdout 不是终端时给出警告；iTerm2 编码器不再发送凭空的
  `width=auto` / `height=auto` 键。

## [0.4.0] - 2026-10-28

### Added

- CLI 增加统一的终端样式层：`--color auto|always|never`、`NO_COLOR` 支持、完整
  的选项列表、彩色错误/警告和更整齐的人类可读输出；无参数运行 `blockscope` 会显示
  带文字字标的欢迎屏并以 `0` 退出。
- `render`、`bench`、`mesh`、`materials` 在交互终端显示 stderr 进度：网格页数和
  单视角栅格化 tile 百分比；多视角显示视角计数。管道、`--json`、`CI` 或
  `--color never` 时自动关闭，进度不会进入 stdout。
- 欢迎屏字标会按启动时的终端宽度和高度降级，窄窗口和分屏下不会横向溢出；帮助
  文本也会按终端宽度换行。
- 长任务进度条在窗口缩放后自动重排，不会因为折行破坏终端输出。

### Fixed

- NBT 列表、int 数组和 long 数组在分配内存前会先检查剩余输入；截断的小文件
  不能再通过声明超大长度强制分配数百 MiB。
- 资源包目录和 zip 的资产读取在读取/解压过程中就受大小上限约束，避免 zip bomb
  或超大本地文件在检查前被完整读入内存。
- `Scene::new` 会拒绝尺寸与 packed 状态数量不一致或 palette 为空的 region，
  避免错误构造的场景在网格构建阶段 panic；`heap_bytes` 现在计入 block entity
  NBT，serve 的缓存预算不再低估容器数据。palette 只包含空气的 region 会产出
  `EMPTY_REGION` 信息诊断。
- `supervise` 的输入 channel 改为有界，协议单行限制为 1 MiB；队列满时提供真正
  的背压，超长行返回 `INVALID_REQUEST` 而不是无限占用内存。
- `supervise` 未显式指定 `--cache-bytes` 时按每个 worker 的内存上限派生缓存预算，
  不再让每个 worker 使用半台机器的可用内存。
- `supervise` 的输入哈希移到独立线程；队列已满时先返回 `BUSY`，不再先读取文件。
  调用方提供 `input.hash` 时直接用于路由，跳过重复哈希。
- 流式渲染不再使用可能因 worker panic 而永久等待的 barrier，投影完成后按普通
  线程 join 传播错误。
- 资源包 blockstate 的 `x`/`y` 旋转在取负前归一化，`i32::MIN` 不再在 debug
  构建中触发溢出 panic。
- 水流和岩浆不再按单个方块渲染成阶梯状盒子。网格构建读取四周流体列和上方方块，
  用原版权重计算顶面四角高度，隐藏同种流体之间的内部面，并在上方仍有同种流体时
  补成整格；流动水面现在是连续斜坡，也不再出现半透明内面的接缝。
- 含水的不完整方块现在会保留自身模型并补上与周围一致的含水层，包括楼梯、台阶、
  墙、树叶、告示牌、珊瑚，以及始终含水的海带/海草。含水的完整不透明方块会跳过
  含水层；气泡柱直接使用流体表面。
- 红石粉在内置色块材质下不再回退成品红色整方块：新增 1/64 高的扁平粉面模型，并按
  `power=0..15` 使用与贴图路径相同的暗红到亮红渐变。渐变改为平方根曲线，power 0
  和 1 在细粉线上也能明显区分。资源包路径的颜色行为同步。
- 重新生成水方块黄金 PNG，并新增流体表面、同流体剔除、含水方块、红石亮度与多构建
  路径一致性测试。

### Changed

- `serve` 和 `render --views` 最多接受 64 个视角；多视角按输出字节预算分批渲染，
  每个视角渲染后立即写出并释放 RGBA 缓冲，内存不再随视角数量线性增长。
- `serve` 在解析输入和建网格之前校验尺寸、视角、光照、背景、纹理过滤和输出目录，
  非法请求不会先消耗一次完整解析/网格构建。
- `input.hash` 在协议文档中明确为调用方声明的缓存身份：命中缓存时信任该值，
  未命中时才重新读取并校验。
- `render` 的 `--stream-mesh` 默认改为 `auto`：固定 LOD 时自动按页流式渲染，
  `--lod auto` 仍保留完整 mesh 以选择因子；`--stream-mesh=off` 可恢复完整 mesh，
  `--stream-mesh` / `on` 强制流式。默认路径避免了大场景全量 mesh 的峰值。
- 半透明 `Triangle` 的投影点改为 f32，并去掉恒为 translucent 的 layer 字段；
  深度排序键仍为 f64，结构从 112 B 降到 72 B。带客户端材质、`000001`、
  `--stream-mesh` 的峰值从 2.28 GB 降到 1.52 GB；黄金 PNG 测试保持不变。
- 流式渲染在半透明三角形超过 262,144 个时，把精确记录溢写到临时 bucket
  文件，并按 bucket 逐个排序渲染。`000001` + client jar 的峰值从 1.52 GB
  降到 **618 MB**，PNG SHA-256 不变；临时目录在渲染结束时删除。新增
  `TEMP_STORAGE_FAILED` 错误码。

### Performance

- 箱子状态先走方块实体模型，不再先解析随后会被丢弃的资源包 blockstate 模型。
- 材质快照不再为每个状态构建最终会被资源包模型替换的回退几何。`ColorBlockCatalog`
  新增只解析颜色/图层/可用性的 `resolve_style`，资源包模型和箱子只取样式；回退模型
  仅在真正需要时构建。`ModelChain` 缓存改为共享 `Arc`，纹理透明度在解码时计算并缓存。
  `000001` + 26.2 client jar、23,813 个状态、512x512/超采样 1 下，材质快照从
  2.6-2.7 s 降到约 0.6 s，`materials` 从 3.1 s 降到 1.3 s，总渲染从 18.6 s
  降到 14.8 s；统计和黄金 PNG 不变。
- 含水层会为每个可见的含水方块增加半透明面。`000001` 这类包含约 320 万个
  `waterlogged=true` 状态的随机压力场景，512x512/超采样 1 的 render 从约
  3.1 s 增至 5.2 s；普通建筑（如实测的整流器细节场景）总耗时基本不变。

## [0.3.0] - 2026-10-27

### Added

- 新增 `--greedy-mesh`：在页内合并同法线、同平面、同材质、同光照且 AO 均匀的
  共面单位面，减少平坦结构的 quad 数和 mesh 内存。默认关闭，因为随机调色板场景
  收益很低而合并本身有成本；大平面、墙体和重复结构的收益明显。

### Changed

- `render`、`bench`、`serve` 和库调用现在拒绝单边超过 4096 像素的输出请求，与协议
  文档一致；总像素预算不变。此前只检查总像素数，极端长宽比可能绕过协议约定。
- 网格构建按 16x16x16 页缓存方块及其边界，并把每个 region 的 palette 预解析为
  不可变材质表，剔除和环境光遮蔽改为数组索引；缓存每个格子的遮挡位并用扁平索引
  偏移访问邻居，完整不透明方块在六个邻居都遮挡时直接跳过模型。测试夹具
  （1024x1024、超采样 2、12 线程、色块材质）的 mesh 耗时：`000001` exact
  17.3-20.4 s → 5.7-5.9 s，`000002` exact 11.7 s → 0.24-0.25 s。网格统计和
  PNG SHA-256 保持不变。
- 网格页索引只包含实际含非空气方块的 page。稀疏结构不再扫描空 page，也不会因为
  包围盒跨越过多 page 而误报 `PAGE_LIMIT_EXCEEDED`；无空气场景仍走包围盒快速路径。
  `blocksVisited` 现在只统计被索引 page 中扫描过的方块，全空气网格为 0。
- `render --stream-mesh` 按 32 页一块构建网格并立即渲染，不再保留完整 Mesh。
  1024x1024、超采样 2、12 线程、色块材质下，`000001` exact 峰值从 7.16 GB 降到
  425 MB，`000002` exact 从 497 MB 降到 182 MB，PNG SHA-256 不变。该模式不能与
  `--lod auto` 同用；多视角会为每个视角重新构建网格。
- 纹理采样按面提前解析纹理并内联采样器，UV 在 `0..1` 内走无回绕路径；合成的
  16x16 采样微基准中 nearest/bilinear 吞吐提升 1.7-2.7 倍。
- 有完整 mesh 时多视角并行渲染，按 `--threads` 预算拆分线程；CLI 和 `serve`
  都保留请求顺序，但每个并发视角各自持有 tile buffer。
- `serve` 在请求带 `input.hash` 且缓存命中时跳过文件读取和重新哈希；
  `supervise` 会把路由用的 hash 注入请求，因此重复输入的缓存命中不再读文件。
- `serve` / `supervise` 的缓存增加估算字节预算 `--cache-bytes`，默认使用可用
  内存的一半；超出时按最旧优先淘汰，单个超大条目仍允许单独保留，避免多个超大
  mesh 同时常驻。
- 渲染瓦片按内部图约 1024 块自适应：1024x1024 内部图使用 32px，2048x2048
  保持 64px，避免 12 线程在 16-64 个 tile 上串行收尾。`000001`（77.6M
  quads，12 线程）的 256x256/超采样 1 渲染从 5.7-6.4 s 降到 3.1-3.2 s，
  512x512/超采样 1 从 4.5-4.6 s 降到 3.1-3.2 s，512x512/超采样 2 从 3.8 s
  降到 3.2 s，1024x1024/超采样 1 从 3.7 s 降到 3.2 s；RGBA 不变，`tiles`
  不随线程数变化。
- 光栅化器按有无纹理分别单态化三角形循环，无纹理路径去掉每像素纹理采样
  分支和 UV 插值；`000001`（0 个纹理三角形）的 1024x1024/超采样 2 渲染从
  4.1 s 降到 3.9 s，输出不变。
- 网格页容量改为按最近几页的 quad/非空气方块中位数估算并留 15% 余量，扩容时
  只增加 25%（不再按 Vec 倍增），明显低于估算的页在收尾时收缩。`000001` 的
  mesh 从 4.0-4.3 s 降到 3.1-3.2 s，常驻 mesh 从 10.7 GB 降到 7.9 GB；
  `000002` 从 310 MB 降到 261 MB，mesh 约 0.22 s。
- release 构建启用 thin LTO 和单 codegen unit。测试机上 `000001` 的 mesh 与
  render 约快 4-5%，`000002` 的 parse 约快 20%；release 构建时间相应增加。
- `--lod auto` 在构建前先抽样 128 个 page 估算 quad 数，只有估算明显超过预算时
  才用计数模式精确确认；计数复用页缓存和剔除逻辑，但不分配顶点、AO 和四边形。
  这样不会再为注定失败的 exact 网格付完整构建成本：`000001` 的 auto mesh 从
  5.0-5.2 s 降到 3.6-3.7 s，最终 factor 和输出不变；`000002` 这类可精确放下的
  场景多付约 30 ms 采样。

## [0.2.0] - 2026-10-25

### Added

- 支持 macOS 平台（Apple Silicon `aarch64-apple-darwin` 与 Intel `x86_64-apple-darwin`）。
- CLI 在 macOS 上通过 `sysctl` 和 Mach VM statistics 自动检测物理内存与可用内存容量。
- 多进程 supervisor 在 macOS 上通过 `proc_pidinfo` 监控子进程实际物理内存（RSS）并执行内存上限保护。
- `bench` 命令在 macOS 上通过 `getrusage` 报告峰值物理内存（peak RSS）。
- CI 测试矩阵和 release 工作流增加 macOS 构建目标与产物打包。
- `render` 支持在终端中直接显示图像：Kitty、iTerm2、Sixel 与半块字符协议，可通过 `--terminal` 使用并自动检测协议。

### Changed

- CPU 渲染器按扫描线与三角形边的交点裁剪每行像素跨度，并让流式渲染的投影与分箱
  和光栅化共用同一批工作线程、按投影四边形数量切块。大夹具（1024x1024、超采样 2）
  的渲染耗时：`DenseNonSolid` 271 ms → 147 ms，`Atrium` 77 ms → 40 ms，
  `Valkyrie` 73 ms → 30 ms。PNG 输出保持逐字节一致；`pixelsTested` 改为统计行裁剪后
  实际测试的片段数。
- 多线程渲染进一步优化：SSAA 降采样、网格页投影、LOD 聚合和并行网格构建按核数扩展，
  大场景与扁平结构的多核利用率更高。
- 网格内存占用下降：`Vertex` 从 36 字节降到 16 字节，`Quad` 从 148 字节降到 88 字节
  （约 -40% 几何内存）；`normal`、`color`、`texture`、`shade`、`ao` 从顶点移到四边形。
  这是一次公开 API 变更，外部代码需要改读 `Quad` 上的对应字段。

### Fixed

- supervisor 在检查内存上限前先采样 worker RSS，避免延迟一个采样周期才触发内存保护。

## [0.1.2] - 2026-10-25

### Fixed

- MSI 显式设置 `ALLUSERS=2` 和 `MSIINSTALLPERUSER=1`：即使安装程序从管理员终端
  启动，也按用户上下文安装，文件写入用户目录、`PATH` 写入当前用户，卸载后不残留。
- CI 增加 MSI 安装/卸载冒烟测试，验证安装目录、按用户注册、`PATH` 条目和文件清理。

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
