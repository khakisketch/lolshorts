# LoLShorts Production Build Guide

Complete guide for building installer artifacts for LoLShorts. Building or signing an artifact is not the same as public installer or updater readiness. Public readiness still requires the field checks in `docs/FIELD_QA_COMMERCIAL_READINESS.md` and the service-readiness limits in `docs/SERVICE_READINESS_POLICY.md`.

## Prerequisites

### Required Software
- **Rust** (latest stable) - [Install](https://www.rust-lang.org/tools/install)
- **Node.js 18+** - [Install](https://nodejs.org/)
- **Tauri CLI** - Installed via npm (see below)
- **WiX Toolset 3.14+** - [Download](https://wixtoolset.org/releases/)
  - Required for MSI installer generation
  - Add to PATH: `C:\Program Files (x86)\WiX Toolset v3.14\bin`
- **Visual Studio Build Tools** - C++ build tools
- **PowerShell 5.1+** (pre-installed on Windows 10/11)

### Verify Prerequisites
```bash
# Check Rust
rustc --version
cargo --version

# Check Node.js
node --version
npm --version

# Check WiX (after installation)
candle -?
light -?

# Check PowerShell
$PSVersionTable.PSVersion
```

## Build Process

### Step 1: Install Dependencies

```bash
# Install Node dependencies
npm install

# Install Tauri CLI (if not already installed)
npm install -g @tauri-apps/cli

# Verify Tauri installation
cargo tauri --version
```

### Step 2: Prepare FFmpeg Binaries

FFmpeg is required for video processing and must be bundled with the installer.

**Option A: Automated Script (Recommended)**
```powershell
# Navigate to build scripts directory
cd src-tauri\build_scripts

# Run FFmpeg preparation script
.\prepare_ffmpeg.ps1
```

**Option B: Manual Download**
1. Download FFmpeg from: https://github.com/BtbN/FFmpeg-Builds/releases
2. Extract `ffmpeg.exe` and `ffprobe.exe`
3. Create `src-tauri/bin/` directory
4. Copy both executables to `src-tauri/bin/`

**Verify FFmpeg Setup**
```bash
# Check if binaries exist
ls src-tauri/bin/ffmpeg.exe
ls src-tauri/bin/ffprobe.exe

# Test FFmpeg
src-tauri/bin/ffmpeg.exe -version
```

### Step 3: Configure Environment

Create `.env` file in project root with production values:

```bash
# Supabase Configuration
SUPABASE_URL=https://your-project.supabase.co
SUPABASE_ANON_KEY=your-anon-key

# Optional: Development overrides
DATABASE_URL=sqlite://lolshorts.db
```

### Step 4: Build Production Release

Before building installer artifacts for a candidate, run the local verification gates:

```bash
npm run verify:frontend
npm run audit:runtime
npm run audit:moderate

cd src-tauri
cargo check
cargo test
cd ..
```

Passing these commands only proves local automated behavior. It does not authorize public readiness, payment keys, paid access, or live Toss integration.

**Development Build (Testing)**
```bash
# Run in development mode
npm run tauri:dev
```

**Production Build (All Installers)**
```bash
# Clean previous builds
cargo clean

# Build for production
cargo tauri build

# This creates:
# - NSIS installer (.exe)
# - MSI installer (.msi)
# - Portable executable
```

**Build Specific Installer Type**
```bash
# Build only MSI
cargo tauri build --bundles msi

# Build only NSIS
cargo tauri build --bundles nsis

# Build without bundling (standalone exe)
cargo tauri build --bundles app
```

### Step 5: Locate Built Artifacts

After successful build, installers are located in:

```
src-tauri/target/release/bundle/
├── nsis/
│   └── LoLShorts_0.1.0_x64-setup.exe    (~75-100 MB with FFmpeg)
├── msi/
│   └── LoLShorts_0.1.0_x64_en-US.msi    (~75-100 MB with FFmpeg)
└── LoLShorts.exe                         (standalone executable)
```

## Build Configuration

### Installer Features

**NSIS Installer (.exe)**
- User-friendly wizard interface
- Custom install location
- Start menu shortcuts
- Uninstaller
- File associations
- Auto-update support

**MSI Installer (.msi)**
- Enterprise deployment friendly
- Group Policy support
- Silent installation options
- Standardized Windows installation
- Better for IT departments

### Bundle Configuration

Edit `src-tauri/tauri.conf.json` for customization:

```json
{
  "bundle": {
    "active": true,
    "targets": ["nsis", "msi"],
    "externalBin": [
      "bin/ffmpeg.exe",
      "bin/ffprobe.exe"
    ],
    "windows": {
      "wix": {
        "language": ["en-US"]
      },
      "nsis": {
        "installMode": "currentUser"
      }
    }
  }
}
```

## Code Signing (Production)

### Prerequisites
- Code signing certificate (.pfx or .p12)
- Certificate password

### Windows Configuration

**Option 1: Certificate Thumbprint**
```json
{
  "bundle": {
    "windows": {
      "certificateThumbprint": "YOUR_THUMBPRINT_HERE",
      "digestAlgorithm": "sha256",
      "timestampUrl": "http://timestamp.digicert.com"
    }
  }
}
```

**Option 2: Environment Variables**
```bash
# Set certificate path and password
$env:TAURI_SIGNING_PRIVATE_KEY = "C:\path\to\certificate.pfx"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "your_password"

# Build with signing
cargo tauri build
```

**SignTool Manual Signing**
```powershell
# Sign after build
signtool sign /f certificate.pfx /p password /t http://timestamp.digicert.com `
  src-tauri/target/release/bundle/nsis/LoLShorts_0.1.0_x64-setup.exe
```

## Testing Installers

The checklist below is a build validation aid. Do not use it by itself to claim signed installer, updater, clean install, upgrade, rollback, or uninstall readiness. Those claims require real Windows field evidence recorded in `docs/FIELD_QA_COMMERCIAL_READINESS.md`.

### Test Checklist

**NSIS Installer Testing**
- [ ] Run installer without admin rights
- [ ] Check custom install location works
- [ ] Verify start menu shortcuts created
- [ ] Test application launches
- [ ] Verify FFmpeg bundled and working
- [ ] Check update mechanism
- [ ] Test uninstaller removes all files

**MSI Installer Testing**
- [ ] Run MSI with `/quiet` for silent install
- [ ] Verify registry entries created
- [ ] Test repair installation
- [ ] Check uninstall through Control Panel
- [ ] Test on different Windows versions (10, 11)

**Functional Testing**
- [ ] Authentication system works
- [ ] League of Legends detection
- [ ] Recording functionality
- [ ] Video processing (FFmpeg operations)
- [ ] Screenshot capture
- [ ] PRO features properly gated

### Test Commands

```powershell
# Silent MSI install
msiexec /i LoLShorts_0.1.0_x64_en-US.msi /quiet /l*v install.log

# Silent MSI uninstall
msiexec /x LoLShorts_0.1.0_x64_en-US.msi /quiet

# NSIS silent install
.\LoLShorts_0.1.0_x64-setup.exe /S

# NSIS silent uninstall
.\uninstall.exe /S
```

## Troubleshooting

### Common Issues

**Issue: "WiX toolset not found"**
```bash
# Solution: Install WiX and add to PATH
# Download: https://wixtoolset.org/releases/
# Add to PATH: C:\Program Files (x86)\WiX Toolset v3.14\bin
```

**Issue: "FFmpeg not found in bundle"**
```bash
# Solution: Re-run FFmpeg preparation
cd src-tauri\build_scripts
.\prepare_ffmpeg.ps1

# Verify binaries exist
ls ..\bin\ffmpeg.exe
ls ..\bin\ffprobe.exe
```

**Issue: "Failed to sign executable"**
```bash
# Solution: Check certificate configuration
# Ensure certificate is valid and password is correct
# Try building without signing first
```

**Issue: "Build fails with linking errors"**
```bash
# Solution: Clean build and rebuild
cargo clean
cargo tauri build
```

**Issue: "Installer too large (>200MB)"**
```bash
# Solution: Verify FFmpeg binary size
# Should be ~75-100MB for FFmpeg + app
# If larger, check for debug symbols

# Build with --release flag
cargo tauri build --release
```

## Optimization

### Reduce Installer Size

1. **Strip Debug Symbols** (already done in release mode)
```toml
# Cargo.toml
[profile.release]
strip = true
lto = true
```

2. **Use Smaller FFmpeg Build**
- Download "essentials" build instead of "full"
- Removes unused codecs and filters
- Reduces size by ~30-40%

3. **Compress Resources**
- Use WebP for images
- Minify JSON/CSS files
- Enable UPX compression (optional)

### Build Performance

```bash
# Parallel compilation (faster builds)
cargo build --release --jobs 8

# Use sccache for faster rebuilds
cargo install sccache
$env:RUSTC_WRAPPER = "sccache"
```

## Distribution

### Release Checklist

- [ ] Version number updated (`tauri.conf.json`, `Cargo.toml`, `package.json`)
- [ ] Changelog updated (`CHANGELOG.md`)
- [ ] All tests passing
- [ ] Code signed (production)
- [ ] Installers tested on clean Windows install
- [ ] Update documentation
- [ ] Create GitHub release
- [ ] Upload installers to release
- [ ] Announce release to users

### Upload to GitHub Releases

```bash
# Create release tag
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0

# Upload installers via GitHub web interface or CLI
# Include:
# - LoLShorts_0.1.0_x64-setup.exe (NSIS)
# - LoLShorts_0.1.0_x64_en-US.msi (MSI)
# - CHANGELOG.md
# - Installation instructions
```

## Continuous Integration (Optional)

### GitHub Actions Example

```yaml
name: Build Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build-windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Install WiX
        run: |
          choco install wixtoolset
      - name: Install dependencies
        run: npm install
      - name: Prepare FFmpeg
        run: |
          cd src-tauri/build_scripts
          .\prepare_ffmpeg.ps1
      - name: Build Tauri App
        run: cargo tauri build
      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: installers
          path: |
            src-tauri/target/release/bundle/nsis/*.exe
            src-tauri/target/release/bundle/msi/*.msi
```

## Support

For build issues or questions:
- GitHub Issues: https://github.com/yourusername/lolshorts/issues
- Documentation: https://docs.lolshorts.com
- Discord: https://discord.gg/lolshorts

---

**Ready to Build?**

```bash
# Quick start (automated)
cd src-tauri/build_scripts
.\prepare_ffmpeg.ps1
cd ../..
cargo tauri build

# Check output in: src-tauri/target/release/bundle/
```
