# Cross-Platform Development Guide

This document outlines the cross-platform implementation of LoLShorts, providing guidelines for developers working on the Windows, macOS, and Linux versions of the application.

> **Support boundary:** the free public release currently supports only
> Windows 11 x64 with NVIDIA NVENC. macOS, Linux, AMD, Intel, and CPU paths are
> experimental development targets; their code and manual CI do not constitute
> release support or replace the Windows/NVIDIA E5 field gates.

## Overview

LoLShorts is built using a cross-platform architecture with:

- **Backend**: Rust with Tauri framework
- **Frontend**: React with TypeScript
- **Video Processing**: FFmpeg (cross-platform)
- **Platform-specific APIs**: Windows Graphics Capture, macOS AVFoundation/ScreenCaptureKit, Linux X11/Pipewire

## Architecture

### Platform Abstraction Layer

The recording system uses a platform abstraction layer located in `src-tauri/src/recording/platform/`:

```
platform/
├── mod.rs              # Main module exports
├── types.rs            # Common types and error handling
├── backend.rs          # CaptureBackend trait and interfaces
├── factory.rs          # Cross-platform backend factory
├── windows/            # Windows-specific implementations
│   └── mod.rs          # Windows Graphics Capture, Direct3D
├── macos/              # macOS-specific implementations
│   └── mod.rs          # AVFoundation, ScreenCaptureKit
└── linux/              # Linux-specific implementations
    └── mod.rs          # X11, Pipewire
```

### Core Components

1. **CaptureBackend Trait**: Defines common interface for all platform backends
2. **BackendFactory**: Creates appropriate backend for current platform
3. **PlatformCapabilities**: Describes what each backend supports
4. **Cross-platform Configuration**: Unified config across all platforms

## Platform-Specific Details

### Windows

**Supported Backends:**
- **Windows Graphics Capture API** (Preferred): Modern, GPU-accelerated capture with window borders
- **Direct3D**: Alternative for systems without Graphics Capture
- **FFmpeg**: Fallback for maximum compatibility

**Capabilities:**
- Hardware encoding via VideoToolbox
- Window and screen capture
- No audio capture (handled separately)

**Requirements:**
- Windows 10 version 1903 or later
- DirectX 11 compatible GPU
- Visual Studio Build Tools

### macOS

**Supported Backends:**
- **ScreenCaptureKit** (Preferred): Modern API on macOS 12.3+
- **AVFoundation**: Legacy support for older macOS versions
- **FFmpeg**: Fallback for compatibility

**Capabilities:**
- Hardware encoding via VideoToolbox
- Screen, window, and region capture
- Audio capture via Core Audio

**Requirements:**
- macOS 11.0+ (Big Sur) for AVFoundation
- macOS 12.3+ for ScreenCaptureKit
- Xcode Command Line Tools

### Linux

**Supported Backends:**
- **Pipewire** (Preferred): Modern display server protocol
- **X11**: Traditional X Window System
- **FFmpeg**: Fallback for compatibility

**Capabilities:**
- Software encoding (hardware varies by GPU/driver)
- Screen and window capture
- Audio capture via PulseAudio/ALSA

**Requirements:**
- Linux kernel 5.10+ for Pipewire
- X11 for legacy support
- FFmpeg development libraries

## Development Setup

### Prerequisites

1. **Rust**: 1.94.1 (repository-pinned)
2. **Node.js**: 24.2.0 with npm 11.6.3 (repository-pinned)
3. **FFmpeg**: Version 4.4 or later
4. **Platform-specific tools**:

#### Windows
```powershell
# Install via Chocolatey
choco install ffmpeg nodejs visualstudio2022buildtools

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

#### macOS
```bash
# Install via Homebrew
brew install ffmpeg node rust

# Install Xcode Command Line Tools
xcode-select --install
```

#### Linux
```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install ffmpeg nodejs npm pkg-config libssl-dev

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Quick Setup

Use the provided setup scripts:

```bash
# Windows
.\scripts\setup-dev-windows.ps1

# macOS
source scripts/setup-dev-macos.sh

# Linux
source scripts/setup-dev-linux.sh
```

### Manual Setup

1. Clone repository:
```bash
git clone https://github.com/your-repo/lolshorts.git
cd lolshorts
```

2. Install frontend dependencies:
```bash
npm ci
```

3. Install Rust tools:
```bash
cargo install tauri-cli --locked
cargo install cargo-watch
```

4. Run development server:
```bash
npm run tauri dev
```

## Building

### Development Build
```bash
npm run tauri dev
```

### Production Build
```bash
npm run tauri build
```

### Cross-Platform Builds
Use GitHub Actions or build manually:

```bash
# Windows (in Windows environment)
npm run tauri build --target x86_64-pc-windows-msvc

# macOS (in macOS environment)
npm run tauri build --target x86_64-apple-darwin

# Linux (in Linux environment)
npm run tauri build --target x86_64-unknown-linux-gnu
```

## Testing

### Unit Tests
```bash
# Rust tests
cargo test

# Frontend tests
npm test
```

### Cross-Platform Tests
```bash
# Platform detection tests
cargo run --bin test_platform_detection

# Cross-platform compilation tests
cargo test cross_platform_compilation
```

### Integration Tests
```bash
# Run all integration tests
cargo test --test '*'

# Run specific integration test
cargo test cross_platform_compilation
```

