[CmdletBinding()]
param(
    [string]$OutputDir = "",
    [string]$FfmpegPath = "ffmpeg"
)

$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = Join-Path (Split-Path -Parent $PSScriptRoot) "test-results\aspect-ratio-fixtures"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function New-MarkerVideo {
    param([int]$Width, [int]$Height, [string]$Name)
    $filter = "drawbox=x=0:y=0:w=64:h=64:color=red:t=fill," +
              "drawbox=x=iw-64:y=0:w=64:h=64:color=lime:t=fill," +
              "drawbox=x=0:y=ih-64:w=64:h=64:color=blue:t=fill," +
              "drawbox=x=iw-64:y=ih-64:w=64:h=64:color=yellow:t=fill"
    $output = Join-Path $OutputDir $Name
    & $FfmpegPath -hide_banner -loglevel error -y -f lavfi -i "color=c=black:s=${Width}x${Height}:r=60:d=5" -vf $filter -c:v libx264 -pix_fmt yuv420p -movflags +faststart $output
    if ($LASTEXITCODE -ne 0) { throw "FFmpeg failed to create $Name" }
}

New-MarkerVideo -Width 1920 -Height 1080 -Name "markers-16x9.mp4"
New-MarkerVideo -Width 1720 -Height 720 -Name "markers-43x18.mp4"
Write-Host "Aspect-ratio fixtures written to: $OutputDir"
