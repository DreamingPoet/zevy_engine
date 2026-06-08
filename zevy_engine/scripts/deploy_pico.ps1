$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$AndroidHome = "F:\AndriodSDK\AndriodSDK"
$Adb = Join-Path $AndroidHome "platform-tools\adb.exe"
$Apk = Join-Path $ProjectRoot "target\debug\apk\zevy_engine.apk"
$PackageName = "com.zevy.engine"
$ActivityName = "android.app.NativeActivity"

if (!(Test-Path $Apk)) {
    throw "APK not found: $Apk. Run scripts\build_android_pico.ps1 first."
}

$Devices = & $Adb devices -l
$Devices
$ConnectedDevices = $Devices | Select-String -Pattern "device product:"
if ($ConnectedDevices.Count -eq 0) {
    throw "No Android XR device is connected over ADB. Connect PICO 4 Ultra, enable USB debugging, then run this script again."
}

& $Adb install -r $Apk
& $Adb shell monkey -p $PackageName -c android.intent.category.LAUNCHER 1

Write-Host "Launched $PackageName/$ActivityName"
