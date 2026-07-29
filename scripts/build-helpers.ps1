# LoLShorts Build Helper Functions
# Shared functions for build scripts

# Function to write colored output
function Write-ColorOutput {
    param(
        [string]$Message,
        [ConsoleColor]$Color = "White"
    )
    Write-Host $Message -ForegroundColor $Color
}

# Function to check if command exists
function Test-Command {
    param([string]$Command)
    try {
        Get-Command $Command -ErrorAction Stop | Out-Null
        return $true
    }
    catch {
        return $false
    }
}

# Function to initialize build environment
function Initialize-BuildEnvironment {
    Write-ColorOutput "🔧 Initializing build environment..." "Blue"

    # Set environment variables
    $env:CARGO_TERM_COLOR = "always"
    $env:RUST_BACKTRACE = "1"

    # Check required tools
    $requiredTools = @("git", "node", "npm", "cargo")
    foreach ($tool in $requiredTools) {
        if (-not (Test-Command $tool)) {
            Write-ColorOutput "❌ Required tool not found: $tool" "Red"
            exit 1
        }
        Write-ColorOutput "✅ $tool found" "Green"
    }
}

# Function to clean build artifacts
function Clean-BuildArtifacts {
    # Clean Cargo artifacts
    if (Test-Path "src-tauri\target") {
        Remove-Item -Recurse -Force "src-tauri\target"
        Write-ColorOutput "✅ Cleaned Cargo artifacts" "Green"
    }

    # Clean Node modules
    if (Test-Path "node_modules") {
        Remove-Item -Recurse -Force "node_modules"
        Write-ColorOutput "✅ Cleaned Node modules" "Green"
    }

    # Clean dist
    if (Test-Path "dist") {
        Remove-Item -Recurse -Force "dist"
        Write-ColorOutput "✅ Cleaned frontend build" "Green"
    }
}

# Function to install dependencies
function Install-Dependencies {
    # Install Node.js dependencies
    Write-ColorOutput "Installing Node.js dependencies..." "Yellow"
    npm ci

    # Install Rust dependencies
    Write-ColorOutput "Installing Rust dependencies..." "Yellow"
    Set-Location "src-tauri"
    cargo fetch
    Set-Location ".."

    Write-ColorOutput "✅ Dependencies installed" "Green"
}

# Function to prepare FFmpeg binaries
function Prepare-FFmpeg {
    Write-ColorOutput "Preparing FFmpeg binaries..." "Yellow"

    $ffmpegDir = "src-tauri\binaries"
    if (-not (Test-Path $ffmpegDir)) {
        New-Item -ItemType Directory -Path $ffmpegDir | Out-Null
    }

    $ffmpegPath = Join-Path $ffmpegDir "ffmpeg.exe"
    if (-not (Test-Path $ffmpegPath)) {
        Write-ColorOutput "Downloading FFmpeg for Windows..." "Yellow"

        # Download FFmpeg
        $ffmpegUrl = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"
        $ffmpegZip = "ffmpeg.zip"

        try {
            Invoke-WebRequest -Uri $ffmpegUrl -OutFile $ffmpegZip
            Expand-Archive -Path $ffmpegZip -DestinationPath "temp_ffmpeg"
            Copy-Item "temp_ffmpeg\ffmpeg-master-latest-win64-gpl\bin\ffmpeg.exe" $ffmpegPath
            Copy-Item "temp_ffmpeg\ffmpeg-master-latest-win64-gpl\bin\ffprobe.exe" (Join-Path $ffmpegDir "ffprobe.exe")
            Remove-Item -Recurse -Force "temp_ffmpeg", $ffmpegZip
            Write-ColorOutput "✅ FFmpeg downloaded successfully" "Green"
        }
        catch {
            Write-ColorOutput "❌ Failed to download FFmpeg: $_" "Red"
            exit 1
        }
    } else {
        Write-ColorOutput "✅ FFmpeg already exists" "Green"
    }
}

