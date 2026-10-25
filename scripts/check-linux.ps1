$ErrorActionPreference = "Stop"

$linuxTarget = "x86_64-unknown-linux-gnu"
if (Get-Command rustup -ErrorAction SilentlyContinue) {
    $installed = rustup target list --installed
    if ($installed -notcontains $linuxTarget) {
        rustup target add $linuxTarget | Out-Null
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }

    cargo clippy --workspace --all-targets --target $linuxTarget -- -D warnings
    exit $LASTEXITCODE
} else {
    Write-Warning "rustup not found; skipping the $linuxTarget clippy check"
}
