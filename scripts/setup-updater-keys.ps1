# LoLShorts - Tauri Auto-Updater Key Setup Script
# Run this script ONCE to generate signing keys for auto-updater
# The private key must be kept secret and stored in GitHub Secrets

$ErrorActionPreference = "Stop"

Write-Host "=== LoLShorts Auto-Updater Key Setup ===" -ForegroundColor Cyan
Write-Host ""

# Check if .tauri directory exists
$tauriDir = Join-Path $PSScriptRoot ".." ".tauri"
if (-not (Test-Path $tauriDir)) {
    New-Item -ItemType Directory -Path $tauriDir | Out-Null
    Write-Host "Created .tauri directory" -ForegroundColor Green
}

$keyPath = Join-Path $tauriDir "tauri.key"
$pubKeyPath = Join-Path $tauriDir "tauri.key.pub"

# Check if keys already exist
if ((Test-Path $keyPath) -or (Test-Path $pubKeyPath)) {
    Write-Host "WARNING: Keys already exist!" -ForegroundColor Yellow
    Write-Host "  Private key: $keyPath"
    Write-Host "  Public key:  $pubKeyPath"
    Write-Host ""
    $confirm = Read-Host "Do you want to regenerate? This will invalidate existing updates! (y/N)"
    if ($confirm -ne "y" -and $confirm -ne "Y") {
        Write-Host "Aborted." -ForegroundColor Red
        exit 0
    }
    Remove-Item $keyPath -ErrorAction SilentlyContinue
    Remove-Item $pubKeyPath -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "Generating Tauri signing keys..." -ForegroundColor Yellow
Write-Host ""

# Prompt for password
Write-Host "Enter a password for the private key (this will be TAURI_KEY_PASSWORD in GitHub Secrets):"
$password = Read-Host -AsSecureString
$passwordPlain = [Runtime.InteropServices.Marshal]::PtrToStringAuto(
    [Runtime.InteropServices.Marshal]::SecureStringToBSTR($password)
)

# Generate keys using Tauri CLI
try {
    Push-Location (Join-Path $PSScriptRoot "..")

    # Generate the key pair
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $passwordPlain
    npx tauri signer generate -w $keyPath

    if ($LASTEXITCODE -ne 0) {
        throw "Failed to generate keys"
    }

    Pop-Location
} catch {
    Pop-Location
    Write-Host "Error generating keys: $_" -ForegroundColor Red
    exit 1
}

# Read the public key
$pubKey = Get-Content $pubKeyPath -Raw
$pubKey = $pubKey.Trim()

Write-Host ""
Write-Host "=== Keys Generated Successfully! ===" -ForegroundColor Green
Write-Host ""
Write-Host "PUBLIC KEY (set as TAURI_UPDATER_PUBKEY in CI/build env):" -ForegroundColor Cyan
Write-Host $pubKey -ForegroundColor White
Write-Host ""
Write-Host "=== GitHub Secrets Required ===" -ForegroundColor Yellow
Write-Host ""
Write-Host "1. TAURI_PRIVATE_KEY:"
Write-Host "   Copy the entire contents of: $keyPath" -ForegroundColor White
Write-Host ""
Write-Host "2. TAURI_KEY_PASSWORD:"
Write-Host "   The password you just entered" -ForegroundColor White
Write-Host ""
Write-Host "3. TAURI_UPDATER_PUBKEY:"
Write-Host "   Copy the public key shown above into a GitHub Secret or Variable" -ForegroundColor White
Write-Host ""
Write-Host "=== Security Reminders ===" -ForegroundColor Red
Write-Host "- NEVER commit the private key (.tauri/tauri.key) to git"
Write-Host "- Add '.tauri/' to your .gitignore"
Write-Host "- Store the private key in a secure backup"
Write-Host ""

# Check .gitignore
$gitignorePath = Join-Path $PSScriptRoot ".." ".gitignore"
$gitignoreContent = Get-Content $gitignorePath -Raw -ErrorAction SilentlyContinue
if ($gitignoreContent -notmatch "\.tauri/") {
    Write-Host "Adding .tauri/ to .gitignore..." -ForegroundColor Yellow
    Add-Content $gitignorePath "`n# Tauri signing keys - NEVER COMMIT`n.tauri/"
    Write-Host "Done!" -ForegroundColor Green
}

Write-Host ""
Write-Host "Setup complete! Don't forget to configure TAURI_UPDATER_PUBKEY for release builds." -ForegroundColor Green
