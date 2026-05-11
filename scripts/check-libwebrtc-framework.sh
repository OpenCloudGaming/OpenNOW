#!/usr/bin/env bash
set -euo pipefail

framework_dir="${1:-third_party/webrtc-official}"
framework_path="${framework_dir%/}/WebRTC.framework"

if [[ ! -d "$framework_path" ]]; then
  xcframework_path="${framework_dir%/}/WebRTC.xcframework/macos-x86_64_arm64/WebRTC.framework"
  if [[ -d "$xcframework_path" ]]; then
    framework_path="$xcframework_path"
  else
    printf 'Missing %s\n' "$framework_path" >&2
    printf 'Also checked %s\n' "$xcframework_path" >&2
    printf 'Provide a macOS arm64 WebRTC.framework or WebRTC.xcframework and run:\n' >&2
    printf '  make WEBRTC_FRAMEWORK_DIR=%s\n' "$framework_dir" >&2
    exit 1
  fi
fi

if [[ ! -f "$framework_path/Headers/WebRTC.h" ]]; then
  printf 'Missing %s/Headers/WebRTC.h\n' "$framework_path" >&2
  exit 1
fi

missing_headers=0
while IFS= read -r line; do
  case "$line" in
    '#import <WebRTC/'*'.h>'*)
      header="${line#*<WebRTC/}"
      header="${header%%>*}"
      if [[ ! -f "$framework_path/Headers/$header" ]]; then
        printf 'Missing imported header %s/Headers/%s\n' "$framework_path" "$header" >&2
        missing_headers=1
      fi
      ;;
  esac
done < "$framework_path/Headers/WebRTC.h"

if [[ "$missing_headers" -ne 0 ]]; then
  printf 'WebRTC.framework headers are incomplete. Provide the full macOS public header set.\n' >&2
  exit 1
fi

binary="$framework_path/WebRTC"
if [[ ! -f "$binary" ]]; then
  printf 'Missing %s\n' "$binary" >&2
  exit 1
fi

if command -v lipo >/dev/null 2>&1; then
  archs="$(lipo -archs "$binary" 2>/dev/null || true)"
  case " $archs " in
    *' arm64 '*) ;;
    *)
      printf 'WebRTC.framework binary does not contain arm64. Found archs: %s\n' "$archs" >&2
      exit 1
      ;;
  esac
  printf 'WebRTC.framework archs: %s\n' "$archs"
fi

printf 'WebRTC.framework looks usable at %s\n' "$framework_path"
