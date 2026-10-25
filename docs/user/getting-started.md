# 安装与快速开始

## 系统要求

- Windows x86_64、Linux x86_64 或 Linux aarch64
- 使用预编译二进制时不需要 Rust 工具链
- 从源码构建时需要 Rust stable（支持 edition 2024；建议 Rust 1.85 或更新）
- 可选：Minecraft 客户端 jar、已解压的 `assets/` 目录或资源包
- 可选：Docker（用于容器部署）
- 常规 1024x1024 预览建议至少 1 GiB 内存；开启超采样、提高尺寸或渲染超大
  结构时需要更多内存

## 下载预编译二进制（推荐）

在 [Releases](https://github.com/Linvin-1233/Blockscope/releases) 下载对应平台的
压缩包：

| 平台 | 目标三元组 | 压缩格式 |
| --- | --- | --- |
| Linux x86_64 | `x86_64-unknown-linux-musl` | `.tar.gz` |
| Linux aarch64 | `aarch64-unknown-linux-musl` | `.tar.gz` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `.zip` |

文件名形如 `blockscope-v0.1.0-x86_64-unknown-linux-musl.tar.gz`，压缩包旁有去掉
扩展名的 `.sha256` 校验文件（如 `blockscope-v0.1.0-x86_64-unknown-linux-musl.sha256`）。
Linux 产物静态链接 musl，Windows 产物静态链接 CRT，目标机器不需要额外安装运行时库。

Linux（以 `v0.1.0`、`x86_64` 为例）：

```bash
curl -LO https://github.com/Linvin-1233/Blockscope/releases/download/v0.1.0/blockscope-v0.1.0-x86_64-unknown-linux-musl.tar.gz
curl -LO https://github.com/Linvin-1233/Blockscope/releases/download/v0.1.0/blockscope-v0.1.0-x86_64-unknown-linux-musl.sha256
sha256sum -c blockscope-v0.1.0-x86_64-unknown-linux-musl.sha256
tar -xzf blockscope-v0.1.0-x86_64-unknown-linux-musl.tar.gz
./blockscope-v0.1.0-x86_64-unknown-linux-musl/blockscope --version
```

Windows PowerShell：

```powershell
$tag = "v0.1.0"
$name = "blockscope-$tag-x86_64-pc-windows-msvc"
Invoke-WebRequest "https://github.com/Linvin-1233/Blockscope/releases/download/$tag/$name.zip" -OutFile "$name.zip"
Invoke-WebRequest "https://github.com/Linvin-1233/Blockscope/releases/download/$tag/$name.sha256" -OutFile "$name.sha256"
Get-FileHash "$name.zip" -Algorithm SHA256
Get-Content "$name.sha256"
Expand-Archive "$name.zip" -DestinationPath .
.\$name\blockscope.exe --version
```

`Get-FileHash` 输出的哈希应与 `.sha256` 文件中的值一致。把解压目录加入 `PATH`，
下文命令中的 `blockscope` 均指该可执行文件。

## 从源码构建

需要 Rust stable。用于开发、审查或自行修改引擎：

```bash
git clone https://github.com/Linvin-1233/Blockscope.git
cd Blockscope
cargo build --release -p blockscope-cli
```

构建产物：

- Linux：`target/release/blockscope`
- Windows：`../../target/release/blockscope.exe`

可以把 `../../target/release` 加入 `PATH`，也可以在命令中直接使用完整路径。

## 运行测试

```bash
cargo test --workspace
```

完整质量门禁：

```bash
# Linux
bash ./scripts/check.sh

# Windows PowerShell 7+
pwsh ./scripts/check.ps1

# Windows PowerShell 5.1
powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/check.ps1
```

## 第一次渲染

没有资源包时使用内置色块：

```bash
blockscope inspect model.litematic --json
blockscope validate model.litematic
blockscope render model.litematic --output out.png
```

使用 Minecraft 客户端 jar 获得真实材质：

```powershell
blockscope render model.litematic `
  --output out.png `
  --client-jar "C:\path\to\client.jar"
```

```bash
blockscope render model.litematic \
  --output out.png \
  --client-jar /path/to/client.jar
```

也可以使用已解压的 assets 目录：

```bash
blockscope render model.litematic --output out.png --assets /path/to/assets
```

## 下一步

- [CLI 命令](cli.md)
- [材质与资源包](materials.md)
- [LOD 与大结构](lod.md)
- [故障排查](troubleshooting.md)
- [English](en/getting-started.md)
