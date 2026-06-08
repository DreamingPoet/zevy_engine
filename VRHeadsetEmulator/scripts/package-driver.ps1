param(
    [ValidateSet("debug", "release")]
    [string]$Profile = "debug",

    [string]$OutputRoot = "dist",

    [ValidateSet("cpp", "rust")]
    [string]$Driver = "cpp"
)

$ErrorActionPreference = "Stop"

$workspace = Resolve-Path (Join-Path $PSScriptRoot "..")
$driverRoot = Join-Path $workspace $OutputRoot
$driverRoot = Join-Path $driverRoot "virtual_hmd"
$driverBin = Join-Path $driverRoot "bin\win64"

Push-Location $workspace
try {
    if ($Driver -eq "cpp") {
        $cmakeConfig = if ($Profile -eq "release") { "Release" } else { "Debug" }
        $buildDir = Join-Path $workspace "build\driver_plugin_cpp\$cmakeConfig"

        cmake -S (Join-Path $workspace "driver_plugin_cpp") -B $buildDir -A x64
        cmake --build $buildDir --config $cmakeConfig

        $driverDll = Join-Path $buildDir "$cmakeConfig\driver_virtual_hmd.dll"
        if (-not (Test-Path -LiteralPath $driverDll)) {
            $driverDll = Join-Path $buildDir "driver_virtual_hmd.dll"
        }
    } else {
        if ($Profile -eq "release") {
            cargo build --release
            $cargoProfileDir = "release"
        } else {
            cargo build
            $cargoProfileDir = "debug"
        }
        $driverDll = Join-Path $workspace "target\$cargoProfileDir\driver_virtual_hmd.dll"
    }

    New-Item -ItemType Directory -Force -Path $driverBin | Out-Null

    Copy-Item `
        -LiteralPath $driverDll `
        -Destination (Join-Path $driverBin "driver_virtual_hmd.dll") `
        -Force

    Copy-Item `
        -LiteralPath (Join-Path $workspace "driver.vrdrivermanifest") `
        -Destination (Join-Path $driverRoot "driver.vrdrivermanifest") `
        -Force

    Copy-Item `
        -LiteralPath (Join-Path $workspace "driver.vrresources") `
        -Destination (Join-Path $driverRoot "driver.vrresources") `
        -Force

    Write-Host "Packaged SteamVR $Driver driver at: $driverRoot"
    Write-Host "Register with: vrpathreg.exe adddriver `"$driverRoot`""
} finally {
    Pop-Location
}
