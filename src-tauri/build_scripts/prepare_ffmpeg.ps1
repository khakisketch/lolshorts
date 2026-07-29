# LoLShorts FFmpeg Preparation Script
# This script downloads and prepares FFmpeg for bundling with the installer
# Run this before building the production installer

$ErrorActionPreference = "Stop"

# Configuration
$FFMPEG_VERSION = "7.1"
$DOWNLOAD_URL = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"
$TEMP_DIR = ".\temp_ffmpeg"
$BIN_DIR = "..\binaries"

Write-Host "🎬 LoLShorts FFmpeg Preparation" -ForegroundColor Cyan
Write-Host "=================================" -ForegroundColor Cyan
Write-Host ""

# Create bin directory if it doesn't exist
if (-not (Test-Path $BIN_DIR)) {
    Write-Host "📁 Creating bin directory..." -ForegroundColor Yellow
    New-Item -ItemType Directory -Path $BIN_DIR | Out-Null
}

# Target filenames for Tauri Sidecar
$ffmpegTargetName = "ffmpeg-x86_64-pc-windows-msvc.exe"
$ffprobeTargetName = "ffprobe-x86_64-pc-windows-msvc.exe"

# Check if FFmpeg already exists
$ffmpegPath = Join-Path $BIN_DIR $ffmpegTargetName
$ffprobePath = Join-Path $BIN_DIR $ffprobeTargetName
$forceDownload = ($env:LOLSHORTS_FORCE_FFMPEG_DOWNLOAD -eq "1") -or ($env:FORCE_FFMPEG_DOWNLOAD -eq "1")

function Test-ToolVersion {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ToolName
    )

    if (-not (Test-Path $Path)) {
        return $false
    }

    try {
        $versionOutput = & $Path -version 2>&1
        $firstLine = $versionOutput | Select-Object -First 1
        if ($LASTEXITCODE -eq 0 -and "$firstLine".StartsWith("$ToolName version")) {
            Write-Host "📦 Current $ToolName version: $firstLine" -ForegroundColor Cyan
            return $true
        }

        Write-Host "⚠️ Existing $ToolName binary failed version validation: $firstLine" -ForegroundColor Yellow
        return $false
    } catch {
        Write-Host "⚠️ Existing $ToolName binary could not be executed: $_" -ForegroundColor Yellow
        return $false
    }
}

if ((Test-Path $ffmpegPath) -or (Test-Path $ffprobePath)) {
    $ffmpegValid = Test-ToolVersion -Path $ffmpegPath -ToolName "ffmpeg"
    $ffprobeValid = Test-ToolVersion -Path $ffprobePath -ToolName "ffprobe"

    if ($ffmpegValid -and $ffprobeValid -and -not $forceDownload) {
        Write-Host "✅ Valid FFmpeg binaries already exist in binaries directory" -ForegroundColor Green

        if ($env:CI -eq "true") {
            Write-Host "✅ CI Environment detected, skipping re-download" -ForegroundColor Green
            exit 0
        }

        $response = Read-Host "Do you want to re-download FFmpeg? (y/N)"
        if ($response -ne "y") {
            Write-Host "✅ Using existing FFmpeg binaries" -ForegroundColor Green
            exit 0
        }
    } else {
        Write-Host "⚠️ Existing FFmpeg sidecars are missing, invalid, or force refresh was requested. Re-downloading." -ForegroundColor Yellow
    }
}

# Create temp directory
Write-Host "📁 Creating temporary directory..." -ForegroundColor Yellow
if (Test-Path $TEMP_DIR) {
    Remove-Item -Recurse -Force $TEMP_DIR
}
New-Item -ItemType Directory -Path $TEMP_DIR | Out-Null

# Download FFmpeg
Write-Host "⬇️  Downloading FFmpeg..." -ForegroundColor Yellow
Write-Host "   URL: $DOWNLOAD_URL" -ForegroundColor Gray
$zipPath = Join-Path $TEMP_DIR "ffmpeg.zip"

try {
    Invoke-WebRequest -Uri $DOWNLOAD_URL -OutFile $zipPath -UseBasicParsing
    Write-Host "✅ Downloaded successfully" -ForegroundColor Green
} catch {
    Write-Host "❌ Failed to download FFmpeg: $_" -ForegroundColor Red
    exit 1
}

# Extract archive
Write-Host "📦 Extracting archive..." -ForegroundColor Yellow
try {
    Expand-Archive -Path $zipPath -DestinationPath $TEMP_DIR -Force
    Write-Host "✅ Extracted successfully" -ForegroundColor Green
} catch {
    Write-Host "❌ Failed to extract archive: $_" -ForegroundColor Red
    exit 1
}

# Find ffmpeg.exe and ffprobe.exe
Write-Host "🔍 Locating FFmpeg binaries..." -ForegroundColor Yellow
$ffmpegSource = Get-ChildItem -Path $TEMP_DIR -Recurse -Filter "ffmpeg.exe" | Select-Object -First 1
$ffprobeSource = Get-ChildItem -Path $TEMP_DIR -Recurse -Filter "ffprobe.exe" | Select-Object -First 1

if (-not $ffmpegSource -or -not $ffprobeSource) {
    Write-Host "❌ Could not find FFmpeg binaries in downloaded archive" -ForegroundColor Red
    exit 1
}

# Copy to bin directory with sidecar naming
Write-Host "📋 Copying binaries to binaries directory..." -ForegroundColor Yellow
Copy-Item -Path $ffmpegSource.FullName -Destination $ffmpegPath -Force
# Also copy standard name for dev convenience? No, stick to sidecar.
# Actually, copy standard name too if needed for debugging?
# Copy-Item -Path $ffmpegSource.FullName -Destination (Join-Path $BIN_DIR "ffmpeg.exe") -Force

Copy-Item -Path $ffprobeSource.FullName -Destination $ffprobePath -Force

# Verify binaries
Write-Host "✅ Verifying binaries..." -ForegroundColor Green
$newFfmpegVersion = & $ffmpegPath -version 2>&1 | Select-Object -First 1
Write-Host "   $newFfmpegVersion" -ForegroundColor Cyan

# Cleanup
Write-Host "🧹 Cleaning up temporary files..." -ForegroundColor Yellow
Remove-Item -Recurse -Force $TEMP_DIR

# Show size info
$ffmpegSize = (Get-Item $ffmpegPath).Length / 1MB
$ffprobeSize = (Get-Item $ffprobePath).Length / 1MB
Write-Host ""
Write-Host "📊 Binary Sizes:" -ForegroundColor Cyan
Write-Host "   ffmpeg.exe:  $($ffmpegSize.ToString('F2')) MB" -ForegroundColor Gray
Write-Host "   ffprobe.exe: $($ffprobeSize.ToString('F2')) MB" -ForegroundColor Gray
Write-Host "   Total:       $(($ffmpegSize + $ffprobeSize).ToString('F2')) MB" -ForegroundColor Gray

Write-Host ""
Write-Host "✅ FFmpeg preparation complete!" -ForegroundColor Green
Write-Host "   Binaries are ready for bundling in: $BIN_DIR" -ForegroundColor Cyan
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Yellow
Write-Host "  1. Run: cargo tauri build" -ForegroundColor Gray
Write-Host "  2. Find installers in: src-tauri\target\release\bundle\" -ForegroundColor Gray
