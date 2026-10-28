set -euo pipefail

[[ "$(uname -m)" == x86_64 ]]
root="${RUNNER_TEMP:?}/opennow-arm64-sdk"
sysroot="$root/sysroot"
tools="$root/tools"
mkdir -p "$root/apt/lists/partial" "$root/apt/archives/partial" "$sysroot" "$tools"
for directory in bin sbin lib; do
  ln -sfn "usr/$directory" "$sysroot/$directory"
done
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  gcc-aarch64-linux-gnu g++-aarch64-linux-gnu clang libclang-dev \
  libdbus-1-dev libsecret-1-dev libgl1-mesa-dev libegl1-mesa-dev \
  libwayland-bin wayland-protocols xvfb ninja-build nasm pkg-config
printf '%s\n' \
  'deb [arch=arm64 signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] https://ports.ubuntu.com/ubuntu-ports noble main universe restricted multiverse' \
  'deb [arch=arm64 signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] https://ports.ubuntu.com/ubuntu-ports noble-updates main universe restricted multiverse' \
  'deb [arch=arm64 signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] https://ports.ubuntu.com/ubuntu-ports noble-security main universe restricted multiverse' \
  > "$root/apt/sources.list"
apt_options=(
  -o "Dir::Etc::sourcelist=$root/apt/sources.list"
  -o "Dir::Etc::sourceparts=-"
  -o "Dir::State::lists=$root/apt/lists"
  -o "Dir::State::status=/dev/null"
  -o "Dir::Cache::archives=$root/apt/archives"
  -o APT::Architecture=arm64 -o APT::Architectures=arm64
)
sudo apt-get "${apt_options[@]}" update
sudo apt-get "${apt_options[@]}" install -y --download-only --no-install-recommends \
  libc6-dev libstdc++-13-dev libasound2-dev libdbus-1-dev libdrm-dev \
  libpulse-dev libsecret-1-dev libva-dev libvulkan-dev libwayland-dev \
  libx11-dev libxcursor-dev libxext-dev libxfixes-dev libxi-dev \
  libxrandr-dev libxrender-dev libxss-dev libudev-dev libgl1-mesa-dev \
  libegl1-mesa-dev libxkbcommon-dev libfontconfig1-dev libfreetype6-dev \
  libxkbcommon-x11-dev libxcb-cursor-dev wayland-protocols
for package in "$root"/apt/archives/*.deb; do
  dpkg-deb -x "$package" "$sysroot"
done
for compiler in gcc g++; do
  cat > "$tools/aarch64-linux-gnu-$compiler" <<EOF
#!/bin/sh
exec /usr/bin/aarch64-linux-gnu-$compiler --sysroot="$sysroot" "\$@"
EOF
  chmod +x "$tools/aarch64-linux-gnu-$compiler"
done
cat > "$tools/aarch64-linux-gnu-pkg-config" <<EOF
#!/bin/sh
export PKG_CONFIG_SYSROOT_DIR="$sysroot"
export PKG_CONFIG_LIBDIR="$sysroot/usr/lib/aarch64-linux-gnu/pkgconfig:$sysroot/usr/lib/pkgconfig:$sysroot/usr/share/pkgconfig"
exec /usr/bin/pkg-config "\$@"
EOF
chmod +x "$tools/aarch64-linux-gnu-pkg-config"
python3 - "$sysroot" <<'PY'
import os
from pathlib import Path
import sys

root = Path(sys.argv[1])
for directory, directories, files in os.walk(root):
    for name in directories + files:
        path = Path(directory) / name
        if path.is_symlink() and path.readlink().is_absolute():
            target = root / str(path.readlink()).lstrip("/")
            path.unlink()
            path.symlink_to(os.path.relpath(target, path.parent))
PY
find "$root/apt/archives" -maxdepth 1 -name '*.deb' -printf '%f\n' | sort > "$root/packages.txt"
dpkg-query -W -f='${binary:Package}=${Version}\n' '*aarch64-linux-gnu*' '*-arm64-cross' \
  | sort >> "$root/packages.txt"
identity=$(sha256sum "$root/packages.txt" | cut -d ' ' -f 1)
{
  echo "OPENNOW_ARM64_SYSROOT=$sysroot"
  echo "OPENNOW_ARM64_TOOLS=$tools"
  echo "OPENNOW_ARM64_SDK_ID=$identity"
  echo "CC_aarch64_unknown_linux_gnu=$tools/aarch64-linux-gnu-gcc"
  echo "CXX_aarch64_unknown_linux_gnu=$tools/aarch64-linux-gnu-g++"
  echo "AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar"
  echo "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=$tools/aarch64-linux-gnu-gcc"
  echo "PKG_CONFIG_aarch64_unknown_linux_gnu=$tools/aarch64-linux-gnu-pkg-config"
  echo "PKG_CONFIG_PATH_aarch64_unknown_linux_gnu="
  echo "BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_gnu=--target=aarch64-unknown-linux-gnu --sysroot=$sysroot -I$sysroot/usr/include/aarch64-linux-gnu"
  echo "CMAKE_TOOLCHAIN_FILE_aarch64_unknown_linux_gnu=$PWD/.github/cmake/linux-arm64-cross.cmake"
} >> "${GITHUB_ENV:?}"
echo "$tools" >> "${GITHUB_PATH:?}"
