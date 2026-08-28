# LoLShorts Installer Validation Script
# Tests MSI and NSIS installers on clean Windows environments

param(
    [Parameter(Mandatory=$false)]
    [ValidateSet("MSI", "NSIS", "Both")]
    [string]$InstallerType = "Both",

    [Parameter(Mandatory=$false)]
    [string]$InstallerPath = "",

    [Parameter(Mandatory=$false)]
    [switch]$NonInteractive,

    [Parameter(Mandatory=$false)]
    [switch]$RunSilentInstall
    ,
    [switch]$ReleaseChannel
)

$ErrorActionPreference = "Stop"

# Colors for output
$Green = [ConsoleColor]::Green
$Red = [ConsoleColor]::Red
$Yellow = [ConsoleColor]::Yellow
$Cyan = [ConsoleColor]::Cyan
$script:ValidationFailures = 0

function Mark-ValidationFailure {
    $script:ValidationFailures += 1
}

function Write-ColorOutput {
    param(
        [Parameter(Mandatory=$true, Position=0)]
        [string]$Message,

        [Parameter(Mandatory=$false, Position=1)]
        [object]$Color = [ConsoleColor]::White
    )

    $parsedColor = [ConsoleColor]::White
    if ($Color -is [ConsoleColor]) {
        $parsedColor = $Color
    }
    elseif (-not [Enum]::TryParse([ConsoleColor], [string]$Color, $true, [ref]$parsedColor)) {
        $parsedColor = [ConsoleColor]::White
    }

    Write-Host $Message -ForegroundColor $parsedColor
}

function Test-InstallerExists {
    param([string]$Path)

    if (-not (Test-Path $Path)) {
        Write-ColorOutput "[FAIL] Installer not found: $Path" $Red
        Mark-ValidationFailure
        return $false
    }

    $fileInfo = Get-Item $Path
    $sizeMB = [math]::Round($fileInfo.Length / 1MB, 2)
    Write-ColorOutput "[PASS] Installer found: $($fileInfo.Name) ($sizeMB MB)" $Green

    return $true
}

function Test-InstallerSignature {
    param([string]$Path)

    Write-ColorOutput "`n[CHECK] Checking digital signature..." $Cyan

    try {
        $signature = Get-AuthenticodeSignature -FilePath $Path

        if ($signature.Status -eq "Valid") {
            Write-ColorOutput "[PASS] Installer is digitally signed" $Green
            Write-ColorOutput "   Signer: $($signature.SignerCertificate.Subject)" $Cyan
            return $true
        }
        elseif ($signature.Status -eq "NotSigned") {
            if ($ReleaseChannel) {
                Write-ColorOutput "[FAIL] Release-channel installer is not Authenticode signed" $Red
                Mark-ValidationFailure
                return $false
            }
            Write-ColorOutput "[WARN] Installer is unsigned (development channel)" $Yellow
            return $true
        }
        else {
            Write-ColorOutput "[FAIL] Invalid signature: $($signature.StatusMessage)" $Red
            Mark-ValidationFailure
            return $false
        }
    }
    catch {
        if ($ReleaseChannel) {
            Write-ColorOutput "[FAIL] Release-channel signature verification errored: $($_.Exception.Message)" $Red
            Mark-ValidationFailure
            return $false
        }
        Write-ColorOutput "[WARN] Unsigned development/fixture channel; Authenticode verification was unavailable: $($_.Exception.Message)" $Yellow
        return $true
    }
}

function Test-InstallerMetadata {
    param([string]$Path)

    Write-ColorOutput "`n[CHECK] Checking installer metadata..." $Cyan

    try {
        $fileInfo = Get-Item $Path
        $versionInfo = $fileInfo.VersionInfo

        if ($versionInfo.ProductName) {
            Write-ColorOutput "[PASS] Product Name: $($versionInfo.ProductName)" $Green
        }

        if ($versionInfo.ProductVersion) {
            Write-ColorOutput "[PASS] Product Version: $($versionInfo.ProductVersion)" $Green
        }

        if ($versionInfo.CompanyName) {
            Write-ColorOutput "[PASS] Company: $($versionInfo.CompanyName)" $Green
        }

        return $true
    }
    catch {
        Write-ColorOutput "[WARN] Could not read metadata: $($_.Exception.Message)" $Yellow
        return $true
    }
}

function Test-FFmpegBundling {
    param([string]$InstallerPath)

    Write-ColorOutput "`n[CHECK] Checking FFmpeg bundling..." $Cyan

    if ($RunSilentInstall) {
        Write-ColorOutput "[PASS] Sidecars will be verified after isolated silent installation" $Green
    } else {
        Write-ColorOutput "[WARN] Sidecar verification requires -RunSilentInstall; installer size is not evidence" $Yellow
    }
    return $true
}

