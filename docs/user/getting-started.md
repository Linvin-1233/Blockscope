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
安装包或压缩包：

| 平台 | 目标三元组 | 格式 |
| --- | --- | --- |
| Linux x86_64 | `x86_64-unknown-linux-musl` | `.tar.gz` |
| Linux aarch64 | `aarch64-unknown-linux-musl` | `.tar.gz` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `.msi` / `.zip` |

文件名形如 `blockscope-v0.1.2-x86_64-unknown-linux-musl.tar.gz`，旁边有去掉扩展名
的 `.sha256` 校验文件（如 `blockscope-v0.1.2-x86_64-unknown-linux-musl.sha256`）。
Linux 产物静态链接 musl，Windows 产物静态链接 CRT，目标机器不需要额外安装运行时库。

### Linux

```bash
curl -LO https://github.com/Linvin-1233/Blockscope/releases/download/v0.1.2/blockscope-v0.1.2-x86_64-unknown-linux-musl.tar.gz
curl -LO https://github.com/Linvin-1233/Blockscope/releases/download/v0.1.2/blockscope-v0.1.2-x86_64-unknown-linux-musl.sha256
sha256sum -c blockscope-v0.1.2-x86_64-unknown-linux-musl.sha256
tar -xzf blockscope-v0.1.2-x86_64-unknown-linux-musl.tar.gz
./blockscope-v0.1.2-x86_64-unknown-linux-musl/blockscope --version
```

### Windows：MSI 安装（推荐）

按用户安装，不需要管理员权限：

```powershell
$tag = "v0.1.2"
$name = "blockscope-$tag-x86_64-pc-windows-msvc"
Invoke-WebRequest "https://github.com/Linvin-1233/Blockscope/releases/download/$tag/$name.msi" -OutFile "$name.msi"
Invoke-WebRequest "https://github.com/Linvin-1233/Blockscope/releases/download/$tag/$name.msi.sha256" -OutFile "$name.msi.sha256"
Get-FileHash "$name.msi" -Algorithm SHA256
Get-Content "$name.msi.sha256"
msiexec /i "$name.msi"
```

安装到 `%LOCALAPPDATA%\Programs\Blockscope`，并把该目录追加到当前用户的 `PATH`；
重新打开终端后 `blockscope --version` 即可用。卸载在“设置 → 应用”里，卸载会同时
移除 `PATH` 条目。静默安装用 `msiexec /i "$name.msi" /qn`。

### Windows：便携 zip

不想安装时直接解压：

```powershell
$tag = "v0.1.2"
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