## Code Quality

### Pre-commit Hooks
Pre-commit hooks automatically check:
- Rust formatting (`cargo fmt`)
- Rust linting (`cargo clippy`)
- TypeScript formatting (`prettier`)
- TypeScript linting (`eslint`)
- Security audits (`cargo audit`, `npm audit`)

### Manual Checks
```bash
# Rust
cargo fmt --check
cargo clippy -- -D warnings
cargo audit

# TypeScript
npm run format:check
npm run lint
npm run type-check
```

## Platform-Specific Development

### Adding New Backends

1. Create backend implementation in `platform/{platform}/mod.rs`
2. Implement `CaptureBackend` trait
3. Add to platform-specific factory
4. Update `BackendSelector` for automatic detection

### Backend Implementation Template

```rust
use async_trait::async_trait;
use lolshorts::recording::platform::{
    CaptureBackend, CaptureConfig, CaptureResult, CaptureStats,
    CaptureStatus, PlatformCapabilities, DisplayInfo, AudioDeviceInfo
};

pub struct NewBackend {
    // Backend-specific fields
}

#[async_trait]
impl CaptureBackend for NewBackend {
    async fn initialize(&mut self, config: CaptureConfig) -> CaptureResult<()> {
        // Implementation
    }

    async fn start_capture(&mut self) -> CaptureResult<()> {
        // Implementation
    }

    // ... implement all required methods
}
```

## Troubleshooting

### Common Issues

#### Windows
- **Build errors**: Ensure Visual Studio Build Tools are installed
- **FFmpeg not found**: Add FFmpeg to system PATH or bundle with application
- **Permission errors**: Run as administrator for screen capture permissions

#### macOS
- **Build errors**: Install Xcode Command Line Tools: `xcode-select --install`
- **Screen recording permission**: Grant permission in System Preferences > Security & Privacy
- **FFmpeg linking**: Use Homebrew FFmpeg or static linking

#### Linux
- **Missing dependencies**: Install development libraries for your distribution
- **Wayland support**: Additional setup required for Wayland compositors
- **Audio permissions**: Add user to audio group: `sudo usermod -a -G audio $USER`

### Debugging

Enable debug logging:
```bash
RUST_LOG=debug npm run tauri dev
```

Platform-specific debugging:
```bash
# Test platform detection
cargo run --bin test_platform_detection

# Test specific backend
RUST_LOG=debug cargo test --test cross_platform_compilation
```

## Performance Optimization

### Platform-Specific Optimizations

#### Windows
- Use Windows Graphics Capture API for GPU-accelerated capture
- Enable hardware encoding via NVENC/Intel QSV/AMD VCE
- Optimize frame buffer allocation

#### macOS
- Use ScreenCaptureKit for efficient capture on macOS 12.3+
- Leverage VideoToolbox for hardware encoding
- Optimize Metal pipeline for frame processing

#### Linux
- Use Pipewire for efficient capture on modern systems
- Optimize X11 shared memory for zero-copy capture
- Consider GPU acceleration via VA-API/NVENC

### Cross-Platform Optimizations

- Profile with `cargo flamegraph` on each platform
- Use platform-specific SIMD optimizations
- Implement efficient memory pools
- Optimize async task scheduling

## Deployment

### Platform-Specific Installers

The build process generates:
- **Windows**: `.exe` installer with bundled dependencies
- **macOS**: `.dmg` disk image with notarization
- **Linux**: `.AppImage` or `.deb` package

### Code Signing

#### Windows
```bash
# Sign with certificate
signtool sign /f certificate.p12 /p password /t http://timestamp.digicert.com lolshorts.exe
```

#### macOS
```bash
# Sign with Apple Developer certificate
codesign --deep --force --verify --verbose --sign "Developer ID Application" LoLShorts.app

# Notarize
xcrun altool --notarize-app --primary-bundle-id com.lolshorts.app --username "apple@id.com" --password "@keychain:AC_PASSWORD" --file LoLShorts.dmg
```

## Continuous Integration

The project uses GitHub Actions for:
- Cross-platform building
- Automated testing
- Security scanning
- Performance benchmarking

See `.github/workflows/` for detailed configuration.

## Contributing

When contributing to cross-platform features:

1. Test on all supported platforms (or use CI)
2. Update platform-specific documentation
3. Add appropriate unit tests
4. Consider platform-specific optimizations
5. Update CI configuration if needed

## Resources

### Rust Cross-Platform Resources
- [Rust Platform Support](https://doc.rust-lang.org/rustc/platform-support.html)
- [Tauri Cross-Platform Guide](https://tauri.app/v1/guides/building/cross-platform/)

### Platform APIs
- [Windows Graphics Capture API](https://docs.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture)
- [macOS ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit)
- [Linux Pipewire](https://pipewire.org/)

### FFmpeg Resources
- [FFmpeg Documentation](https://ffmpeg.org/documentation.html)
- [Cross-Platform FFmpeg Builds](https://ffmpeg.org/download.html)

## Security Considerations

### Platform-Specific Security
- **Windows**: Handle UAC and Windows Defender properly
- **macOS**: Manage sandboxing and notarization requirements
- **Linux**: Handle AppArmor/SELinux and package signing

### General Security
- Validate all platform-specific inputs
- Handle cross-platform path differences securely
- Implement proper error handling for platform APIs
- Use secure IPC for cross-platform communication
