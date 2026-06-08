$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$AndroidHome = "F:\AndriodSDK\AndriodSDK"
$JavaHome = Join-Path $AndroidHome "JAVA\jdk-17.0.10"
$NdkHome = Join-Path $AndroidHome "ndk\25.1.8937393"

$env:ANDROID_HOME = $AndroidHome
$env:JAVA_HOME = $JavaHome
$env:NDK_HOME = $NdkHome
$env:NDKROOT = $NdkHome
$env:PATH = "$JavaHome\bin;$AndroidHome\platform-tools;$AndroidHome\build-tools\34.0.0;$env:PATH"

Remove-Item Env:ANDROID_SDK_ROOT -ErrorAction SilentlyContinue

Push-Location $ProjectRoot
try {
    cargo apk build --lib
    Write-Host "APK: $ProjectRoot\target\debug\apk\zevy_engine.apk"
}
finally {
    Pop-Location
}
