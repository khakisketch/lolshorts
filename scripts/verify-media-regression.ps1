[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$ProjectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $ProjectRoot
try {
    # The regression creates all fixtures under tempfile::TempDir and drives
    # VideoProcessor::compose_with_options with CPU libx264. Pixel framing,
    # audio/silence, VFR, duration, and truncated-input assertions therefore
    # cover the production Rust path instead of a duplicated PowerShell graph.
    cargo test --manifest-path src-tauri/Cargo.toml --test ffmpeg_integration -- --nocapture
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}
