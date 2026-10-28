param(
    [Parameter(Mandatory = $true)][string]$Type,
    [string]$Path = "",
    [string]$Args = ""
)

$ErrorActionPreference = "Stop"

if ($Type -eq "litematic") {
    if ($Path) { $env:LITEMATIC_FIXTURES_DIR = $Path }
    cargo test -p litematic-core --test real_files -- --ignored $Args
    exit $LASTEXITCODE
}

if ($Type -eq "jar") {
    if ($Path) { $env:MINECRAFT_CLIENT_JAR = $Path }
    cargo test -p litematic-material --test real_jar -- --ignored $Args
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo test -p litematic-render --test real_materials -- --ignored $Args
    exit $LASTEXITCODE
}

throw "Unknown real test type: $Type"