function Test-InstalledSidecars {
    param([string]$InstallRoot)
    $ffmpeg = Get-ChildItem -LiteralPath $InstallRoot -Recurse -File -Filter "ffmpeg*.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    $ffprobe = Get-ChildItem -LiteralPath $InstallRoot -Recurse -File -Filter "ffprobe*.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $ffmpeg -or -not $ffprobe) {
        Write-ColorOutput "[FAIL] Installed ffmpeg/ffprobe sidecars were not found" $Red
        Mark-ValidationFailure
        return $false
    }
    $ffmpegHash = (Get-FileHash -LiteralPath $ffmpeg.FullName -Algorithm SHA256).Hash
    $ffprobeHash = (Get-FileHash -LiteralPath $ffprobe.FullName -Algorithm SHA256).Hash
    & $ffmpeg.FullName -version *> $null
    $ffmpegOk = $LASTEXITCODE -eq 0
    & $ffprobe.FullName -version *> $null
    $ffprobeOk = $LASTEXITCODE -eq 0
    if (-not $ffmpegOk -or -not $ffprobeOk) {
        Write-ColorOutput "[FAIL] Installed media sidecar did not execute -version" $Red
        Mark-ValidationFailure
        return $false
    }
    Write-ColorOutput "[PASS] Installed sidecars found, SHA-256 hashed, and executable (ffmpeg $($ffmpegHash.Substring(0,12))..., ffprobe $($ffprobeHash.Substring(0,12))...)" $Green
    return $true
}

