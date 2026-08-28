<#
Evaluate a gameplay evidence folder against the public-preview performance gate.
Missing measurements are reported as NOT_RUN and prevent a complete pass.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$RunDir,

    [double]$LeagueBaselineMedianFps = [double]::NaN,
    [double]$LeagueCaptureMedianFps = [double]::NaN,
    [double]$VmafScore = [double]::NaN,
    [string]$ClipLatencyCsv = "",

    [ValidateSet("Pass", "Fail", "NotRun")]
    [string]$VisualInspection = "NotRun",

    [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"
$ResolvedRunDir = (Resolve-Path -LiteralPath $RunDir).Path
$MetricsDir = Join-Path $ResolvedRunDir "03_capture_audio_gpu"
$ProcessCsv = Join-Path $MetricsDir "process-metrics.csv"
$MediaCsv = Join-Path $MetricsDir "media-validation.csv"
if (-not (Test-Path -LiteralPath $ProcessCsv -PathType Leaf)) { throw "Missing process metrics: $ProcessCsv" }
if (-not (Test-Path -LiteralPath $MediaCsv -PathType Leaf)) { throw "Missing media validation: $MediaCsv" }

function Convert-Number {
    param([object]$Value)
    $number = 0.0
    if ([double]::TryParse([string]$Value, [ref]$number)) { return $number }
    return $null
}

function Get-Percentile {
    param([double[]]$Values, [double]$Percentile)
    $sorted = @($Values | Sort-Object)
    if ($sorted.Count -eq 0) { return $null }
    $index = [math]::Max(0, [math]::Ceiling($sorted.Count * $Percentile) - 1)
    return [double]$sorted[$index]
}

function Add-Gate {
    param([string]$Name, [string]$Status, [string]$Measured, [string]$Target)
    $script:Gates.Add([pscustomobject]@{ Name = $Name; Status = $Status; Measured = $Measured; Target = $Target }) | Out-Null
}

$processRows = @(Import-Csv -LiteralPath $ProcessCsv)
$mediaRows = @(Import-Csv -LiteralPath $MediaCsv | Where-Object { $_.validation -ne "ACTIVE" })
$Gates = New-Object System.Collections.Generic.List[object]

$captureFps = @($mediaRows | ForEach-Object { Convert-Number $_.captureFps } | Where-Object { $null -ne $_ })
if ($captureFps.Count -eq 0) { Add-Gate "Median capture FPS" "NOT_RUN" "No measured media FPS" ">= 59" }
else {
    $medianCaptureFps = Get-Percentile $captureFps 0.5
    Add-Gate "Median capture FPS" $(if ($medianCaptureFps -ge 59) { "PASS" } else { "FAIL" }) "$([math]::Round($medianCaptureFps, 2))" ">= 59"
}

$frames = 0.0
$drops = 0.0
foreach ($row in $mediaRows) {
    $fps = Convert-Number $row.captureFps
    $duration = Convert-Number $row.durationSeconds
    $drop = Convert-Number $row.frameDrops
    if ($null -ne $fps -and $null -ne $duration -and $null -ne $drop) {
        $frames += $fps * $duration
        $drops += $drop
    }
}
if ($frames -le 0) { Add-Gate "Frame drop rate" "NOT_RUN" "No frame counts" "< 1%" }
else {
    $dropRate = ($drops / ($frames + $drops)) * 100.0
    Add-Gate "Frame drop rate" $(if ($dropRate -lt 1) { "PASS" } else { "FAIL" }) "$([math]::Round($dropRate, 3))%" "< 1%"
}

if ([double]::IsNaN($LeagueBaselineMedianFps) -or [double]::IsNaN($LeagueCaptureMedianFps) -or $LeagueBaselineMedianFps -le 0) {
    Add-Gate "League median FPS degradation" "NOT_RUN" "Provide baseline and capture medians" "<= 3%"
} else {
    $degradation = (($LeagueBaselineMedianFps - $LeagueCaptureMedianFps) / $LeagueBaselineMedianFps) * 100.0
    Add-Gate "League median FPS degradation" $(if ($degradation -le 3) { "PASS" } else { "FAIL" }) "$([math]::Round($degradation, 2))%" "<= 3%"
}

$p95Bitrates = @($mediaRows | ForEach-Object { Convert-Number $_.p95BitrateMbps } | Where-Object { $null -ne $_ })
if ($p95Bitrates.Count -eq 0) { Add-Gate "Worst file p95 bitrate" "NOT_RUN" "No packet bitrate data" "<= 27.5 Mbps" }
else {
    $worstP95 = ($p95Bitrates | Measure-Object -Maximum).Maximum
    Add-Gate "Worst file p95 bitrate" $(if ($worstP95 -le 27.5) { "PASS" } else { "FAIL" }) "$([math]::Round($worstP95, 2)) Mbps" "<= 27.5 Mbps"
}

if ([double]::IsNaN($VmafScore)) { Add-Gate "VMAF" "NOT_RUN" "Provide measured VMAF" ">= 95" }
else { Add-Gate "VMAF" $(if ($VmafScore -ge 95) { "PASS" } else { "FAIL" }) "$VmafScore" ">= 95" }

if ([string]::IsNullOrWhiteSpace($ClipLatencyCsv)) {
    Add-Gate "Clip save p95 latency" "NOT_RUN" "Provide clip latency CSV with latencySeconds" "<= 5 seconds"
} else {
    $latencies = @(Import-Csv -LiteralPath $ClipLatencyCsv | ForEach-Object { Convert-Number $_.latencySeconds } | Where-Object { $null -ne $_ })
    $latencyP95 = Get-Percentile $latencies 0.95
    if ($null -eq $latencyP95) { Add-Gate "Clip save p95 latency" "NOT_RUN" "No numeric latencySeconds rows" "<= 5 seconds" }
    else { Add-Gate "Clip save p95 latency" $(if ($latencyP95 -le 5) { "PASS" } else { "FAIL" }) "$([math]::Round($latencyP95, 2)) seconds" "<= 5 seconds" }
}

$rssValues = @($processRows | ForEach-Object {
    $appProcesses = Convert-Number $_.lolshortsProcesses
    $ffmpegProcesses = Convert-Number $_.ffmpegProcesses
    $app = Convert-Number $_.lolshortsMemoryMiB
    $ffmpeg = Convert-Number $_.ffmpegMemoryMiB
    if (
        $null -ne $appProcesses -and $appProcesses -gt 0 -and
        $null -ne $ffmpegProcesses -and $ffmpegProcesses -gt 0 -and
        $null -ne $app -and $null -ne $ffmpeg
    ) { $app + $ffmpeg }
} | Where-Object { $null -ne $_ })
if ($rssValues.Count -eq 0) { Add-Gate "App + FFmpeg RSS" "NOT_RUN" "No concurrent app/FFmpeg samples" "<= 1126.4 MiB" }
else {
    $maxRss = ($rssValues | Measure-Object -Maximum).Maximum
    Add-Gate "App + FFmpeg RSS" $(if ($maxRss -le 1126.4) { "PASS" } else { "FAIL" }) "$([math]::Round($maxRss, 1)) MiB" "<= 1126.4 MiB"
}

if ($rssValues.Count -lt 12) { Add-Gate "Warm memory growth" "NOT_RUN" "Need at least 12 samples" "<= 15%" }
else {
    $window = [math]::Max(3, [math]::Floor($rssValues.Count * 0.1))
    $early = Get-Percentile @($rssValues | Select-Object -First $window) 0.5
    $late = Get-Percentile @($rssValues | Select-Object -Last $window) 0.5
    $growth = if ($early -gt 0) { (($late - $early) / $early) * 100.0 } else { [double]::PositiveInfinity }
    Add-Gate "Warm memory growth" $(if ($growth -le 15) { "PASS" } else { "FAIL" }) "$([math]::Round($growth, 2))%" "<= 15%"
}

$decodeFailures = @($mediaRows | Where-Object { $_.validation -eq "FAIL" -or $_.fullDecode -eq "FAIL" }).Count
Add-Gate "Decode errors" $(if ($decodeFailures -eq 0 -and $mediaRows.Count -gt 0) { "PASS" } elseif ($mediaRows.Count -eq 0) { "NOT_RUN" } else { "FAIL" }) "$decodeFailures failures across $($mediaRows.Count) files" "0"
Add-Gate "Black/freeze/encoder-restart inspection" $VisualInspection $VisualInspection "Pass"

$overall = if (@($Gates | Where-Object Status -eq "FAIL").Count -gt 0) { "FAIL" }
elseif (@($Gates | Where-Object Status -eq "NOT_RUN").Count -gt 0) { "INCOMPLETE" }
else { "PASS" }

if ([string]::IsNullOrWhiteSpace($OutputPath)) { $OutputPath = Join-Path $ResolvedRunDir "performance-gate.md" }
$report = @("# Gameplay Performance Gate", "", "- Overall: **$overall**", "", "| Gate | Status | Measured | Target |", "| --- | --- | --- | --- |")
foreach ($gate in $Gates) { $report += "| $($gate.Name) | $($gate.Status) | $($gate.Measured) | $($gate.Target) |" }
$report += @("", "`NOT_RUN` is intentionally non-passing. Complete the missing real-game or visual measurement before release signoff.")
Set-Content -LiteralPath $OutputPath -Value $report -Encoding utf8
Write-Host "Performance gate result: $overall"
Write-Host "Report written to: $OutputPath"
if ($overall -eq "FAIL") { exit 1 }
if ($overall -eq "INCOMPLETE") { exit 2 }
