#!/bin/bash
#
# OpenNOW Installer Script
# https://github.com/zortos293/OpenNOW
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/zortos293/OpenNOW/main/scripts/install.sh | bash
#
# This script:
#   1. Detects your OS and architecture
#   2. Installs GStreamer dependencies
#   3. Downloads the latest OpenNOW release
#   4. Installs OpenNOW to ~/.local/bin (Linux) or /Applications (macOS)
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# GitHub repo
REPO="zortos293/OpenNOW"

print_banner() {
    echo -e "${GREEN}"
    echo "  ___                   _   _  _____      __"
    echo " / _ \ _ __   ___ _ __ | \ | |/ _ \ \    / /"
    echo "| | | | '_ \ / _ \ '_ \|  \| | | | \ \/\/ / "
    echo "| |_| | |_) |  __/ | | | |\  | |_| |\    /  "
    echo " \___/| .__/ \___|_| |_|_| \_|\___/  \/\/   "
    echo "      |_|                                   "
    echo -e "${NC}"
    echo "Open Source GeForce NOW Client"
    echo ""
}

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

detect_os() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        OS="macos"
        ARCH=$(uname -m)
        if [[ "$ARCH" == "arm64" ]]; then
            PLATFORM="macos-arm64"
        else
            PLATFORM="macos-arm64"  # Intel Macs use ARM64 build via Rosetta 2
            log_info "Intel Mac detected - will use ARM64 build with Rosetta 2"
        fi
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        OS="linux"
        ARCH=$(uname -m)
        if [[ "$ARCH" == "x86_64" ]]; then
            PLATFORM="linux-x64"
        elif [[ "$ARCH" == "aarch64" ]]; then
            PLATFORM="linux-arm64"
        else
            log_error "Unsupported architecture: $ARCH"
            exit 1
        fi
    else
        log_error "Unsupported operating system: $OSTYPE"
        exit 1
    fi
    
    log_info "Detected: $OS ($ARCH) -> $PLATFORM"
}

detect_package_manager() {
    if [[ "$OS" == "macos" ]]; then
        if command -v brew &> /dev/null; then
            PKG_MANAGER="brew"
        else
            log_error "Homebrew not found. Please install it first:"
            echo '  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"'
            exit 1
        fi
    elif [[ "$OS" == "linux" ]]; then
        if command -v apt-get &> /dev/null; then
            PKG_MANAGER="apt"
        elif command -v dnf &> /dev/null; then
            PKG_MANAGER="dnf"
        elif command -v pacman &> /dev/null; then
            PKG_MANAGER="pacman"
        elif command -v zypper &> /dev/null; then
            PKG_MANAGER="zypper"
        else
            log_warn "Unknown package manager. You may need to install GStreamer manually."
            PKG_MANAGER="unknown"
        fi
    fi
    
    log_info "Package manager: $PKG_MANAGER"
}

