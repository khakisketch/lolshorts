#!/bin/bash

# Pre-commit hook for LoLShorts
# This script runs before each commit to ensure code quality

set -e

echo "🚀 Running pre-commit checks..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    local color=$1
    local message=$2
    echo -e "${color}${message}${NC}"
}

# Check if we're in a git repository
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    print_status $RED "❌ Not in a git repository"
    exit 1
fi

# Get list of staged files
STAGED_FILES=$(git diff --cached --name-only --diff-filter=ACM)

if [ -z "$STAGED_FILES" ]; then
    print_status $YELLOW "⚠️  No staged files to check"
    exit 0
fi

print_status $BLUE "📁 Staged files:"
echo "$STAGED_FILES"

# Check if there are Rust files staged
RUST_STAGED=$(echo "$STAGED_FILES" | grep -E "\.rs$" || true)
if [ ! -z "$RUST_STAGED" ]; then
    print_status $BLUE "🦀 Checking Rust files..."

    cd src-tauri

    # Check formatting
    print_status $BLUE "Checking Rust formatting..."
    if ! cargo fmt -- --check; then
        print_status $RED "❌ Rust code is not properly formatted."
        print_status $YELLOW "Run 'cd src-tauri && cargo fmt' to fix formatting issues."
        exit 1
    fi

    # Run clippy
    print_status $BLUE "Running Rust lints..."
    if ! cargo clippy --all-targets -- -D warnings; then
        print_status $RED "❌ Rust code has linting errors."
        print_status $YELLOW "Fix the linting errors and try again."
        exit 1
    fi

    # Run tests for changed files if requested
    if [ "$1" = "--run-tests" ]; then
        print_status $BLUE "Running Rust tests..."
        if ! cargo test; then
            print_status $RED "❌ Rust tests failed."
            exit 1
        fi
    fi

    cd ..
    print_status $GREEN "✅ Rust checks passed!"
fi

# Check if there are TypeScript/JavaScript files staged
TS_STAGED=$(echo "$STAGED_FILES" | grep -E "\.(ts|tsx|js|jsx)$" || true)
if [ ! -z "$TS_STAGED" ]; then
    print_status $BLUE "⚛️  Checking TypeScript/JavaScript files..."

    # Check formatting
    print_status $BLUE "Checking TypeScript formatting..."
    if ! npm run format:check; then
        print_status $RED "❌ TypeScript code is not properly formatted."
        print_status $YELLOW "Run 'npm run format' to fix formatting issues."
        exit 1
    fi

    # Run linter
    print_status $BLUE "Running TypeScript lints..."
    if ! npm run lint; then
        print_status $RED "❌ TypeScript code has linting errors."
        print_status $YELLOW "Fix the linting errors and try again."
        exit 1
    fi

    # Run type check
    print_status $BLUE "Running TypeScript type check..."
    if ! npm run type-check; then
        print_status $RED "❌ TypeScript type check failed."
        exit 1
    fi

    print_status $GREEN "✅ TypeScript checks passed!"
fi

# Check if package.json or package-lock.json changed
PACKAGE_STAGED=$(echo "$STAGED_FILES" | grep -E "(package\.json|package-lock\.json)$" || true)
if [ ! -z "$PACKAGE_STAGED" ]; then
    print_status $BLUE "📦 Checking package files..."

    # Check if package-lock.json is consistent with package.json
    if ! npm ls --depth=0 > /dev/null 2>&1; then
        print_status $RED "❌ Package dependencies are inconsistent."
        print_status $YELLOW "Run 'npm install' to fix dependency issues."
        exit 1
    fi

    print_status $GREEN "✅ Package checks passed!"
fi

# Check for security issues in Rust dependencies
if [ ! -z "$RUST_STAGED" ]; then
    print_status $BLUE "🔒 Checking Rust security..."

    if ! cargo audit --file Cargo.lock --quiet; then
        print_status $RED "❌ Security vulnerabilities found in Rust dependencies."
        print_status $YELLOW "Run 'cargo audit --file Cargo.lock' for details."
        print_status $YELLOW "Update vulnerable dependencies with 'cargo update'."
        exit 1
    fi

    print_status $GREEN "✅ Security checks passed!"
fi

# Check for security issues in Node.js dependencies
if [ ! -z "$TS_STAGED" ] || [ ! -z "$PACKAGE_STAGED" ]; then
    print_status $BLUE "🔒 Checking Node.js security..."

    if ! npm audit --audit-level=moderate; then
        print_status $RED "❌ Security vulnerabilities found in Node.js dependencies."
        print_status $YELLOW "Run 'npm audit fix' to fix moderate vulnerabilities."
        exit 1
    fi

    print_status $GREEN "✅ Node.js security checks passed!"
fi

# Check for large files
print_status $BLUE "📏 Checking for large files..."
LARGE_FILES=$(git diff --cached --name-only | xargs -I {} git show --format=%b -- {} | wc -c)
MAX_FILE_SIZE=$((10 * 1024 * 1024))  # 10MB

if [ "$LARGE_FILES" -gt "$MAX_FILE_SIZE" ]; then
    print_status $YELLOW "⚠️  Warning: Large files detected in commit"
    print_status $YELLOW "Consider using Git LFS for large binary files."
fi

# Check for secrets and sensitive data
print_status $BLUE "🔐 Checking for sensitive data..."
SECRET_PATTERNS=("password" "secret" "token" "key" "api_key" "private_key")

for pattern in "${SECRET_PATTERNS[@]}"; do
    if git diff --cached --name-only | xargs git diff --cached --text | grep -i "$pattern" > /dev/null; then
        print_status $YELLOW "⚠️  Potential sensitive data detected: '$pattern'"
        print_status $YELLOW "Please verify that no secrets are being committed."
    fi
done

print_status $GREEN "🎉 All pre-commit checks passed!"
print_status $BLUE "✅ Ready to commit!"
