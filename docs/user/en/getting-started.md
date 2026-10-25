# Installation and Quick Start

## Requirements

- Windows x86_64, Linux x86_64, or Linux aarch64
- No Rust toolchain is needed when using prebuilt binaries
- Rust stable (edition 2024; Rust 1.85 or newer recommended) when building from
  source
- Optional: a Minecraft client jar, an extracted `assets/` directory, or a
  resource pack
- Optional: Docker
- At least 1 GiB of memory is recommended for a regular 1024x1024 preview.
  Supersampling, larger images, or huge structures need more.

## Download prebuilt binaries (recommended)

Download the archive for your platform from
[Releases](https://github.com/Linvin-1233/Blockscope/releases):

| Platform | Target triple | Archive |
| --- | --- | --- |
| Linux x86_64 | `x86_64-unknown-linux-musl` | `.tar.gz` |
| Linux aarch64 | `aarch64-unknown-linux-musl` | `.tar.gz` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `.zip` |

Names look like `blockscope-v0.1.0-x86_64-unknown-linux-musl.tar.gz`, and each
archive has a `.sha256` file next to it with the extension removed
(`blockscope-v0.1.0-x86_64-unknown-linux-musl.sha256`). Linux artifacts are
linked statically against musl and the Windows artifact uses the static CRT, so
the target machine needs no extra runtime libraries.

Linux (using `v0.1.0` and `x86_64` as an example):

```bash
curl -LO https://github.com/Linvin-1233/Blockscope/releases/download/v0.1.0/blockscope-v0.1.0-x86_64-unknown-linux-musl.tar.gz
curl -LO https://github.com/Linvin-1233/Blockscope/releases/download/v0.1.0/blockscope-v0.1.0-x86_64-unknown-linux-musl.sha256
sha256sum -c blockscope-v0.1.0-x86_64-unknown-linux-musl.sha256
tar -xzf blockscope-v0.1.0-x86_64-unknown-linux-musl.tar.gz
./blockscope-v0.1.0-x86_64-unknown-linux-musl/blockscope --version
```

Windows PowerShell:

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

The hash printed by `Get-FileHash` must match the one in the `.sha256` file. Add
the extracted directory to `PATH`; the examples below use `blockscope` for that
executable.

## Build from source

Use this for development or when you need to modify the engine:

```bash
git clone https://github.com/Linvin-1233/Blockscope.git
cd Blockscope
cargo build --release -p blockscope-cli
```

Artifacts:

- Linux: `target/release/blockscope`
- Windows: `../../../target/release/blockscope.exe`

Add `../../../target/release` to `PATH` or use the full path.

## Test

```bash
cargo test --workspace
```

Full quality gate:

```bash
# Linux
bash ./scripts/check.sh

# Windows PowerShell 7+
pwsh ./scripts/check.ps1

# Windows PowerShell 5.1
powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/check.ps1
```

## First Render

The built-in color catalog works without a resource pack:

```bash
blockscope inspect model.litematic --json
blockscope validate model.litematic
blockscope render model.litematic --output out.png
```

Use a Minecraft client jar for real materials:

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

You can also point at an extracted assets directory:

```bash
blockscope render model.litematic --output out.png --assets /path/to/assets
```

## Next

- [CLI Reference](cli.md)
- [Materials and Resource Packs](materials.md)
- [LOD and Large Structures](lod.md)
- [Troubleshooting](troubleshooting.md)
- [中文](../getting-started.md)
