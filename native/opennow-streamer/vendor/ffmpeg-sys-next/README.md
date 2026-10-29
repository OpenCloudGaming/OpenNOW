[![ffmpeg-sys-next on crates.io](https://img.shields.io/crates/v/ffmpeg-sys-next?cacheSeconds=3600)](https://crates.io/crates/ffmpeg-sys-next)
[![build](https://github.com/zmwangx/rust-ffmpeg-sys/actions/workflows/build.yml/badge.svg?branch=master)](https://github.com/zmwangx/rust-ffmpeg-sys/actions)

This is a fork of the abandoned [ffmpeg-sys](https://github.com/meh/rust-ffmpeg-sys) crate. You can find this crate as [ffmpeg-sys-next](https://crates.io/crates/ffmpeg-sys-next) on crates.io.

This crate contains low level bindings to FFmpeg. You're probably interested in the high level bindings instead: [ffmpeg-next](https://github.com/zmwangx/rust-ffmpeg).

A word on versioning: major and minor versions track major and minor versions of FFmpeg, e.g. 4.2.x of this crate has been updated to support the 4.2.x series of FFmpeg. Patch level is reserved for bug fixes of this crate and does not track FFmpeg patch versions.

## OpenNOW bundled source selection

With the `build` feature, Linux `aarch64` targets build the FFmpeg `n9.0`
release (or the `OPENNOW_FFMPEG_ARCHIVE` tarball) and apply every
`patches/v4l2-request/*.patch` in name order with `patch -p1 --fuzz=0`:

- `0001-v4l2-request.patch`: the V4L2 Request API hwaccels for MPEG-2, H.264,
  HEVC, VP8, VP9 and AV1 (FFmpeg pull request 20847), as carried by LibreELEC
  and rebased without fuzz onto the 9.0 tarball.
- `0002-hevc-ext-sps-rps.patch`: the extended SPS RPS controls that Rockchip
  VDPU381/VDPU383 HEVC decoders require, used only when the driver exposes them.
- `0003-opennow-uapi-compat.patch`: definitions for the Linux 7.0 HEVC controls
  and the Raspberry Pi COL128 capture formats, which SDK and distribution UAPI
  headers lack.
- `0004-opennow-sand-capture-padding.patch`: reserves 4096 extra bytes per
  8-bit SAND capture memory plane for Vulkan import alignment and GPU read-ahead.
  Negotiated sizes, pitches, and DRM descriptor extents remain unchanged, as do
  OUTPUT allocations, other capture formats, and 10-bit SAND. Overflow is rejected.

The source and install directories include a hash of the patch contents, so
editing a patch invalidates the bundled cache. Other targets retain the upstream
FFmpeg `release/9.0` path. System FFmpeg builds are unchanged.

This target configures `--enable-libdrm --enable-libudev --enable-v4l2-request`;
their development headers, libraries, and pkg-config metadata must be available
for the target architecture, and `patch` must be on `PATH`. Linux UAPI headers
come from the target toolchain and must declare the stateless H.264, HEVC and AV1
controls (Linux 6.5 or newer). The build rejects configurations missing the
request hwaccel for any of those codecs. Cross builds must use target pkg-config
metadata, not host libraries. OpenNOW's `build-portable` feature remains enabled
and no SoC-specific CPU tuning is introduced.

The patches carry FFmpeg's LGPL-2.1-or-later license headers, not the binding
crate's WTFPL. These changes do not enable FFmpeg's GPL, version3, or nonfree
build options. Distributors must retain the applicable FFmpeg notices and satisfy
its source and relinking requirements for the statically linked libraries.



## Feature flags

In addition to feature flags declared in `Cargo.toml`, this crate performs various compile-time version and feature detections and exposes the results in additional flags. These flags are briefly documented below; run `cargo build -vv` to view more details.

- `ffmpeg_<x>_<y>` flags (new in v4.3.2), e.g. `ffmpeg_4_4`, indicating the FFmpeg installation being compiled against is at least version `<x>.<y>`. Currently available:

  - `ffmpeg_3_0`
  - `ffmpeg_3_1`
  - `ffmpeg_3_2`
  - `ffmpeg_3_3`
  - `ffmpeg_3_4`
  - `ffmpeg_4_0`
  - `ffmpeg_4_1`
  - `ffmpeg_4_2`
  - `ffmpeg_4_3`
  - `ffmpeg_4_4`
  - `ffmpeg_5_0`
  - `ffmpeg_5_1`
  - `ffmpeg_6_0`
  - `ffmpeg_6_1`
  - `ffmpeg_7_0`
  - `ffmpeg_7_1`
  - `ffmpeg_8_0`
  - `ffmpeg_8_1`
  - `ffmpeg_9_0`

- `avcodec_version_greater_than_<x>_<y>` (new in v4.3.2), e.g., `avcodec_version_greater_than_58_90`. The name should be self-explanatory.

- `ff_api_<feature>`, e.g. `ff_api_vaapi`, corresponding to whether their respective uppercase deprecation guards evaluate to true.

- `ff_api_<feature>_is_defined`, e.g. `ff_api_vappi_is_defined`, similar to above except these are enabled as long as the corresponding deprecation guards are defined.
