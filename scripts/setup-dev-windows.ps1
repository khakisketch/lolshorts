# Development Environment Setup Script for Windows
# Run as Administrator for proper installation permissions

Write-Host "🚀 Setting up LoLShorts development environment on Windows..." -ForegroundColor Green

# Check if running as Administrator
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "⚠️  Warning: Not running as Administrator. Some installations may require elevated permissions." -ForegroundColor Yellow
}

# Install system dependencies
Write-Host "📦 Installing system dependencies..." -ForegroundColor Cyan

# Install Chocolatey if not present
if (!(Get-Command choco -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Chocolatey..."
    Set-ExecutionPolicy Bypass -Scope Process -Force
    [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
    iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
}

# Install FFmpeg
Write-Host "Installing FFmpeg..."
choco install ffmpeg -y

# Install Git
Write-Host "Installing Git..."
choco install git -y

# Install Visual Studio Build Tools
Write-Host "Installing Visual Studio Build Tools..."
choco install visualstudio2022buildtools -y --package-parameters "--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"

# Install Node.js
Write-Host "Installing Node.js..."
choco install nodejs -y

# Install Rust
Write-Host "Installing Rust..."
Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile "rustup-init.exe"
.\rustup-init.exe -y --default-toolchain stable
Remove-Item .\rustup-init.exe

# Refresh environment variables
$env:PATH = [System.Environment]::GetEnvironmentVariable("PATH", "Machine") + ";" + [System.Environment]::GetEnvironmentVariable("PATH", "User")

# Verify installations
Write-Host "✅ Verifying installations..." -ForegroundColor Cyan

# Check Rust
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    $rustVersion = cargo --version
    Write-Host "✅ Rust: $rustVersion" -ForegroundColor Green
} else {
    Write-Host "❌ Rust not found" -ForegroundColor Red
}

# Check Node.js
if (Get-Command node -ErrorAction SilentlyContinue) {
    $nodeVersion = node --version
    Write-Host "✅ Node.js: $nodeVersion" -ForegroundColor Green
} else {
    Write-Host "❌ Node.js not found" -ForegroundColor Red
}

# Check FFmpeg
if (Get-Command ffmpeg -ErrorAction SilentlyContinue) {
    $ffmpegVersion = ffmpeg -version | Select-String "ffmpeg version"
    Write-Host "✅ FFmpeg: $ffmpegVersion" -ForegroundColor Green
} else {
    Write-Host "❌ FFmpeg not found - please add FFmpeg to PATH" -ForegroundColor Red
}

# Install Tauri CLI
Write-Host "🔧 Installing Tauri CLI..."
cargo install tauri-cli --locked

# Install useful Rust development tools
Write-Host "🔧 Installing Rust development tools..."
cargo install cargo-watch
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-deny
cargo install cargo-expand

# Setup pre-commit hooks
Write-Host "🪝 Setting up pre-commit hooks..."
if (Test-Path ".git") {
    # Create pre-commit hook
    $preCommitHook = @"
#!/bin/sh
echo "Running pre-commit checks..."

# Rust formatting check
echo "Checking Rust formatting..."
cd src-tauri
if ! cargo fmt -- --check; then
    echo "❌ Rust code is not properly formatted. Run 'cargo fmt' to fix."
    exit 1
fi

# Rust linting
echo "Running Rust lints..."
if ! cargo clippy -- -D warnings; then
    echo "❌ Rust code has linting errors."
    exit 1
fi

# TypeScript formatting check
echo "Checking TypeScript formatting..."
cd ..
if ! npm run format:check; then
    echo "❌ TypeScript code is not properly formatted. Run 'npm run format' to fix."
    exit 1
fi

# TypeScript linting
echo "Running TypeScript lints..."
if ! npm run lint; then
    echo "❌ TypeScript code has linting errors."
    exit 1
fi

echo "✅ All pre-commit checks passed!"
"@

    $preCommitHook | Out-File -FilePath ".git\hooks\pre-commit" -Encoding ASCII
    git config core.filemode false
    Write-Host "✅ Pre-commit hooks installed" -ForegroundColor Green
} else {
    Write-Host "⚠️  Not in a git repository, skipping pre-commit hooks" -ForegroundColor Yellow
}

# Create development scripts
Write-Host "📝 Creating development scripts..."

$devScript = @"
# Development helper script for LoLShorts

function dev-rust() {
    param(
        [Parameter(Mandatory=`$false)]
        [string]`$action = "dev"
    )

    Write-Host "Running Rust development server..." -ForegroundColor Cyan
    cd src-tauri

    switch (`$action) {
        "dev" { cargo run --bin lolshorts-tauri }
        "build" { cargo build }
        "test" { cargo test }
        "check" { cargo check }
        "fmt" { cargo fmt }
        "lint" { cargo clippy -- -D warnings }
        "bench" { cargo bench }
        default { Write-Host "Unknown action: `$action" -ForegroundColor Red }
    }

    cd ..
}

function dev-frontend() {
    param(
        [Parameter(Mandatory=`$false)]
        [string]`$action = "dev"
    )

    Write-Host "Running frontend development server..." -ForegroundColor Cyan

    switch (`$action) {
        "dev" { npm run dev }
        "build" { npm run build }
        "preview" { npm run preview }
        "type-check" { npm run type-check }
        "lint" { npm run lint }
        "format" { npm run format }
        default { Write-Host "Unknown action: `$action" -ForegroundColor Red }
    }
}

function dev-full() {
    Write-Host "Starting full development environment..." -ForegroundColor Cyan
    Start-Job -ScriptBlock { dev-frontend dev }
    Start-Sleep -Seconds 2
    dev-rust dev
}

# Add to PowerShell profile
Export-ModuleMember -Function dev-rust, dev-frontend, dev-full
"@

$devScript | Out-File -FilePath "dev-helper.ps1" -Encoding UTF8
Write-Host "✅ Development helper script created: dev-helper.ps1" -ForegroundColor Green

Write-Host ""
Write-Host "🎉 Development environment setup complete!" -ForegroundColor Green
Write-Host ""
Write-Host "📋 Next steps:" -ForegroundColor Cyan
Write-Host "1. Restart your terminal or run 'refreshenv' to update PATH"
Write-Host "2. Install frontend dependencies: npm ci"
Write-Host "3. Run Rust development: ./dev-helper.ps1; dev-rust"
Write-Host "4. Run frontend development: ./dev-helper.ps1; dev-frontend"
Write-Host "5. Run full development: ./dev-helper.ps1; dev-full"
Write-Host ""
Write-Host "🔧 Useful commands:" -ForegroundColor Cyan
Write-Host "- Build project: npm run tauri build"
Write-Host "- Run tests: npm run test"
Write-Host "- Format code: npm run format"
Write-Host "- Check for security vulnerabilities: cargo audit --file Cargo.lock"
Write-Host ""
Write-Host "⚠️  Important Notes:" -ForegroundColor Yellow
Write-Host "- Make sure FFmpeg is in your system PATH"
Write-Host "- On Windows, you may need to restart your computer for all PATH changes to take effect"
Write-Host "- For Tauri development, install the Tauri VSCode extension for better IDE support"
