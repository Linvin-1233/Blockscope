$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if (-not (Get-Command worker-build -ErrorAction SilentlyContinue)) {
        cargo install worker-build --version 0.8.7 --locked
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }

    Push-Location crates/worker
    try {
        worker-build --release
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    finally {
        Pop-Location
    }

    node scripts/pack-worker.mjs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally {
    Pop-Location
}
