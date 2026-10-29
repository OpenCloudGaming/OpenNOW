set -euo pipefail

host=$(uname -m)
target=${OPENNOW_MACOS_ARCH:?}
expected_host=${OPENNOW_MACOS_HOST_ARCH:-$target}
if [[ "$host" != "$expected_host" ]]; then
  echo "Expected macOS build host $expected_host, got $host" >&2
  exit 1
fi
case "$host" in
  arm64) rust_host=aarch64-apple-darwin ;;
  x86_64) rust_host=x86_64-apple-darwin ;;
  *) echo "Unsupported macOS build host: $host" >&2; exit 1 ;;
esac
case "$target" in
  arm64|x86_64) ;;
  *) echo "Unsupported macOS build target: $target" >&2; exit 1 ;;
esac
rustc -vV | grep -Fx "host: $rust_host"
if [[ "$host" == arm64 && "$target" == x86_64 ]]; then
  if ! arch -x86_64 /usr/bin/true; then
    softwareupdate --install-rosetta --agree-to-license
  fi
  arch -x86_64 /usr/bin/true
  {
    echo "CARGO_TARGET_X86_64_APPLE_DARWIN_RUNNER=/usr/bin/arch -x86_64"
    printf 'SDKROOT=%s\n' "$(xcrun --sdk macosx --show-sdk-path)"
  } >> "$GITHUB_ENV"
elif [[ "$host" != "$target" ]]; then
  echo "Unsupported macOS cross-build: $host to $target" >&2
  exit 1
fi
echo "macOS build host=$host target=$target compiler=$rust_host"
