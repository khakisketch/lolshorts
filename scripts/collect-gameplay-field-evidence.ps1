<#
Collect privacy-safe, supporting evidence during a real gameplay capture run.
This script deliberately avoids raw LCU payloads, player identifiers, paths,
logs, command lines, and audio device names.
#>
[CmdletBinding()]
param(
    [ValidateRange(5, 10800)]
    [int]$DurationSeconds = 60,

    [ValidateRange(1, 300)]
    [int]$SampleIntervalSeconds = 5,

    [ValidateRange(1, 100)]
    [int]$MaxMediaFiles = 25,

    [ValidateSet("desktop_duplication", "gdi_fallback", "unspecified")]
    [string]$CaptureMode = "unspecified",

    [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot

function Get-SafeCommandVersion {
    param([string]$CommandPath)
    if ([string]::IsNullOrWhiteSpace($CommandPath)) { return "unavailable" }
    try {
        $line = (& $CommandPath -version 2>$null | Select-Object -First 1)
        if ([string]::IsNullOrWhiteSpace($line)) { return "usable (version unavailable)" }
        return ($line -replace '[\r\n]+', ' ').Trim()
    } catch { return "unusable" }
}

function Get-CommandPath {
    param([string[]]$Candidates, [string]$FallbackCommand = "")
    foreach ($candidate in $Candidates) {
        if (-not [string]::IsNullOrWhiteSpace($candidate) -and (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }
    if ([string]::IsNullOrWhiteSpace($FallbackCommand)) { return $null }
    $command = Get-Command $FallbackCommand -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    return $null
}

function Get-ProcessFootprint {
    param([string[]]$NamePrefixes)
    $processes = @(Get-Process -ErrorAction SilentlyContinue)
    $matching = @($processes | Where-Object {
        $processName = $_.ProcessName
        @($NamePrefixes | Where-Object { $processName -like "$($_)*" }).Count -gt 0
    })
    return [pscustomobject]@{
        count = $matching.Count
        memoryMiB = [math]::Round((($matching | Measure-Object -Property WorkingSet64 -Sum).Sum / 1MB), 1)
    }
}

function Get-LiveClientSample {
    $result = [ordered]@{ reachable = "no"; gameMode = ""; gameTimeSeconds = "" }
    try {
        # The endpoint is intentionally reduced to these two non-identifying fields.
        $game = Invoke-RestMethod -Uri "https://127.0.0.1:2999/liveclientdata/gamestats" -SkipCertificateCheck -TimeoutSec 2 -ErrorAction Stop
        $result.reachable = "yes"
        $result.gameMode = if ($null -ne $game.gameMode) { [string]$game.gameMode } else { "unknown" }
        $result.gameTimeSeconds = if ($null -ne $game.gameTime) { [math]::Round([double]$game.gameTime, 1) } else { "unknown" }
    } catch { }
    return [pscustomobject]$result
}

function Get-NvidiaMetrics {
    $smi = Get-Command "nvidia-smi" -ErrorAction SilentlyContinue
    if (-not $smi) { return [pscustomobject]@{ available = "no"; gpuUtilPercent = ""; encoderUtilPercent = ""; temperatureC = ""; memoryUsedMiB = ""; memoryTotalMiB = "" } }
    try {
        $rows = @(& $smi.Source "--query-gpu=utilization.gpu,utilization.encoder,temperature.gpu,memory.used,memory.total" "--format=csv,noheader,nounits" 2>$null)
        $first = $rows | Select-Object -First 1
        if ($first -match '^\s*([^,]+),\s*([^,]+),\s*([^,]+),\s*([^,]+),\s*([^,]+)') {
            return [pscustomobject]@{ available = "yes"; gpuUtilPercent = $Matches[1].Trim(); encoderUtilPercent = $Matches[2].Trim(); temperatureC = $Matches[3].Trim(); memoryUsedMiB = $Matches[4].Trim(); memoryTotalMiB = $Matches[5].Trim() }
        }
    } catch { }
    return [pscustomobject]@{ available = "yes"; gpuUtilPercent = "unavailable"; encoderUtilPercent = "unavailable"; temperatureC = "unavailable"; memoryUsedMiB = "unavailable"; memoryTotalMiB = "unavailable" }
}

function Add-Observation {
    param([string]$Status, [string]$Observation)
    $script:Observations.Add([pscustomobject]@{ Status = $Status; Observation = $Observation }) | Out-Null
}

function Write-CsvSafe {
    param([object[]]$Rows, [string]$Path, [string[]]$Headers)
    if ($Rows.Count -gt 0) { $Rows | Select-Object $Headers | Export-Csv -NoTypeInformation -Encoding utf8 -Path $Path }
    else { ($Headers -join ',') | Set-Content -Encoding utf8 -Path $Path }
}

function Convert-FrameRate {
    param([string]$Ratio)
    if ([string]::IsNullOrWhiteSpace($Ratio)) { return $null }
    $parts = $Ratio.Split('/')
    if ($parts.Count -eq 2 -and [double]$parts[1] -ne 0) {
        return [double]$parts[0] / [double]$parts[1]
    }
    $value = 0.0
    if ([double]::TryParse($Ratio, [ref]$value)) { return $value }
    return $null
}

function Get-ConfiguredVideoQuality {
    $quality = [ordered]@{ bitrateMbps = 20; fps = 60; source = "defaults" }
    $settingsCandidates = @(
        (Join-Path $env:LOCALAPPDATA "lolshorts\settings.json"),
        (Join-Path $env:APPDATA "lolshorts\settings.json")
    )
    $settingsPath = $settingsCandidates | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) } | Select-Object -First 1
    if (-not $settingsPath) { return [pscustomobject]$quality }
    try {
        $settings = Get-Content -Raw -LiteralPath $settingsPath | ConvertFrom-Json
        $bitrateMap = @{ low = 10; medium = 20; high = 40; very_high = 80 }
        $fpsMap = @{ fps30 = 30; fps60 = 60; fps120 = 120; fps144 = 144 }
        $preset = [string]$settings.video.bitrate_preset
        $frameRate = [string]$settings.video.frame_rate
        if ($bitrateMap.ContainsKey($preset)) { $quality.bitrateMbps = $bitrateMap[$preset] }
        if ($fpsMap.ContainsKey($frameRate)) { $quality.fps = $fpsMap[$frameRate] }
        $quality.source = "saved settings"
    } catch { }
    return [pscustomobject]$quality
}

function Get-VideoBitrateDistribution {
    param([string]$FfprobePath, [string]$MediaPath)
    $result = [ordered]@{ p95Mbps = ""; maxMbps = "" }
    try {
        $json = & $FfprobePath -v error -select_streams v:0 -show_packets -show_entries "packet=pts_time,size" -of json -- $MediaPath 2>$null | Out-String
        $packets = @((($json | ConvertFrom-Json).packets))
        $bins = @{}
        foreach ($packet in $packets) {
            if ($null -eq $packet.pts_time -or $null -eq $packet.size) { continue }
            $second = [math]::Floor([double]$packet.pts_time)
            if (-not $bins.ContainsKey($second)) { $bins[$second] = 0.0 }
            $bins[$second] += [double]$packet.size * 8.0 / 1000000.0
        }
        $values = @($bins.Values | Sort-Object)
        if ($values.Count -gt 0) {
            $p95Index = [math]::Max(0, [math]::Ceiling($values.Count * 0.95) - 1)
            $result.p95Mbps = [math]::Round([double]$values[$p95Index], 2)
            $result.maxMbps = [math]::Round([double]$values[-1], 2)
        }
    } catch { }
    return [pscustomobject]$result
}

if ([string]::IsNullOrWhiteSpace($OutputDir)) { $OutputDir = Join-Path $ProjectRoot "field-qa" }
$package = Get-Content -Raw -LiteralPath (Join-Path $ProjectRoot "package.json") | ConvertFrom-Json
$version = ($package.version -replace '[^A-Za-z0-9_.-]', '-')
$commit = (& git -C $ProjectRoot rev-parse --short HEAD 2>$null).Trim()
if ([string]::IsNullOrWhiteSpace($commit)) { $commit = "nogit" }
$dirtyLines = @(& git -C $ProjectRoot status --porcelain 2>$null)
$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$RunDir = Join-Path $OutputDir "$version-$commit-$timestamp"
$lcuDir = Join-Path $RunDir "01_lol_lcu"
$metricsDir = Join-Path $RunDir "03_capture_audio_gpu"
New-Item -ItemType Directory -Force -Path $lcuDir, $metricsDir | Out-Null
$Observations = New-Object System.Collections.Generic.List[object]

$binariesDir = Join-Path $ProjectRoot "src-tauri\binaries"
$ffmpegCandidates = @(
    (Join-Path $binariesDir "ffmpeg-x86_64-pc-windows-msvc.exe"),
    (Join-Path $binariesDir "ffmpeg.exe"),
    (Join-Path $binariesDir "ffmpeg")
) + @(Get-ChildItem -LiteralPath $binariesDir -Filter "ffmpeg*.exe" -File -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName)
$ffprobeCandidates = @(
    (Join-Path $binariesDir "ffprobe-x86_64-pc-windows-msvc.exe"),
    (Join-Path $binariesDir "ffprobe.exe"),
    (Join-Path $binariesDir "ffprobe")
) + @(Get-ChildItem -LiteralPath $binariesDir -Filter "ffprobe*.exe" -File -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName)
$ffmpegPath = Get-CommandPath -Candidates $ffmpegCandidates -FallbackCommand "ffmpeg"
$ffprobePath = Get-CommandPath -Candidates $ffprobeCandidates -FallbackCommand "ffprobe"
$ffmpegVersion = Get-SafeCommandVersion -CommandPath $ffmpegPath
$ffprobeVersion = Get-SafeCommandVersion -CommandPath $ffprobePath
$configuredQuality = Get-ConfiguredVideoQuality

$os = Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue
$cpu = Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1
$gpus = @(Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Name) (driver $($_.DriverVersion))" })
$disk = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='C:'" -ErrorAction SilentlyContinue
$lolCandidates = @(
    "C:\Riot Games\League of Legends\LeagueClient.exe",
    "C:\Program Files\Riot Games\League of Legends\LeagueClient.exe"
)
$lolPath = $lolCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
$lolVersion = if ($lolPath) { (Get-Item -LiteralPath $lolPath).VersionInfo.ProductVersion } else { "not found" }
$audioDeviceCount = @(Get-CimInstance Win32_SoundDevice -ErrorAction SilentlyContinue).Count

$environment = @(
    "# Gameplay Field Evidence Environment",
    "",
    "- Package version: $version",
    "- Git commit: $commit",
    "- Dirty worktree: $(if ($dirtyLines.Count -gt 0) { 'yes' } else { 'no' }) ($($dirtyLines.Count) entries)",
    "- Requested capture evidence mode: $CaptureMode",
    "- Sampling duration: $DurationSeconds seconds",
    "- OS: $($os.Caption) $($os.Version)",
    "- CPU: $($cpu.Name)",
    "- Installed RAM GiB: $([math]::Round($os.TotalVisibleMemorySize / 1MB, 1))",
    "- GPU(s): $($gpus -join '; ')",
    "- C: free GiB: $([math]::Round($disk.FreeSpace / 1GB, 1))",
    "- League client installed: $(if ($lolPath) { 'yes' } else { 'no' })",
    "- League client version: $lolVersion",
    "- Bundled ffmpeg: $ffmpegVersion",
    "- Bundled or system ffprobe: $ffprobeVersion",
    "- Configured video bitrate: $($configuredQuality.bitrateMbps) Mbps ($($configuredQuality.source))",
    "- Configured capture FPS: $($configuredQuality.fps) ($($configuredQuality.source))",
    "- Audio devices detected (count only): $audioDeviceCount",
    "",
    "Privacy: no usernames, audio-device names, filesystem paths, command lines, LCU payloads, or secrets are recorded."
)
Set-Content -LiteralPath (Join-Path $RunDir "00_environment.md") -Value $environment -Encoding utf8

if ($lolPath) { Add-Observation "PASS" "League client installation detected." } else { Add-Observation "WARN" "League client installation was not found in standard locations." }
if ($ffmpegVersion -notmatch "^(unavailable|unusable)$") { Add-Observation "PASS" "ffmpeg is usable for capture-sidecar evidence." } else { Add-Observation "WARN" "ffmpeg was unavailable or unusable." }
if ($ffprobeVersion -notmatch "^(unavailable|unusable)$") { Add-Observation "PASS" "ffprobe is usable for media validation." } else { Add-Observation "WARN" "ffprobe was unavailable or unusable; media validation will be limited." }

$liveRows = New-Object System.Collections.Generic.List[object]
$metricRows = New-Object System.Collections.Generic.List[object]
$endAt = (Get-Date).AddSeconds($DurationSeconds)
do {
    $now = Get-Date -Format "o"
    $live = Get-LiveClientSample
    $liveRows.Add([pscustomobject]@{ timestamp = $now; reachable = $live.reachable; gameMode = $live.gameMode; gameTimeSeconds = $live.gameTimeSeconds }) | Out-Null
    $systemCpu = (Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Measure-Object -Property LoadPercentage -Average).Average
    $nvidia = Get-NvidiaMetrics
    $lolshorts = Get-ProcessFootprint @("lolshorts")
    $league = Get-ProcessFootprint @("LeagueClient", "League of Legends")
    $ffmpeg = Get-ProcessFootprint @("ffmpeg")
    $metricRows.Add([pscustomobject]@{
        timestamp = $now; captureMode = $CaptureMode; lolshortsProcesses = $lolshorts.count; lolshortsMemoryMiB = $lolshorts.memoryMiB; leagueProcesses = $league.count; leagueMemoryMiB = $league.memoryMiB; ffmpegProcesses = $ffmpeg.count; ffmpegMemoryMiB = $ffmpeg.memoryMiB; systemCpuPercent = $systemCpu; nvidiaSmi = $nvidia.available; nvidiaGpuUtilPercent = $nvidia.gpuUtilPercent; nvidiaEncoderUtilPercent = $nvidia.encoderUtilPercent; nvidiaTemperatureC = $nvidia.temperatureC; nvidiaMemoryUsedMiB = $nvidia.memoryUsedMiB; nvidiaMemoryTotalMiB = $nvidia.memoryTotalMiB
    }) | Out-Null
    if ((Get-Date) -lt $endAt) { Start-Sleep -Seconds ([math]::Min($SampleIntervalSeconds, [math]::Max(1, [int][math]::Ceiling(($endAt - (Get-Date)).TotalSeconds)))) }
} while ((Get-Date) -lt $endAt)

Write-CsvSafe -Rows $liveRows.ToArray() -Path (Join-Path $lcuDir "live-samples.csv") -Headers @("timestamp", "reachable", "gameMode", "gameTimeSeconds")
Write-CsvSafe -Rows $metricRows.ToArray() -Path (Join-Path $metricsDir "process-metrics.csv") -Headers @("timestamp", "captureMode", "lolshortsProcesses", "lolshortsMemoryMiB", "leagueProcesses", "leagueMemoryMiB", "ffmpegProcesses", "ffmpegMemoryMiB", "systemCpuPercent", "nvidiaSmi", "nvidiaGpuUtilPercent", "nvidiaEncoderUtilPercent", "nvidiaTemperatureC", "nvidiaMemoryUsedMiB", "nvidiaMemoryTotalMiB")
if (@($liveRows | Where-Object reachable -eq "yes").Count -gt 0) { Add-Observation "PASS" "Live Client gamestats endpoint was reachable during sampling." } else { Add-Observation "WARN" "Live Client gamestats endpoint was not reachable; start a practice or live game for LCU evidence." }
if (@($metricRows | Where-Object { $_.ffmpegProcesses -gt 0 }).Count -gt 0) { Add-Observation "PASS" "ffmpeg process was observed during sampling." } else { Add-Observation "WARN" "ffmpeg process was not observed during sampling." }

$mediaRows = New-Object System.Collections.Generic.List[object]
$mediaRoots = @((Join-Path $env:LOCALAPPDATA "lolshorts"), (Join-Path $env:APPDATA "lolshorts")) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
$recentMedia = @(foreach ($root in $mediaRoots) { Get-ChildItem -LiteralPath $root -Filter *.mp4 -File -Recurse -ErrorAction SilentlyContinue | Where-Object { $_.LastWriteTime -gt (Get-Date).AddHours(-24) } }) | Sort-Object LastWriteTime -Descending | Select-Object -First $MaxMediaFiles
foreach ($media in @($recentMedia)) {
    $row = [ordered]@{ basename = $media.Name; durationSeconds = ""; sizeBytes = $media.Length; videoCodec = ""; resolution = ""; configuredBitrateMbps = $configuredQuality.bitrateMbps; averageBitrateMbps = ""; p95BitrateMbps = ""; maxBitrateMbps = ""; sizeMiBPerMinute = ""; configuredFps = $configuredQuality.fps; captureFps = ""; frameDrops = ""; audioStartOffsetMs = ""; audioCodec = ""; validation = "WARN"; fullDecode = "NOT_RUN" }
    $isActiveSegment = ($media.Directory.Name -eq "segments" -and $media.Name -like "segment_*.mp4")
    if ($media.Length -le 0 -and $isActiveSegment) { $row.validation = "ACTIVE" }
    elseif ($media.Length -le 0) { $row.validation = "FAIL" }
    elseif ($ffprobePath) {
        try {
            $probeJson = & $ffprobePath -v error -count_frames -show_entries "format=duration:stream=codec_type,codec_name,width,height,avg_frame_rate,nb_read_frames,start_time" -of json -- $media.FullName 2>$null | Out-String
            $probe = $probeJson | ConvertFrom-Json
            $video = @($probe.streams | Where-Object codec_type -eq "video" | Select-Object -First 1)[0]
            $audio = @($probe.streams | Where-Object codec_type -eq "audio" | Select-Object -First 1)[0]
            $duration = [double]$probe.format.duration
            $frames = if ($video.nb_read_frames -match '^\d+$') { [double]$video.nb_read_frames } else { 0.0 }
            $streamFps = Convert-FrameRate ([string]$video.avg_frame_rate)
            $captureFps = if ($duration -gt 0 -and $frames -gt 0) { $frames / $duration } else { $streamFps }
            $row.durationSeconds = [math]::Round($duration, 2)
            $row.videoCodec = $video.codec_name
            $row.resolution = "$($video.width)x$($video.height)"
            $row.averageBitrateMbps = if ($duration -gt 0) { [math]::Round(($media.Length * 8.0 / $duration) / 1000000.0, 2) } else { "" }
            $distribution = Get-VideoBitrateDistribution -FfprobePath $ffprobePath -MediaPath $media.FullName
            $row.p95BitrateMbps = $distribution.p95Mbps
            $row.maxBitrateMbps = $distribution.maxMbps
            $row.sizeMiBPerMinute = if ($duration -gt 0) { [math]::Round(($media.Length / 1MB) * (60.0 / $duration), 2) } else { "" }
            $row.captureFps = if ($null -ne $captureFps) { [math]::Round([double]$captureFps, 2) } else { "" }
            $row.frameDrops = if ($frames -gt 0 -and $duration -gt 0) { [math]::Max(0, [math]::Round($duration * $configuredQuality.fps - $frames)) } else { "" }
            $row.audioStartOffsetMs = if ($audio -and $null -ne $audio.start_time -and $null -ne $video.start_time) { [math]::Round(([double]$audio.start_time - [double]$video.start_time) * 1000.0, 1) } else { "" }
            $row.audioCodec = $audio.codec_name
            $row.validation = if ($video -and $probe.format.duration -gt 0) { "PASS" } else { "WARN" }
            if (-not $isActiveSegment -and $ffmpegPath) {
                & $ffmpegPath -v error -xerror -i $media.FullName -map 0:v:0 -f null NUL 2>$null
                $row.fullDecode = if ($LASTEXITCODE -eq 0) { "PASS" } else { "FAIL" }
                if ($row.fullDecode -eq "FAIL") { $row.validation = "FAIL" }
            }
        } catch { $row.validation = "WARN" }
    }
    $mediaRows.Add([pscustomobject]$row) | Out-Null
}
Write-CsvSafe -Rows $mediaRows.ToArray() -Path (Join-Path $metricsDir "media-validation.csv") -Headers @("basename", "durationSeconds", "sizeBytes", "videoCodec", "resolution", "configuredBitrateMbps", "averageBitrateMbps", "p95BitrateMbps", "maxBitrateMbps", "sizeMiBPerMinute", "configuredFps", "captureFps", "frameDrops", "audioStartOffsetMs", "audioCodec", "validation", "fullDecode")

$labelRows = @($recentMedia | Where-Object { $_.Directory.Name -ne "segments" } | ForEach-Object {
    [pscustomobject]@{ basename = $_.Name; gameGroup = ""; keepWorthy = ""; duplicateGroup = ""; missingLeadIn = ""; excessiveTail = ""; eventMisclassified = ""; videoIssue = ""; audioIssue = ""; notes = "" }
})
Write-CsvSafe -Rows $labelRows -Path (Join-Path $metricsDir "highlight-labels.csv") -Headers @("basename", "gameGroup", "keepWorthy", "duplicateGroup", "missingLeadIn", "excessiveTail", "eventMisclassified", "videoIssue", "audioIssue", "notes")
if ($mediaRows.Count -eq 0) { Add-Observation "WARN" "No MP4 written in the last 24 hours was found in approved local scan roots." }
elseif (@($mediaRows | Where-Object validation -eq "FAIL").Count -gt 0) { Add-Observation "WARN" "One or more recent MP4s failed size, metadata, or full-decode validation." }
else { Add-Observation "PASS" "Recent MP4 metadata was collected without retaining paths or media contents." }

$summary = @("# Gameplay Field Evidence Summary", "", "Supporting evidence only; this run is not E5 signoff and does not replace human QA.", "", "## Observations", "")
foreach ($item in $Observations) { $summary += "- **$($item.Status)**: $($item.Observation)" }
$summary += @("", "## Limits", "", "- LCU sampling retains only reachability, game mode, and game time; it never saves allgamedata, player names, or raw responses.", "- Process sampling retains process counts and aggregate working-set memory only; it never saves process paths or command lines.", "- Audio reporting is device count only; names and identifiers are intentionally excluded.", "- MP4 validation retains basename and technical metadata only; no file paths, logs, or media are copied.")
Set-Content -LiteralPath (Join-Path $RunDir "summary.md") -Value $summary -Encoding utf8
Write-Host "Supporting evidence written to: $RunDir"
