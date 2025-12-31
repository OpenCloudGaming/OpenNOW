#!/bin/bash
# Bundle opennow-streamer as a macOS .app for Game Mode support

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
APP_NAME="OpenNOW.app"
APP_DIR="$PROJECT_DIR/target/release/$APP_NAME"

# Build release
echo "Building release..."
cd "$PROJECT_DIR"
cargo build --release

# Create app bundle structure
echo "Creating app bundle..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

# Copy binary
cp "$PROJECT_DIR/target/release/opennow-streamer" "$APP_DIR/Contents/MacOS/"

# Copy Info.plist
cp "$SCRIPT_DIR/Info.plist" "$APP_DIR/Contents/"

# Create PkgInfo
echo -n "APPL????" > "$APP_DIR/Contents/PkgInfo"

echo ""
echo "App bundle created: $APP_DIR"
echo ""
echo "To run with Game Mode support:"
echo "  open '$APP_DIR'"
echo ""
echo "Or run directly:"
echo "  '$APP_DIR/Contents/MacOS/opennow-streamer'"
