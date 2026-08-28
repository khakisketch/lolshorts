[CmdletBinding()]
param(
    [switch]$SkipE2E,
    [switch]$SkipMedia
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot

function Invoke-Gate {
    param([string]$Name, [string]$Command, [string[]]$Arguments)
    Write-Host "[non-game] $Name"
    Push-Location $ProjectRoot
    try {
        & $Command @Arguments
        if ($LASTEXITCODE -ne 0) { throw "$Name failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

Invoke-Gate "FFmpeg sidecars" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "src-tauri/build_scripts/prepare_ffmpeg.ps1", "-Source", "Auto")
Invoke-Gate "Pinned build environment" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "src-tauri/build_scripts/verify_environment.ps1")
Invoke-Gate "Frontend" "npm" @("run", "verify:frontend")
Invoke-Gate "Node dependency audit" "npm" @("run", "audit:all")
Invoke-Gate "Rust format" "cargo" @("fmt", "--manifest-path", "src-tauri/Cargo.toml", "--all", "--", "--check")
Invoke-Gate "Rust clippy" "cargo" @("clippy", "--manifest-path", "src-tauri/Cargo.toml", "--all-targets", "--", "-D", "warnings")
Invoke-Gate "Rust tests" "cargo" @("test", "--manifest-path", "src-tauri/Cargo.toml")
Invoke-Gate "Rust dependency audit" "cargo" @("audit", "--file", "Cargo.lock")
Invoke-Gate "Release contract" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/verify-release-contract.ps1")
Invoke-Gate "Field evidence tool regression" "npm" @("run", "test:field-tools")

Invoke-Gate "Supabase quota tests" "npm" @("run", "test:supabase")

if (-not $SkipMedia) {
    Invoke-Gate "Synthetic media regression" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/verify-media-regression.ps1")
}
if (-not $SkipE2E) {
    Invoke-Gate "Playwright" "npm" @("run", "test:e2e")
}
Invoke-Gate "Whitespace" "git" @("diff", "--check")
Write-Host "[non-game] PASS"
