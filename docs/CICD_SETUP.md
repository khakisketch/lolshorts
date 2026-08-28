# LoLShorts CI/CD Pipeline Setup Guide

This comprehensive guide covers the setup, configuration, and usage of LoLShorts' cross-platform CI/CD pipeline.

## 📋 Table of Contents

1. [Overview](#overview)
2. [Prerequisites](#prerequisites)
3. [GitHub Actions Setup](#github-actions-setup)
4. [Build Pipeline Architecture](#build-pipeline-architecture)
5. [Release Management](#release-management)
6. [Code Signing](#code-signing)
7. [Quality Assurance](#quality-assurance)
8. [Monitoring & Analytics](#monitoring--analytics)
9. [Development Workflow](#development-workflow)
10. [Troubleshooting](#troubleshooting)

## 🎯 Overview

LoLShorts uses a sophisticated CI/CD pipeline designed for cross-platform development (Windows + macOS) with comprehensive testing, security scanning, and automated releases.

### Key Features

- **Cross-Platform Builds**: Windows (.exe, .msi) and macOS (.dmg, .app)
- **Automated Testing**: Unit, integration, and E2E tests across platforms
- **Security Scanning**: Dependency audits, CodeQL analysis, and vulnerability scanning
- **Quality Gates**: Code formatting, linting, and performance benchmarks
- **Automated Releases**: Version tagging, artifact generation, and distribution
- **Monitoring**: Build health tracking, performance metrics, and error reporting

## 🔧 Prerequisites

### Required Tools

1. **Git**: Version control system
2. **Node.js**: 24.2.0 with npm 11.6.3 (frontend development and CI)
3. **Rust**: 1.94.1 (backend development and CI)
4. **FFmpeg**: Video processing (bundled with builds)

### Platform-Specific Requirements

#### Windows

- Windows 11 x64 for the formally supported release target
- Visual Studio Build Tools 2022
- WiX Toolset (for MSI installers)
- Windows SDK

#### macOS

- macOS 10.15+ (Catalina or later)
- Xcode Command Line Tools
- Create DMG tool

#### Linux (for CI/CD)

- Ubuntu 20.04+
- Basic build tools and libraries

### GitHub Repository Setup

1. Fork/clone the repository:

   ```bash
   git clone https://github.com/your-org/lolshorts.git
   cd lolshorts
   ```

2. Configure repository secrets:
   - Go to Settings → Secrets and variables → Actions
   - Add required secrets (see [Configuration](#configuration))

## 🚀 GitHub Actions Setup

### Workflow Files

The CI/CD pipeline consists of several workflow files in `.github/workflows/`:

1. **`ci-cross-platform.yml`**: Main CI pipeline with cross-platform builds
2. **`release-cross-platform.yml`**: Release automation and distribution
3. **`quality-assurance.yml`**: Code quality, security scanning, and testing
4. **`monitoring.yml`**: Build health monitoring and analytics

### Configuration

#### Required Repository Secrets

Add these secrets to your GitHub repository:

**Build & Signing:**

- `TAURI_PRIVATE_KEY`: Tauri private key for signing
- `TAURI_KEY_PASSWORD`: Password for Tauri private key

**Windows Code Signing:**

- `WINDOWS_CERTIFICATE_BASE64`: Base64-encoded code signing certificate
- `WINDOWS_CERTIFICATE_PASSWORD`: Certificate password
- `WINDOWS_CERTIFICATE_THUMBPRINT`: Certificate thumbprint

**macOS Code Signing:**

- `MACOS_CERTIFICATE_BASE64`: Base64-encoded macOS signing certificate
- `MACOS_CERTIFICATE_PASSWORD`: Certificate password
- `MACOS_SIGNING_IDENTITY`: Apple Developer signing identity
- `MACOS_KEYCHAIN_PASSWORD`: Keychain password

**Optional Integrations:**

- `DISCORD_WEBHOOK_URL`: For release notifications
- `LHCI_GITHUB_APP_TOKEN`: For Lighthouse CI integration

#### Environment Variables

Key environment variables used in workflows:

```yaml
env:
  CARGO_TERM_COLOR: always # Colorize Rust output
  RUST_BACKTRACE: 1 # Show Rust backtraces
  NODE_ENV: production # Node.js environment
```

## 🏗️ Build Pipeline Architecture

### Cross-Platform Matrix Strategy

The pipeline uses GitHub Actions matrix strategy to build across platforms:

```yaml
strategy:
  matrix:
    os: [windows-latest, macos-latest, ubuntu-latest]
    include:
      - os: windows-latest
        rust_target: x86_64-pc-windows-msvc
        artifact_pattern: "*.msi,*.exe"
      - os: macos-latest
        rust_target: x86_64-apple-darwin
        artifact_pattern: "*.dmg,*.app"
```

### Build Stages

1. **Setup & Dependencies**: Install tools and dependencies
2. **Code Quality**: Formatting, linting, and static analysis
3. **Testing**: Unit, integration, and E2E tests
4. **Building**: Platform-specific application builds
5. **Signing**: Code signing for security (if configured)
6. **Artifacts**: Package and upload build artifacts
7. **Validation**: Verify build integrity and functionality

### Caching Strategy

Multi-layer caching for performance optimization:

```yaml
# Rust dependencies
- name: Cache Cargo registry
  uses: actions/cache@v4
  with:
    path: ~/.cargo/registry
    key: ${{ runner.os }}-cargo-registry-${{ hashFiles('**/Cargo.lock') }}

# Node.js dependencies
- name: Cache Node modules
  uses: actions/cache@v4
  with:
    path: node_modules
    key: ${{ runner.os }}-node-${{ hashFiles('**/package-lock.json') }}

# Build artifacts
- name: Cache build artifacts
  uses: actions/cache@v4
  with:
    path: src-tauri/target
    key: ${{ runner.os }}-cargo-build-${{ hashFiles('**/Cargo.lock') }}
```

## 📦 Release Management

### Versioning Strategy

LoLShorts follows Semantic Versioning (SemVer):

- **Major** (1.0.0): Breaking changes, major features
- **Minor** (1.1.0): New features, backward compatible
- **Patch** (1.1.1): Bug fixes, security updates

### Release Process

#### Automated Releases

1. **Tag Creation**:

   ```bash
   git tag v1.2.3
   git push origin v1.2.3
   ```

2. **Trigger Pipeline**: Tag push automatically triggers release workflow

3. **Build & Sign**: Cross-platform builds with code signing

4. **Release Creation**: GitHub release with notes and artifacts

5. **Distribution**: Upload to GitHub Releases and update channels

#### Manual Releases

For hotfixes or special releases:

1. Create release branch: `git checkout -b release/v1.2.3`
2. Make necessary changes
3. Update version numbers in `package.json` and `Cargo.toml`
4. Commit and tag: `git tag v1.2.3`
5. Push and trigger release

### Release Channels

- **Stable**: Production releases (`v1.x.x`)
- **Prerelease**: Beta/alpha versions (`v1.2.3-beta.1`)
- **Development**: Development builds (from `develop` branch)

### Auto-Updater Configuration

Tauri auto-updater is configured to check for updates:

```toml
[tauri.updater]
active = true
endpoints = ["https://releases.lolshorts.com/updates.json"]
dialog = true
pubkey = "YOUR_PUBLIC_KEY"
```

## 🔐 Code Signing

### Windows Code Signing

#### Certificate Setup

1. Obtain a code signing certificate from a Certificate Authority (CA)
2. Export certificate as PFX with private key
3. Encode to base64 and add to GitHub secrets:

```bash
# Convert certificate to base64
base64 -i certificate.pfx
```

#### Signing Process

Windows installers are signed using `signtool`:

```powershell
signtool sign /f certificate.pfx /p password /t http://timestamp.digicert.com /fd sha256 installer.exe
```

### macOS Code Signing

#### Apple Developer Setup

1. Enroll in Apple Developer Program
2. Create Developer ID certificate
3. Download and install certificate on your Mac
4. Export certificate for CI/CD

#### Notarization Process

macOS apps are notarized through Apple's notary service:

```bash
# Notarize app
xcrun notarytool submit --apple-id "your@email.com" --password "app-specific-password" --team-id "TEAM_ID" --file app.dmg

# Staple notary ticket
xcrun stapler staple app.dmg
```

## 🧪 Quality Assurance

### Testing Strategy

#### Unit Tests

- **Backend**: Rust unit tests with `cargo test`
- **Frontend**: Jest tests for React components
- **Coverage**: Aim for >80% code coverage

#### Integration Tests

- **Backend**: Integration tests with `cargo test --test integration`
- **API**: Test Tauri command interfaces
- **External Services**: Mock external service calls

#### E2E Tests

- **Playwright**: Cross-browser E2E testing
- **Accessibility**: WCAG 2.1 AA compliance testing
- **Performance**: Lighthouse performance audits

### Code Quality Gates

#### Rust Quality Checks

```yaml
- name: Rust formatting check
  run: cargo fmt --all -- --check

- name: Clippy linting
  run: cargo clippy --all-targets -- -D warnings

- name: Security audit
  run: cargo audit
```

#### TypeScript Quality Checks

```yaml
- name: ESLint
  run: npm run lint

- name: Type checking
  run: npm run type-check

- name: Prettier formatting
  run: npx prettier --check "src/**/*.{ts,tsx}"
```

### Security Scanning

#### Dependency Security

- **cargo-audit**: Rust dependency vulnerability scanning
- **npm audit**: Node.js dependency security checks
- **cargo-deny**: License and policy compliance

#### Code Analysis

- **CodeQL**: GitHub's advanced code analysis
- **Semgrep**: Static analysis security scanning
- **Secret Scanning**: Detect hardcoded secrets and credentials

## 📊 Monitoring & Analytics

### Build Health Monitoring

#### Metrics Tracked

- **Build Time**: Total build duration
- **Bundle Size**: Frontend bundle size monitoring
- **Binary Size**: Application binary size tracking
- **Success Rate**: Build success/failure rates

#### Performance Monitoring

- **Benchmarking**: Performance regression detection
- **Memory Usage**: Memory leak detection
- **CPU Performance**: Performance profiling

### Error Tracking

#### Application Telemetry

Configured error tracking for production builds:

```toml
[telemetry]
enabled = true
endpoint = "https://telemetry.lolshorts.com"
sample_rate = 0.1
```

#### Error Types Monitored

- **Application Crashes**: Unhandled exceptions
- **Performance Issues**: Slow operations and timeouts
- **User Experience**: UI errors and interaction failures

### Analytics Dashboard

Real-time monitoring dashboard provides:

- Build health metrics
- Performance trends
- Security status
- Release statistics

## 👨‍💻 Development Workflow

### Local Development Setup

#### 1. Environment Setup

```bash
# Clone repository
git clone https://github.com/your-org/lolshorts.git
cd lolshorts

# Setup build environment
chmod +x scripts/build-setup.sh
./scripts/build-setup.sh

# Install dependencies
npm ci
```

#### 2. Development Server

```bash
# Start development server
npm run tauri:dev

# Or use cross-platform script
./scripts/build-cross-platform.ps1 -BuildType debug -SkipTests
```

#### 3. Local Testing

```bash
# Run all tests
npm test

# Run specific test suites
npm run test:unit      # Unit tests only
npm run test:integration # Integration tests only
npm run test:e2e      # E2E tests only
```

### Pre-Commit Workflow

#### 1. Code Quality Checks

```bash
# Rust formatting
cargo fmt

# Rust linting
cargo clippy

# Frontend linting
npm run lint

# Type checking
npm run type-check
```

#### 2. Testing

```bash
# Run unit tests
cargo test
npm test

# Run integration tests
cargo test --test integration
```

#### 3. Build Verification

```bash
# Verify build works locally
npm run tauri:build
```

### Branch Strategy

#### Main Branches

- **`main`/`master`**: Production-ready code
- **`develop`**: Integration branch for features
- **`feature/*`**: Feature development branches
- **`release/*`**: Release preparation branches
- **`hotfix/*`**: Emergency fixes

#### Branch Protection Rules

```yaml
# .github/branch-protection.yml
protection_rules:
  main:
    required_status_checks:
      strict: true
      contexts:
        - "CI - Cross-Platform Build and Test"
        - "Quality Assurance"
    enforce_admins: true
    required_pull_request_reviews:
      required_approving_review_count: 2
```

### Commit Guidelines

#### Commit Message Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

#### Types

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `style`: Code formatting
- `refactor`: Code refactoring
- `test`: Testing
- `chore`: Maintenance

#### Examples

```bash
feat(recording): add pentakill detection with 10s time window

Implements kill sequence tracking to detect multi-kills.
Uses sliding time window of 10 seconds to group consecutive
kills by the same player.

Closes #42

fix(video): prevent FFmpeg zombie processes

Ensures FFmpeg child processes are properly terminated
when video generation is cancelled.
```

## 🔧 Troubleshooting

### Common Build Issues

#### 1. Rust Compilation Errors

**Problem**: Rust compilation fails with dependency errors

**Solution**:

```bash
# Clean and rebuild
cargo clean
cargo update
cargo build --release

# Check for toolchain issues
rustup update
rustup component add rustfmt clippy
```

#### 2. Node.js Dependency Issues

**Problem**: npm install fails with permission errors

**Solution**:

```bash
# Clear npm cache
npm cache clean --force

# Delete node_modules and package-lock.json
rm -rf node_modules package-lock.json

# Reinstall dependencies
npm ci
```

#### 3. FFmpeg Issues

**Problem**: FFmpeg not found or version incompatible

**Solution**:

```powershell
# Windows release/CI, from the repository root
.\src-tauri\build_scripts\prepare_ffmpeg.ps1 -Source Download
```

```bash
# Experimental Linux/macOS jobs
cd src-tauri/build_scripts
./prepare_ffmpeg.sh
```

#### 4. Code Signing Issues

**Problem**: Code signing fails with certificate errors

**Solution**:

1. Verify certificate is valid and not expired
2. Check certificate password and thumbprint
3. Ensure timestamp server is accessible
4. Verify certificate has code signing extended key usage

### CI/CD Pipeline Issues

#### 1. Build Timeouts

**Problem**: CI jobs timeout due to long build times

**Solution**:

- Optimize caching strategy
- Reduce build complexity
- Increase timeout limits in workflows
- Use smaller, more frequent builds

#### 2. Memory Issues

**Problem**: CI runners run out of memory

**Solution**:

```yaml
# Use larger runners
runs-on: ubuntu-latest-4-cores

# Optimize build processes
env:
  CARGO_PROFILE_RELEASE_LTO: false
  CARGO_PROFILE_RELEASE_CODEGEN_UNITS: 1
```

#### 3. Test Failures

**Problem**: Tests fail intermittently in CI

**Solution**:

- Add retry logic for flaky tests
- Increase test timeouts
- Use deterministic test data
- Isolate tests from external dependencies

### Performance Issues

#### 1. Slow Builds

**Problem**: Build times are excessive

**Solution**:

- Enable parallel builds
- Optimize dependency caching
- Use pre-built binaries for large dependencies
- Consider build caching services

#### 2. Large Bundle Sizes

**Problem**: Frontend bundle is too large

**Solution**:

- Implement code splitting
- Optimize imports and tree shaking
- Compress assets
- Use dynamic imports for large libraries

## 📚 Additional Resources

### Documentation

- [Tauri Documentation](https://tauri.app/v1/guides/)
- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Rust Documentation](https://doc.rust-lang.org/)
- [React Documentation](https://react.dev/)

### Tools & Services

- [Codecov](https://codecov.io/): Code coverage tracking
- [Dependabot](https://dependabot.com/): Automated dependency updates
- [Sentry](https://sentry.io/): Error tracking and performance monitoring
- [Codecov](https://codecov.io/): Test coverage visualization

### Security

- [OWASP Security Guidelines](https://owasp.org/)
- [Rust Security Advisories](https://rustsec.org/)
- [Node.js Security](https://nodejs.org/en/security)

### Performance

- [Lighthouse CI](https://github.com/GoogleChrome/lighthouse-ci)
- [WebPageTest](https://www.webpagetest.org/)
- [Rust Benchmarking](https://doc.rust-lang.org/unstable-book/library-features/test.html)

---

For additional help or questions:

- Create an issue in the repository
- Join our Discord community
- Check the troubleshooting section above
- Review existing documentation and examples
