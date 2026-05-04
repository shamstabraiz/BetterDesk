#!/bin/bash
# =============================================================================
# Yomie Server - Interactive Build Script
# =============================================================================
# This script automates building Yomie enhanced binaries from source.
# It handles downloading RustDesk sources, applying Yomie modifications,
# and compiling the final binaries.
#
# Usage:
#   ./build-yomie.sh              # Interactive mode
#   ./build-yomie.sh --auto       # Non-interactive (use defaults)
#   ./build-yomie.sh --clean      # Clean build directory
#   ./build-yomie.sh --help       # Show help
#
# Requirements:
#   - Rust toolchain (rustup)
#   - Build essentials (gcc, make)
#   - SQLite3 development files
#   - OpenSSL development files
#   - Git
# =============================================================================

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="$SCRIPT_DIR/build"
PATCHES_DIR="$SCRIPT_DIR/hbbs-patch-v2/src"
OUTPUT_DIR="$SCRIPT_DIR/hbbs-patch-v2"

# Default RustDesk version (tag-based)
DEFAULT_RUSTDESK_VERSION="1.1.14"
RUSTDESK_REPO="https://github.com/rustdesk/rustdesk-server.git"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Command line options
AUTO_MODE=false
CLEAN_MODE=false
TARGET_PLATFORM=""
RUSTDESK_VERSION=""

# =============================================================================
# Helper Functions
# =============================================================================

