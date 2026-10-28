$ErrorActionPreference = "Stop"
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

cargo fmt --all --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo clippy --workspace --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Code behind `cfg(target_os = "linux")` is not compiled by the host clippy run,
# so a Windows-only check lets Linux lints reach CI first. Check the same targets
# against the Linux triple; clippy only type-checks, so no cross linker is needed.
& "$PSScriptRoot/check-linux.ps1"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cargo test --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
