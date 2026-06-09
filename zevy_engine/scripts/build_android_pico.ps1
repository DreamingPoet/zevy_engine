param(
    [ValidateSet("release", "debug")]
    [string]$Profile = "release"
)

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

if ($Profile -eq "release") {
    $env:CARGO_APK_RELEASE_KEYSTORE = Join-Path $env:USERPROFILE ".android\debug.keystore"
    $env:CARGO_APK_RELEASE_KEYSTORE_PASSWORD = "android"
}

Remove-Item Env:ANDROID_SDK_ROOT -ErrorAction SilentlyContinue

Push-Location $ProjectRoot
try {
    if ($Profile -eq "release") {
        cargo apk build --lib --release
    }
    else {
        cargo apk build --lib
    }

    $Apk = Join-Path $ProjectRoot "target\$Profile\apk\zevy_engine.apk"
    Write-Host "APK: $Apk"
}
finally {
    Pop-Location
}
