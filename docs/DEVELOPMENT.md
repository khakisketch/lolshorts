# LoLShorts Development Guide

Complete guide for setting up your development environment and building LoLShorts from source.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Project Setup](#project-setup)
- [Development Workflow](#development-workflow)
- [Project Structure](#project-structure)
- [Testing](#testing)
- [Debugging](#debugging)
- [Common Issues](#common-issues)

---

## Prerequisites

### Required Software

#### Node.js and npm
- **Node.js 24.2.0 and npm 11.6.3** (pinned by `.nvmrc`, `package.json`, and CI)
- [Download](https://nodejs.org/)
- Verify installation:
  ```bash
  node --version
  npm --version
  ```

#### Rust
- **Rust 1.94.1** (pinned by `rust-toolchain.toml`)
- [Install Rust](https://www.rust-lang.org/tools/install)
- Verify installation:
  ```bash
  rustc --version
  cargo --version
  ```

#### FFmpeg
- **FFmpeg 6.0+** with gdigrab support (Windows) or avfoundation (macOS)
- Windows: Download from [FFmpeg Builds](https://github.com/BtbN/FFmpeg-Builds/releases)
  - Run `prepare_ffmpeg.ps1 -Source System` with the extracted `ffmpeg.exe`
    and `ffprobe.exe` paths; do not manually invent Tauri sidecar names
- Verify installation:
  ```bash
  ffmpeg -version
  ffprobe -version
  ```

#### C/C++ Build Tools
- **Windows**: Visual Studio Build Tools (C++ development tools)
  - [Download](https://visualstudio.microsoft.com/downloads/)
  - Select "Desktop development with C++"
- **macOS**: Xcode Command Line Tools
  ```bash
  xcode-select --install
  ```
- **Linux**: gcc/clang toolchain
  ```bash
  # Ubuntu/Debian
  sudo apt-get install build-essential
  ```

### Platform-Specific Requirements

#### Windows
- Windows 10 (21H2) or Windows 11
- Git for Windows
- PowerShell 5.1+ (pre-installed)

#### macOS
- macOS 12.0 or later
- Xcode 13+

#### Linux
- Ubuntu 20.04+ or equivalent
- X11 development libraries:
  ```bash
  sudo apt-get install libx11-dev libxrandr-dev libxcb1-dev
  ```

---

## Project Setup

### 1. Clone Repository

```bash
git clone https://github.com/KhakiSkech/lolshorts.git
cd lolshorts
```

### 2. Install Node Dependencies

```bash
npm install
```

This installs all frontend and development dependencies defined in `package.json`.

### 3. Prepare FFmpeg Binaries

FFmpeg must be present as triplet-named Tauri sidecars in `src-tauri/binaries/`
for the desktop app to build.

**Option A: Automated Setup (Recommended)**

```powershell
# On Windows, from the repository root. This validates existing/system tools
# and otherwise uses the checksum-pinned archive.
.\src-tauri\build_scripts\prepare_ffmpeg.ps1 -Source Auto
```

```bash
# On macOS/Linux
cd src-tauri/build_scripts
./prepare_ffmpeg.sh
cd ../..
```

**Option B: Manual Setup**

1. Download FFmpeg from [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds/releases)
2. Extract `ffmpeg.exe` and `ffprobe.exe` (Windows) or `ffmpeg` and `ffprobe` (macOS/Linux)
3. On Windows, pass both extracted executable paths to
   `prepare_ffmpeg.ps1 -Source System`; the script validates and names them
   correctly under `src-tauri/binaries/`
4. Release automation must continue to use `-Source Download` so the immutable
   URL and SHA-256 contract are enforced

Verify FFmpeg setup:
```powershell
Get-ChildItem src-tauri/binaries/*-x86_64-pc-windows-msvc.exe
```

Output should show:
```
ffmpeg-x86_64-pc-windows-msvc.exe
ffprobe-x86_64-pc-windows-msvc.exe
```

### 4. Environment Configuration

Create `.env` file in project root for development:

```bash
# Supabase Configuration (required)
SUPABASE_URL=https://your-project.supabase.co
SUPABASE_ANON_KEY=your-anon-key

# Optional: Override database URL
DATABASE_URL=sqlite://lolshorts.db

# Optional: Logging level
RUST_LOG=info
```

Get Supabase credentials from your project settings at https://app.supabase.com

---

## Development Workflow

### Start Development Server

```bash
npm run dev
```

This starts:
- Vite dev server on `http://localhost:5181`
- Hot module reloading for React components
- Watch mode for TypeScript compilation

### Run Tauri App in Development

In a **separate terminal**, run:

```bash
npm run tauri:dev
```

This launches the desktop app with:
- Development tools enabled
- Hot reload on frontend changes
- Console output from Rust backend

### Rebuild Rust Backend

If you modify Rust code, the Tauri app automatically recompiles. To force a rebuild:

```bash
cd src-tauri
cargo build
```

### TypeScript Type Checking

```bash
npm run typecheck
```

Validates TypeScript without building.

### Code Formatting

```bash
npm run format
```

Formats all TypeScript/TSX files with Prettier.

### Linting

```bash
npm run lint
```

Checks code style with ESLint.

### Candidate Verification

Before handing a code candidate to field QA, run the strongest practical local checks:

```bash
npm run verify:frontend
npm run audit:runtime
npm run audit:moderate
```

When Rust or Tauri code changes, also run:

```bash
cd src-tauri
cargo check
cargo test
```

These commands verify automated and mocked code paths only. They do not prove real LoL, LCU, replay, YouTube, installer, updater, GPU, audio, or support readiness; record those results in `docs/FIELD_QA_COMMERCIAL_READINESS.md`.

---

## Project Structure

### Root Directory

```
lolshorts/
├── src/                    # React frontend source
├── src-tauri/              # Rust backend source
├── docs/                   # Documentation
├── tests/                  # End-to-end tests (Playwright)
├── package.json            # Node.js dependencies
├── tsconfig.json           # TypeScript configuration
├── vite.config.ts          # Vite bundler config
├── tailwind.config.js      # Tailwind CSS config
├── jest.config.js          # Jest test config
├── playwright.config.ts    # Playwright test config
└── README.md               # Project overview
```

### Frontend (`src/`)

```
src/
├── pages/                  # Page components (Dashboard, Editor, etc.)
├── components/             # Reusable UI components
│   ├── auth/              # Login/signup components
│   ├── editor/            # Video editor components
│   ├── recording/         # Recording UI components
│   ├── settings/          # Settings components
│   └── layout/            # Layout components (AppShell, Sidebar)
├── api/                    # API client functions (Tauri commands)
├── hooks/                  # Custom React hooks
├── stores/                 # Zustand state management
├── locales/               # i18n translation files
├── styles/                # Global CSS
└── lib/                   # Utility functions
```

### Backend (`src-tauri/src/`)

```
src-tauri/src/
├── recording/             # Screen capture & FFmpeg recording
│   ├── integration_backend.rs      # Main recording engine
│   ├── game_monitor.rs            # LoL process detection
│   ├── live_client.rs             # Live Client API integration
│   ├── audio.rs                   # Audio capture
│   └── mod.rs                     # Recording module root
├── video/                 # Video processing & editing
│   ├── processor.rs               # FFmpeg video operations
│   ├── auto_composer.rs           # Auto-edit algorithm
│   └── mod.rs                     # Video module root
├── auth/                  # Supabase authentication
│   ├── mod.rs                     # Auth logic
│   └── middleware.rs              # Auth middleware
├── lcu/                   # League Client Update API
│   └── mod.rs                     # LCU client
├── youtube/               # YouTube upload integration
│   ├── oauth.rs                   # OAuth2 flow
│   ├── upload.rs                  # Video upload
│   └── mod.rs                     # YouTube module root
├── settings/              # User settings & platform config
│   ├── platform_config.rs         # Hardware detection
│   ├── storage.rs                 # Settings persistence
│   └── mod.rs                     # Settings module root
├── storage/               # Local SQLite database
│   ├── models.rs                  # Data models
│   └── mod.rs                     # Storage module root
├── main.rs                # Application entry point
└── lib.rs                 # Library root (exposes public API)
```

### Tests (`tests/`)

```
tests/
└── e2e/                   # Playwright end-to-end tests
    ├── auth.spec.ts              # Authentication tests
    ├── recording.spec.ts         # Recording functionality tests
    ├── auto-edit.spec.ts         # Auto-edit feature tests
    └── ...
```

---

## Testing

### Unit Tests (Jest)

Run all unit tests:

```bash
npm run test
```

Run tests in watch mode (re-runs on file changes):

```bash
npm run test:watch
```

Run specific test file:

```bash
npm run test -- src/components/__tests__/MyComponent.test.tsx
```

Test coverage:

```bash
npm run test -- --coverage
```

### End-to-End Tests (Playwright)

Run all E2E tests:

```bash
npm run test:e2e
```

Run specific test file:

```bash
npm run test:e2e -- tests/e2e/auth.spec.ts
```

Run with headed browser (visible):

```bash
npm run test:e2e:headed
```

Debug mode (step through tests):

```bash
npm run test:e2e:debug
```

View test report:

```bash
npm run test:e2e:report
```

### Rust Integration Tests

From `src-tauri/` directory:

```bash
# Run all tests
cargo test

# Run specific test
cargo test recording::tests::test_buffer_management

# Run with output display
cargo test -- --nocapture

# Run ignored tests only
cargo test -- --ignored
```

### Test Coverage

Do not treat historical test counts as current readiness evidence. For each candidate, rerun the commands above, record the exact results in `docs/FIELD_QA_COMMERCIAL_READINESS.md`, and qualify Playwright results as browser-flow evidence only.

---

## Debugging

### Frontend Debugging

#### React DevTools
1. Install [React DevTools](https://react-devtools-tutorial.vercel.app/) browser extension
2. Open browser DevTools (F12)
3. View component tree and state in "Components" tab

#### Console Logging
```typescript
// In React components
console.log('Debug info:', variable)

// Better: Use proper logging
import { logger } from '@/lib/logger'
logger.debug('Meaningful message', context)
```

#### Browser DevTools
- Open DevTools: Press `F12` in Tauri app
- Network tab: See Tauri command calls
- Console: View frontend logs
- Sources: Debug JavaScript (set breakpoints)

### Backend Debugging

#### View Logs
Logs are written to: `C:\Users\[You]\AppData\Roaming\lolshorts\logs\app.log`

View live logs:
```bash
# Windows PowerShell
Get-Content -Path "$env:APPDATA\lolshorts\logs\app.log" -Tail 100 -Wait

# macOS/Linux
tail -f ~/.local/share/lolshorts/logs/app.log
```

#### Rust Logging
Set logging level:
```bash
RUST_LOG=debug npm run tauri:dev
```

Log levels: `trace`, `debug`, `info`, `warn`, `error`

#### Tauri Command Debugging
Commands are logged automatically. Check browser DevTools Network tab for `tauri://` requests.

### Performance Profiling

#### React Performance
Use React Profiler tab in DevTools:
1. Open DevTools
2. Switch to "Profiler" tab
3. Record a session
4. Analyze component render times

#### FFmpeg Performance
Monitor during recording:
```bash
# Windows: Open Task Manager
# Look for ffmpeg.exe process - monitor CPU and memory usage

# macOS/Linux: Use top or Activity Monitor
top -p $(pgrep -f ffmpeg)
```

---

## Common Issues

### Issue: `FFmpeg not found`

**Cause**: FFmpeg sidecars are missing from `src-tauri/binaries/`

**Solution**:
```powershell
# Windows, from the repository root
.\src-tauri\build_scripts\prepare_ffmpeg.ps1 -Source Auto
```

```bash
# macOS/Linux
cd src-tauri/build_scripts
./prepare_ffmpeg.sh   # macOS/Linux
cd ../..
```

### Issue: Rust compilation error: "error\[E0433\]: cannot find crate"

**Cause**: Dependencies not installed or workspace corruption

**Solution**:
```bash
cd src-tauri
cargo clean
cargo build
```

### Issue: Tauri app won't start

**Cause**: Development server not running or port conflict

**Solution**:
```bash
# Terminal 1: Start dev server
npm run dev

# Terminal 2: Start Tauri app
npm run tauri:dev

# Verify port 5181 is not in use
# On Windows: netstat -ano | findstr :5181
# On macOS/Linux: lsof -i :5181
```

### Issue: Hot reload not working

**Cause**: Vite cache corruption or file watcher limit

**Solution**:
```bash
# Clear Vite cache
rm -rf dist node_modules/.vite

# Restart dev server
npm run dev

# On Linux, increase file watch limit
echo fs.inotify.max_user_watches=524288 | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```

### Issue: Tests fail with "Cannot find module"

**Cause**: Missing dependencies or Jest configuration issue

**Solution**:
```bash
# Reinstall dependencies
rm -rf node_modules package-lock.json
npm install

# Clear Jest cache
npm run test -- --clearCache

# Run tests again
npm run test
```

### Issue: TypeScript errors in IDE but code compiles

**Cause**: TypeScript version mismatch or cache issue

**Solution**:
```bash
# Use workspace TypeScript
npx tsc --version

# Clear TypeScript cache and rebuild
npm run typecheck

# Restart your IDE/editor
```

### Issue: "Port 5181 already in use"

**Cause**: Another process using the dev server port

**Solution**:
```bash
# Kill process using port 5181
# Windows
netstat -ano | findstr :5181
taskkill /PID <PID> /F

# macOS/Linux
lsof -i :5181 | awk 'NR!=1 {print $2}' | xargs kill -9
```

---

## Next Steps

- Read [ARCHITECTURE.md](./ARCHITECTURE.md) to understand system design
- Check out [AUTO_EDIT_GUIDE.md](./AUTO_EDIT_GUIDE.md) for feature details
- See [BUILD_GUIDE.md](../BUILD_GUIDE.md) for production builds
- Review [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) for runtime issues

---

## Getting Help

- **GitHub Issues**: https://github.com/KhakiSkech/lolshorts/issues
- **Documentation**: Browse `/docs` directory
- **Code Examples**: Check `/tests/e2e` for usage patterns