install_gstreamer() {
    log_info "Installing GStreamer dependencies..."
    
    case "$PKG_MANAGER" in
        brew)
            log_info "Installing GStreamer via Homebrew..."
            brew install gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav
            ;;
        apt)
            log_info "Installing GStreamer via apt..."
            
            # Check Ubuntu/Debian version for PPA support
            if command -v lsb_release &> /dev/null; then
                DISTRO=$(lsb_release -is 2>/dev/null || echo "unknown")
                VERSION=$(lsb_release -rs 2>/dev/null || echo "0")
                
                # Add GStreamer PPA for newer versions on Ubuntu 22.04/24.04
                # Ubuntu 22.04 ships with outdated GStreamer 1.20.x
                # This PPA provides GStreamer 1.24.x with better codec support
                if [[ "$DISTRO" == "Ubuntu" ]]; then
                    log_info "Ubuntu $VERSION detected - adding GStreamer PPA for newer version..."
                    sudo apt-get update
                    sudo apt-get install -y software-properties-common
                    sudo add-apt-repository -y ppa:ubuntuhandbook1/gstreamer || log_warn "PPA add failed, using distro version"
                fi
            fi
            
            # Full GStreamer installation as per official docs
            # https://gstreamer.freedesktop.org/documentation/installing/on-linux.html
            sudo apt-get update
            sudo apt-get install -y \
                libgstreamer1.0-dev \
                libgstreamer-plugins-base1.0-dev \
                libgstreamer-plugins-bad1.0-dev \
                gstreamer1.0-plugins-base \
                gstreamer1.0-plugins-good \
                gstreamer1.0-plugins-bad \
                gstreamer1.0-plugins-ugly \
                gstreamer1.0-libav \
                gstreamer1.0-tools \
                gstreamer1.0-x \
                gstreamer1.0-alsa \
                gstreamer1.0-gl \
                gstreamer1.0-gtk3 \
                gstreamer1.0-qt5 \
                gstreamer1.0-pulseaudio \
                gstreamer1.0-vaapi
            ;;
        dnf)
            log_info "Installing GStreamer via dnf..."
            # Full Fedora installation as per official docs
            sudo dnf install -y \
                gstreamer1-devel \
                gstreamer1-plugins-base-tools \
                gstreamer1-plugins-base-devel \
                gstreamer1-plugins-good \
                gstreamer1-plugins-good-extras \
                gstreamer1-plugins-ugly \
                gstreamer1-plugins-bad-free \
                gstreamer1-plugins-bad-free-devel \
                gstreamer1-plugins-bad-free-extras
            ;;
        pacman)
            log_info "Installing GStreamer via pacman..."
            sudo pacman -S --noconfirm \
                gstreamer \
                gst-plugins-base \
                gst-plugins-base-libs \
                gst-plugins-good \
                gst-plugins-bad \
                gst-plugins-bad-libs \
                gst-plugins-ugly \
                gst-libav
            ;;
        zypper)
            log_info "Installing GStreamer via zypper..."
            sudo zypper install -y \
                gstreamer-devel \
                gstreamer-plugins-base \
                gstreamer-plugins-base-devel \
                gstreamer-plugins-good \
                gstreamer-plugins-bad \
                gstreamer-plugins-ugly \
                gstreamer-plugins-libav
            ;;
        *)
            log_warn "Please install GStreamer manually for your distribution."
            log_warn "See: https://gstreamer.freedesktop.org/documentation/installing/on-linux.html"
            echo ""
            echo "Ubuntu/Debian:"
            echo "  sudo apt-get install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \\"
            echo "    libgstreamer-plugins-bad1.0-dev gstreamer1.0-plugins-base gstreamer1.0-plugins-good \\"
            echo "    gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav gstreamer1.0-tools"
            echo ""
            echo "Fedora:"
            echo "  sudo dnf install gstreamer1-devel gstreamer1-plugins-base-devel \\"
            echo "    gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-plugins-ugly"
            ;;
    esac
    
    # Verify GStreamer installation
    if command -v gst-inspect-1.0 &> /dev/null; then
        GST_VERSION=$(gst-inspect-1.0 --version | head -1)
        log_success "GStreamer installed: $GST_VERSION"
    else
        log_warn "GStreamer installation could not be verified"
    fi
}