print_header() {
    echo ""
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║${NC} ${BOLD}$1${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

print_success() { echo -e "${GREEN}✓${NC} $1"; }
print_error() { echo -e "${RED}✗${NC} $1"; }
print_warning() { echo -e "${YELLOW}⚠${NC} $1"; }
print_info() { echo -e "${BLUE}ℹ${NC} $1"; }
print_step() { echo -e "${CYAN}→${NC} $1"; }

show_help() {
    echo "Yomie Server - Build Script"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --auto          Non-interactive mode (use default settings)"
    echo "  --clean         Clean build directory and exit"
    echo "  --version VER   Specify RustDesk version (default: $DEFAULT_RUSTDESK_VERSION)"
    echo "  --platform PLT  Target platform: linux-x64, linux-arm64, windows-x64"
    echo "  --help          Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0                           # Interactive build"
    echo "  $0 --auto                    # Build with defaults"
    echo "  $0 --version 1.1.15          # Build specific version"
    echo "  $0 --platform linux-arm64    # Cross-compile for ARM64"
    echo ""
}

# =============================================================================
# Dependency Checks
# =============================================================================

check_dependencies() {
    print_header "Checking Dependencies"
    
    local missing=0
    
    # Check Rust
    if command -v cargo &> /dev/null; then
        print_success "Rust/Cargo: $(rustc --version 2>/dev/null | head -1)"
    else
        print_error "Rust/Cargo not found"
        echo "  Install: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        missing=1
    fi
    
    # Check Git
    if command -v git &> /dev/null; then
        print_success "Git: $(git --version)"
    else
        print_error "Git not found"
        echo "  Install: sudo apt-get install git"
        missing=1
    fi
    
    # Check pkg-config
    if command -v pkg-config &> /dev/null; then
        print_success "pkg-config found"
    else
        print_error "pkg-config not found"
        echo "  Install: sudo apt-get install pkg-config"
        missing=1
    fi
    
    # Check SQLite3
    if pkg-config --exists sqlite3 2>/dev/null; then
        print_success "SQLite3 development files found"
    else
        print_error "SQLite3 development files not found"
        echo "  Install: sudo apt-get install libsqlite3-dev"
        missing=1
    fi
    
    # Check OpenSSL
    if pkg-config --exists openssl 2>/dev/null; then
        print_success "OpenSSL development files found"
    else
        print_error "OpenSSL development files not found"
        echo "  Install: sudo apt-get install libssl-dev"
        missing=1
    fi
    
    # Check build essentials
    if command -v gcc &> /dev/null; then
        print_success "GCC: $(gcc --version | head -1)"
    else
        print_error "GCC not found"
        echo "  Install: sudo apt-get install build-essential"
        missing=1
    fi
    
    if [ $missing -ne 0 ]; then
        echo ""
        print_error "Missing dependencies. Please install them first."
        echo ""
        echo "Quick install (Debian/Ubuntu):"
        echo "  sudo apt-get update"
        echo "  sudo apt-get install -y build-essential pkg-config libsqlite3-dev libssl-dev git curl"
        echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        echo ""
        exit 1
    fi
    
    print_success "All dependencies satisfied!"
}

# =============================================================================
# Interactive Configuration
# =============================================================================

interactive_config() {
    print_header "Build Configuration"
    
    # Select RustDesk version
    echo -e "${BOLD}Available RustDesk versions:${NC}"
    echo "  1) 1.1.14 (stable, recommended)"
    echo "  2) 1.1.13 (older stable)"
    echo "  3) Custom (enter version)"
    echo ""
    
    if [ "$AUTO_MODE" = true ]; then
        RUSTDESK_VERSION="$DEFAULT_RUSTDESK_VERSION"
        print_info "Auto mode: Using version $RUSTDESK_VERSION"
    else
        read -p "Select version [1]: " version_choice
        case "$version_choice" in
            2) RUSTDESK_VERSION="1.1.13" ;;
            3) 
                read -p "Enter version (e.g., 1.1.15): " RUSTDESK_VERSION
                ;;
            *) RUSTDESK_VERSION="$DEFAULT_RUSTDESK_VERSION" ;;
        esac
    fi
    
    print_success "Selected RustDesk version: $RUSTDESK_VERSION"
    echo ""
    
    # Select target platform
    echo -e "${BOLD}Target platform:${NC}"
    echo "  1) Linux x86_64 (native)"
    echo "  2) Linux ARM64 (cross-compile)"
    echo "  3) Windows x86_64 (cross-compile, requires mingw)"
    echo ""
    
    if [ "$AUTO_MODE" = true ]; then
        TARGET_PLATFORM="linux-x64"
        print_info "Auto mode: Building for $TARGET_PLATFORM"
    else
        read -p "Select platform [1]: " platform_choice
        case "$platform_choice" in
            2) TARGET_PLATFORM="linux-arm64" ;;
            3) TARGET_PLATFORM="windows-x64" ;;
            *) TARGET_PLATFORM="linux-x64" ;;
        esac
    fi
    
    print_success "Target platform: $TARGET_PLATFORM"
    echo ""
    
    # Confirm
    if [ "$AUTO_MODE" = false ]; then
        echo -e "${BOLD}Build Summary:${NC}"
        echo "  RustDesk Version: $RUSTDESK_VERSION"
        echo "  Target Platform:  $TARGET_PLATFORM"
        echo "  Build Directory:  $BUILD_DIR"
        echo "  Output Directory: $OUTPUT_DIR"
        echo ""
        read -p "Continue with build? [Y/n] " confirm
        if [[ "$confirm" =~ ^[Nn]$ ]]; then
            echo "Build cancelled."
            exit 0
        fi
    fi
}

# =============================================================================
# Download RustDesk Sources
# =============================================================================

download_rustdesk() {
    print_header "Downloading RustDesk Server Sources"
    
    local source_dir="$BUILD_DIR/rustdesk-server-$RUSTDESK_VERSION"
    
    if [ -d "$source_dir" ]; then
        print_info "Source directory exists: $source_dir"
        
        if [ "$AUTO_MODE" = false ]; then
            read -p "Re-download sources? [y/N] " redownload
            if [[ ! "$redownload" =~ ^[Yy]$ ]]; then
                print_success "Using existing sources"
                return 0
            fi
        else
            print_info "Auto mode: Using existing sources"
            return 0
        fi
        
        rm -rf "$source_dir"
    fi
    
    mkdir -p "$BUILD_DIR"
    cd "$BUILD_DIR"
    
    print_step "Cloning rustdesk-server repository..."
    git clone --depth 1 --branch "$RUSTDESK_VERSION" "$RUSTDESK_REPO" "rustdesk-server-$RUSTDESK_VERSION"
    
    cd "rustdesk-server-$RUSTDESK_VERSION"
    
    print_step "Initializing submodules..."
    git submodule update --init --recursive
    
    print_success "RustDesk sources downloaded successfully"
}

