# LoLShorts Build Scripts

Automated scripts for preparing the production build environment.

## Scripts Overview

### `verify_environment.ps1`
**Purpose**: Verify all build prerequisites are installed and properly configured

**What it checks**:
- ✅ Rust and Cargo installation
- ✅ Node.js (v18+) and npm
- ✅ WiX Toolset (for MSI installer)
- ✅ Visual Studio Build Tools
- ✅ Tauri CLI
- ✅ FFmpeg binaries presence
- ✅ Node dependencies (node_modules)

**Usage**:
```powershell
.\verify_environment.ps1
```

**Output**:
- Green ✅ - All requirements met
- Yellow ⚠️ - Warning (non-critical)
- Red ❌ - Critical issue (must fix)

---

### `prepare_ffmpeg.ps1`
**Purpose**: Validate and prepare reproducible Windows FFmpeg sidecars for Tauri

**What it does**:
1. Resolves `src-tauri/binaries` from the script location, independent of the caller's current directory
2. Reuses existing sidecars only after executable `-version` validation
3. In `Auto` mode, copies validated real system executables while rejecting package-manager shims
4. If system tools are unavailable, downloads an immutable BtbN archive and verifies its pinned SHA-256
5. Copies triplet-named `ffmpeg` and `ffprobe` sidecars required by Tauri
6. Cleans only the uniquely named, validated temporary directory it created

**Usage**:
```powershell
# From the repository root; prefer validated system tools and download if absent
.\src-tauri\build_scripts\prepare_ffmpeg.ps1 -Source Auto

# Release/CI: always use the checksum-pinned archive
.\src-tauri\build_scripts\prepare_ffmpeg.ps1 -Source Download
```

**Requirements**:
- A working system FFmpeg/ffprobe pair, or an internet connection
- ~150 MB free disk space
- PowerShell 5.1+

**Output Location**:
- `src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe`
- `src-tauri/binaries/ffprobe-x86_64-pc-windows-msvc.exe`

---

## Quick Start

### First-Time Setup

1. **Verify environment**:
```powershell
cd src-tauri\build_scripts
.\verify_environment.ps1
```

2. **Fix any critical issues** reported by verification script

3. **Prepare FFmpeg**:
```powershell
cd ..\..
.\src-tauri\build_scripts\prepare_ffmpeg.ps1 -Source Auto
```

4. **Build the app**:
```powershell
npm run tauri:build
```

### Subsequent Builds

If FFmpeg is already prepared:
```powershell
npm run tauri:build
```

### Clean Build

```powershell
# Clean previous builds
cd src-tauri
cargo clean
cd ..

# Re-verify environment
cd src-tauri\build_scripts
.\verify_environment.ps1

# Rebuild
cd ..\..
npm run tauri:build
```

## Build Output

After successful build, installers are located in:
```
src-tauri/target/release/bundle/
├── nsis/
│   └── LoLShorts_1.2.0_x64-setup.exe
├── msi/
│   └── LoLShorts_1.2.0_x64_en-US.msi
└── LoLShorts.exe
```

## Troubleshooting

### "WiX toolset not found"
**Solution**: Download and install WiX Toolset v3.14+
- Download: https://wixtoolset.org/releases/
- Add to PATH: `C:\Program Files (x86)\WiX Toolset v3.14\bin`
- Restart PowerShell

### "FFmpeg download failed"
**Solution**:
- Check internet connection
- Do not switch release builds to a mutable `latest` URL
- Install FFmpeg locally and use `-Source System`, or provide explicit validated paths:
```powershell
.\src-tauri\build_scripts\prepare_ffmpeg.ps1 -Source System `
  -FfmpegPath C:\tools\ffmpeg\bin\ffmpeg.exe `
  -FfprobePath C:\tools\ffmpeg\bin\ffprobe.exe
```

### "Node modules not found"
**Solution**:
```powershell
cd ../..
npm ci
```

### "Rust compilation errors"
**Solution**:
```powershell
# Install the repository-pinned toolchain
rustup toolchain install 1.94.1

# Clean and rebuild
cargo clean
cargo build --release
```

## Advanced Usage

### Build Specific Installer Type

```powershell
# MSI only
npm run tauri -- build --bundles msi

# NSIS only
npm run tauri -- build --bundles nsis

# Portable exe only
npm run tauri -- build --bundles app
```

### Development Build

```powershell
# Run in dev mode (no installer creation)
npm run tauri:dev
```

### Custom FFmpeg Source

If you need a specific FFmpeg version:

1. Download from https://ffmpeg.org/download.html
2. Extract `ffmpeg.exe` and `ffprobe.exe`
3. Run `prepare_ffmpeg.ps1 -Source System` with `-FfmpegPath` and `-FfprobePath`
4. Keep `-Source Download` for release builds so the release provenance remains reproducible

## CI/CD Integration

These scripts can be used in GitHub Actions or other CI systems:

```yaml
- name: Verify Build Environment
  run: |
    cd src-tauri/build_scripts
    .\verify_environment.ps1

- name: Prepare FFmpeg
  run: |
    .\src-tauri\build_scripts\prepare_ffmpeg.ps1 -Source Download

- name: Build Release
  run: npm run tauri:build
```

## Additional Resources

- [BUILD_GUIDE.md](../../BUILD_GUIDE.md) - Complete build documentation
- [Tauri Documentation](https://tauri.app/v2/guides/)
- [WiX Toolset Documentation](https://wixtoolset.org/documentation/)
- [FFmpeg Documentation](https://ffmpeg.org/documentation.html)

## Support

For build issues:
- Check [BUILD_GUIDE.md](../../BUILD_GUIDE.md) for detailed troubleshooting
- Run `verify_environment.ps1` to diagnose environment issues
- Check GitHub Issues for known build problems
