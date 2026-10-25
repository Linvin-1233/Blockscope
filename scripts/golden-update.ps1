$ErrorActionPreference = "Stop"
$env:UPDATE_GOLDEN = "1"
cargo test -p blockscope-cli --test golden
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test -p litematic-render --test golden_png
exit $LASTEXITCODE