# =============================================================================
# Apply Yomie Modifications
# =============================================================================

apply_modifications() {
    print_header "Applying Yomie Modifications"
    
    local source_dir="$BUILD_DIR/rustdesk-server-$RUSTDESK_VERSION"
    
    if [ ! -d "$source_dir" ]; then
        print_error "Source directory not found: $source_dir"
        exit 1
    fi
    
    cd "$source_dir"
    
    # List of files to copy from patches
    local patch_files=(
        "main.rs"
        "http_api.rs"
        "database.rs"
        "database_fixed.rs"
        "peer.rs"
        "peer_fixed.rs"
        "rendezvous_server_core.rs"
    )
    
    print_step "Copying Yomie modifications..."
    
    for file in "${patch_files[@]}"; do
        if [ -f "$PATCHES_DIR/$file" ]; then
            # Determine target directory
            case "$file" in
                main.rs)
                    cp "$PATCHES_DIR/$file" "src/main.rs"
                    print_success "Applied: main.rs (HTTP API integration)"
                    ;;
                http_api.rs)
                    cp "$PATCHES_DIR/$file" "src/http_api.rs"
                    print_success "Applied: http_api.rs (REST API module)"
                    ;;
                database*.rs|peer*.rs|rendezvous_server_core.rs)
                    cp "$PATCHES_DIR/$file" "src/$file"
                    print_success "Applied: $file"
                    ;;
            esac
        else
            print_warning "Patch file not found: $file"
        fi
    done
    
    # Update Cargo.toml to include new dependencies
    print_step "Updating Cargo.toml with Yomie dependencies..."
    
    # Check if we need to add axum dependency
    if ! grep -q "axum" Cargo.toml; then
        print_info "Adding HTTP API dependencies to Cargo.toml..."
        
        # Add dependencies before [features] or at end of [dependencies]
        sed -i '/^\[dependencies\]/a \
# Yomie HTTP API dependencies\
axum = { version = "0.5", features = ["ws"] }\
chrono = { version = "0.4", features = ["serde"] }' Cargo.toml
        
        print_success "Updated Cargo.toml"
    else
        print_info "Cargo.toml already has required dependencies"
    fi
    
    print_success "Yomie modifications applied successfully"
}

# =============================================================================
# Build Binaries
# =============================================================================

build_binaries() {
    print_header "Building Yomie Binaries"
    
    local source_dir="$BUILD_DIR/rustdesk-server-$RUSTDESK_VERSION"
    cd "$source_dir"
    
    # Set up cross-compilation if needed
    case "$TARGET_PLATFORM" in
        linux-arm64)
            print_step "Setting up ARM64 cross-compilation..."
            rustup target add aarch64-unknown-linux-gnu
            export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
            local target_flag="--target aarch64-unknown-linux-gnu"
            local binary_suffix="-linux-arm64"
            ;;
        windows-x64)
            print_step "Setting up Windows cross-compilation..."
            rustup target add x86_64-pc-windows-gnu
            local target_flag="--target x86_64-pc-windows-gnu"
            local binary_suffix="-windows-x86_64.exe"
            ;;
        *)
            local target_flag=""
            local binary_suffix="-linux-x86_64"
            ;;
    esac
    
    print_step "Building HBBS (Signal Server)..."
    cargo build --release $target_flag -p hbbs
    
    print_step "Building HBBR (Relay Server)..."
    cargo build --release $target_flag -p hbbr
    
    # Find binaries
    local target_dir="target"
    if [ -n "$target_flag" ]; then
        target_dir="target/$(echo $target_flag | sed 's/--target //')"
    fi
    
    local hbbs_binary="$target_dir/release/hbbs"
    local hbbr_binary="$target_dir/release/hbbr"
    
    if [ "$TARGET_PLATFORM" = "windows-x64" ]; then
        hbbs_binary="${hbbs_binary}.exe"
        hbbr_binary="${hbbr_binary}.exe"
    fi
    
    if [ ! -f "$hbbs_binary" ] || [ ! -f "$hbbr_binary" ]; then
        print_error "Build failed - binaries not found"
        exit 1
    fi
    
    print_success "Build completed successfully!"
    
    # Copy to output directory
    print_step "Copying binaries to output directory..."
    
    cp "$hbbs_binary" "$OUTPUT_DIR/hbbs$binary_suffix"
    cp "$hbbr_binary" "$OUTPUT_DIR/hbbr$binary_suffix"
    
    chmod +x "$OUTPUT_DIR/hbbs$binary_suffix" "$OUTPUT_DIR/hbbr$binary_suffix"
    
    print_success "Binaries saved to:"
    echo "  - $OUTPUT_DIR/hbbs$binary_suffix"
    echo "  - $OUTPUT_DIR/hbbr$binary_suffix"
}