get_latest_release() {
    log_info "Fetching latest release from GitHub..."
    
    # First try /releases/latest (stable releases only)
    LATEST_RELEASE=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
    
    # If no stable release, get the most recent release (including prereleases)
    if [[ -z "$LATEST_RELEASE" ]]; then
        log_info "No stable release found, checking prereleases..."
        LATEST_RELEASE=$(curl -s "https://api.github.com/repos/$REPO/releases" | grep '"tag_name"' | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
    fi
    
    if [[ -z "$LATEST_RELEASE" ]]; then
        log_error "Failed to fetch latest release. No releases found."
        log_error "Please check: https://github.com/$REPO/releases"
        exit 1
    fi
    
    log_info "Latest release: $LATEST_RELEASE"
}

download_and_install() {
    log_info "Downloading OpenNOW for $PLATFORM..."
    
    # Construct download URL based on platform
    case "$PLATFORM" in
        macos-arm64)
            FILENAME="OpenNOW-macos-arm64.zip"
            ;;
        linux-x64)
            FILENAME="OpenNOW-linux-x64.AppImage"
            ;;
        linux-arm64)
            FILENAME="OpenNOW-linux-arm64.zip"
            ;;
    esac
    
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_RELEASE/$FILENAME"
    
    # Create temp directory
    TEMP_DIR=$(mktemp -d)
    cd "$TEMP_DIR"
    
    log_info "Downloading from: $DOWNLOAD_URL"
    curl -fsSL -o "$FILENAME" "$DOWNLOAD_URL"
    
    if [[ ! -f "$FILENAME" ]]; then
        log_error "Download failed"
        exit 1
    fi
    
    log_success "Download complete"
    
    # Install based on platform
    case "$PLATFORM" in
        macos-arm64)
            log_info "Installing to /Applications..."
            unzip -q "$FILENAME"
            
            # Remove quarantine attribute and old installation
            if [[ -d "/Applications/OpenNOW.app" ]]; then
                rm -rf "/Applications/OpenNOW.app"
            fi
            
            mv "OpenNOW.app" "/Applications/"
            xattr -rd com.apple.quarantine "/Applications/OpenNOW.app" 2>/dev/null || true
            
            log_success "OpenNOW installed to /Applications/OpenNOW.app"
            echo ""
            echo "To run: open /Applications/OpenNOW.app"
            echo "Or find it in Launchpad/Spotlight"
            ;;
            
        linux-x64)
            log_info "Installing AppImage to ~/.local/bin..."
            mkdir -p "$HOME/.local/bin"
            
            chmod +x "$FILENAME"
            mv "$FILENAME" "$HOME/.local/bin/OpenNOW.AppImage"
            
            # Create symlink
            ln -sf "$HOME/.local/bin/OpenNOW.AppImage" "$HOME/.local/bin/opennow"
            
            log_success "OpenNOW installed to ~/.local/bin/OpenNOW.AppImage"
            echo ""
            echo "To run: ~/.local/bin/OpenNOW.AppImage"
            echo "Or:     opennow (if ~/.local/bin is in PATH)"
            
            # Check if ~/.local/bin is in PATH
            if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
                log_warn "~/.local/bin is not in your PATH"
                echo "Add this to your ~/.bashrc or ~/.zshrc:"
                echo '  export PATH="$HOME/.local/bin:$PATH"'
            fi
            ;;
            
        linux-arm64)
            log_info "Installing to ~/.local/share/opennow..."
            INSTALL_DIR="$HOME/.local/share/opennow"
            mkdir -p "$INSTALL_DIR"
            mkdir -p "$HOME/.local/bin"
            
            unzip -q "$FILENAME"
            
            # Move bundle contents
            if [[ -d "bundle" ]]; then
                rm -rf "$INSTALL_DIR"
                mv "bundle" "$INSTALL_DIR"
            fi
            
            # Create launcher script
            cat > "$HOME/.local/bin/opennow" << 'LAUNCHER'
#!/bin/bash
SCRIPT_DIR="$HOME/.local/share/opennow"
export LD_LIBRARY_PATH="$SCRIPT_DIR/lib:$LD_LIBRARY_PATH"
export GST_REGISTRY_UPDATE=yes
exec "$SCRIPT_DIR/opennow-streamer" "$@"
LAUNCHER
            chmod +x "$HOME/.local/bin/opennow"
            
            log_success "OpenNOW installed to $INSTALL_DIR"
            echo ""
            echo "To run: opennow"
            
            # Check if ~/.local/bin is in PATH
            if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
                log_warn "~/.local/bin is not in your PATH"
                echo "Add this to your ~/.bashrc or ~/.zshrc:"
                echo '  export PATH="$HOME/.local/bin:$PATH"'
            fi
            ;;
    esac
    
    # Cleanup
    cd /
    rm -rf "$TEMP_DIR"
}

create_desktop_entry() {
    if [[ "$OS" == "linux" ]]; then
        log_info "Creating desktop entry..."
        
        DESKTOP_DIR="$HOME/.local/share/applications"
        mkdir -p "$DESKTOP_DIR"
        
        cat > "$DESKTOP_DIR/opennow.desktop" << EOF
[Desktop Entry]
Name=OpenNOW
Comment=Open Source GeForce NOW Client
Exec=$HOME/.local/bin/opennow
Icon=applications-games
Terminal=false
Type=Application
Categories=Game;
EOF
        
        log_success "Desktop entry created"
    fi
}

main() {
    print_banner
    
    detect_os
    detect_package_manager
    
    echo ""
    read -p "Install GStreamer dependencies? [Y/n] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Nn]$ ]]; then
        install_gstreamer
    fi
    
    echo ""
    get_latest_release
    download_and_install
    create_desktop_entry
    
    echo ""
    log_success "Installation complete!"
    echo ""
    echo "Enjoy OpenNOW! Report issues at: https://github.com/$REPO/issues"
}

# Run main function
main