# Function to run tests
function Run-Tests {
    Write-ColorOutput "Running tests..." "Yellow"

    # Backend tests
    Write-ColorOutput "Running backend tests..." "Yellow"
    Set-Location "src-tauri"
    cargo test
    Set-Location ".."

    # Frontend tests
    Write-ColorOutput "Running frontend tests..." "Yellow"
    npm test

    Write-ColorOutput "✅ All tests passed" "Green"
}

# Function to build for Windows
function Build-Windows {
    param(
        [string]$BuildType,
        [switch]$SkipSigning
    )

    Write-ColorOutput "🪟 Building for Windows ($BuildType)..." "Yellow"

    # Configure Tauri for Windows
    $tauriConfig = Get-Content "src-tauri\tauri.conf.json" | ConvertFrom-Json
    $tauriConfig.bundle.targets = @("nsis", "msi")
    $tauriConfig | ConvertTo-Json -Depth 10 | Set-Content "src-tauri\tauri.conf.json"

    # Build command
    $buildArgs = @("tauri", "build")
    if ($BuildType -eq "debug") {
        $buildArgs += @("--", "--debug")
    }

    # Build
    & cargo @buildArgs

    if ($LASTEXITCODE -ne 0) {
        Write-ColorOutput "❌ Build failed" "Red"
        exit 1
    }

    # Sign binaries if not skipped and certificate is available
    if (-not $SkipSigning -and $env:WINDOWS_CERTIFICATE_BASE64) {
        Write-ColorOutput "🔐 Signing Windows binaries..." "Yellow"
        Sign-WindowsBinaries
    }

    Write-ColorOutput "✅ Windows build completed" "Green"
}

# Function to build for macOS (cross-compilation from Windows)
function Build-macOS {
    param(
        [string]$BuildType,
        [switch]$SkipSigning
    )

    Write-ColorOutput "🍎 Building for macOS ($BuildType)..." "Yellow"
    Write-ColorOutput "⚠️  macOS cross-compilation from Windows requires additional setup" "Yellow"

    # Add macOS target
    cargo target add x86_64-apple-darwin

    # Set environment for cross-compilation
    $env:CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER = "x86_64-apple-darwin-clang"
    $env:CC = "x86_64-apple-darwin-clang"
    $env:CXX = "x86_64-apple-darwin-clang++"

    Write-ColorOutput "ℹ️  macOS cross-compilation is experimental. Use a macOS runner for best results." "Yellow"
}

# Function to build for Linux (cross-compilation from Windows)
function Build-Linux {
    param([string]$BuildType)

    Write-ColorOutput "🐧 Building for Linux ($BuildType)..." "Yellow"
    Write-ColorOutput "⚠️  Linux cross-compilation from Windows requires additional setup" "Yellow"

    # Add Linux target
    cargo target add x86_64-unknown-linux-gnu

    # Set environment for cross-compilation
    $env:CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = "x86_64-linux-gnu-gcc"
    $env:CC = "x86_64-linux-gnu-gcc"
    $env:CXX = "x86_64-linux-gnu-g++"

    Write-ColorOutput "ℹ️  Linux cross-compilation is experimental. Use a Linux runner for best results." "Yellow"
}

