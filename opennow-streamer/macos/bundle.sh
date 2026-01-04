#!/bin/bash
# Bundle opennow-streamer as a macOS .app for Game Mode support

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
APP_NAME="OpenNOW.app"
APP_DIR="$PROJECT_DIR/target/release/$APP_NAME"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
FRAMEWORKS_DIR="$CONTENTS_DIR/Frameworks"
BINARY="$PROJECT_DIR/target/release/opennow-streamer"

# Build release
echo "Building release..."
cd "$PROJECT_DIR"

# Extract version from Cargo.toml if not provided
if [ -z "$VERSION" ]; then
  VERSION=$(grep "^version" Cargo.toml | head -1 | awk -F '"' '{print $2}')
  echo "Detected version from Cargo.toml: $VERSION"
fi
if [ -z "$VERSION" ]; then
  VERSION="0.1.0"
  echo "Could not detect version, defaulting to $VERSION"
fi

cargo build --release

# Create app bundle structure
echo "Creating app bundle..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

# Copy and rename binary
echo "Copying binary..."
cp "$BINARY" "$MACOS_DIR/$APP_NAME"
chmod +x "$MACOS_DIR/$APP_NAME"

# Create Info.plist
echo "Creating Info.plist..."
cat > "$CONTENTS_DIR/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>com.opennow.streamer</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

# Function to copy a library and fix its install name
copy_lib() {
  local lib="$1"
  local libname=$(basename "$lib")

  if [ ! -f "$FRAMEWORKS_DIR/$libname" ] && [ -f "$lib" ]; then
    echo "Copying: $libname"
    cp "$lib" "$FRAMEWORKS_DIR/"
    chmod 755 "$FRAMEWORKS_DIR/$libname"

    # Fix the library's own install name to be relative to the framework folder
    install_name_tool -id "@executable_path/../Frameworks/$libname" "$FRAMEWORKS_DIR/$libname" 2>/dev/null || true
    return 0
  fi
  return 0
}

# Function to fix references in a binary/library
fix_refs() {
  local target="$1"
  # Only fix references to Homebrew/system libs that we are bundling
  for dep in $(otool -L "$target" 2>/dev/null | grep -E '/opt/homebrew|/usr/local' | awk '{print $1}'); do
    local depname=$(basename "$dep")
    install_name_tool -change "$dep" "@executable_path/../Frameworks/$depname" "$target" 2>/dev/null || true
  done
}

echo ""
echo "=== Phase 1: Explicitly copy FFmpeg libraries ==="
# FFmpeg libraries might not show up in otool if dlopened or linked with @rpath
# So we explicitly find and copy them

FFMPEG_PREFIX=$(brew --prefix ffmpeg)
echo "FFmpeg prefix: $FFMPEG_PREFIX"

# List of core FFmpeg libraries to bundle
FFMPEG_LIBS=(
  "libavcodec"
  "libavdevice"
  "libavfilter"
  "libavformat"
  "libavutil"
  "libswresample"
  "libswscale"
)

for lib_base in "${FFMPEG_LIBS[@]}"; do
   echo "Looking for $lib_base in $FFMPEG_PREFIX/lib..."
   FOUND_LIBS=$(find "$FFMPEG_PREFIX/lib" -name "${lib_base}.*.dylib" -type f )
   
   for lib in $FOUND_LIBS; do
      echo "Found FFmpeg lib: $lib"
      copy_lib "$lib"
   done
done


echo ""
echo "=== Phase 2: Copy direct dependencies detected by otool ==="
DIRECT_DEPS=$(otool -L "$MACOS_DIR/$APP_NAME" 2>/dev/null | grep -E '/opt/homebrew|/usr/local' | awk '{print $1}' || true)
if [ -z "$DIRECT_DEPS" ]; then
  echo "No Homebrew dependencies found in binary (may be statically linked or using system libs)"
else
  for lib in $DIRECT_DEPS; do
    copy_lib "$lib"
  done
fi

echo ""
echo "=== Phase 3: Copy transitive dependencies (3 passes) ==="
BREW_PREFIX=$(brew --prefix)
echo "Resolving @rpath against: $BREW_PREFIX/lib"

for pass in 1 2 3; do
  echo "Pass $pass..."
  if ls "$FRAMEWORKS_DIR/"*.dylib 1>/dev/null 2>&1; then
    for bundled_lib in "$FRAMEWORKS_DIR/"*.dylib; do
      [ -f "$bundled_lib" ] || continue
      # Grep for /opt/homebrew, /usr/local, AND @rpath
      for dep in $(otool -L "$bundled_lib" 2>/dev/null | grep -E '/opt/homebrew|/usr/local|@rpath' | awk '{print $1}'); do
        
        # Handle @rpath references
        if [[ "$dep" == "@rpath/"* ]]; then
           # Extract filename
           local filename="${dep#@rpath/}"
           # Construct absolute path assuming it's in Homebrew lib
           local resolved_path="$BREW_PREFIX/lib/$filename"
           
           if [ -f "$resolved_path" ]; then
               # echo "Resolved $dep to $resolved_path"
               copy_lib "$resolved_path"
           else
               # Try strict FFmpeg prefix if different? usually brew prefix is enough
               :
           fi
        else
           # Absolute path
           copy_lib "$dep"
        fi
      done
    done
  else
    echo "  No dylibs to process"
    break
  fi
done

echo ""
echo "=== Phase 4: Fix all library references ==="
# Fix the main binary
fix_refs "$MACOS_DIR/$APP_NAME"

# Fix all bundled libraries
if ls "$FRAMEWORKS_DIR/"*.dylib 1>/dev/null 2>&1; then
  for bundled_lib in "$FRAMEWORKS_DIR/"*.dylib; do
    [ -f "$bundled_lib" ] || continue
    fix_refs "$bundled_lib"
  done
fi

# Special pass: Fix FFmpeg internal references
echo "Fixing internal references in bundled libs..."
if ls "$FRAMEWORKS_DIR/"*.dylib 1>/dev/null 2>&1; then
  for bundled_lib in "$FRAMEWORKS_DIR/"*.dylib; do
     for dep in $(otool -L "$bundled_lib" 2>/dev/null | grep -E '/opt/homebrew|/usr/local' | awk '{print $1}'); do
        local depname=$(basename "$dep")
        if [ -f "$FRAMEWORKS_DIR/$depname" ]; then
            echo "  Fixing ref in $(basename "$bundled_lib") to $depname"
            install_name_tool -change "$dep" "@loader_path/$depname" "$bundled_lib" 2>/dev/null || true
        fi
     done
  done
fi

echo ""
echo "=== Final verification ==="
echo "Binary dependencies:"
otool -L "$MACOS_DIR/$APP_NAME"

echo ""
echo "Bundled Frameworks:"
ls -1 "$FRAMEWORKS_DIR"

echo ""
echo "=== Phase 5: Code Signing ==="
echo "Signing app bundle..."
# Sign with ad-hoc signature (-)
# Use --force to replace any existing signature
# Use --deep to sign all nested frameworks and plugins
if codesign --force --deep --sign - "$APP_DIR"; then
  echo "Code signing successful"
else
  echo "Code signing failed"
  exit 1
fi

echo ""
echo "App bundle created: $APP_DIR"
echo ""
echo "To run with Game Mode support:"
echo "  open '$APP_DIR'"
echo ""
echo "Or run directly:"
echo "  '$APP_DIR/Contents/MacOS/opennow-streamer'"