# =============================================================================
# Generate Checksums
# =============================================================================

generate_checksums() {
    print_header "Generating Checksums"
    
    cd "$OUTPUT_DIR"
    
    local checksums_file="CHECKSUMS.md"
    local date_now=$(date +"%Y-%m-%d %H:%M:%S")
    
    cat > "$checksums_file" << EOF
# Yomie Server - Binary Checksums

Generated: $date_now
RustDesk Base Version: $RUSTDESK_VERSION
Yomie Version: 2.0.1

## SHA256 Checksums

\`\`\`
EOF
    
    for binary in hbbs-* hbbr-*; do
        if [ -f "$binary" ]; then
            sha256sum "$binary" >> "$checksums_file"
        fi
    done
    
    echo '```' >> "$checksums_file"
    
    print_success "Checksums saved to: $OUTPUT_DIR/$checksums_file"
}

# =============================================================================
# Clean Build
# =============================================================================

clean_build() {
    print_header "Cleaning Build Directory"
    
    if [ -d "$BUILD_DIR" ]; then
        print_step "Removing: $BUILD_DIR"
        rm -rf "$BUILD_DIR"
        print_success "Build directory cleaned"
    else
        print_info "Build directory does not exist"
    fi
}

# =============================================================================
# Main
# =============================================================================

main() {
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --auto)
                AUTO_MODE=true
                shift
                ;;
            --clean)
                CLEAN_MODE=true
                shift
                ;;
            --version)
                RUSTDESK_VERSION="$2"
                shift 2
                ;;
            --platform)
                TARGET_PLATFORM="$2"
                shift 2
                ;;
            --help)
                show_help
                exit 0
                ;;
            *)
                print_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done
    
    # Handle clean mode
    if [ "$CLEAN_MODE" = true ]; then
        clean_build
        exit 0
    fi
    
    # Banner
    echo ""
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║${NC}     ${BOLD}Yomie Server - Build from Source${NC}               ${CYAN}║${NC}"
    echo -e "${CYAN}║${NC}     Enhanced RustDesk with HTTP API & Management       ${CYAN}║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════╝${NC}"
    echo ""
    
    # Run build steps
    check_dependencies
    interactive_config
    download_rustdesk
    apply_modifications
    build_binaries
    generate_checksums
    
    # Final message
    print_header "Build Complete!"
    
    echo -e "${GREEN}Yomie binaries have been built successfully!${NC}"
    echo ""
    echo "Output location: $OUTPUT_DIR"
    echo ""
    echo "Next steps:"
    echo "  1. Test the binaries:"
    echo "     cd $OUTPUT_DIR"
    echo "     ./hbbs-linux-x86_64 --help"
    echo ""
    echo "  2. Run the installer to deploy:"
    echo "     sudo ./install-improved.sh"
    echo ""
    echo "  3. Or manually start the servers:"
    echo "     ./hbbs-linux-x86_64 -k _ --api-port 21114 &"
    echo "     ./hbbr-linux-x86_64 &"
    echo ""
}

main "$@"
