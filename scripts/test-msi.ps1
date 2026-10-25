param(
    [string]$Msi = "out/blockscope-msi-check.msi",
    [string]$LogDir = "out"
)

$ErrorActionPreference = "Stop"

$msiPath = (Resolve-Path $Msi).Path
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$installLog = Join-Path $LogDir "install.log"
$uninstallLog = Join-Path $LogDir "uninstall.log"

# Install MSI
$install = Start-Process msiexec -ArgumentList "/i `"$msiPath`" /qn /norestart /l*v `"$installLog`"" -Wait -PassThru
if ($install.ExitCode -ne 0) { throw "MSI install failed with $($install.ExitCode)" }

# Verify install and PATH
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\Blockscope'
if (-not (Test-Path "$installDir\blockscope.exe")) { throw "executable missing after install" }
& "$installDir\blockscope.exe" --version
if ($LASTEXITCODE -ne 0) { throw "installed executable failed" }

# Windows Installer registers per-user products under HKCU and may
# still write the Add/Remove entry to HKLM, so accept either location.
$arp = @(
    Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue |
        Where-Object { $_.DisplayName -eq 'Blockscope' }
    Get-ItemProperty 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue |
        Where-Object { $_.DisplayName -eq 'Blockscope' }
)
if (-not $arp) { throw "missing uninstall entry" }

$perUser = @(
    Get-ChildItem 'HKCU:\Software\Microsoft\Installer\Products' -ErrorAction SilentlyContinue |
        ForEach-Object { Get-ItemProperty $_.PSPath } |
        Where-Object { $_.ProductName -eq 'Blockscope' }
)
if (-not $perUser) { throw "product is not registered per user" }

# The MSI appends the directory with a trailing separator.
$path = (Get-ItemProperty 'HKCU:\Environment' -Name Path -ErrorAction SilentlyContinue).Path
$entries = ($path -split ';') | Where-Object { $_ } | ForEach-Object { $_.TrimEnd('\') }
if ($entries -notcontains $installDir) { throw "PATH does not contain $installDir" }

# Uninstall MSI
$uninstall = Start-Process msiexec -ArgumentList "/x `"$msiPath`" /qn /norestart /l*v `"$uninstallLog`"" -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw "MSI uninstall failed with $($uninstall.ExitCode)" }

# Verify uninstall cleanup
if (Test-Path "$installDir\blockscope.exe") { throw "executable still present after uninstall" }
$arpAfter = @(
    Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue |
        Where-Object { $_.DisplayName -eq 'Blockscope' }
    Get-ItemProperty 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue |
        Where-Object { $_.DisplayName -eq 'Blockscope' }
)
if ($arpAfter) { throw "uninstall entry still present" }

$pathAfter = (Get-ItemProperty 'HKCU:\Environment' -Name Path -ErrorAction SilentlyContinue).Path
$entriesAfter = ($pathAfter -split ';') | Where-Object { $_ } | ForEach-Object { $_.TrimEnd('\') }
if ($entriesAfter -contains $installDir) { throw "PATH entry was not removed" }

Write-Host "installer lifecycle ok"
