<#
Prepare the Windows FFmpeg/ffprobe executables required by Tauri externalBin.

Development prefers already-installed, executable tools. Release automation can
request the immutable, checksum-pinned BtbN archive with `-Source Download`.
The output path is resolved from this script, never from the caller's current
directory.
#>
[CmdletBinding()]
param(
    [ValidateSet("Auto", "System", "Download")]
    [string]$Source = "Auto",

    [switch]$Force,

    [string]$FfmpegPath = "",

    [string]$FfprobePath = "",

    [string]$DownloadUrl = "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-20-13-45/ffmpeg-N-126229-gf101fce22d-win64-gpl.zip",

    [ValidatePattern("^[0-9a-fA-F]{64}$")]
    [string]$ExpectedSha256 = "c4e072ab7d22f9bfddfedc0acd3c0613120475345b51a6a245d42faa05a7349b"
)

$ErrorActionPreference = "Stop"
$BuildScriptsDir = $PSScriptRoot
$TauriDir = Split-Path -Parent $BuildScriptsDir
$BinDir = Join-Path $TauriDir "binaries"
$FfmpegTarget = Join-Path $BinDir "ffmpeg-x86_64-pc-windows-msvc.exe"
$FfprobeTarget = Join-Path $BinDir "ffprobe-x86_64-pc-windows-msvc.exe"

function Test-MediaTool {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][ValidateSet("ffmpeg", "ffprobe")][string]$Name,
        [switch]$Quiet
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    try {
        $firstLine = (& $Path -version 2>$null | Select-Object -First 1)
        # Capturing the first pipeline item can leave LASTEXITCODE unset on
        # Windows PowerShell even though the executable succeeded. A valid
        # tool-identifying version line is the portable contract we need here.
        $valid = "$firstLine".StartsWith("$Name version")
        if ($valid -and -not $Quiet) { Write-Host "Validated $firstLine" }
        return $valid
    } catch {
        return $false
    }
}

function Get-MediaToolVersionToken {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][ValidateSet("ffmpeg", "ffprobe")][string]$Name
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
    try {
        $firstLine = (& $Path -version 2>$null | Select-Object -First 1)
        $match = [regex]::Match("$firstLine", "^$Name version ([^\s]+)")
        if ($match.Success) { return $match.Groups[1].Value }
    } catch {
        return $null
    }
    return $null
}

function Test-CompatibleMediaPair {
    param(
        [Parameter(Mandatory = $true)][string]$FfmpegPath,
        [Parameter(Mandatory = $true)][string]$FfprobePath
    )

    $ffmpegVersion = Get-MediaToolVersionToken -Path $FfmpegPath -Name "ffmpeg"
    $ffprobeVersion = Get-MediaToolVersionToken -Path $FfprobePath -Name "ffprobe"
    return $ffmpegVersion -and $ffprobeVersion -and
        [System.StringComparer]::OrdinalIgnoreCase.Equals($ffmpegVersion, $ffprobeVersion)
}

function Resolve-SystemMediaTool {
    param(
        [Parameter(Mandatory = $true)][ValidateSet("ffmpeg", "ffprobe")][string]$Name,
        [string]$ExplicitPath = ""
    )

    $candidates = New-Object System.Collections.Generic.List[string]
    if (-not [string]::IsNullOrWhiteSpace($ExplicitPath)) { $candidates.Add($ExplicitPath) }

    $environmentName = "LOLSHORTS_$($Name.ToUpperInvariant())_PATH"
    $environmentPath = [Environment]::GetEnvironmentVariable($environmentName)
    if (-not [string]::IsNullOrWhiteSpace($environmentPath)) { $candidates.Add($environmentPath) }

    $command = Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($command) { $candidates.Add($command.Source) }

    # Chocolatey exposes small shim executables on PATH. Prefer the real static
    # executable inside the package when it is available.
    $chocolateyRoot = [Environment]::GetEnvironmentVariable("ChocolateyInstall")
    if ([string]::IsNullOrWhiteSpace($chocolateyRoot)) { $chocolateyRoot = "C:\ProgramData\chocolatey" }
    $chocolateyLib = Join-Path $chocolateyRoot "lib"
    if (Test-Path -LiteralPath $chocolateyLib -PathType Container) {
        Get-ChildItem -LiteralPath $chocolateyLib -Recurse -Filter "$Name.exe" -File -ErrorAction SilentlyContinue |
            Sort-Object Length -Descending |
            ForEach-Object { $candidates.Add($_.FullName) }
    }

    foreach ($candidate in @($candidates | Select-Object -Unique)) {
        if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) { continue }
        # Exclude package-manager shims; a copied shim is not a self-contained
        # Tauri sidecar even if it works in its original installation directory.
        if ((Get-Item -LiteralPath $candidate).Length -lt 1MB) { continue }
        if (Test-MediaTool -Path $candidate -Name $Name -Quiet) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }
    return $null
}

