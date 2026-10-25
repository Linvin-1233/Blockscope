$ErrorActionPreference = "Stop"
$env:RUSTFLAGS = "-C target-feature=+crt-static"
cargo build --release --locked -p blockscope-cli --bin blockscope
exit $LASTEXITCODE
