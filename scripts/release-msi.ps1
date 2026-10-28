param(
    [Parameter(Mandatory = $true)][string]$Tag,
    [string]$Triple = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"

$version = $Tag.TrimStart('v')
$msi = "blockscope-$Tag-$Triple.msi"
./scripts/build-msi.ps1 -Version $version -BinDir "target/$Triple/release" -Output $msi
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$sha256 = [System.Security.Cryptography.SHA256]::Create()
$stream = [System.IO.File]::OpenRead($msi)
try {
    $hashBytes = $sha256.ComputeHash($stream)
} finally {
    $stream.Dispose()
    $sha256.Dispose()
}
$hash = [System.BitConverter]::ToString($hashBytes).Replace('-', '').ToLowerInvariant()
"$hash  $msi" | Out-File -Encoding ascii "$msi.sha256"
gh release upload $Tag $msi "$msi.sha256" --clobber
exit $LASTEXITCODE
