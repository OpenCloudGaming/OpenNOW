# OpenNOW Yocto layer

`meta-opennow` builds the Qt Quick client, Rust application core, and native NVST
streamer into a Linux image. It does not include a second desktop runtime.

The initial baseline is Yocto 6.0 Wrynose, Qt 6.11, glibc, and either x86-64 or
AArch64. Wrynose supplies Rust 1.94; older Yocto releases do not provide a new
enough Rust toolchain for the locked dependencies. The layer requires these
additional layers:

| Repository | Branch | Layers |
| --- | --- | --- |
| OpenEmbedded Core | `wrynose` | `meta` |
| BitBake | `2.18` | Build tool |
| meta-openembedded | `wrynose` | `meta-oe`, `meta-python` |
| meta-qt6 | `6.11` | Repository root |

The [kas configuration](kas/qemux86-64.yml) pins exact commits of those
dependencies. This is a reference integration, not a certified board support
package. Parsing recipes or booting QEMU does not validate GPU decoding or a
real GFN session.

## Add the client to an existing image

Add the dependency layers and this repository's `meta-opennow` directory to
`BBLAYERS`. In your distribution or local configuration, enable:

```bitbake
DISTRO_FEATURES:append = " opengl vulkan wayland"
PACKAGECONFIG:append:pn-qtbase = " vulkan"
PACKAGECONFIG:append:pn-ffmpeg = " vaapi"
LICENSE_FLAGS_ACCEPTED += "commercial_ffmpeg"
```

Accepting `commercial_ffmpeg` is an explicit distribution licensing decision.
Review FFmpeg's configuration, codec patent requirements, Qt's licensing terms,
and the resulting license manifest before distributing an image. OpenNOW's MIT
license does not replace its dependencies' licenses.

Install the client from your image recipe:

```bitbake
IMAGE_INSTALL:append = " opennow"
```

Then build `bitbake opennow` or your image. The recipes fetch a fixed OpenNOW
commit, not the working tree that contains the layer. Recipe modifications take
effect immediately; application changes require updating the source pin.

## Build the reference image

From the OpenNOW repository root, with [kas](https://kas.readthedocs.io/)
installed and a host supported by Wrynose:

```sh
export KAS_WORK_DIR="$PWD/build/yocto"
mkdir -p "$KAS_WORK_DIR"
kas build meta-opennow/kas/qemux86-64.yml
kas shell meta-opennow/kas/qemux86-64.yml -c 'runqemu qemux86-64 slirp'
```

The reference image includes Weston and OpenNOW. It accepts FFmpeg's license
flag and permits empty-password root login for development. Remove the kas
configuration's `EXTRA_IMAGE_FEATURES`, provision authentication, and apply your
distribution's security policy before shipping an image. The layer does not install a root-run kiosk service or
automatically start a gaming session.

`KAS_WORK_DIR` keeps dependency checkouts and build output under the ignored
`build/` directory instead of adding them to the source tree.

Use `meta-opennow/kas/qemuarm64.yml` instead for the AArch64 reference machine,
and pass `qemuarm64` to `runqemu`. Both configurations use the same pinned layers.

Launch `opennow-qt` from a terminal in the Weston user session. For a hardware
image, select your BSP's machine and GPU driver configuration rather than
treating the QEMU machine as a hardware template.

## Runtime requirements

The image must provide a Vulkan-capable GPU driver, a working Wayland compositor,
audio devices and their drivers, network connectivity, correct system time, and
a writable home directory. The client requires access to its render, audio, and
controller devices. Set permissions through your distribution's session and
device policy, not by running OpenNOW as root. AArch64 support in the recipe
does not imply that every board's Vulkan or video driver supports streaming.

Qt's QML imports and platform plugins are explicit runtime dependencies. The
streamer links against the image's FFmpeg libraries with its `linux-ffmpeg` and
`linux-vaapi` features. VAAPI drivers are board-specific and must come from the
BSP or image. Bundled FFmpeg's driver interfaces are not automatically enabled
in the system FFmpeg build. In particular, NVDEC and Vulkan Video support depend
on how the distribution builds FFmpeg and its GPU drivers.

The streamer retains its upstream, statically built SDL2, Opus, and OpenH264
dependencies; the Qt shell uses the image's SDL3. BitBake fetches the pinned
SDL2 Git source and its submodules before compilation. No Cargo or FFmpeg build
step is granted network access. Configuration checks both Cargo graphs with
`cargo metadata --frozen` against the fetched sources before compiling.

Provide a session D-Bus and a Secret Service provider for normal keyring-backed
credential storage. If the image has no browser, complete the login flow on
another device using the client's login link or QR code. A real NVIDIA/GFN
account is required to validate gameplay. Manage installed application updates
through the image or package feed; do not use desktop AppImage or Debian updates
to replace a Yocto-managed installation.

## Recipe ownership

* `opennow-license-report-native` builds the existing license-report tool for the
  build host. Target executables are never run while cross-compiling.
* `opennow-runtime` builds both Rust workspaces in release mode with the Yocto
  linker and sysroot, then stages their artifacts and generated notices. This
  recipe only populates the sysroot; it does not produce an image package.
* `opennow` builds Qt using `OPENNOW_PREBUILT_NATIVE_DIR` and owns the installed
  client and native runtimes. CMake does not launch Cargo in this mode. Native
  binaries remain beside the client so capability probes and core startup keep
  their existing path contract.
* `opennow-image` extends the standard Weston image with the client. Existing
  product images can install `opennow` without using this image recipe.

Yocto retains responsibility for stripping and debug splitting. The Cargo
release profile's upstream stripping is disabled by the recipes. License
notices are installed in `/usr/share/doc/opennow` alongside the Qt packaging
notices, and BitBake generates its normal package license metadata.

## Update sources and verify

Commit the application changes first, then update the source revision and crate
fetch metadata from the repository root:

```sh
python3 meta-opennow/scripts/update-sources.py --revision HEAD
python3 meta-opennow/scripts/update-sources.py --check
python3 -m unittest discover -s meta-opennow/tests -v
```

The generator reads both Cargo lockfiles, deduplicates registry packages,
preserves their SHA-256 checksums, and pins SDL2 to its locked Git revision. It
rejects unknown Git dependencies rather than silently allowing Cargo to fetch
them. Push the source commit before distributing a layer revision that pins it.

Inside the configured Yocto build environment, validate metadata and fetches:

```sh
bitbake -p
bitbake -g opennow-image
bitbake opennow-runtime -c fetch
bitbake opennow
```

After populating a download mirror with all dependencies, rebuild with
`BB_NO_NETWORK = "1"` to verify the offline build. On the target, verify app
startup, login, capability detection, audio and video, controllers, reconnects,
and windowed/fullscreen overlays on the same live stream before considering
the image production-ready.
