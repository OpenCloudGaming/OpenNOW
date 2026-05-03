# GStreamer Bundling

OpenNOW's native streamer is dynamically linked against GStreamer. Release builds should not depend on a user having a matching global GStreamer install, so the packaged app should either ship a private runtime next to `opennow-streamer` or declare a platform package dependency.

## Current Implementation

The Windows x64 release path now supports a private runtime bundle:

1. CI installs the official GStreamer MSVC runtime and development MSI packages.
2. `npm run native:build` builds and verifies the Rust streamer with `OPENNOW_NATIVE_STREAMER_FEATURES=gstreamer`.
3. When `OPENNOW_BUNDLE_GSTREAMER_RUNTIME=1`, `scripts/bundle-gstreamer-runtime.mjs` copies the Windows runtime layout to `native/opennow-streamer/bin/win32-x64/gstreamer`.
4. Electron packages that directory through the existing `extraResources` rule.
5. The Electron main process detects a sibling `gstreamer` directory next to the selected streamer binary and injects `PATH`, `GST_PLUGIN_PATH`, `GST_PLUGIN_SYSTEM_PATH`, `GST_PLUGIN_SCANNER`, and `GIO_MODULE_DIR` only into the native streamer child process.

That keeps the app isolated from broken system installs and avoids leaking bundled GStreamer paths into the Electron process.

## Platform Strategy

| Platform | Release strategy | Status |
| --- | --- | --- |
| Windows x64 | Bundle official MSVC runtime privately next to `opennow-streamer.exe`. | Implemented in CI. |
| Windows arm64 | Needs either upstream arm64 GStreamer binaries or a Cerbero-built runtime and a Rust cross-build target. | Not enabled yet; keep web fallback. |
| macOS x64/arm64 | Bundle `GStreamer.framework` into `Contents/Frameworks`, then codesign/notarize the framework with the app. | Planned. |
| Linux deb | Prefer distro dependencies for GStreamer packages and GPU driver plugins. | CI builds against system packages; `.deb` declares core GStreamer dependencies. |
| Linux AppImage | Bundle a private runtime or use linuxdeploy/AppImage tooling, but keep VAAPI/V4L2 driver plugins host-compatible. | Planned. |

## CI/CD Notes

- `auto-build.yml` and `release.yml` build the native streamer before `electron-builder` on Windows x64 and Linux.
- Windows x64 sets `OPENNOW_BUNDLE_GSTREAMER_RUNTIME=1`, so generated installers include the private runtime.
- Linux currently validates the GStreamer backend in CI but does not copy a private runtime into the AppImage. The `.deb` path is a better fit for distro packages; AppImage bundling needs a separate dependency closure pass.
- macOS native packaging should be enabled only after the framework copy and signing path is complete. Unsigned or partially relocated GStreamer dylibs will fail under Gatekeeper.

## Manual Windows Packaging

From `opennow-stable`:

```powershell
$env:OPENNOW_NATIVE_STREAMER_FEATURES = "gstreamer"
$env:OPENNOW_BUNDLE_GSTREAMER_RUNTIME = "1"
npm run native:build
npm run dist
```

The runtime bundle is intentionally ignored by git under `native/opennow-streamer/bin/*/gstreamer/`.
