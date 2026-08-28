<#
Validate the reproducible Windows build prerequisites for LoLShorts.

All repository paths are resolved from this script so the result does not
depend on the caller's current directory. This helper does not print service
configuration values or other environment secrets.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$TauriDir = Split-Path -Parent $PSScriptRoot
$ProjectRoot = Split-Path -Parent $TauriDir
$PackageLock = Get-Content -LiteralPath (Join-Path $ProjectRoot "package-lock.json") -Raw
$TauriCliLockMatch = [regex]::Match(
    $PackageLock,
    '(?s)"node_modules/@tauri-apps/cli"\s*:\s*\{\s*"version"\s*:\s*"([^"]+)"'
)
if (-not $TauriCliLockMatch.Success) { throw "Could not resolve the Tauri CLI version from package-lock.json." }
$ExpectedNode = "24.2.0"
$ExpectedNpm = "11.6.3"
$ExpectedRust = "1.94.1"
$ExpectedTauriCli = $TauriCliLockMatch.Groups[1].Value
$Issues = New-Object System.Collections.Generic.List[string]
$Warnings = New-Object System.Collections.Generic.List[string]

function Invoke-VersionProbe {
    param(
        [Parameter(Mandatory = $true)][string]$Command,
        [string[]]$Arguments = @()
    )

    try {
        $output = (& $Command @Arguments 2>&1 | Out-String).Trim()
        return [pscustomobject]@{
            Success = ($LASTEXITCODE -eq 0)
            Output = $output
        }
    } catch {
        return [pscustomobject]@{
            Success = $false
            Output = $_.Exception.Message
        }
    }
}

function Write-CheckResult {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][bool]$Success,
        [string]$Detail = ""
    )

    $color = if ($Success) { "Green" } else { "Red" }
    $marker = if ($Success) { "PASS" } else { "FAIL" }
    Write-Host ("{0,-28} {1}" -f $Name, $marker) -ForegroundColor $color
    if (-not [string]::IsNullOrWhiteSpace($Detail)) {
        Write-Host "  $Detail" -ForegroundColor Gray
    }
}

Write-Host "LoLShorts reproducible build environment" -ForegroundColor Cyan
Write-Host "Project: $ProjectRoot" -ForegroundColor DarkGray
Write-Host ""

$node = Invoke-VersionProbe -Command "node" -Arguments @("--version")
$nodeMatches = $node.Success -and $node.Output -eq "v$ExpectedNode"
Write-CheckResult -Name "Node.js $ExpectedNode" -Success $nodeMatches -Detail $node.Output
if (-not $nodeMatches) { $Issues.Add("Install the repository-pinned Node.js $ExpectedNode runtime.") }

$npm = Invoke-VersionProbe -Command "npm" -Arguments @("--version")
$npmMatches = $npm.Success -and $npm.Output -eq $ExpectedNpm
Write-CheckResult -Name "npm $ExpectedNpm" -Success $npmMatches -Detail $npm.Output
if (-not $npmMatches) { $Issues.Add("Install npm $ExpectedNpm (npm install --global npm@$ExpectedNpm).") }

$rust = Invoke-VersionProbe -Command "rustc" -Arguments @("--version")
$rustMatches = $rust.Success -and $rust.Output -match "^rustc $([regex]::Escape($ExpectedRust))\b"
Write-CheckResult -Name "Rust $ExpectedRust" -Success $rustMatches -Detail $rust.Output
if (-not $rustMatches) { $Issues.Add("Install the rust-toolchain.toml toolchain ($ExpectedRust).") }

$cargo = Invoke-VersionProbe -Command "cargo" -Arguments @("--version")
Write-CheckResult -Name "Cargo" -Success $cargo.Success -Detail $cargo.Output
if (-not $cargo.Success) { $Issues.Add("Cargo is unavailable.") }

$NodeModules = Join-Path $ProjectRoot "node_modules"
$nodeModulesReady = Test-Path -LiteralPath $NodeModules -PathType Container
$nodeModulesDetail = if ($nodeModulesReady) { "node_modules is present" } else { "Run npm ci from the repository root" }
Write-CheckResult -Name "npm dependencies" -Success $nodeModulesReady -Detail $nodeModulesDetail
if (-not $nodeModulesReady) { $Issues.Add("Install the lockfile dependencies with npm ci.") }

