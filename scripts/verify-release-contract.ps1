[CmdletBinding()]
param([string]$BundleDir = "")

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$Package = Get-Content -LiteralPath (Join-Path $ProjectRoot "package.json") -Raw | ConvertFrom-Json
$Tauri = Get-Content -LiteralPath (Join-Path $ProjectRoot "src-tauri/tauri.conf.json") -Raw | ConvertFrom-Json
$DefaultCapability = Get-Content -LiteralPath (Join-Path $ProjectRoot "src-tauri/capabilities/default.json") -Raw | ConvertFrom-Json
$OverlayCapability = Get-Content -LiteralPath (Join-Path $ProjectRoot "src-tauri/capabilities/overlay.json") -Raw | ConvertFrom-Json
$Cargo = Get-Content -LiteralPath (Join-Path $ProjectRoot "src-tauri/Cargo.toml") -Raw
$CargoVersion = [regex]::Match($Cargo, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
$FfmpegPrepare = Get-Content -LiteralPath (Join-Path $ProjectRoot "src-tauri/build_scripts/prepare_ffmpeg.ps1") -Raw
$ProductionRelease = Get-Content -LiteralPath (Join-Path $ProjectRoot ".github/workflows/release.yml") -Raw
$ReleaseReadiness = Get-Content -LiteralPath (Join-Path $ProjectRoot ".github/workflows/release-readiness.yml") -Raw

function Get-Sha256Hash {
    param([Parameter(Mandatory)][string]$LiteralPath)

    # Use the .NET implementation directly so the release gate behaves the
    # same under Windows PowerShell 5.1, PowerShell 7, and restricted runners
    # where the Get-FileHash module may not be auto-loaded.
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    $stream = [System.IO.File]::OpenRead($LiteralPath)
    try {
        return (([BitConverter]::ToString($sha256.ComputeHash($stream))) -replace '-', '').ToLowerInvariant()
    } finally {
        $stream.Dispose()
        $sha256.Dispose()
    }
}

if ($Package.version -ne $Tauri.version -or $Package.version -ne $CargoVersion) {
    throw "Version mismatch: package=$($Package.version), tauri=$($Tauri.version), cargo=$CargoVersion"
}
if (-not $Tauri.bundle.createUpdaterArtifacts) { throw "Updater artifacts are disabled" }
if (@($Tauri.bundle.targets) -notcontains "msi" -or @($Tauri.bundle.targets) -notcontains "nsis") { throw "MSI and NSIS targets are required" }
if (@($Tauri.bundle.externalBin) -notcontains "binaries/ffmpeg" -or @($Tauri.bundle.externalBin) -notcontains "binaries/ffprobe") { throw "FFmpeg and ffprobe sidecars must be explicit externalBin entries" }

# Release runners must never resolve a mutable "latest" FFmpeg archive. The
# preparation script validates this immutable archive's SHA-256 before copying
# either executable into the Tauri sidecar directory.
$PinnedDownload = [regex]::Match(
    $FfmpegPrepare,
    '(?m)\[string\]\$DownloadUrl\s*=\s*"(https://github\.com/BtbN/FFmpeg-Builds/releases/download/autobuild-\d{4}-\d{2}-\d{2}-\d{2}-\d{2}/[^"]+)"'
)
$PinnedSha256 = [regex]::Match(
    $FfmpegPrepare,
    '(?m)\[string\]\$ExpectedSha256\s*=\s*"([0-9a-fA-F]{64})"'
)
if (-not $PinnedDownload.Success) { throw "FFmpeg download must use an immutable BtbN autobuild tag" }
if ($PinnedDownload.Groups[1].Value -match '(?i)latest') { throw "FFmpeg download must not use a mutable latest archive" }
if (-not $PinnedSha256.Success) { throw "FFmpeg download must declare a full SHA-256 checksum" }
foreach ($Workflow in @($ProductionRelease, $ReleaseReadiness)) {
    if ($Workflow -notmatch '(?m)prepare_ffmpeg\.ps1\s+-Source\s+Download') {
        throw "Release workflows must prepare checksum-pinned FFmpeg sidecars with -Source Download"
    }
}
$UpdaterEndpoints = @($Tauri.plugins.updater.endpoints)
if ($UpdaterEndpoints.Count -eq 0) { throw "At least one updater endpoint is required" }
foreach ($Endpoint in $UpdaterEndpoints) {
    if ($Endpoint -notmatch '^https://') { throw "Updater endpoint must use HTTPS" }
}

# The WebView may read only app-owned staged media. User-selected source files
# are validated and copied into this directory by Rust before preview.
$AssetScopes = @($Tauri.app.security.assetProtocol.scope)
if (-not $Tauri.app.security.assetProtocol.enable) { throw "Asset protocol must be enabled for staged media previews" }
if ($AssetScopes.Count -ne 1 -or $AssetScopes[0] -ne '$DATA/lolshorts/**') {
    throw "Asset protocol scope must be exactly `$DATA/lolshorts/**"
}

# Keep the capture-excluded overlay event-only. Any shell, updater, autostart,
# dialog, notification, path, or broad window permission here would expand the
# impact of renderer content that is visible during a game.
$DefaultWindows = @($DefaultCapability.windows)
if ($DefaultWindows.Count -ne 1 -or $DefaultWindows[0] -ne "main") {
    throw "Default capability must target only the main window"
}
$OverlayWindows = @($OverlayCapability.windows)
if ($OverlayWindows.Count -ne 1 -or $OverlayWindows[0] -ne "overlay") {
    throw "Overlay capability must target only the overlay window"
}
$OverlayPermissions = @($OverlayCapability.permissions)
if ($OverlayPermissions.Count -ne 1 -or $OverlayPermissions[0] -ne "core:event:allow-listen") {
    throw "Overlay capability must contain only core:event:allow-listen"
}
$SidecarVersions = @{}
foreach ($Name in @("ffmpeg", "ffprobe")) {
    $Binary = Get-ChildItem -LiteralPath (Join-Path $ProjectRoot "src-tauri/binaries") -File -Filter "$Name*" -ErrorAction Stop | Select-Object -First 1
    if (-not $Binary) { throw "Missing bundled $Name sidecar" }
    $null = Get-Sha256Hash -LiteralPath $Binary.FullName
    $VersionLines = & $Binary.FullName -version 2>&1
    $BinaryExitCode = $LASTEXITCODE
    $VersionOutput = ($VersionLines | Out-String).Trim()
    if ($BinaryExitCode -ne 0) { throw "$Name sidecar failed -version" }
    $VersionLine = ($VersionOutput -split "`r?`n" | Select-Object -First 1)
    $VersionMatch = [regex]::Match($VersionLine, "^$Name version ([^\s]+)")
    if (-not $VersionMatch.Success) { throw "$Name sidecar returned an unexpected version contract" }
    $SidecarVersions[$Name] = $VersionMatch.Groups[1].Value
}
if (-not [System.StringComparer]::OrdinalIgnoreCase.Equals($SidecarVersions.ffmpeg, $SidecarVersions.ffprobe)) {
    throw "FFmpeg and ffprobe sidecars must come from the same versioned build"
}
if (-not [string]::IsNullOrWhiteSpace($BundleDir)) {
    $ResolvedBundle = [System.IO.Path]::GetFullPath($BundleDir)
    if (-not (Test-Path -LiteralPath $ResolvedBundle)) { throw "Bundle directory missing: $ResolvedBundle" }
    if (-not (Get-ChildItem -LiteralPath $ResolvedBundle -Recurse -File -Filter "*.msi" | Select-Object -First 1)) { throw "MSI artifact missing" }
    if (-not (Get-ChildItem -LiteralPath $ResolvedBundle -Recurse -File -Filter "*-setup.exe" | Select-Object -First 1)) { throw "NSIS artifact missing" }
    if (-not (Get-ChildItem -LiteralPath $ResolvedBundle -Recurse -File -Filter "*.sig" | Select-Object -First 1)) { throw "Updater signature missing" }
}
Write-Host "Release contract PASS ($($Package.version))"
