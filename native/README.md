# OpenNOW Native macOS

This directory contains the macOS-only C++ migration work for the Electron desktop
client. The existing Electron app remains the shipping implementation while the
native core is built and validated in parallel.

## Layout

- `core/` - C++20 library for portable OpenNOW behavior.
- `core/tests/` - fixture-backed tests for behavior copied from the TypeScript app.
- `docs/` - migration inventories and release notes for the native effort.
- `macos/` - native macOS host shell and Objective-C++ bridge scaffolding.

## Current Scope

The native migration is intentionally macOS desktop only. The iOS SwiftUI
prototype is not part of this port.

## Build

```sh
cmake -S native/core -B native/core/build
cmake --build native/core/build
ctest --test-dir native/core/build
```

To build the native macOS host as a local `.app` bundle:

```sh
native/macos/build-app.sh
open native/macos/build/OpenNOWMac.app
```

## macOS Release Preflight

Run the same native-only validation used by CI from the repository root:

```sh
native/scripts/validate-macos-release.sh
```

The script configures a Release CMake build, runs `opennow_core_tests`, stages an
install payload, produces a `opennow-native-core-*.tar.gz` archive under
`native/core/build/macos-release/packages/`, and builds an unsigned
`OpenNOWMac.app` host bundle for local validation.