# Function to sign Windows binaries
function Sign-WindowsBinaries {
    try {
        # Decode certificate from environment variable
        $certBytes = [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE_BASE64)
        $certPath = Join-Path $env:TEMP "certificate.pfx"
        [System.IO.File]::WriteAllBytes($certPath, $certBytes)

        # Find installers
        $msiPath = Get-ChildItem "src-tauri\target\$($BuildType)\bundle\msi" -Filter "*.msi" | Select-Object -First 1
        $nsisPath = Get-ChildItem "src-tauri\target\$($BuildType)\bundle\nsis" -Filter "*-setup.exe" | Select-Object -First 1

        # Sign MSI
        if ($msiPath) {
            Write-ColorOutput "Signing MSI: $($msiPath.Name)" "Yellow"
            signtool sign /f $certPath /p $env:WINDOWS_CERTIFICATE_PASSWORD /t http://timestamp.digicert.com /fd sha256 $msiPath.FullName
        }

        # Sign NSIS
        if ($nsisPath) {
            Write-ColorOutput "Signing NSIS: $($nsisPath.Name)" "Yellow"
            signtool sign /f $certPath /p $env:WINDOWS_CERTIFICATE_PASSWORD /t http://timestamp.digicert.com /fd sha256 $nsisPath.FullName
        }

        Write-ColorOutput "✅ Binary signing completed" "Green"
    }
    catch {
        Write-ColorOutput "❌ Binary signing failed: $_" "Red"
        # Don't fail the build for signing issues
    }
}

# Function to generate build report
function Generate-BuildReport {
    Write-ColorOutput "📊 Generating build report..." "Yellow"

    $reportPath = "build-report.json"
    $report = @{
        timestamp = Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"
        platform = $Platform
        buildType = $BuildType
        artifacts = @{}
    }

    # Add Windows artifacts
    if (Test-Path "src-tauri\target\$($BuildType)\bundle\msi") {
        $msiFiles = Get-ChildItem "src-tauri\target\$($BuildType)\bundle\msi" -Filter "*.msi"
        $report.artifacts.windows_msi = $msiFiles | ForEach-Object {
            @{
                name = $_.Name
                size = $_.Length
                path = $_.FullName
            }
        }
    }

    if (Test-Path "src-tauri\target\$($BuildType)\bundle\nsis") {
        $nsisFiles = Get-ChildItem "src-tauri\target\$($BuildType)\bundle\nsis" -Filter "*-setup.exe"
        $report.artifacts.windows_nsis = $nsisFiles | ForEach-Object {
            @{
                name = $_.Name
                size = $_.Length
                path = $_.FullName
            }
        }
    }

    $report | ConvertTo-Json -Depth 10 | Set-Content $reportPath

    Write-ColorOutput "✅ Build report generated: $reportPath" "Green"
}

# Function to get version from package.json
function Get-ProjectVersion {
    $packageJson = Get-Content "package.json" | ConvertFrom-Json
    return $packageJson.version
}

# Function to validate build artifacts
function Test-BuildArtifacts {
    param([string]$Platform, [string]$BuildType)

    Write-ColorOutput "🔍 Validating build artifacts..." "Yellow"

    $success = $true

    switch ($Platform) {
        "windows" {
            # Check MSI
            $msiPath = Join-Path "src-tauri\target\$($BuildType)\bundle\msi" "*.msi"
            if (-not (Test-Path $msiPath)) {
                Write-ColorOutput "❌ MSI installer not found" "Red"
                $success = $false
            }

            # Check NSIS
            $nsisPath = Join-Path "src-tauri\target\$($BuildType)\bundle\nsis" "*-setup.exe"
            if (-not (Test-Path $nsisPath)) {
                Write-ColorOutput "❌ NSIS installer not found" "Red"
                $success = $false
            }
        }
        "macos" {
            # Check DMG
            $dmgPath = Join-Path "src-tauri\target\$($BuildType)\bundle\dmg" "*.dmg"
            if (-not (Test-Path $dmgPath)) {
                Write-ColorOutput "❌ DMG installer not found" "Red"
                $success = $false
            }
        }
        "linux" {
            # Check AppImage
            $appImagePath = Join-Path "src-tauri\target\$($BuildType)\bundle\appimage" "*.AppImage"
            if (-not (Test-Path $appImagePath)) {
                Write-ColorOutput "❌ AppImage not found" "Red"
                $success = $false
            }
        }
    }

    if ($success) {
        Write-ColorOutput "✅ All build artifacts found" "Green"
    } else {
        Write-ColorOutput "❌ Some build artifacts are missing" "Red"
        exit 1
    }
}

Export-ModuleMember -Function *
