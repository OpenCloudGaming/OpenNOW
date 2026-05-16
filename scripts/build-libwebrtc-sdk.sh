#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/build-libwebrtc-sdk.sh [options]

Builds and packages a native C++ libwebrtc SDK for the Qt client.

Options:
  --platform <macos|linux|windows>  Target platform. Defaults to host platform.
  --arch <arm64|x64>                Target architecture. Defaults to host architecture.
  --work-dir <path>                 WebRTC checkout/build workspace. Defaults to build/libwebrtc-src.
  --sdk-dir <path>                  Output SDK root. Defaults to third_party/libwebrtc.
  --depot-tools <path>              Existing depot_tools checkout. Defaults to build/depot_tools.
  --no-fetch                        Reuse existing checkout; do not run fetch/sync.
  --debug                           Build debug instead of release.
  -h, --help                        Show this help.

Required tools:
  git, python3, ninja, and Chromium depot_tools.

Output layout:
  third_party/libwebrtc/include/api/peer_connection_interface.h
  third_party/libwebrtc/lib/<platform>/libwebrtc.a
EOF
}

host_platform() {
  case "$(uname -s)" in
    Darwin) printf 'macos' ;;
    Linux) printf 'linux' ;;
    MINGW*|MSYS*|CYGWIN*) printf 'windows' ;;
    *) printf 'Unsupported host platform: %s\n' "$(uname -s)" >&2; exit 1 ;;
  esac
}

host_arch() {
  case "$(uname -m)" in
    arm64|aarch64) printf 'arm64' ;;
    x86_64|amd64) printf 'x64' ;;
    *) printf 'Unsupported host architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
  esac
}

platform="$(host_platform)"
arch="$(host_arch)"
work_dir="build/libwebrtc-src"
sdk_dir="third_party/libwebrtc"
depot_tools_dir="build/depot_tools"
fetch_sources=1
is_debug=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --platform) platform="${2:?missing --platform value}"; shift 2 ;;
    --arch) arch="${2:?missing --arch value}"; shift 2 ;;
    --work-dir) work_dir="${2:?missing --work-dir value}"; shift 2 ;;
    --sdk-dir) sdk_dir="${2:?missing --sdk-dir value}"; shift 2 ;;
    --depot-tools) depot_tools_dir="${2:?missing --depot-tools value}"; shift 2 ;;
    --no-fetch) fetch_sources=0; shift ;;
    --debug) is_debug=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'Unknown argument: %s\n' "$1" >&2; usage >&2; exit 1 ;;
  esac
done

case "$platform" in
  macos|linux|windows) ;;
  *) printf 'Unsupported platform: %s\n' "$platform" >&2; exit 1 ;;
esac

case "$arch" in
  arm64|x64) ;;
  *) printf 'Unsupported architecture: %s\n' "$arch" >&2; exit 1 ;;
esac

for tool in git python3 ninja; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'Missing required tool: %s\n' "$tool" >&2
    exit 1
  fi
done

if [[ ! -d "$depot_tools_dir/.git" ]]; then
  mkdir -p "$(dirname "$depot_tools_dir")"
  git clone https://chromium.googlesource.com/chromium/tools/depot_tools.git "$depot_tools_dir"
fi

export PATH="$(cd "$depot_tools_dir" && pwd):$PATH"

checkout_parent="$(dirname "$work_dir")"
checkout_name="$(basename "$work_dir")"
mkdir -p "$checkout_parent"
src_dir="$work_dir/src"

if [[ "$fetch_sources" -eq 1 ]]; then
  if [[ ! -d "$src_dir" ]]; then
    (cd "$checkout_parent" && fetch --nohooks webrtc)
    if [[ "$checkout_name" != "src" ]]; then
      mkdir -p "$work_dir"
      if [[ -d "$work_dir/.git" ]]; then
        mv "$work_dir" "$src_dir"
      else
        mv "$checkout_parent/src" "$src_dir"
      fi
    else
      src_dir="$checkout_parent/src"
    fi
  fi
  (cd "$src_dir" && gclient sync --nohooks)
  (cd "$src_dir" && gclient runhooks)
elif [[ -d "$work_dir/.git" && ! -d "$src_dir" ]]; then
  mkdir -p "$work_dir.migrating"
  mv "$work_dir" "$work_dir.migrating/src"
  mv "$work_dir.migrating" "$work_dir"
  src_dir="$work_dir/src"
elif [[ ! -d "$src_dir" ]]; then
  printf 'Missing checkout at %s and --no-fetch was provided.\n' "$src_dir" >&2
  exit 1
fi

target_os="$platform"
target_cpu="$arch"
if [[ "$platform" == "macos" ]]; then
  target_os="mac"
fi
if [[ "$platform" == "windows" ]]; then
  target_os="win"
fi

out_dir="out/${platform}-${arch}-$([[ "$is_debug" == true ]] && printf debug || printf release)"
gn_args=(
  "target_os=\"$target_os\""
  "target_cpu=\"$target_cpu\""
  "is_debug=$is_debug"
  "is_component_build=false"
  "rtc_include_tests=false"
  "rtc_build_examples=false"
  "rtc_build_tools=false"
  "rtc_use_h264=true"
  "proprietary_codecs=true"
  "use_custom_libcxx=false"
  "treat_warnings_as_errors=false"
)

(cd "$src_dir" && gn gen "$out_dir" --args="${gn_args[*]}")
(cd "$src_dir" && autoninja -C "$out_dir" webrtc)

lib_name="libwebrtc.a"
if [[ "$platform" == "windows" ]]; then
  lib_name="webrtc.lib"
fi

built_lib="$src_dir/$out_dir/obj/$lib_name"
if [[ ! -f "$built_lib" ]]; then
  built_lib="$src_dir/$out_dir/$lib_name"
fi
if [[ ! -f "$built_lib" ]]; then
  printf 'Could not find built libwebrtc library in %s.\n' "$src_dir/$out_dir" >&2
  exit 1
fi

include_dir="$sdk_dir/include"
lib_dir="$sdk_dir/lib/$platform"
mkdir -p "$include_dir" "$lib_dir"

rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/api" "$include_dir/"
rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/rtc_base" "$include_dir/"
rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/media" "$include_dir/"
rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/modules" "$include_dir/"
rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/pc" "$include_dir/"
rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/call" "$include_dir/"
rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/common_audio" "$include_dir/"
rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/common_video" "$include_dir/"
rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/logging" "$include_dir/"
rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/p2p" "$include_dir/"
rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/system_wrappers" "$include_dir/"
rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/video" "$include_dir/"
rsync -a --delete --include='*/' --include='*.h' --include='*.hpp' --exclude='*' "$src_dir/third_party/abseil-cpp/absl" "$include_dir/"
cp "$built_lib" "$lib_dir/$lib_name"

printf 'Native C++ libwebrtc SDK packaged at %s\n' "$sdk_dir"
printf 'Include dir: %s\n' "$include_dir"
printf 'Library: %s\n' "$lib_dir/$lib_name"
