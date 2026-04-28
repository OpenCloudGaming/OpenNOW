#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "native macOS release validation must run on macOS." >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
BUILD_DIR="${OPENNOW_NATIVE_BUILD_DIR:-${REPO_ROOT}/native/core/build/macos-release}"
PACKAGE_DIR="${OPENNOW_NATIVE_PACKAGE_DIR:-${BUILD_DIR}/packages}"
INSTALL_DIR="${OPENNOW_NATIVE_INSTALL_DIR:-${BUILD_DIR}/install}"
CONFIG="${OPENNOW_NATIVE_CONFIG:-Release}"

echo "Configuring native core (${CONFIG})..."
cmake -S "${REPO_ROOT}/native/core" -B "${BUILD_DIR}" \
  -DCMAKE_BUILD_TYPE="${CONFIG}" \
  -DOPENNOW_BUILD_TESTS=ON

echo "Building native core..."
cmake --build "${BUILD_DIR}" --config "${CONFIG}"

echo "Running native core tests..."
ctest --test-dir "${BUILD_DIR}" --build-config "${CONFIG}" --output-on-failure

echo "Installing native core staging payload..."
cmake --install "${BUILD_DIR}" --config "${CONFIG}" --prefix "${INSTALL_DIR}"

echo "Packaging native core archive..."
cmake --build "${BUILD_DIR}" --target package --config "${CONFIG}"
mkdir -p "${PACKAGE_DIR}"
mv "${BUILD_DIR}"/opennow-native-core-*.tar.gz "${PACKAGE_DIR}/"

if [[ ! -f "${REPO_ROOT}/native/macos/release/OpenNOWMac.entitlements" ]]; then
  echo "Missing native macOS entitlements file." >&2
  exit 1
fi

if [[ ! -x "${REPO_ROOT}/native/macos/release/sign-and-notarize.sh" ]]; then
  echo "Missing executable native macOS signing/notarization helper." >&2
  exit 1
fi

echo "Building native macOS host app bundle..."
OPENNOW_CORE_BUILD_DIR="${BUILD_DIR}/host-core" \
OPENNOW_MACOS_BUILD_DIR="${BUILD_DIR}/OpenNOWMacHost" \
  "${REPO_ROOT}/native/macos/build-app.sh"

if [[ ! -d "${BUILD_DIR}/OpenNOWMacHost/OpenNOWMac.app" ]]; then
  echo "Native macOS host bundle was not produced." >&2
  exit 1
fi

echo "Native macOS release validation complete."
echo "Packages: ${PACKAGE_DIR}"
