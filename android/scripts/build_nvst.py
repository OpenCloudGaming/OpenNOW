#!/usr/bin/env python3
"""Build the pinned NVST JNI library for every packaged Android ABI. No downloads of binaries."""
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

ndk = Path(sys.argv[1])
output = Path(sys.argv[2])
root = Path(__file__).resolve().parents[1] / 'nvst'
host = {'Darwin': 'darwin-x86_64', 'Linux': 'linux-x86_64', 'Windows': 'windows-x86_64'}[platform.system()]
toolchain = ndk / 'toolchains/llvm/prebuilt' / host / 'bin'
cargo = os.environ.get('CARGO') or shutil.which('cargo') or str(Path.home() / '.cargo/bin/cargo')
cmake = os.environ.get('CMAKE') or shutil.which('cmake')
if not cmake:
    # Android Studio installs CMake under the SDK without adding it to Gradle's PATH.
    candidates = list((ndk.parents[1] / 'cmake').glob('*/bin/cmake' + ('.exe' if os.name == 'nt' else '')))
    if candidates:
        cmake = str(max(candidates, key=lambda p: tuple(int(v) for v in p.parents[1].name.split('.') if v.isdigit())))
if not cmake:
    raise SystemExit('Install CMake through Android SDK Manager or set CMAKE to its executable.')
# Prefer rustup's target-aware toolchain over a Homebrew host-only installation.
if 'CARGO' not in os.environ and (Path.home() / '.cargo/bin/cargo').exists():
    cargo = str(Path.home() / '.cargo/bin/cargo')
for abi, target, clang_target in [
    ('arm64-v8a', 'aarch64-linux-android', 'aarch64-linux-android'),
    ('armeabi-v7a', 'armv7-linux-androideabi', 'armv7a-linux-androideabi'),
    ('x86_64', 'x86_64-linux-android', 'x86_64-linux-android'),
    ('x86', 'i686-linux-android', 'i686-linux-android'),
]:
    env = os.environ.copy()
    suffix = '.cmd' if os.name == 'nt' else ''
    cc = str(toolchain / (clang_target + '23-clang' + suffix))
    env['CARGO_TARGET_' + target.upper().replace('-', '_') + '_LINKER'] = cc
    env['CC_' + target.replace('-', '_')] = cc
    env['AR_' + target.replace('-', '_')] = str(toolchain / ('llvm-ar.exe' if os.name == 'nt' else 'llvm-ar'))
    env['CXX_' + target.replace('-', '_')] = str(toolchain / (clang_target + '23-clang++' + suffix))
    env['CMAKE'] = cmake
    env['OPUS_NO_PKG'] = '1'
    env['OPENNOW_ANDROID_NDK'] = str(ndk)
    env['OPENNOW_ANDROID_ABI'] = abi
    env['CMAKE_TOOLCHAIN_FILE'] = str(root / 'android-toolchain.cmake')
    env['CARGO_TARGET_DIR'] = str(root / 'target')
    env['RUSTFLAGS'] = env.get('RUSTFLAGS', '') + ' -C link-arg=-Wl,-z,max-page-size=16384'
    subprocess.run([cargo, 'build', '--locked', '--release', '--manifest-path', str(root / 'Cargo.toml'), '--target', target], env=env, check=True)
    destination = output / abi
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copy2(root / 'target' / target / 'release/libopennow_nvst.so', destination)
