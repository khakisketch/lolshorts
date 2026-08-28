#!/bin/bash

# LoLShorts Development Environment Setup Script
# Automated setup for cross-platform development

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Configuration
REPO_URL="https://github.com/khakisketch/lolshorts.git"
DEV_DIR="$HOME/Development"
PROJECT_NAME="lolshorts"

# Logging functions
log_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

log_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

log_error() {
    echo -e "${RED}❌ $1${NC}"
}

log_step() {
    echo -e "${PURPLE}🔧 $1${NC}"
}

log_command() {
    echo -e "${CYAN}💻 $1${NC}"
}

# Detect platform
detect_platform() {
    case "$(uname -s)" in
        Darwin*)
            PLATFORM="macos"
            ;;
        Linux*)
            PLATFORM="linux"
            ;;
        CYGWIN*|MINGW*|MSYS*)
            PLATFORM="windows"
            ;;
        *)
            log_error "Unsupported platform: $(uname -s)"
            exit 1
            ;;
    esac

    log_info "Detected platform: $PLATFORM"
}

# Check system requirements
check_requirements() {
    log_step "Checking system requirements..."

    local missing_tools=()

    # Check for required command-line tools
    local required_tools=("git" "curl" "unzip" "tar")

    for tool in "${required_tools[@]}"; do
        if ! command -v "$tool" &> /dev/null; then
            missing_tools+=("$tool")
        fi
    done

    if [ ${#missing_tools[@]} -ne 0 ]; then
        log_error "Missing required tools: ${missing_tools[*]}"
        log_info "Please install these tools and try again"
        exit 1
    fi

    log_success "System requirements check passed"
}

# Install Rust
install_rust() {
    log_step "Installing Rust..."

    if command -v cargo &> /dev/null; then
        log_info "Rust is already installed"
        cargo --version
        rustup --version
    else
        log_command "Installing Rust..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"

        # Add required components
        rustup component add rustfmt clippy

        log_success "Rust installed successfully"
    fi

    # Install Tauri CLI
    if ! command -v cargo-tauri &> /dev/null; then
        log_command "Installing Tauri CLI..."
        cargo install tauri-cli --version "^2.0"
        log_success "Tauri CLI installed"
    else
        log_info "Tauri CLI is already installed"
    fi

    # Install useful Rust tools
    log_command "Installing Rust development tools..."
    cargo install cargo-audit --version 0.22.2 --locked
    cargo install cargo-outdated cargo-deny cargo-watch

    # Add platform-specific targets
    case $PLATFORM in
        macos)
            rustup target add x86_64-apple-darwin
            rustup target add aarch64-apple-darwin
            ;;
        linux)
            rustup target add x86_64-unknown-linux-gnu
            rustup target add x86_64-pc-windows-gnu
            ;;
        windows)
            rustup target add x86_64-pc-windows-msvc
            ;;
    esac

    log_success "Rust setup completed"
}

# Install Node.js
install_nodejs() {
    log_step "Installing Node.js..."

    if command -v node &> /dev/null; then
        local node_version=$(node --version | cut -d'v' -f2)
        local major_version=$(echo $node_version | cut -d'.' -f1)

        if [ "$major_version" -ge "20" ]; then
            log_info "Node.js $node_version is already installed"
        else
            log_warning "Node.js version $node_version is outdated. Installing Node.js 20..."
            install_nodejs_platform
        fi
    else
        install_nodejs_platform
    fi

    # Install global npm packages
    log_command "Installing global npm packages..."
    npm install -g @playwright/test typescript ts-node nodemon

    log_success "Node.js setup completed"
}

