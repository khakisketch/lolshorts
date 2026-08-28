#!/bin/bash

# Development Environment Setup Script for macOS
set -e

echo "🚀 Setting up LoLShorts development environment on macOS..."

# Check if Homebrew is installed
if ! command -v brew &> /dev/null; then
    echo "Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

    # Add Homebrew to PATH for the current session
    eval "$(/opt/homebrew/bin/brew shellenv)"
fi

echo "📦 Installing system dependencies..."

# Install FFmpeg
echo "Installing FFmpeg..."
brew install ffmpeg

# Install Git (if not installed)
echo "Installing Git..."
brew install git

# Install Node.js
echo "Installing Node.js..."
brew install node

# Install pkg-config (needed for some Rust dependencies)
echo "Installing pkg-config..."
brew install pkg-config

# Install Xcode Command Line Tools (if not installed)
if ! xcode-select -p &> /dev/null; then
    echo "Installing Xcode Command Line Tools..."
    xcode-select --install
    echo "⚠️  Please complete the Xcode installation and then run this script again."
    exit 1
fi

echo "🔧 Installing Rust..."

# Install Rust
if ! command -v cargo &> /dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

echo "✅ Verifying installations..."

# Check Rust
if command -v cargo &> /dev/null; then
    rust_version=$(cargo --version)
    echo "✅ Rust: $rust_version"
else
    echo "❌ Rust not found"
    exit 1
fi

# Check Node.js
if command -v node &> /dev/null; then
    node_version=$(node --version)
    echo "✅ Node.js: $node_version"
else
    echo "❌ Node.js not found"
    exit 1
fi

# Check FFmpeg
if command -v ffmpeg &> /dev/null; then
    ffmpeg_version=$(ffmpeg -version 2>/dev/null | head -n 1)
    echo "✅ FFmpeg: $ffmpeg_version"
else
    echo "❌ FFmpeg not found"
    exit 1
fi

echo "🔧 Installing Rust development tools..."

# Install Tauri CLI
cargo install tauri-cli --locked

# Install useful Rust development tools
cargo install cargo-watch
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-deny
cargo install cargo-expand

echo "🪝 Setting up pre-commit hooks..."

if [ -d ".git" ]; then
    # Create pre-commit hook
    cat > .git/hooks/pre-commit << 'EOF'
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
EOF

    chmod +x .git/hooks/pre-commit
    echo "✅ Pre-commit hooks installed"
else
    echo "⚠️  Not in a git repository, skipping pre-commit hooks"
fi

echo "📝 Creating development scripts..."

# Create development helper script
cat > dev-helper.sh << 'EOF'
#!/bin/bash

# Development helper script for LoLShorts

dev_rust() {
    local action=${1:-dev}

    echo "Running Rust development server..."
    cd src-tauri

    case $action in
        "dev")
            cargo run --bin lolshorts-tauri
            ;;
        "build")
            cargo build
            ;;
        "test")
            cargo test
            ;;
        "check")
            cargo check
            ;;
        "fmt")
            cargo fmt
            ;;
        "lint")
            cargo clippy -- -D warnings
            ;;
        "bench")
            cargo bench
            ;;
        *)
            echo "Unknown action: $action"
            exit 1
            ;;
    esac

    cd ..
}

dev_frontend() {
    local action=${1:-dev}

    echo "Running frontend development server..."

    case $action in
        "dev")
            npm run dev
            ;;
        "build")
            npm run build
            ;;
        "preview")
            npm run preview
            ;;
        "type-check")
            npm run type-check
            ;;
        "lint")
            npm run lint
            ;;
        "format")
            npm run format
            ;;
        *)
            echo "Unknown action: $action"
            exit 1
            ;;
    esac
}

dev_full() {
    echo "Starting full development environment..."
    # Start frontend in background
    dev_frontend dev &
    FRONTEND_PID=$!

    # Wait a moment for frontend to start
    sleep 2

    # Start Rust in foreground
    dev_rust dev

    # Clean up frontend process on exit
    kill $FRONTEND_PID 2>/dev/null || true
}

# Show usage if no arguments provided
if [ "$1" = "help" ] || [ "$1" = "--help" ]; then
    echo "Development helper for LoLShorts"
    echo ""
    echo "Usage: source dev-helper.sh"
    echo ""
    echo "Commands:"
    echo "  dev_rust [action]  - Run Rust development (dev, build, test, check, fmt, lint, bench)"
    echo "  dev_frontend [action] - Run frontend development (dev, build, preview, type-check, lint, format)"
    echo "  dev_full          - Start both Rust and frontend development servers"
fi
EOF

chmod +x dev-helper.sh
echo "✅ Development helper script created: dev-helper.sh"

echo ""
echo "🎉 Development environment setup complete!"
echo ""
echo "📋 Next steps:" $'\033[0;36m'
echo "1. Source the development helper: source scripts/dev-helper.sh"
echo "2. Install frontend dependencies: npm ci"
echo "3. Run Rust development: dev_rust"
echo "4. Run frontend development: dev_frontend"
echo "5. Run full development: dev_full"
echo ""
echo "🔧 Useful commands:" $'\033[0;36m'
echo "- Build project: npm run tauri build"
echo "- Run tests: npm run test"
echo "- Format code: npm run format"
echo "- Check for security vulnerabilities: cargo audit --file Cargo.lock"
echo ""
echo "⚠️  Important Notes:" $'\033[0;33m'
echo "- Make sure your Xcode Command Line Tools are properly installed"
echo "- If you encounter build errors, try running: xcode-select --install"
echo "- For Tauri development, install the Tauri VSCode extension for better IDE support"
echo "- On macOS with Apple Silicon, make sure Rosetta 2 is installed: softwareupdate --install-rosetta"

# Add Rust to PATH
if [ -f "$HOME/.cargo/env" ]; then
    echo ""
    echo "🔧 Adding Rust to PATH for current session..."
    source "$HOME/.cargo/env"
    echo "✅ Rust added to PATH"
fi

echo ""
echo "📚 Additional resources:"
echo "- Rust documentation: https://doc.rust-lang.org/"
echo "- Tauri documentation: https://tauri.app/v1/guides/"
echo "- React documentation: https://react.dev/"
