# OpenNOW Rewrite (Bootstrap)

This folder contains the new rewrite scaffold requested for a cross-platform, lightweight architecture.

## Current status

- ✅ Separate rewrite project folder created
- ✅ CMake + C++20 bootstrap target compiles
- ✅ Core library boundary started (`opennow_core`)
- ✅ Login base scaffolding started (provider model, PKCE bootstrap, auth URL builder)
- ✅ Desktop bootstrap app target started (`opennow_desktop_bootstrap`)
- 🚧 Qt 6 integration (next)
- 🚧 GStreamer/WebRTC integration (next)

## Build

```bash
cd opennow-rewrite
cmake -S . -B build
cmake --build build
ctest --test-dir build --output-on-failure
./build/opennow_desktop_bootstrap
```

## Initial structure

- `include/opennow/core/` - public core interfaces
- `src/core/` - core implementation
- `apps/desktop/` - desktop executable entrypoint
- `platform/<os>/` - platform adapter implementation boundaries
- `docs/` - rewrite architecture/planning docs

## Next milestones

1. Add Qt 6 app shell target and startup window.
2. Add OAuth callback listener abstraction in platform layer (current version is synchronous socket-based).
3. Add token exchange + refresh (`/token`) and secure persistence.
4. Add media abstraction interfaces (`IMediaPipeline`, `ISessionClient`).
5. Add first Linux adapter and capability probe.
6. Add CI pipeline for Linux/macOS/Windows + arm64 runner.
