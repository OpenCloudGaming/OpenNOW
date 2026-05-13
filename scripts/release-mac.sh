#!/usr/bin/env bash
set -euo pipefail

APP_NAME="OpenNOW"
BUNDLE_ID="com.opennow.mac"
VERSION="${VERSION:-1.0.0}"
BUILD_NUMBER="${BUILD_NUMBER:-1}"
MIN_MACOS="${MIN_MACOS:-14.0}"
ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
CALLER_DIR="$(pwd)"
ICON_PATH_INPUT="${1:-${ICON_PATH:-}}"
if [[ -z "${ICON_PATH_INPUT}" ]]; then
  ICON_PATH_INPUT="${ROOT_DIR}/assets/logo-mac.png"
fi
BUILD_DIR="${ROOT_DIR}/build"
RELEASE_DIR="${BUILD_DIR}/release"
APP_DIR="${RELEASE_DIR}/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
FRAMEWORKS_DIR="${CONTENTS_DIR}/Frameworks"
ICONSET_DIR="${RELEASE_DIR}/AppIcon.iconset"
ZIP_PATH="${RELEASE_DIR}/${APP_NAME}-macOS-arm64.zip"
DMG_PATH="${RELEASE_DIR}/${APP_NAME}-macOS-arm64.dmg"
WEBRTC_FRAMEWORK_DIR="${WEBRTC_FRAMEWORK_DIR:-${ROOT_DIR}/third_party/webrtc-official}"
WEBRTC_FRAMEWORK_PATH=""

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'Missing required tool: %s\n' "$1" >&2
    exit 1
  fi
}

