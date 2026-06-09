param(
    [ValidateSet("release", "debug")]
    [string]$Profile = "release"
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$AndroidHome = "F:\AndriodSDK\AndriodSDK"
$Adb = Join-Path $AndroidHome "platform-tools\adb.exe"
$Apk = Join-Path $ProjectRoot "target\$Profile\apk\zevy_engine.apk"
$PackageName = "com.zevy.engine"
$ActivityName = "android.app.NativeActivity"

if (!(Test-Path $Apk)) {
    throw "APK not found: $Apk. Run scripts\build_android_pico.ps1 -Profile $Profile first."
}

$Devices = & $Adb devices -l
$Devices
$ConnectedDevices = $Devices | Select-String -Pattern "device product:"
if ($ConnectedDevices.Count -eq 0) {
    throw "No Android XR device is connected over ADB. Connect PICO 4 Ultra, enable USB debugging, then run this script again."
}

& $Adb install -r $Apk
& $Adb shell setprop debug.xr.graphicsPlugin Vulkan
& $Adb shell am force-stop $PackageName
& $Adb shell am start -W `
    -n "$PackageName/$ActivityName" `
    -a android.intent.action.MAIN `
    -c android.intent.category.LAUNCHER `
    -c com.picovr.intent.category.VR

Write-Host "Launched $PackageName/$ActivityName"