if ($nodeModulesReady) {
    $tauri = Invoke-VersionProbe -Command "npx" -Arguments @("--no-install", "tauri", "--version")
    $tauriMatches = $tauri.Success -and $tauri.Output -eq "tauri-cli $ExpectedTauriCli"
    Write-CheckResult -Name "Local Tauri CLI" -Success $tauriMatches -Detail $tauri.Output
    if (-not $tauriMatches) { $Issues.Add("The lockfile-installed Tauri CLI is missing or does not match package.json.") }
}

$MediaVersions = @{}
foreach ($Name in @("ffmpeg", "ffprobe")) {
    $path = Join-Path $TauriDir "binaries/$Name-x86_64-pc-windows-msvc.exe"
    $probe = if (Test-Path -LiteralPath $path -PathType Leaf) {
        Invoke-VersionProbe -Command $path -Arguments @("-version")
    } else {
        [pscustomobject]@{ Success = $false; Output = "missing: $path" }
    }
    $versionLine = ($probe.Output -split "`r?`n" | Select-Object -First 1)
    $valid = $probe.Success -and $versionLine.StartsWith("$Name version")
    Write-CheckResult -Name "$Name Tauri sidecar" -Success $valid -Detail $versionLine
    if ($valid) {
        $versionMatch = [regex]::Match($versionLine, "^$Name version ([^\s]+)")
        if ($versionMatch.Success) { $MediaVersions[$Name] = $versionMatch.Groups[1].Value }
    } else {
        $Issues.Add("Run src-tauri/build_scripts/prepare_ffmpeg.ps1 -Source Auto.")
    }
}
if ($MediaVersions.Count -eq 2 -and
    -not [System.StringComparer]::OrdinalIgnoreCase.Equals($MediaVersions.ffmpeg, $MediaVersions.ffprobe)) {
    Write-CheckResult -Name "FFmpeg/ffprobe build pair" -Success $false -Detail "$($MediaVersions.ffmpeg) != $($MediaVersions.ffprobe)"
    $Issues.Add("Regenerate FFmpeg and ffprobe from the same versioned build.")
}

# MSI packaging requires WiX, while a no-bundle debug shell can still be built
# without it. Keep local WiX absence actionable but non-blocking for game smoke.
$candle = Get-Command "candle" -CommandType Application -ErrorAction SilentlyContinue
$light = Get-Command "light" -CommandType Application -ErrorAction SilentlyContinue
$wixReady = [bool]($candle -and $light)
$wixDetail = if ($wixReady) { $candle.Source } else { "Required only for local MSI packaging; CI installs WiX explicitly" }
$wixMarker = if ($wixReady) { "PASS" } else { "WARN" }
$wixColor = if ($wixReady) { "Green" } else { "Yellow" }
Write-Host ("{0,-28} {1}" -f "WiX Toolset", $wixMarker) -ForegroundColor $wixColor
Write-Host "  $wixDetail" -ForegroundColor Gray
if (-not $wixReady) { $Warnings.Add("WiX is absent; use npm run tauri:build:debug for a no-bundle game smoke build.") }

$vswhere = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
$vsReady = Test-Path -LiteralPath $vswhere -PathType Leaf
$vsMarker = if ($vsReady) { "PASS" } else { "WARN" }
$vsColor = if ($vsReady) { "Green" } else { "Yellow" }
Write-Host ("{0,-28} {1}" -f "Visual Studio Build Tools", $vsMarker) -ForegroundColor $vsColor
if (-not $vsReady) { $Warnings.Add("vswhere.exe was not found; the MSVC toolchain may still be available through another installation.") }

Write-Host ""
if ($Warnings.Count -gt 0) {
    Write-Host "Warnings:" -ForegroundColor Yellow
    foreach ($warning in $Warnings) { Write-Host "- $warning" -ForegroundColor Yellow }
    Write-Host ""
}
if ($Issues.Count -gt 0) {
    Write-Host "Build environment FAIL:" -ForegroundColor Red
    foreach ($issue in @($Issues | Select-Object -Unique)) { Write-Host "- $issue" -ForegroundColor Red }
    exit 1
}

Write-Host "Build environment PASS" -ForegroundColor Green
Write-Host "Next: npm run verify:non-game-readiness" -ForegroundColor Cyan
exit 0