resolve_file_path() {
  local value="$1"
  if [[ "${value}" = /* ]]; then
    printf '%s\n' "${value}"
  elif [[ -f "${CALLER_DIR}/${value}" ]]; then
    printf '%s\n' "${CALLER_DIR}/${value}"
  else
    printf '%s\n' "${ROOT_DIR}/${value}"
  fi
}

ICON_PATH="$(resolve_file_path "${ICON_PATH_INPUT}")"
if [[ -z "${ICON_PATH}" || ! -f "${ICON_PATH}" ]]; then
  printf 'Usage: %s /path/to/icon.png\n' "$0" >&2
  printf 'Or set ICON_PATH=/path/to/icon.png. Defaults to %s.\n' "${ROOT_DIR}/assets/logo.png" >&2
  exit 1
fi

require_tool make
require_tool python3
require_tool otool
require_tool install_name_tool
require_tool codesign
require_tool ditto
require_tool hdiutil
require_tool sips
require_tool iconutil

if [[ -n "${WEBRTC_FRAMEWORK_DIR}" ]]; then
  "${ROOT_DIR}/scripts/check-libwebrtc-framework.sh" "${WEBRTC_FRAMEWORK_DIR}"
  if [[ -d "${WEBRTC_FRAMEWORK_DIR%/}/WebRTC.framework" ]]; then
    WEBRTC_FRAMEWORK_PATH="${WEBRTC_FRAMEWORK_DIR%/}/WebRTC.framework"
  elif [[ -d "${WEBRTC_FRAMEWORK_DIR%/}/WebRTC.xcframework/macos-x86_64_arm64/WebRTC.framework" ]]; then
    WEBRTC_FRAMEWORK_PATH="${WEBRTC_FRAMEWORK_DIR%/}/WebRTC.xcframework/macos-x86_64_arm64/WebRTC.framework"
  else
    printf 'Unable to resolve WebRTC.framework under %s\n' "${WEBRTC_FRAMEWORK_DIR}" >&2
    exit 1
  fi
fi

printf 'Building %s...\n' "${APP_NAME}"
make -C "${ROOT_DIR}" WEBRTC_FRAMEWORK_DIR="${WEBRTC_FRAMEWORK_DIR}"

printf 'Creating app bundle...\n'
rm -rf "${APP_DIR}" "${ICONSET_DIR}" "${ZIP_PATH}" "${DMG_PATH}"
mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}" "${FRAMEWORKS_DIR}" "${ICONSET_DIR}"

cp "${BUILD_DIR}/${APP_NAME}" "${MACOS_DIR}/${APP_NAME}.bin"
chmod 755 "${MACOS_DIR}/${APP_NAME}.bin"

if [[ -n "${WEBRTC_FRAMEWORK_PATH}" ]]; then
  printf 'Bundling WebRTC.framework from %s...\n' "${WEBRTC_FRAMEWORK_PATH}"
  rm -rf "${FRAMEWORKS_DIR}/WebRTC.framework"
  cp -R "${WEBRTC_FRAMEWORK_PATH}" "${FRAMEWORKS_DIR}/WebRTC.framework"
  chmod -R u+w "${FRAMEWORKS_DIR}/WebRTC.framework"
  chmod 755 "${FRAMEWORKS_DIR}/WebRTC.framework/WebRTC"
fi

cat > "${CONTENTS_DIR}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${BUILD_NUMBER}</string>
    <key>LSMinimumSystemVersion</key>
    <string>${MIN_MACOS}</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
    <key>NSMicrophoneUsageDescription</key>
    <string>OpenNOW uses the microphone for in-game voice chat and stream recordings when microphone support is enabled.</string>
    <key>NSScreenCaptureUsageDescription</key>
    <string>OpenNOW captures stream audio for MP4 recordings saved to your Movies folder.</string>
</dict>
</plist>
PLIST

printf 'APPL????' > "${CONTENTS_DIR}/PkgInfo"

cat > "${MACOS_DIR}/${APP_NAME}" <<'LAUNCHER'
#!/bin/sh
CONTENTS_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

exec "$CONTENTS_DIR/MacOS/OpenNOW.bin" "$@"
LAUNCHER
chmod 755 "${MACOS_DIR}/${APP_NAME}"

printf 'Generating icon from %s...\n' "${ICON_PATH}"
sips -z 16 16 "${ICON_PATH}" --out "${ICONSET_DIR}/icon_16x16.png" >/dev/null
sips -z 32 32 "${ICON_PATH}" --out "${ICONSET_DIR}/icon_16x16@2x.png" >/dev/null
sips -z 32 32 "${ICON_PATH}" --out "${ICONSET_DIR}/icon_32x32.png" >/dev/null
sips -z 64 64 "${ICON_PATH}" --out "${ICONSET_DIR}/icon_32x32@2x.png" >/dev/null
sips -z 128 128 "${ICON_PATH}" --out "${ICONSET_DIR}/icon_128x128.png" >/dev/null
sips -z 256 256 "${ICON_PATH}" --out "${ICONSET_DIR}/icon_128x128@2x.png" >/dev/null
sips -z 256 256 "${ICON_PATH}" --out "${ICONSET_DIR}/icon_256x256.png" >/dev/null
sips -z 512 512 "${ICON_PATH}" --out "${ICONSET_DIR}/icon_256x256@2x.png" >/dev/null
sips -z 512 512 "${ICON_PATH}" --out "${ICONSET_DIR}/icon_512x512.png" >/dev/null
sips -z 1024 1024 "${ICON_PATH}" --out "${ICONSET_DIR}/icon_512x512@2x.png" >/dev/null
iconutil -c icns "${ICONSET_DIR}" -o "${RESOURCES_DIR}/AppIcon.icns"

printf 'Rewriting Mach-O install names...\n'
python3 - "${CONTENTS_DIR}" <<'PY'
from pathlib import Path
import subprocess
import sys

contents = Path(sys.argv[1])
frameworks = contents / 'Frameworks'
webrtc_binary = frameworks / 'WebRTC.framework/WebRTC'
files = [contents / 'MacOS/OpenNOW.bin'] + sorted(frameworks.glob('*.dylib'))
if webrtc_binary.exists():
    files.append(webrtc_binary)

def run(args: list[str]) -> None:
    subprocess.run(args, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

def deps_for(path: Path) -> list[str]:
    output = subprocess.check_output(['otool', '-L', str(path)], text=True, stderr=subprocess.DEVNULL)
    return [line.strip().split(' ', 1)[0] for line in output.splitlines()[1:]]

def is_bundled_abs(dep: str) -> bool:
    return dep.startswith('/opt/homebrew/') or dep.startswith('/usr/local/')

for file in files:
    if file.suffix == '.dylib':
        run(['install_name_tool', '-id', f'@rpath/{file.name}', str(file)])
    elif file == webrtc_binary:
        run(['install_name_tool', '-id', '@rpath/WebRTC.framework/WebRTC', str(file)])
        run(['install_name_tool', '-add_rpath', '@loader_path/..', str(file)])
    if file == contents / 'MacOS/OpenNOW.bin':
        run(['install_name_tool', '-add_rpath', '@executable_path/../Frameworks', str(file)])
    elif file.parent == frameworks:
        run(['install_name_tool', '-add_rpath', '@loader_path', str(file)])
    for dep in deps_for(file):
        if is_bundled_abs(dep):
            run(['install_name_tool', '-change', dep, f'@rpath/{Path(dep).name}', str(file)])

unresolved = []
for file in files:
    for dep in deps_for(file):
        if is_bundled_abs(dep):
            unresolved.append((str(file), dep))
if unresolved:
    for file, dep in unresolved[:50]:
        print(f'UNRESOLVED {file}: {dep}')
    raise SystemExit(f'{len(unresolved)} unresolved Homebrew references')
print(f'Rewrote {len(files)} Mach-O files with no Homebrew install-name references')
PY

printf 'Signing app ad-hoc...\n'
for dylib in "${FRAMEWORKS_DIR}"/*.dylib; do
  [[ -e "${dylib}" ]] || continue
  codesign --force --sign - "${dylib}" >/dev/null
done
if [[ -d "${FRAMEWORKS_DIR}/WebRTC.framework" ]]; then
  codesign --force --sign - "${FRAMEWORKS_DIR}/WebRTC.framework" >/dev/null
fi
codesign --force --sign - "${MACOS_DIR}/${APP_NAME}.bin" >/dev/null
codesign --force --sign - "${APP_DIR}" >/dev/null

printf 'Verifying bundle...\n'
plutil -lint "${CONTENTS_DIR}/Info.plist" >/dev/null
codesign --verify --deep --strict --verbose=2 "${APP_DIR}" >/dev/null
python3 - "${CONTENTS_DIR}" <<'PY'
from pathlib import Path
import subprocess
import sys

contents = Path(sys.argv[1])
webrtc_binary = contents / 'Frameworks/WebRTC.framework/WebRTC'
files = [contents / 'MacOS/OpenNOW.bin'] + sorted((contents / 'Frameworks').glob('*.dylib'))
if webrtc_binary.exists():
    files.append(webrtc_binary)
unresolved = []
for file in files:
    output = subprocess.check_output(['otool', '-L', str(file)], text=True, stderr=subprocess.DEVNULL)
    for line in output.splitlines()[1:]:
        dep = line.strip().split(' ', 1)[0]
        if dep.startswith('/opt/homebrew/') or dep.startswith('/usr/local/'):
            unresolved.append((str(file), dep))
if unresolved:
    for file, dep in unresolved[:50]:
        print(f'{file}: {dep}')
    raise SystemExit(f'{len(unresolved)} unresolved external references')
print(f'No Homebrew install-name references in {len(files)} bundled Mach-O files')
PY

printf 'Packaging release artifacts...\n'
ditto -c -k --keepParent "${APP_DIR}" "${ZIP_PATH}"
hdiutil create -volname "${APP_NAME}" -srcfolder "${APP_DIR}" -ov -format UDZO "${DMG_PATH}" >/dev/null
hdiutil verify "${DMG_PATH}" >/dev/null

printf '\nRelease complete:\n'
du -sh "${APP_DIR}" "${ZIP_PATH}" "${DMG_PATH}"
printf '%s\n%s\n' "${ZIP_PATH}" "${DMG_PATH}"
