param(
    [string]$DriverRoot = "",
    [string]$SteamVrRoot = ""
)

$ErrorActionPreference = "Stop"

$workspace = Resolve-Path (Join-Path $PSScriptRoot "..")
if ([string]::IsNullOrWhiteSpace($DriverRoot)) {
    $DriverRoot = Join-Path $workspace "dist\virtual_hmd"
}
$DriverRoot = (Resolve-Path $DriverRoot).Path

if ([string]::IsNullOrWhiteSpace($SteamVrRoot)) {
    $uninstallKey = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Steam App 250820"
    $SteamVrRoot = (Get-ItemProperty -LiteralPath $uninstallKey).InstallLocation
}

$vrpathreg = Join-Path $SteamVrRoot "bin\win64\vrpathreg.exe"
if (-not (Test-Path -LiteralPath $vrpathreg)) {
    throw "vrpathreg.exe not found at $vrpathreg"
}

& $vrpathreg adddriver $DriverRoot
& $vrpathreg show

