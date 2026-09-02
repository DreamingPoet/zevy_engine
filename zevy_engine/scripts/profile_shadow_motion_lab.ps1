param(
    [Parameter(Mandatory = $true)]
    [string]$Serial,

    [ValidateRange(1, 64)]
    [int]$LightCount = 16,

    [ValidateSet("geometry", "direct", "shadow", "full")]
    [string]$Mode = "full",

    [ValidateSet("forward", "deferred")]
    [string]$LightingPipeline = "forward",

    [ValidateRange(5, 600)]
    [int]$WarmupSeconds = 30,

    [ValidateRange(5, 300)]
    [int]$SampleCount = 20,

    [ValidateRange(0, 2000)]
    [int]$GpuFrequencyMHz = 599,

    [string]$AdbPath = "F:\AndriodSDK\AndriodSDK\platform-tools\adb.exe"
)

$ErrorActionPreference = "Stop"

$PackageName = "com.zevy.engine"
$ActivityName = "android.app.NativeActivity"
$Level = "shadow-motion-$LightCount"
$PointDirect = if ($Mode -in @("direct", "full")) { "1" } else { "0" }
$PointShadows = if ($Mode -in @("shadow", "full")) { "1" } else { "0" }

if (!(Test-Path -LiteralPath $AdbPath)) {
    throw "ADB not found: $AdbPath"
}

function Invoke-Adb {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)

    & $AdbPath -s $Serial @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "ADB command failed ($LASTEXITCODE): adb $($Arguments -join ' ')"
    }
}

function Get-NearestRankPercentile {
    param(
        [double[]]$Values,
        [ValidateRange(0.0, 1.0)]
        [double]$Percentile
    )

    if ($Values.Count -eq 0) {
        throw "Cannot calculate a percentile from zero samples."
    }

    $Sorted = @($Values | Sort-Object)
    $Index = [Math]::Min(
        $Sorted.Count - 1,
        [Math]::Max(0, [Math]::Ceiling($Sorted.Count * $Percentile) - 1)
    )
    return $Sorted[$Index]
}

Invoke-Adb -Arguments @("get-state") | Out-Null
Invoke-Adb -Arguments @("shell", "setprop", "debug.zevy.level", $Level)
Invoke-Adb -Arguments @("shell", "setprop", "debug.zevy.hud_page", "materials")
Invoke-Adb -Arguments @("shell", "setprop", "debug.zevy.point_direct", $PointDirect)
Invoke-Adb -Arguments @("shell", "setprop", "debug.zevy.point_shadows", $PointShadows)
Invoke-Adb -Arguments @("shell", "setprop", "debug.zevy.local_lighting", $LightingPipeline)
Invoke-Adb -Arguments @("logcat", "-c")
Invoke-Adb -Arguments @("shell", "am", "force-stop", $PackageName)
Invoke-Adb -Arguments @(
    "shell", "am", "start", "-W",
    "-n", "$PackageName/$ActivityName",
    "-a", "android.intent.action.MAIN",
    "-c", "android.intent.category.LAUNCHER",
    "-c", "com.picovr.intent.category.VR"
) | Out-Null

Start-Sleep -Seconds $WarmupSeconds

$AppPid = (Invoke-Adb -Arguments @("shell", "pidof", $PackageName) | Out-String).Trim()
if ([string]::IsNullOrWhiteSpace($AppPid)) {
    throw "$PackageName exited during the warmup period."
}

$Lines = & $AdbPath -s $Serial logcat -d -v raw -s PxrMetric:I "*:S"
if ($LASTEXITCODE -ne 0) {
    throw "Failed to read PxrMetric from logcat."
}

$Samples = @(
    $Lines | ForEach-Object {
        if ($_ -match "FPS=(?<fps>[0-9.]+)/[0-9.]+.*FrmCpu=(?<cpu>[0-9.]+)ms,FrmGpu=(?<gpu>[0-9.]+)ms.*GPU=(?<load>[0-9.]+)%/(?<freq>[0-9.]+)Mhz,GPUTemp=(?<temp>[0-9.]+)C") {
            [pscustomobject]@{
                Fps = [double]$Matches.fps
                CpuMs = [double]$Matches.cpu
                GpuMs = [double]$Matches.gpu
                GpuLoad = [double]$Matches.load
                GpuFrequencyMHz = [double]$Matches.freq
                TemperatureC = [double]$Matches.temp
            }
        }
    }
)

$Stable = @(
    $Samples |
        Where-Object {
            $_.GpuMs -gt 0 -and
            ($GpuFrequencyMHz -eq 0 -or $_.GpuFrequencyMHz -eq $GpuFrequencyMHz)
        } |
        Select-Object -Last $SampleCount
)

if ($Stable.Count -lt $SampleCount) {
    $FrequencyLabel = if ($GpuFrequencyMHz -eq 0) { "non-zero" } else { "$GpuFrequencyMHz MHz" }
    throw "Only $($Stable.Count) stable $FrequencyLabel samples were available; requested $SampleCount."
}

[pscustomobject]@{
    Level = $Level
    Mode = $Mode
    LightingPipeline = $LightingPipeline
    Samples = $Stable.Count
    FpsAverage = [Math]::Round(($Stable.Fps | Measure-Object -Average).Average, 2)
    CpuAverageMs = [Math]::Round(($Stable.CpuMs | Measure-Object -Average).Average, 2)
    CpuP95Ms = Get-NearestRankPercentile -Values $Stable.CpuMs -Percentile 0.95
    GpuAverageMs = [Math]::Round(($Stable.GpuMs | Measure-Object -Average).Average, 2)
    GpuP95Ms = Get-NearestRankPercentile -Values $Stable.GpuMs -Percentile 0.95
    GpuCappedSamples = @($Stable | Where-Object { $_.GpuMs -ge 66.66 }).Count
    GpuLoadAverage = [Math]::Round(($Stable.GpuLoad | Measure-Object -Average).Average, 1)
    TemperatureAverageC = [Math]::Round(($Stable.TemperatureC | Measure-Object -Average).Average, 1)
    GpuFrequencyAverageMHz = [Math]::Round(($Stable.GpuFrequencyMHz | Measure-Object -Average).Average, 1)
    GpuFrequencyMinMHz = ($Stable.GpuFrequencyMHz | Measure-Object -Minimum).Minimum
    GpuFrequencyMaxMHz = ($Stable.GpuFrequencyMHz | Measure-Object -Maximum).Maximum
}
