param(
    [Parameter(Mandatory = $true)][string]$Tag
)

$ErrorActionPreference = "Stop"

$cleanTag = $Tag.TrimStart('v')
$match = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"$').Matches
if (-not $match) {
    throw "Cannot read [workspace.package] version from Cargo.toml"
}
$version = $match[0].Groups[1].Value
if ($cleanTag -ne $version) {
    throw "Tag $Tag does not match workspace version $version"
}
Write-Host "Tag $Tag matches workspace version $version"
