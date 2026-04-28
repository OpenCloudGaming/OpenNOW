#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 /path/to/OpenNOWMac.app"
  echo
  echo "Required environment:"
  echo "  DEVELOPER_ID_APPLICATION   Developer ID Application signing identity"
  echo "  APPLE_ID                   Apple ID used for notarization"
  echo "  APPLE_TEAM_ID              Apple Developer Team ID"
  echo "  APP_SPECIFIC_PASSWORD      App-specific password for notarytool"
  echo
  echo "Optional environment:"
  echo "  OPENNOW_ENTITLEMENTS       Entitlements plist path"
  echo "  OPENNOW_DRY_RUN=1          Print actions without signing or submitting"
}

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 64
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Signing and notarization must run on macOS." >&2
  exit 1
fi

APP_PATH="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENTITLEMENTS="${OPENNOW_ENTITLEMENTS:-${SCRIPT_DIR}/OpenNOWMac.entitlements}"
DRY_RUN="${OPENNOW_DRY_RUN:-0}"

if [[ ! -d "${APP_PATH}" ]]; then
  echo "App bundle not found: ${APP_PATH}" >&2
  exit 66
fi

if [[ ! -f "${ENTITLEMENTS}" ]]; then
  echo "Entitlements file not found: ${ENTITLEMENTS}" >&2
  exit 66
fi

require_env() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "Missing required environment variable: ${name}" >&2
    exit 78
  fi
}

require_env DEVELOPER_ID_APPLICATION
require_env APPLE_ID
require_env APPLE_TEAM_ID
require_env APP_SPECIFIC_PASSWORD

ARCHIVE_PATH="${APP_PATH%/}.zip"

if [[ "${DRY_RUN}" == "1" ]]; then
  echo "Would codesign ${APP_PATH} with ${DEVELOPER_ID_APPLICATION}"
  echo "Would create ${ARCHIVE_PATH}"
  echo "Would submit ${ARCHIVE_PATH} to Apple notarization and staple ${APP_PATH}"
  exit 0
fi

echo "Signing app bundle..."
codesign --force --deep --options runtime --timestamp \
  --entitlements "${ENTITLEMENTS}" \
  --sign "${DEVELOPER_ID_APPLICATION}" \
  "${APP_PATH}"

echo "Verifying code signature..."
codesign --verify --deep --strict --verbose=2 "${APP_PATH}"
spctl --assess --type execute --verbose=2 "${APP_PATH}"

echo "Creating notarization archive..."
ditto -c -k --keepParent "${APP_PATH}" "${ARCHIVE_PATH}"

echo "Submitting archive for notarization..."
xcrun notarytool submit "${ARCHIVE_PATH}" \
  --apple-id "${APPLE_ID}" \
  --team-id "${APPLE_TEAM_ID}" \
  --password "${APP_SPECIFIC_PASSWORD}" \
  --wait

echo "Stapling notarization ticket..."
xcrun stapler staple "${APP_PATH}"
xcrun stapler validate "${APP_PATH}"

echo "Signing and notarization complete: ${APP_PATH}"