function Test-SilentInstall {
    param([string]$InstallerPath, [string]$Type)

    Write-ColorOutput "`n[CHECK] Testing silent installation..." $Cyan

    $tempInstallDir = Join-Path $env:TEMP "LoLShorts-Test-$([Guid]::NewGuid())"

    try {
        if ($Type -eq "MSI") {
            # Test MSI silent install
            Write-ColorOutput "Testing MSI silent install to: $tempInstallDir" $Cyan

            $msiArgs = @(
                "/i", "`"$InstallerPath`"",
                "/qn",  # Quiet, no UI
                "/norestart",
                "INSTALLDIR=`"$tempInstallDir`"",
                "/l*v", "`"$env:TEMP\lolshorts-install-test.log`""
            )

            $process = Start-Process -FilePath "msiexec.exe" -ArgumentList $msiArgs -Wait -PassThru -NoNewWindow

            if ($process.ExitCode -eq 0) {
                Write-ColorOutput "[PASS] MSI silent install succeeded" $Green

                # Check if files were installed
                if (Test-Path $tempInstallDir) {
                    $fileCount = (Get-ChildItem $tempInstallDir -Recurse -File).Count
                    Write-ColorOutput "   Installed $fileCount files" $Green
                    Test-InstalledSidecars $tempInstallDir | Out-Null

                    # Uninstall
                    Write-ColorOutput "   Cleaning up test installation..." $Cyan
                    Start-Process -FilePath "msiexec.exe" -ArgumentList "/x `"$InstallerPath`" /qn" -Wait -NoNewWindow
                }

                return $true
            }
            else {
                Write-ColorOutput "[FAIL] MSI install failed with exit code: $($process.ExitCode)" $Red
                Write-ColorOutput "   Check log: $env:TEMP\lolshorts-install-test.log" $Yellow
                Mark-ValidationFailure
                return $false
            }
        }
        elseif ($Type -eq "NSIS") {
            # Test NSIS silent install
            Write-ColorOutput "Testing NSIS silent install..." $Cyan

            $nsisArgs = @("/S")  # Silent mode

            $process = Start-Process -FilePath $InstallerPath -ArgumentList $nsisArgs -Wait -PassThru -NoNewWindow

            if ($process.ExitCode -eq 0) {
                Write-ColorOutput "[PASS] NSIS silent install succeeded" $Green

                # Check if uninstaller was created
                $uninstallerPath = "$env:LOCALAPPDATA\Programs\LoLShorts\uninstall.exe"
                if (Test-Path $uninstallerPath) {
                    $installRoot = Split-Path -Parent $uninstallerPath
                    Test-InstalledSidecars $installRoot | Out-Null
                    Write-ColorOutput "   Uninstaller created successfully" $Green

                    # Run uninstaller
                    Write-ColorOutput "   Cleaning up test installation..." $Cyan
                    Start-Process -FilePath $uninstallerPath -ArgumentList "/S" -Wait -NoNewWindow
                }

                return $true
            }
            else {
                Write-ColorOutput "[FAIL] NSIS install failed with exit code: $($process.ExitCode)" $Red
                Mark-ValidationFailure
                return $false
            }
        }
    }
    catch {
        Write-ColorOutput "[FAIL] Silent install test failed: $($_.Exception.Message)" $Red
        Mark-ValidationFailure
        return $false
    }
    finally {
        # Cleanup temp directory
        if (Test-Path $tempInstallDir) {
            Remove-Item $tempInstallDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

function Test-Prerequisites {
    Write-ColorOutput "`n[CHECK] Checking system prerequisites..." $Cyan

    # Check Windows version
    $osVersion = [System.Environment]::OSVersion.Version
    if ($osVersion.Major -ge 10) {
        Write-ColorOutput "[PASS] Windows 10+ detected ($($osVersion.ToString()))" $Green
    }
    else {
        Write-ColorOutput "[WARN] Windows version $($osVersion.ToString()) may not be supported" $Yellow
    }

    # Check if running as Administrator
    $isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if ($isAdmin) {
        Write-ColorOutput "[PASS] Running as Administrator" $Green
    }
    else {
        Write-ColorOutput "[WARN] Not running as Administrator (some tests may be limited)" $Yellow
    }

    # Check available disk space
    $drive = (Get-Item $env:TEMP).PSDrive
    $freeSpaceGB = [math]::Round($drive.Free / 1GB, 2)
    if ($freeSpaceGB -gt 2) {
        Write-ColorOutput "[PASS] Available disk space: $freeSpaceGB GB" $Green
    }
    else {
        Write-ColorOutput "[WARN] Low disk space: $freeSpaceGB GB" $Yellow
    }
}

function Should-TestSilentInstall {
    if ($RunSilentInstall) {
        return $true
    }

    if ($NonInteractive) {
        Write-ColorOutput "`nSkipping silent installation test in non-interactive mode." $Yellow
        return $false
    }

    $testInstall = Read-Host "`nTest silent installation? (y/n)"
    return $testInstall -eq "y"
}

# Main validation logic
Write-ColorOutput "===============================================" $Cyan
Write-ColorOutput "  LoLShorts Installer Validation" $Cyan
Write-ColorOutput "===============================================" $Cyan

Test-Prerequisites

# Determine installer paths
if ($InstallerPath -eq "") {
    $projectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
    $bundleDir = Join-Path $projectRoot "src-tauri\target\release\bundle"

    if ($InstallerType -eq "MSI" -or $InstallerType -eq "Both") {
        $msiPath = Get-ChildItem -Path (Join-Path $bundleDir "msi") -Filter "*.msi" -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName

        if ($msiPath) {
            Write-ColorOutput "`n=== MSI Installer Validation ===" $Cyan

            if (Test-InstallerExists $msiPath) {
                Test-InstallerSignature $msiPath
                Test-InstallerMetadata $msiPath
                Test-FFmpegBundling $msiPath

                # Optionally test silent install (requires admin)
                if (Should-TestSilentInstall) {
                    Test-SilentInstall $msiPath "MSI"
                }
            }
        }
        else {
            Write-ColorOutput "`n[FAIL] MSI installer not found in $bundleDir\msi" $Red
            Mark-ValidationFailure
        }
    }

    if ($InstallerType -eq "NSIS" -or $InstallerType -eq "Both") {
        $nsisPath = Get-ChildItem -Path (Join-Path $bundleDir "nsis") -Filter "*-setup.exe" -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName

        if ($nsisPath) {
            Write-ColorOutput "`n=== NSIS Installer Validation ===" $Cyan

            if (Test-InstallerExists $nsisPath) {
                Test-InstallerSignature $nsisPath
                Test-InstallerMetadata $nsisPath
                Test-FFmpegBundling $nsisPath

                # Optionally test silent install
                if (Should-TestSilentInstall) {
                    Test-SilentInstall $nsisPath "NSIS"
                }
            }
        }
        else {
            Write-ColorOutput "`n[FAIL] NSIS installer not found in $bundleDir\nsis" $Red
            Mark-ValidationFailure
        }
    }
}
else {
    # Use provided installer path
    if (Test-InstallerExists $InstallerPath) {
        $extension = [System.IO.Path]::GetExtension($InstallerPath)
        $type = if ($extension -eq ".msi") { "MSI" } else { "NSIS" }

        Test-InstallerSignature $InstallerPath
        Test-InstallerMetadata $InstallerPath
        Test-FFmpegBundling $InstallerPath

        if (Should-TestSilentInstall) {
            Test-SilentInstall $InstallerPath $type
        }
    }
}

Write-ColorOutput "`n===============================================" $Cyan
Write-ColorOutput "  Validation Complete" $Cyan
Write-ColorOutput "===============================================" $Cyan

if ($script:ValidationFailures -gt 0) {
    exit 1
}
