[![ffmpeg-sys-next on crates.io](https://img.shields.io/crates/v/ffmpeg-sys-next?cacheSeconds=3600)](https://crates.io/crates/ffmpeg-sys-next)
[![build](https://github.com/zmwangx/rust-ffmpeg-sys/actions/workflows/build.yml/badge.svg?branch=master)](https://github.com/zmwangx/rust-ffmpeg-sys/actions)

This is a fork of the abandoned [ffmpeg-sys](https://github.com/meh/rust-ffmpeg-sys) crate. You can find this crate as [ffmpeg-sys-next](https://crates.io/crates/ffmpeg-sys-next) on crates.io.

This crate contains low level bindings to FFmpeg. You're probably interested in the high level bindings instead: [ffmpeg-next](https://github.com/zmwangx/rust-ffmpeg).

A word on versioning: major and minor versions track major and minor versions of FFmpeg, e.g. 4.2.x of this crate has been updated to support the 4.2.x series of FFmpeg. Patch level is reserved for bug fixes of this crate and does not track FFmpeg patch versions.

## OpenNOW bundled source selection

With the `build` feature, Linux `aarch64` targets fetch
[`jc-kynesim/rpi-ffmpeg`](https://github.com/jc-kynesim/rpi-ffmpeg) at commit
`f43bd9dafd9349b6824ee1fdcd967662fcc94c20`, originally resolved from
`test/9.0/main`. The branch is provenance only: builds fetch and check out the
commit directly, without updating the branch. Both the source directory and
installed-library directory include the commit, so changing the pin invalidates
the bundled cache. Other targets retain the upstream FFmpeg `release/9.0` path.
System FFmpeg builds are unchanged.

For this bundled target, the build script emits `rpi=true` dependency metadata.
The upstream `ffmpeg-next` build script consumes it to enable its existing
Raspberry Pi pixel-format variants, preserving the fork's enum values without
patching the wrapper or changing system FFmpeg bindings.

The pinned fork provides `--enable-v4l2-request` and `--enable-sand` for Raspberry
Pi HEVC request decoding. This target also explicitly enables `libdrm` and
`libudev`; their development headers, libraries, and pkg-config metadata must be
available for the target architecture. Linux UAPI headers come from the target
toolchain, while the fork retains its HEVC request compatibility headers. Cross
builds must use target pkg-config metadata, not host libraries. OpenNOW's
`build-portable` feature remains enabled and no Raspberry Pi-specific CPU tuning
is introduced.

`--enable-v4l2-m2m` is also required: this fork builds the shared V4L2 pixel-format
helper used by the request decoder under its M2M configuration. Enabling that
build dependency does not select an M2M decoder at runtime. The build rejects
configurations missing request decoding, this helper dependency, or SAND.

The checkout preserves the fork's original copyright headers, `LICENSE.md`, and
`COPYING.*` files. Its default combined license is LGPL-2.1-or-later, not the
binding crate's WTFPL. These changes do not enable FFmpeg's GPL, version3, or
nonfree build options. See the pinned
[license description](https://github.com/jc-kynesim/rpi-ffmpeg/blob/f43bd9dafd9349b6824ee1fdcd967662fcc94c20/LICENSE.md)
and [LGPL text](https://github.com/jc-kynesim/rpi-ffmpeg/blob/f43bd9dafd9349b6824ee1fdcd967662fcc94c20/COPYING.LGPLv2.1).
Distributors must retain the applicable FFmpeg notices and satisfy its source
and relinking requirements for the statically linked libraries.

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
