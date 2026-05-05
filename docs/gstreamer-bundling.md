# GStreamer Bundling

OpenNOW's native streamer is dynamically linked against GStreamer. Packaged builds use a private runtime where that is reliable and distro packages where host GPU/driver compatibility matters more than isolation.

## Platform strategy

| Platform | Release strategy | Status |
| --- | --- | --- |
| Windows x64 | Bundle the official MSVC runtime privately inside the packaged app's platform bundle at `native/opennow-streamer/win32-x64/gstreamer`, sourced from the `bin/win32-x64` staging directory during build. | Implemented in CI/release. |
| Windows arm64 | No official upstream arm64 runtime path is enabled yet. | Native streamer packaging disabled; web fallback remains. |
| macOS x64/arm64 | Install the official universal runtime/devel `.pkg` files, copy the framework version root into `native/opennow-streamer/darwin-*/gstreamer`, and relocate Mach-O load commands to the private runtime. | Implemented in CI/release. |
| Linux deb | Use distro GStreamer packages. The `.deb` declares Debian/Ubuntu runtime dependencies. | Implemented. |
| Linux AppImage | Use the host distro's GStreamer install and show Settings install commands when missing. | Implemented by runtime guidance, not private bundling. |

## Private runtime layout

`npm run native:build` copies the Rust streamer into both `native/opennow-streamer/bin/opennow-streamer` and `native/opennow-streamer/bin/<platformKey>/opennow-streamer`. Electron packages `native/opennow-streamer/bin` via `extraResources`, and packaged builds prefer the platform-specific bundle so the installed app keeps the verified layout that `npm run native:build` tested:

```text
resources/native/opennow-streamer/
  opennow-streamer(.exe)  (staging/dev copy)
  <platformKey>/
    opennow-streamer(.exe)
    *.dll  (Windows loader/runtime helpers)
    gstreamer/
      bin/
      lib/
      libexec/
      share/
      etc/
      OPENNOW-GSTREAMER-RUNTIME.txt
```

The Electron main process prefers the packaged `<platformKey>/opennow-streamer(.exe)` path when it exists, detects a sibling `gstreamer` directory next to the selected streamer executable, and injects runtime paths only into the native streamer child process. It does not mutate the Electron process environment globally.

## Windows

CI installs the official GStreamer MSVC runtime and development MSI packages. With `OPENNOW_BUNDLE_GSTREAMER_RUNTIME=1`, `scripts/bundle-gstreamer-runtime.mjs` copies `bin`, plugins, GIO modules, helper scanners, shared data, and metadata into the private runtime directory. It also copies the GStreamer core loader DLL subset and available Microsoft VC runtime DLLs next to `opennow-streamer.exe` so Windows process loading succeeds before the child PATH is applied. Runtime detection prepends the executable directory and private `bin`, then sets GStreamer plugin/scanner environment variables for the child process.

## macOS

CI/release installs official universal packages from:

```text
https://gstreamer.freedesktop.org/data/pkg/macos/${GSTREAMER_VERSION}/gstreamer-1.0-${GSTREAMER_VERSION}-universal.pkg
https://gstreamer.freedesktop.org/data/pkg/macos/${GSTREAMER_VERSION}/gstreamer-1.0-devel-${GSTREAMER_VERSION}-universal.pkg
```

The build exports `GSTREAMER_1_0_ROOT_MACOS=/Library/Frameworks/GStreamer.framework/Versions/1.0`, prepends its `bin`, and adds its `lib/pkgconfig` directory to `PKG_CONFIG_PATH`. Local development can still fall back to `GSTREAMER_1_0_ROOT_MACOS`, `/Library/Frameworks/GStreamer.framework/Versions/Current`, Homebrew `brew --prefix`, `/opt/homebrew`, or `/usr/local` if the root has both `lib/pkgconfig/gstreamer-1.0.pc` and `lib/libgstreamer-1.0.dylib`.

The bundler copies framework/Homebrew runtime content into the private `gstreamer` directory and, when `otool` and `install_name_tool` are available, rewrites the packaged native executable, copied dylibs, plugins, GIO modules, and helper executables away from framework/Homebrew absolute paths. The native executable resolves dependencies through `@executable_path/gstreamer/lib`; files inside the runtime resolve through `@loader_path` relative paths.

## Linux

Linux private GStreamer bundling is not treated as reliable by default. VAAPI, V4L2, Vulkan, `libdrm`, GLib/glibc, Mesa, and proprietary GPU driver stacks must match the host distro closely; an AppImage-private dependency closure can break hardware decode or plugin loading on otherwise supported systems.

The `.deb` package declares Debian/Ubuntu GStreamer dependencies, including core libraries, base/good/bad/ugly/libav plugins, GL/X/ALSA helpers, VAAPI, libva, Vulkan, and Mesa Vulkan drivers. AppImage builds use the host distro's packages. If GStreamer is missing, Settings shows distro-specific install commands for Debian/Ubuntu-family, Fedora/RHEL-family, Arch-family, and openSUSE systems.
