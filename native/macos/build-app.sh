#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "OpenNOWMac.app can only be built on macOS." >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
CORE_DIR="${REPO_ROOT}/native/core"
HOST_DIR="${SCRIPT_DIR}/OpenNOWMac"
BUILD_DIR="${OPENNOW_MACOS_BUILD_DIR:-${SCRIPT_DIR}/build}"
CORE_BUILD_DIR="${OPENNOW_CORE_BUILD_DIR:-${CORE_DIR}/build}"
APP_NAME="OpenNOWMac"
APP_BUNDLE="${BUILD_DIR}/${APP_NAME}.app"
EXECUTABLE_PATH="${BUILD_DIR}/${APP_NAME}"
BRIDGE_OBJECT="${BUILD_DIR}/AppCoreBridge.o"

echo "Configuring native core..."
cmake -S "${CORE_DIR}" -B "${CORE_BUILD_DIR}" -DOPENNOW_BUILD_TESTS=ON

echo "Building native core..."
cmake --build "${CORE_BUILD_DIR}"

mkdir -p "${BUILD_DIR}"

echo "Compiling Objective-C++ bridge..."
xcrun clang++ \
  -std=c++20 \
  -fobjc-arc \
  -I "${CORE_DIR}/include" \
  -c "${HOST_DIR}/AppCoreBridge.mm" \
  -o "${BRIDGE_OBJECT}"

echo "Linking SwiftUI host..."
swiftc \
  -import-objc-header "${HOST_DIR}/OpenNOWMac-Bridging-Header.h" \
  "${HOST_DIR}/OpenNOWMacApp.swift" \
  "${HOST_DIR}/ContentView.swift" \
  "${BRIDGE_OBJECT}" \
  "${CORE_BUILD_DIR}/libopennow_core.a" \
  -Xcc -I \
  -Xcc "${CORE_DIR}/include" \
  -lc++ \
  -framework AppKit \
  -framework Security \
  -o "${EXECUTABLE_PATH}"

echo "Assembling ${APP_NAME}.app..."
rm -rf "${APP_BUNDLE}"
mkdir -p "${APP_BUNDLE}/Contents/MacOS" "${APP_BUNDLE}/Contents/Resources"
cp "${EXECUTABLE_PATH}" "${APP_BUNDLE}/Contents/MacOS/${APP_NAME}"
chmod +x "${APP_BUNDLE}/Contents/MacOS/${APP_NAME}"

cat > "${APP_BUNDLE}/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>OpenNOWMac</string>
  <key>CFBundleIdentifier</key>
  <string>com.opennow.native.macos</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>OpenNOW Native</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSMicrophoneUsageDescription</key>
  <string>OpenNOW uses microphone access when voice input is enabled for a cloud gaming session.</string>
</dict>
</plist>
PLIST

echo "Built ${APP_BUNDLE}"
