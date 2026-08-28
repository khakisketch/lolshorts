[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$HostExecutable = (Get-Process -Id $PID).Path
$TempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$TestRoot = Join-Path $TempBase ("lolshorts-field-tools-test-" + [guid]::NewGuid().ToString("N"))

function Invoke-ScriptExpectExit {
    param(
        [Parameter(Mandatory = $true)][string]$ScriptPath,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][int]$ExpectedExit
    )

    $previousErrorActionPreference = $ErrorActionPreference
    try {
        # Windows PowerShell promotes native stderr to a NativeCommandError when
        # ErrorActionPreference is Stop. Expected fail-closed child exits must be
        # captured and asserted, not mistaken for a harness failure.
        $ErrorActionPreference = "Continue"
        & $HostExecutable -NoProfile -ExecutionPolicy Bypass -File $ScriptPath @Arguments *> $null
        $actualExit = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($actualExit -ne $ExpectedExit) {
        throw "$([System.IO.Path]::GetFileName($ScriptPath)) returned $actualExit; expected $ExpectedExit."
    }
}

function Write-ProcessFixture {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][bool]$ProcessesRunning
    )

    $rows = 1..12 | ForEach-Object {
        [pscustomobject]@{
            timestamp = "2026-08-21T00:00:$($_.ToString('00'))+09:00"
            captureMode = "desktop_duplication"
            lolshortsProcesses = $(if ($ProcessesRunning) { 1 } else { 0 })
            lolshortsMemoryMiB = $(if ($ProcessesRunning) { 256 } else { 0 })
            leagueProcesses = $(if ($ProcessesRunning) { 1 } else { 0 })
            leagueMemoryMiB = $(if ($ProcessesRunning) { 512 } else { 0 })
            ffmpegProcesses = $(if ($ProcessesRunning) { 1 } else { 0 })
            ffmpegMemoryMiB = $(if ($ProcessesRunning) { 640 } else { 0 })
            systemCpuPercent = 10
            nvidiaSmi = "yes"
            nvidiaGpuUtilPercent = 20
            nvidiaEncoderUtilPercent = 30
            nvidiaTemperatureC = 55
            nvidiaMemoryUsedMiB = 1024
            nvidiaMemoryTotalMiB = 8192
        }
    }
    $rows | Export-Csv -LiteralPath $Path -NoTypeInformation -Encoding utf8
}

function Assert-ReportContains {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Pattern
    )

    if (-not (Select-String -LiteralPath $Path -Pattern $Pattern -Quiet)) {
        throw "Expected pattern '$Pattern' in $Path."
    }
}

New-Item -ItemType Directory -Path $TestRoot -Force | Out-Null
$ResolvedTestRoot = [System.IO.Path]::GetFullPath((Resolve-Path -LiteralPath $TestRoot).Path)
$NormalizedTempBase = $TempBase.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if (
    -not $ResolvedTestRoot.StartsWith($NormalizedTempBase, [System.StringComparison]::OrdinalIgnoreCase) -or
    -not ([System.IO.Path]::GetFileName($ResolvedTestRoot)).StartsWith("lolshorts-field-tools-test-", [System.StringComparison]::Ordinal)
) {
    throw "Refusing to use an unexpected test directory: $ResolvedTestRoot"
}

try {
    $RunDir = Join-Path $ResolvedTestRoot "run"
    $MetricsDir = Join-Path $RunDir "03_capture_audio_gpu"
    New-Item -ItemType Directory -Path $MetricsDir -Force | Out-Null
    $ProcessCsv = Join-Path $MetricsDir "process-metrics.csv"
    $MediaCsv = Join-Path $MetricsDir "media-validation.csv"
    @([pscustomobject]@{
        validation = "ACTIVE"
        captureFps = ""
        estimatedDroppedFrames = ""
        frameCount = ""
        p95BitrateMbps = ""
        fullDecode = "ACTIVE"
    }) | Export-Csv -LiteralPath $MediaCsv -NoTypeInformation -Encoding utf8

    Write-ProcessFixture -Path $ProcessCsv -ProcessesRunning $false
    Invoke-ScriptExpectExit -ScriptPath (Join-Path $PSScriptRoot "evaluate-gameplay-field-evidence.ps1") -Arguments @("-RunDir", $RunDir) -ExpectedExit 2
    $PerformanceReport = Join-Path $RunDir "performance-gate.md"
    Assert-ReportContains -Path $PerformanceReport -Pattern '\| App \+ FFmpeg RSS \| NOT_RUN \| No concurrent app/FFmpeg samples \|'

    Write-ProcessFixture -Path $ProcessCsv -ProcessesRunning $true
    Invoke-ScriptExpectExit -ScriptPath (Join-Path $PSScriptRoot "evaluate-gameplay-field-evidence.ps1") -Arguments @("-RunDir", $RunDir) -ExpectedExit 2
    Assert-ReportContains -Path $PerformanceReport -Pattern '\| App \+ FFmpeg RSS \| PASS \| 896 MiB \|'
    Assert-ReportContains -Path $PerformanceReport -Pattern '\| Warm memory growth \| PASS \| 0% \|'

    $LabelsCsv = Join-Path $ResolvedTestRoot "highlight-labels.csv"
    $labelRows = 1..30 | ForEach-Object {
        [pscustomobject]@{
            keepWorthy = $(if ($_ -le 21) { "yes" } else { "no" })
            duplicateGroup = ""
            missingLeadIn = "no"
            excessiveTail = "no"
            eventMisclassified = $(if ($_ -le 3) { "yes" } else { "no" })
            videoIssue = "no"
            audioIssue = "no"
        }
    }
    @($labelRows | Select-Object -First 29) | Export-Csv -LiteralPath $LabelsCsv -NoTypeInformation -Encoding utf8
    Invoke-ScriptExpectExit -ScriptPath (Join-Path $PSScriptRoot "analyze-highlight-labels.ps1") -Arguments @("-InputCsv", $LabelsCsv) -ExpectedExit 1

    $labelRows | Export-Csv -LiteralPath $LabelsCsv -NoTypeInformation -Encoding utf8
    Invoke-ScriptExpectExit -ScriptPath (Join-Path $PSScriptRoot "analyze-highlight-labels.ps1") -Arguments @("-InputCsv", $LabelsCsv) -ExpectedExit 0
    $LabelReport = [System.IO.Path]::ChangeExtension($LabelsCsv, ".summary.md")
    Assert-ReportContains -Path $LabelReport -Pattern 'Overall quality gate: \*\*PASS\*\*'
    Assert-ReportContains -Path $LabelReport -Pattern 'Event misclassification: 3 clips'

    Write-Host "Field evidence tool regression PASS"
} finally {
    if (Test-Path -LiteralPath $ResolvedTestRoot) {
        Remove-Item -LiteralPath $ResolvedTestRoot -Recurse -Force
    }
}