# Platform-specific Node.js installation
install_nodejs_platform() {
    case $PLATFORM in
        macos)
            if command -v brew &> /dev/null; then
                brew install node@20
            else
                log_command "Installing Node.js via nvm..."
                curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
                source ~/.nvm/nvm.sh
                nvm install 20
                nvm use 20
                nvm alias default 20
            fi
            ;;
        linux)
            if command -v apt-get &> /dev/null; then
                curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
                sudo apt-get install -y nodejs
            elif command -v dnf &> /dev/null; then
                curl -fsSL https://rpm.nodesource.com/setup_20.x | sudo bash -
                sudo dnf install -y nodejs npm
            else
                log_command "Installing Node.js via nvm..."
                curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
                source ~/.nvm/nvm.sh
                nvm install 20
                nvm use 20
                nvm alias default 20
            fi
            ;;
        windows)
            log_warning "Please download and install Node.js 20+ from https://nodejs.org/"
            read -p "Press Enter after installing Node.js..."
            ;;
    esac
}

# Install platform-specific dependencies
install_platform_deps() {
    log_step "Installing platform-specific dependencies..."

    case $PLATFORM in
        macos)
            install_macos_deps
            ;;
        linux)
            install_linux_deps
            ;;
        windows)
            install_windows_deps
            ;;
    esac

    log_success "Platform dependencies installed"
}

# macOS dependencies
install_macos_deps() {
    if ! command -v brew &> /dev/null; then
        log_command "Installing Homebrew..."
        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    fi

    # Install Xcode command line tools
    if ! xcode-select -p &> /dev/null; then
        log_command "Installing Xcode command line tools..."
        xcode-select --install
    fi

    # Install additional tools
    brew install create-dmg watchman
}

# Linux dependencies
install_linux_deps() {
    if command -v apt-get &> /dev/null; then
        sudo apt-get update
        sudo apt-get install -y \
            build-essential \
            pkg-config \
            libgtk-3-dev \
            libwebkit2gtk-4.0-dev \
            libappindicator3-dev \
            librsvg2-dev \
            patchelf \
            libssl-dev \
            libglib2.0-dev
    elif command -v dnf &> /dev/null; then
        sudo dnf install -y \
            gcc \
            gcc-c++ \
            pkgconfig \
            gtk3-devel \
            webkit2gtk3-devel \
            librsvg2-devel \
            openssl-devel \
            glib2-devel
    fi
}

# Windows dependencies
install_windows_deps() {
    log_warning "For Windows development, please ensure you have:"
    echo "  • Visual Studio Build Tools 2022"
    echo "  • Git for Windows"
    echo "  • Node.js 20+ from nodejs.org"
    echo ""
    log_info "You can install Visual Studio Build Tools from:"
    echo "https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022"
}

# Clone and setup repository
setup_repository() {
    log_step "Setting up LoLShorts repository..."

    # Create development directory
    mkdir -p "$DEV_DIR"
    cd "$DEV_DIR"

    # Clone repository if it doesn't exist
    if [ ! -d "$PROJECT_NAME" ]; then
        log_command "Cloning repository..."
        git clone "$REPO_URL" "$PROJECT_NAME"
    else
        log_info "Repository already exists, updating..."
        cd "$PROJECT_NAME"
        git pull origin main
    fi

    cd "$PROJECT_NAME"

    log_success "Repository setup completed"
}

# Install project dependencies
install_project_deps() {
    log_step "Installing project dependencies..."

    # Install Node.js dependencies
    log_command "Installing Node.js dependencies..."
    npm ci

    # Install Playwright browsers
    log_command "Installing Playwright browsers..."
    npx playwright install --with-deps

    # Install Rust dependencies
    log_command "Installing Rust dependencies..."
    cd src-tauri
    cargo fetch
    cd ..

    log_success "Project dependencies installed"
}

# Setup development tools
setup_dev_tools() {
    log_step "Setting up development tools..."

    # Git hooks
    log_command "Setting up Git hooks..."
    if [ -f ".githooks/pre-commit" ]; then
        cp .githooks/pre-commit .git/hooks/
        chmod +x .git/hooks/pre-commit
    fi

    # VS Code settings (optional)
    if command -v code &> /dev/null; then
        log_command "Installing VS Code extensions..."
        code --install-extension rust-lang.rust-analyzer
        code --install-extension ms-vscode.vscode-typescript-next
        code --install-extension bradlc.vscode-tailwindcss
        code --install-extension esbenp.prettier-vscode
        code --install-extension dbaeumer.vscode-eslint
    fi

    # Environment file
    if [ ! -f ".env" ]; then
        log_command "Creating .env file from template..."
        cp .env.example .env
        log_warning "Please update .env file with your configuration"
    fi

    log_success "Development tools setup completed"
}

