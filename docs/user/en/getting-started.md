# Installation and Quick Start

## Requirements

- Windows x86_64, Linux x86_64 / aarch64, or macOS aarch64 / x86_64
- No Rust toolchain is needed when using prebuilt binaries
- Rust stable (edition 2024; Rust 1.85 or newer recommended) when building from
  source
- Optional: a Minecraft client jar, an extracted `assets/` directory, or a
  resource pack
- Optional: Docker
- At least 1 GiB of memory is recommended for a regular 1024x1024 preview.
  Supersampling, larger images, or huge structures need more.

## Download prebuilt binaries (recommended)

Download the installer or archive for your platform from
[Releases](https://github.com/Linvin-1233/Blockscope/releases):

| Platform | Target triple | Format |
| --- | --- | --- |
| Linux x86_64 | `x86_64-unknown-linux-musl` | `.tar.gz` |
| Linux aarch64 | `aarch64-unknown-linux-musl` | `.tar.gz` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `.msi` / `.zip` |
| macOS aarch64 | `aarch64-apple-darwin` | `.tar.gz` |
| macOS x86_64 | `x86_64-apple-darwin` | `.tar.gz` |

Names look like `blockscope-v0.2.0-x86_64-unknown-linux-musl.tar.gz`, and each
file has a `.sha256` next to it with the extension removed
(`blockscope-v0.2.0-x86_64-unknown-linux-musl.sha256`). Linux artifacts are
linked statically against musl and the Windows artifact uses the static CRT, so
the target machine needs no extra runtime libraries.

### Linux

```bash
curl -LO https://github.com/Linvin-1233/Blockscope/releases/download/v0.2.0/blockscope-v0.2.0-x86_64-unknown-linux-musl.tar.gz
curl -LO https://github.com/Linvin-1233/Blockscope/releases/download/v0.2.0/blockscope-v0.2.0-x86_64-unknown-linux-musl.sha256
sha256sum -c blockscope-v0.2.0-x86_64-unknown-linux-musl.sha256
tar -xzf blockscope-v0.2.0-x86_64-unknown-linux-musl.tar.gz
./blockscope-v0.2.0-x86_64-unknown-linux-musl/blockscope --version
```

### macOS

Use `aarch64-apple-darwin` for Apple Silicon (M-series) or `x86_64-apple-darwin` for Intel Macs:

```bash
curl -LO https://github.com/Linvin-1233/Blockscope/releases/download/v0.2.0/blockscope-v0.2.0-aarch64-apple-darwin.tar.gz
curl -LO https://github.com/Linvin-1233/Blockscope/releases/download/v0.2.0/blockscope-v0.2.0-aarch64-apple-darwin.sha256
shasum -a 256 -c blockscope-v0.2.0-aarch64-apple-darwin.sha256
tar -xzf blockscope-v0.2.0-aarch64-apple-darwin.tar.gz
./blockscope-v0.2.0-aarch64-apple-darwin/blockscope --version
```

### Windows: MSI installer (recommended)

Installs per user, so no administrator rights are required:

```powershell
$tag = "v0.2.0"
$name = "blockscope-$tag-x86_64-pc-windows-msvc"
Invoke-WebRequest "https://github.com/Linvin-1233/Blockscope/releases/download/$tag/$name.msi" -OutFile "$name.msi"
Invoke-WebRequest "https://github.com/Linvin-1233/Blockscope/releases/download/$tag/$name.msi.sha256" -OutFile "$name.msi.sha256"
Get-FileHash "$name.msi" -Algorithm SHA256
Get-Content "$name.msi.sha256"
msiexec /i "$name.msi"
```

It installs to `%LOCALAPPDATA%\Programs\Blockscope` and appends that directory to
the current user's `PATH`; open a new terminal and `blockscope --version` works.
Uninstall from Settings > Apps, which also removes the `PATH` entry. Use
`msiexec /i "$name.msi" /qn` for a silent install.

### Windows: portable zip

Unpack it instead if you do not want an installed copy:

```powershell
$tag = "v0.2.0"
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
- Windows: `target/release/blockscope.exe`

Add `target/release` to `PATH` or use the full path.

## Test

```bash
cargo test --workspace
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

- [CLI Reference](./cli.md)
- [Materials and Resource Packs](./materials.md)
- [LOD and Large Structures](./lod.md)
- [Troubleshooting](./troubleshooting.md)
- [中文](../getting-started.md)
