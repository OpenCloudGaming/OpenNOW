# Build and install the Flatpak

Build the Qt/native application as a local Flatpak bundle on Linux x86_64.
This does not publish OpenNOW to Flathub or change the existing signed release packages.

## Install the build tools

Install `flatpak`, `flatpak-builder`, Python 3.11 or newer, and stable Rust with Cargo.
On Ubuntu, install the Flatpak tools with:

```sh
sudo apt-get install flatpak flatpak-builder
```

Add Flathub and install the runtime, SDK, and compiler extensions:

```sh
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y flathub \
	org.kde.Platform//6.10 org.kde.Sdk//6.10 \
	org.freedesktop.Sdk.Extension.rust-stable//25.08 \
	org.freedesktop.Sdk.Extension.llvm20//25.08
```

## Build the bundle

Run these commands from the repository root:

```sh
python3 opennow-qt/packaging/flatpak/prepare_sources.py
flatpak-builder --user --force-clean --jobs=4 \
	--state-dir=build/flatpak/state --repo=build/flatpak/repo \
	build/flatpak/app opennow-qt/packaging/flatpak/io.github.opencloudgaming.OpenNOW.json
flatpak build-bundle build/flatpak/repo build/flatpak/OpenNOW-x86_64.flatpak \
	io.github.opencloudgaming.OpenNOW master \
	--runtime-repo=https://flathub.org/repo/flathub.flatpakrepo
```

Rerun `prepare_sources.py` after either Rust lockfile changes. It uses `cargo vendor --locked`
for both workspaces, including Git dependencies and their submodules. Downloads happen before
compilation. The Flatpak build has no network access and uses the generated sources under
`build/flatpak/cargo`. Do not commit that directory.

The package downloads a checksum-pinned FFmpeg 9 source archive before entering the build
sandbox. `OPENNOW_FFMPEG_ARCHIVE` tells the existing bundled-FFmpeg build to extract that
archive into its private build directory instead of fetching Git sources. Decoder features
and GPU presentation stay the same as the native package. The Flatpak does not
include the Raspberry Pi-specific FFmpeg fork used by the ARM64 native packages.

To build in CI, select **Actions → Qt Flatpak build → Run workflow**. The workflow builds,
installs, and checks an x86_64 bundle, then uploads `opennow-qt-flatpak-x86_64`. It does not
create a GitHub release or require release-signing credentials.

## Install and check the bundle

```sh
flatpak install --user -y build/flatpak/OpenNOW-x86_64.flatpak
python3 opennow-qt/packaging/verify_flatpak.py
flatpak run io.github.opencloudgaming.OpenNOW
```

The verification command checks the installed streamer's VAAPI and FFmpeg capabilities,
external update handling, the acceptance verifier, and Qt startup without a display. For playback validation, log in
with a real GFN account and test a session on the target GPU. Check audio, controller hotplug,
captures, browser authentication, and both Wayland and X11. The host needs working Flatpak
desktop portals and a Secret Service keyring.

The sandbox grants network access for GFN, display sockets for Qt, and device access for
GPU decoding and SDL controllers. `--device=all` keeps controller support on Flatpak versions
that lack the newer `input` permission. It is broader than GPU-only access. PipeWire uses
the host's `pipewire-0` socket, and PulseAudio compatibility uses its standard Flatpak socket.
Only the `OpenNOW` subdirectory of your Pictures directory is writable for captures. The
manifest grants neither your entire home directory nor unrestricted D-Bus access.

## Update or remove the package

The in-app self-updater is disabled in Flatpak. If you install from a Flatpak repository,
update through your software manager or run:

```sh
flatpak update io.github.opencloudgaming.OpenNOW
```

A standalone local bundle has no update feed. Rebuild it from the desired source revision
and install the new bundle with `flatpak install --user -y build/flatpak/OpenNOW-x86_64.flatpak`.

To remove the app while retaining its settings:

```sh
flatpak uninstall io.github.opencloudgaming.OpenNOW
```

Flatpak keeps application data under `~/.var/app/io.github.opencloudgaming.OpenNOW/`.
The package does not import settings from a native OpenNOW installation. Use `--delete-data`
with `flatpak uninstall` only if you also want to remove the sandbox's saved data.