# Verify installation
verify_installation() {
    log_step "Verifying installation..."

    local verification_failed=false

    # Check Rust
    if ! command -v cargo &> /dev/null; then
        log_error "Rust not found"
        verification_failed=true
    else
        log_success "Rust: $(cargo --version)"
    fi

    # Check Node.js
    if ! command -v node &> /dev/null; then
        log_error "Node.js not found"
        verification_failed=true
    else
        log_success "Node.js: $(node --version)"
    fi

    # Check Tauri CLI
    if ! command -v cargo-tauri &> /dev/null; then
        log_error "Tauri CLI not found"
        verification_failed=true
    else
        log_success "Tauri CLI: $(cargo-tauri --version)"
    fi

    # Check project structure
    if [ ! -f "package.json" ] || [ ! -f "src-tauri/Cargo.toml" ]; then
        log_error "Project structure is invalid"
        verification_failed=true
    else
        log_success "Project structure verified"
    fi

    # Test build
    log_command "Testing build configuration..."
    if cargo check --manifest-path src-tauri/Cargo.toml; then
        log_success "Build configuration test passed"
    else
        log_error "Build configuration test failed"
        verification_failed=true
    fi

    if [ "$verification_failed" = true ]; then
        log_error "Installation verification failed"
        exit 1
    fi

    log_success "Installation verification completed successfully"
}

# Print next steps
print_next_steps() {
    log_step "Development environment setup completed! 🎉"
    echo ""
    echo -e "${BLUE}Next steps:${NC}"
    echo ""
    echo -e "${CYAN}1. Start development server:${NC}"
    echo "   cd $DEV_DIR/$PROJECT_NAME"
    echo "   npm run tauri:dev"
    echo ""
    echo -e "${CYAN}2. Run tests:${NC}"
    echo "   npm test                    # Run all tests"
    echo "   npm run test:unit          # Unit tests only"
    echo "   npm run test:e2e           # E2E tests only"
    echo ""
    echo -e "${CYAN}3. Build for production:${NC}"
    echo "   npm run tauri:build        # Build application"
    echo ""
    echo -e "${CYAN}4. Code quality checks:${NC}"
    echo "   npm run lint               # Lint code"
    echo "   npm run format             # Format code"
    echo "   cargo fmt                  # Format Rust code"
    echo "   cargo clippy               # Lint Rust code"
    echo ""
    echo -e "${CYAN}5. Useful commands:${NC}"
    echo "   cargo watch                # Watch for changes and rebuild"
    echo "   cargo test                 # Run Rust tests"
    echo "   npx playwright test        # Run E2E tests"
    echo "   npx playwright codegen     # Generate E2E test code"
    echo ""
    echo -e "${YELLOW}⚠️  Don't forget to:${NC}"
    echo "  • Update .env file with your configuration"
    echo "  • Review CONTRIBUTING.md for development guidelines"
    echo "  • Set up your IDE with the recommended extensions"
    echo "  • Read the documentation in the docs/ directory"
    echo ""
    echo -e "${GREEN}Happy coding! 🚀${NC}"
}

# Main setup function
main() {
    echo -e "${BLUE}🚀 LoLShorts Development Environment Setup${NC}"
    echo "This script will set up your development environment for LoLShorts"
    echo ""

    detect_platform
    check_requirements

    log_info "Starting installation process..."
    echo ""

    install_rust
    install_nodejs
    install_platform_deps
    setup_repository
    install_project_deps
    setup_dev_tools
    verify_installation

    print_next_steps
}

# Handle interruption
trap 'log_warning "Setup interrupted by user"; exit 1' INT

# Run main function
main "$@"