function Install-Sidecars {
    param(
        [Parameter(Mandatory = $true)][string]$FfmpegSource,
        [Parameter(Mandatory = $true)][string]$FfprobeSource
    )

    if (-not (Test-MediaTool -Path $FfmpegSource -Name "ffmpeg" -Quiet)) {
        throw "FFmpeg source failed executable validation."
    }
    if (-not (Test-MediaTool -Path $FfprobeSource -Name "ffprobe" -Quiet)) {
        throw "ffprobe source failed executable validation."
    }
    if (-not (Test-CompatibleMediaPair -FfmpegPath $FfmpegSource -FfprobePath $FfprobeSource)) {
        throw "FFmpeg and ffprobe must come from the same versioned build."
    }

    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
    Copy-Item -LiteralPath $FfmpegSource -Destination $FfmpegTarget -Force
    Copy-Item -LiteralPath $FfprobeSource -Destination $FfprobeTarget -Force

    if (-not (Test-MediaTool -Path $FfmpegTarget -Name "ffmpeg")) {
        throw "Prepared FFmpeg sidecar failed validation."
    }
    if (-not (Test-MediaTool -Path $FfprobeTarget -Name "ffprobe")) {
        throw "Prepared ffprobe sidecar failed validation."
    }
}

$existingValid = (Test-MediaTool -Path $FfmpegTarget -Name "ffmpeg" -Quiet) -and
    (Test-MediaTool -Path $FfprobeTarget -Name "ffprobe" -Quiet) -and
    (Test-CompatibleMediaPair -FfmpegPath $FfmpegTarget -FfprobePath $FfprobeTarget)
if ($existingValid -and -not $Force) {
    Write-Host "FFmpeg sidecars are already present and valid."
    exit 0
}

if ($Source -in @("Auto", "System")) {
    $systemFfmpeg = Resolve-SystemMediaTool -Name "ffmpeg" -ExplicitPath $FfmpegPath
    $systemFfprobe = Resolve-SystemMediaTool -Name "ffprobe" -ExplicitPath $FfprobePath
    if ($systemFfmpeg -and $systemFfprobe -and
        (Test-CompatibleMediaPair -FfmpegPath $systemFfmpeg -FfprobePath $systemFfprobe)) {
        Install-Sidecars -FfmpegSource $systemFfmpeg -FfprobeSource $systemFfprobe
        Write-Host "Prepared FFmpeg sidecars from validated system tools."
        exit 0
    }
    if ($Source -eq "System") {
        throw "A matching, validated system FFmpeg/ffprobe pair was not found. Set LOLSHORTS_FFMPEG_PATH and LOLSHORTS_FFPROBE_PATH, or use -Source Download."
    }
}

if ($DownloadUrl -notmatch '^https://github\.com/BtbN/FFmpeg-Builds/releases/download/') {
    throw "FFmpeg download must use the approved BtbN GitHub release origin."
}

$TempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd(
    [System.IO.Path]::DirectorySeparatorChar,
    [System.IO.Path]::AltDirectorySeparatorChar
)
$TempLeaf = "lolshorts-ffmpeg-" + [guid]::NewGuid().ToString("N")
$TempRoot = [System.IO.Path]::GetFullPath((Join-Path $TempBase $TempLeaf))
$tempParentIsExpected = [System.StringComparer]::OrdinalIgnoreCase.Equals(
    [System.IO.Path]::GetDirectoryName($TempRoot),
    $TempBase
)
if (-not $tempParentIsExpected -or $TempLeaf -notmatch '^lolshorts-ffmpeg-[0-9a-f]{32}$') {
    throw "Refusing to use an unexpected FFmpeg temporary directory."
}
New-Item -ItemType Directory -Path $TempRoot -Force | Out-Null
try {
    $ArchivePath = Join-Path $TempRoot "ffmpeg.zip"
    $ExtractDir = Join-Path $TempRoot "extract"
    Write-Host "Downloading checksum-pinned FFmpeg archive..."
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ArchivePath -UseBasicParsing
    $actualSha256 = (Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualSha256 -ne $ExpectedSha256.ToLowerInvariant()) {
        throw "FFmpeg archive SHA-256 mismatch. Expected $ExpectedSha256, got $actualSha256."
    }

    Expand-Archive -LiteralPath $ArchivePath -DestinationPath $ExtractDir -Force
    $downloadedFfmpeg = Get-ChildItem -LiteralPath $ExtractDir -Recurse -Filter "ffmpeg.exe" -File | Select-Object -First 1
    $downloadedFfprobe = Get-ChildItem -LiteralPath $ExtractDir -Recurse -Filter "ffprobe.exe" -File | Select-Object -First 1
    if (-not $downloadedFfmpeg -or -not $downloadedFfprobe) {
        throw "The verified FFmpeg archive did not contain both ffmpeg.exe and ffprobe.exe."
    }

    Install-Sidecars -FfmpegSource $downloadedFfmpeg.FullName -FfprobeSource $downloadedFfprobe.FullName
    Write-Host "Prepared FFmpeg sidecars from the pinned BtbN release."
    # Keep the download path's process exit code explicit. Windows PowerShell
    # can otherwise inherit a non-zero native-tool status from validation even
    # after both sidecars were copied and verified successfully.
    exit 0
} finally {
    if (Test-Path -LiteralPath $TempRoot) {
        Remove-Item -LiteralPath $TempRoot -Recurse -Force
    }
}
