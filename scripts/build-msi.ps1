param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Output,
    [string]$BinDir = "target/release"
)

$ErrorActionPreference = "Stop"
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:USERPROFILE\.dotnet;$env:USERPROFILE\.dotnet\tools;$env:Path"
# A user-scoped SDK lives outside the default location, so the WiX apphost needs
# DOTNET_ROOT to locate the runtime.
if (-not $env:DOTNET_ROOT -and (Test-Path "$env:USERPROFILE\.dotnet\dotnet.exe")) {
    $env:DOTNET_ROOT = "$env:USERPROFILE\.dotnet"
}

# Windows Installer versions are numeric; pre-release suffixes cannot be encoded.
if ($Version -match '-') {
    throw "MSI version cannot carry a pre-release suffix: $Version"
}

if (-not (Get-Command wix -ErrorAction SilentlyContinue)) {
    dotnet tool install --global wix --version 6.0.2
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$bin = (Resolve-Path $BinDir).Path
$out = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $out) | Out-Null

wix build installer/blockscope.wxs -arch x64 -d Version=$Version -d BinDir=$bin -o $out
exit $LASTEXITCODE
