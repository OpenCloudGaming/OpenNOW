# Raspberry Pi: Qt desktop streaming

The Raspberry Pi target is **Raspberry Pi OS Desktop, 64-bit (Trixie)** on Pi 4
and Pi 5. Use the Linux ARM64 OpenNOW build. The 32-bit operating system is not
supported by the native streamer. Raspberry Pi OS Lite needs a working graphical
session; OpenNOW is a Qt application, not a direct-to-display video player.

## Decoder paths

| Board | H.264 | HEVC/H.265 | AV1 |
| --- | --- | --- | --- |
| Pi 4 | Stateful V4L2 hardware decoder | Stateless V4L2 Request hardware decoder | Not supported by this Pi path |
| Pi 5 | No H.264 hardware decoder | Stateless V4L2 Request hardware decoder | Not supported by this Pi path |

The Pi decoder paths support **8-bit 4:2:0 SDR**. Do not select 10-bit, HDR, or
4:4:4 for them. These restrictions do not change the separate Vulkan Video and
VA-API paths on other hardware. The embedded Linux runtime does not silently
switch to software decoding when hardware initialization fails.

Both Pi decoder implementations use the existing **V4L2** backend setting.
Auto can select it when the requested codec is available. For an initial
hardware check, select V4L2 explicitly, HEVC, 8-bit 4:2:0, HDR off, and
1920×1080 at 60 FPS. Pi 4 H.264 should be checked separately. A higher resolution
or frame-rate setting is not evidence that the board can decode and present it
reliably.

Qt still owns the only window and video surface. HEVC uses the Raspberry Pi
FFmpeg hardware acceleration code to retain DRM DMA-BUF frames. The GPU converts
the Pi's SAND128 column layout into textures used by the existing embedded
Vulkan renderer, without mapping video pixels into CPU memory. This is a GPU
copy, not literal zero-copy presentation. The existing stateful H.264 path uses
CPU-backed capture planes and a Vulkan upload.

## Operating-system requirements

Use a current Raspberry Pi OS kernel and Mesa/V3DV Vulkan driver, with the
normal KMS desktop configuration. Vulkan rendering support is separate from
Vulkan Video decoding support: the Pi HEVC decoder uses V4L2 Request, not Vulkan
Video. Do not force Qt to use OpenGL or software rendering.

The user running OpenNOW must be able to open the decoder's `/dev/video*` and
`/dev/media*` nodes and the GPU's `/dev/dri/renderD*` node. Device numbers can
change; do not assume that the decoder is always `/dev/video19`. Fix missing
device permissions through the operating system's session/group policy rather
than running OpenNOW as root.

Useful checks on the Pi:

```sh
uname -m
cat /etc/os-release
vulkaninfo --summary
v4l2-ctl --list-devices
ls -l /dev/media* /dev/video* /dev/dri/renderD*
```

`uname -m` should report `aarch64`. Vulkan should report the board's V3DV driver,
not llvmpipe/lavapipe. Install `vulkan-tools` and `v4l-utils` if the diagnostic
commands are missing. An advertised decoder or a working desktop alone does not
prove that compressed video has been decoded successfully.

Linux ARM64 bundled builds include a pinned Raspberry Pi FFmpeg source revision
with V4L2 Request and SAND support; they do not depend on the system `ffmpeg`
command to decode a live stream. Other platforms retain their existing bundled
FFmpeg source. Use the bundled build for this target; custom system-FFmpeg
configurations are not covered by these Pi build checks, and a generic FFmpeg
installation does not provide the required hardware accelerator.

Follow the Qt build instructions in [opennow-qt/README.md](../opennow-qt/README.md)
for the SDK, SDL3, Rust, and media dependencies. Do not assume that a binary built
against a newer distribution's C library will run on legacy Raspberry Pi OS
Bookworm. This target does not establish compatibility with 32-bit Raspbian or
older Raspberry Pi boards.

## Real-device acceptance

For a local decoder-only check on the Pi, install an FFmpeg command-line build
with `libx265` for generating the test input, then run:

```sh
cargo test --manifest-path native/opennow-streamer/Cargo.toml \
  -p opennow-streamer-platform-linux --features ffmpeg-bundled \
  raspberry_pi_hevc_request_drm_only -- --ignored --nocapture
```

This opt-in test exercises the bundled HEVC request decoder and checks retained
DRM output, not Qt display, network streaming, or sustained performance. It must
run on actual Pi hardware; skipping it is not a passing hardware test.

Run this matrix on **each board** before treating Raspberry Pi support as
hardware-validated:

1. Confirm the selected hardware backend and negotiated codec in diagnostics.
   On Pi 5, HEVC must use V4L2 Request; a software decoder is not a passing result.
2. Stream moving content for at least ten minutes at 1080p60. Check receive,
   decode, presentation, dropped-frame, and latency statistics separately. Audio
   continuing while video freezes is a failure.
3. Repeat in windowed and fullscreen modes. Resize the window, open and close
   F3 statistics, the Ctrl+G menu, and exit confirmation without restarting media.
4. Stop and start several sessions. Exercise reconnects and a resolution change;
   verify that old frames do not reappear and decoder/GPU resources are released.
5. Exercise packet loss and recovery. The client must request a fresh reference
   frame when prediction state is lost, rather than continue displaying damage.
6. Repeat Pi 4 testing with H.264. Test higher HEVC resolutions only after the
   1080p60 baseline passes, and record the exact resolution, FPS, bitrate, kernel,
   Mesa version, board model, cooling, and power setup.

Use **Settings → About → Copy diagnostics** for the OpenNOW report. Review it
before sharing; never publish account credentials or session authentication
material. Include the operating-system and device checks above when reporting a
missing decoder or failed DMA-BUF import.

Unit tests, ARM64 compilation, and software-Vulkan rendering tests can validate
contracts and conversion logic. They cannot validate `rpivid`, Pi DMA-BUF
interoperability, sustained performance, or live GFN gameplay without a Pi.
