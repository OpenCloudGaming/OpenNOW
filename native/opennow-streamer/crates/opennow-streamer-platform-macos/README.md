# OpenNOW native macOS platform backend

This isolated crate implements the macOS media path without GStreamer or FFmpeg:

- H.264 and HEVC/H.265 Annex B or four-byte AVCC/HVCC access units, plus AV1 low-overhead
  temporal units, are copied into CoreMedia sample buffers and decoded asynchronously by
  VideoToolbox. AV1 starts only after a keyframe supplies a valid sequence-header OBU from which
  the `av1C` codec configuration can be derived.
- VideoToolbox is asked for Metal-compatible, IOSurface-backed NV12 (`420v`), ten-bit P010
  (`x420`), or ten-bit P410 (`x444`) pixel buffers. A `CVMetalTextureCache` maps both planes into Metal without a CPU
  pixel copy. The embedded Qt path records conversion into Qt's Metal command buffer; the
  standalone `CAMetalLayer` presenter remains limited to eight-bit NV12.
- Opus packets are decoded to interleaved `f32` PCM by the reference libopus decoder. The
  dependency builds libopus statically on macOS, so it adds no runtime media-framework
  dependency. A default-output Audio Unit pulls PCM from a fixed-size SPSC ring in its real-time
  callback.

The crate is a member of the native-streamer workspace and is linked only on macOS.

## Qt embedded integration

`MacOsBackend::start_embedded_with_publisher` runs on a session-owned worker and creates no
AppKit window, Metal device, command queue, or display link. Hardware VideoToolbox decoding is
required; the real-time property is requested as a best-effort latency hint. The single-frame
mailbox drops obsolete decoded frames without blocking the decoder. Qt owns composition,
fullscreen, overlays, and the graphics device throughout the session.

The pixel buffer and its CoreVideo/Metal textures remain retained until Qt's command buffer
completes, including when the stream item's graphics resources are retired before completion.
`video_metal_completed` measures completion of this GPU work, separately from the standalone
presenter's `video_presented` scanout count. Both paths can signal playback readiness. Persistent
embedded decoder or GPU failure is reported rather than switching to an invisible SDL/software
presenter.

The core adapts the SDR Auto codec policy from
[OpenNOW-Mac's stream settings](https://github.com/OpenCloudGaming/OpenNOW-Mac/blob/0a68e94c58b8190bbe3f789beb14a35c9ae99c6d/OPN/Stream/WebRTCMediaStreamSettings.swift):
prefer H.264 at 144 FPS or higher; otherwise prefer hardware AV1 for constrained bitrate or 4K,
and HEVC for 1440p or high bitrate. Selection stays within codecs actually supported by the Mac.
The decoder's real-time hint follows its `NvstVideoToolboxDecoder.swift`. These settings do not
import the reference's Swift/WebRTC runtime or change OpenNOW's NVST transport.

The embedded path preserves ten-bit SDR through RGB10A2 and BT.2020 PQ HDR through RGBA16F.
HDR requires ten-bit decode and an HDR-capable Qt output; texture precision alone does not
prove that a display supports HDR. `probe_h265_hdr_hardware` decodes an actual Main10 PQ
fixture and completes Metal RGBA16F conversion before permitting HEVC HDR capability.
P410 preserves full-resolution chroma, but a codec-wide
hardware query does not establish 4:4:4 support. `probe_h265_444_ten_bit_hardware` decodes a
real HEVC Main44410 IDR with a hardware-required session and verifies ten-bit, full-resolution
chroma output with IOSurface backing and successful Metal conversion. Only a successful probe permits advertising that profile.
Unsupported Macs return false; there is no software fallback or 4:2:0 downgrade. AV1 4:4:4
remains unadvertised without a separate hardware profile probe, as does AV1 HDR.

### Optional MetalFX spatial upscaling

`AdoptedMetalContext::upscale_width` and `upscale_height` request an output size in physical
viewport pixels; pass both as zero to disable scaling. Scaling is attempted only when neither
axis shrinks and at least one grows. As an allocation safety policy, targets above 8192 on
either axis or above 7680 × 4320 total pixels use normal conversion instead.

MetalFX is dynamically loaded at runtime and checked against the adopted device. It is not a
required framework at application load time. Unsupported systems, devices, formats, dimensions,
allocation failures, and Objective-C scaler exceptions fall back to the source-size conversion.
The descriptor uses perceptual SDR processing with matching RGBA8 or RGB10A2 input/output; it
never reduces P010 to eight-bit or enables HDR processing. An unsupported RGB10A2 scaler keeps
the normal ten-bit conversion.

Each of the existing eight maximum retired Qt frame slots caches at most one scaler, its input
texture, and its output texture. Identical configurations reuse these objects; failed creation
is also cached until that slot's configuration changes. Conversion and scaling encode into the
same Qt command buffer, with no new queue, commit, GPU wait, CPU copy, or readback in production.
Completion handlers retain both textures and the scaler even after graphics-surface retirement.
An encoding exception or failed GPU command buffer disables scaling for that device state. GPU
failure is asynchronous and cannot repair the already-submitted frame; following frames use
normal conversion, and persistent failures of that normal path still use the existing fatal
error policy. Graphics-resource recreation resets this optional-scaler failure state.

The API follows Apple's [spatial scaler contract](https://developer.apple.com/documentation/metalfx/mtlfxspatialscaler)
and [metal-cpp declarations](https://github.com/apple/metal-cpp/blob/main/MetalFX/MTLFXSpatialScaler.hpp).
The focused Linux tests cover sizing, bounds, and format keys. On a MetalFX-capable Mac, run
`cargo test -p opennow-streamer-platform-macos spatial_ -- --include-ignored` from the native
streamer workspace to additionally check cache/retirement behavior and GPU constant-color
preservation for eight- and ten-bit SDR. Only that hardware test reads GPU pixels back to the CPU.

## Standalone integration API

Create `H264ParameterSets` from the current SPS and PPS (or `H265ParameterSets` from VPS, SPS and
PPS) and start the backend on AppKit's main thread. OpenNOW's native host creates a standalone SDL/AppKit window and supplies its dedicated
`NSView` through `SurfaceTarget::NsView`, allowing SDL to own window input while Metal replaces
that view's backing layer. `OwnedOverlay` remains available to other same-process integrations and
debug tooling:

```rust,no_run
use opennow_streamer_platform_macos::{
    AudioFormat, BackendConfig, H264Format, H264ParameterSets, MacOsBackend,
    OwnedOverlayConfig, QueueLimits, ScreenRect, SurfaceTarget, VideoColorSpace,
};

let parameter_sets = H264ParameterSets::new(sps, pps)?;
let mut backend = MacOsBackend::start(BackendConfig {
    surface: SurfaceTarget::OwnedOverlay(OwnedOverlayConfig::new(
        ScreenRect::new(120.0, 80.0, 1280.0, 720.0),
        true,
    )),
    video: H264Format::new(parameter_sets, VideoColorSpace::Bt709).into(),
    audio: AudioFormat::OPUS_STEREO_48KHZ,
    audio_output_device: None,
    queues: QueueLimits::default(),
})?;

let sink = backend.sink();
// Move `sink` to the transport thread and call submit_h264/submit_opus there.
// Keep `backend` on the AppKit main thread.

backend.stop();
# Ok::<(), Box<dyn std::error::Error>>(())
```

The shell never passes its own `NSView` or `NSWindow` address across the process boundary. In
standalone mode the native host passes only the streamer's own SDL `NSView`. The alternative
same-process passive-overlay API uses absolute screen rectangles:

```rust,no_run
use opennow_streamer_platform_macos::ScreenRect;

backend.update_owned_overlay(ScreenRect::new(120.0, 80.0, 960.0, 540.0), true)?;
backend.update_owned_overlay(ScreenRect::new(120.0, 80.0, 960.0, 540.0), false)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Screen rectangles in overlay mode use the shell's top-left device-independent coordinates. The backend converts
them to AppKit's bottom-left coordinate space. Its concrete `NSPanel` is borderless and
non-activating, ignores mouse events, and orders without stealing focus. The helper runs from a
regular LaunchServices application bundle; bare command-line executables are not a supported
deployment shape because WindowServer does not reliably composite their cross-process windows. The
native streamer also compares the frontmost application with its shell parent process on every
main-thread pump, ordering the panel out while another application is active. The separately owned
SDL window used by the software fallback applies the same parent/frontmost gate.

`StreamSink` is `Send + Sync`; `MacOsBackend` is intentionally neither. Video and audio can be
reconfigured through `StreamSink` while running. Video reconfiguration constructs a new
VideoToolbox session before replacing the old one. Audio reconfiguration stops the old Audio Unit
and decoder worker before constructing the new format, so a failed audio reconfiguration leaves
audio stopped rather than running under an ambiguous format.

H.264, HEVC and AV1 are implemented and probed independently. The workspace advertises each codec
only when `VTIsHardwareDecodeSupported` succeeds for that codec and Metal can create a device and
command queue. This crate makes no software-decoder or non-macOS capability claim.

The workspace performs full backend construction after the first complete parameter-set group
arrives. If H.264 VideoToolbox, Metal, CoreAudio, or surface construction fails at that point, the
main-thread host destroys the partial macOS output, initializes the existing SDL output, and hands
the pending keyframe plus the same bounded H.264/Opus queues to the OpenH264/software workers.
HEVC and AV1 have no bundled macOS software decoder, so initialization or fatal decode failure for
either codec stops that media path and disables that codec in later capability replies instead of
silently mis-negotiating a fallback.

## Embedded color precision and HDR

H.264/H.265 callers pass negotiated precision with
`with_bit_depth(VideoBitDepth::Ten)` on initial configuration and reconfiguration. Their existing
constructors default to eight-bit. AV1 reads precision from its `av1C` record. CoreMedia's
`BitsPerComponent` metadata can raise the requested output to ten-bit but cannot lower a
ten-bit request. Unsupported bit depths fail configuration, and a decoder that returns an
eight-bit buffer for a ten-bit request reports a fatal VideoToolbox failure. H.264/H.265
callers pass `with_chroma(VideoChroma::Yuv444)` for ten-bit 4:4:4; AV1 reads chroma from
`av1C` and rejects monochrome and 4:2:2. Output callbacks reject chroma downgrades. Embedded
Metal validates both plane dimensions before importing the IOSurface.

The embedded converter maps NV12 SDR to `MetalFrameFormat::Rgba8Unorm` and P010/P410 SDR to
`MetalFrameFormat::Rgb10a2Unorm`. HDR callers explicitly configure `VideoColorSpace::Bt2020`
and `with_transfer(VideoTransfer::Pq)`. HDR uses `MetalFrameFormat::Rgba16Float`, containing
**encoded PQ RGB in BT.2020**, not linear light. `MetalRecordedFrame.color_space` and `transfer`
retain source color metadata. Qt owns PQ decoding, gamut conversion, HDR display configuration,
reference-white scaling and tone mapping; the native conversion must not apply these twice.
Conversion honors BT.601/BT.709/BT.2020 matrix attachments; absent attachments use the negotiated
matrix. Conflicting transfer or primaries attachments fail rather than displaying HDR as SDR.
HLG and BT.2020 SDR are not supported by this backend. Pixel-buffer format determines full
versus video range independently of depth and matrix. The SDR-only MetalFX scaler is bypassed
for HDR; standalone presentation rejects HDR and 4:4:4 rather than switching output paths.

P010's high-aligned ten-bit values are normalized from R16Unorm before applying the exact
luma endpoints, chroma midpoint, and range-specific chroma gain. The output pool retains at
most eight frame slots and three format-specific pipelines. Reusing a slot requires Qt to
retire its previous GPU work. Slot textures are
reused only when both dimensions and pixel format match; changing depth does not release
other slots' in-flight resources. RGB10A2 allocation or pipeline failure is returned as an
error, never silently retried as RGBA8. RGBA16F failure likewise never lowers HDR precision.

The bounded Main44410 probe fixture in `src/macos/profile_probe.rs` was generated with:

```sh
ffmpeg -f lavfi -i 'color=c=gray:s=64x64:r=1,format=yuv444p10le' -frames:v 1 \
  -c:v libx265 -profile:v main444-10 \
  -x265-params 'pools=none:frame-threads=1:repeat-headers=1:info=0:log-level=error' \
  -f hevc main44410.hevc
```

The probe caches the result once per process and should run off the Qt GUI/render threads.
It proves profile support, not every stream resolution or level; actual session creation still
requires hardware and validates each output. On macOS, run
`cargo test -p opennow-streamer-platform-macos hdr_shader_ -- --include-ignored` to compile
the actual Metal shader and read back adjacent PQ code values from RGBA16F. The
`report_main44410_hardware_probe` ignored test reports the real hardware result; false is a
valid result on hardware without that profile. Linux tests cannot establish hardware support.
The Main10 HDR fixture uses the same command with `format=yuv420p10le`, `-profile:v main10`,
and `-color_primaries bt2020 -color_trc smpte2084 -colorspace bt2020nc`. Its hardware result
is reported by `report_main10_hdr_hardware_probe`.

## Queue and lifecycle behavior

All buffering is explicitly bounded:

- VideoToolbox admission returns `SubmitOutcome::Backpressured` when the in-flight decode limit is
  reached; accepted decoder work is never silently invalidated.
- The decoded-frame queue drops its oldest frame when rendering falls behind, keeping latency
  bounded.
- While a supplied-window child is hidden, decoded frames are discarded before requesting a
  `CAMetalDrawable`; showing it resumes from the newest frame without accumulating hidden work.
- The Opus packet queue drops its oldest packet and reports `SubmitOutcome::ReplacedOldest`.
- The PCM ring never allocates or locks in the CoreAudio callback. If decoding outruns playback,
  new samples are dropped and counted. If playback underruns, the callback emits silence and
  counts the missing frames.

`stop` is idempotent. It first rejects new submissions, waits for VideoToolbox's outstanding
callbacks, stops CoreAudio, discards queued work, joins both workers, and waits for submitted Metal
command buffers. A supplied view's previous backing layer is then restored; an owned window is
closed. A supplied window's passive child view is removed without modifying the caller's content
view or renderer layer.

## Safety invariants

`BorrowedNsView::from_raw` and `BorrowedNsWindow::from_raw` are the only public unsafe entry
points. Their pointers must be live objects of exactly the documented AppKit class and must be
created and consumed on AppKit's main thread. The backend retains the object for its lifetime. A
supplied view is treated as a dedicated presentation surface because its backing layer is replaced
until shutdown. A supplied window instead receives an owned child surface; geometry/visibility
updates, child insertion, and child removal all require AppKit's main thread.

The `NativeSurfaceHandle` pointers borrow the running backend. For a supplied window, `ns_view`
identifies the owned passive child rather than the caller's content view. The pointers are
valid only until `stop` or `Drop`, may be used only on AppKit's main thread, and must never be
released by the caller.

VideoToolbox and CoreAudio callback contexts are heap allocated at stable addresses and outlive
their registered sessions. The VideoToolbox session is accessed behind a mutex; decoded
`CVPixelBuffer`s are immutable after the callback and retained across the queue. The Metal worker
retains each `CVMetalTexture` until its command buffer completes. The CoreAudio callback has one
consumer and the Opus worker has one producer for the PCM ring.

## Checks

On macOS, run:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

On a Mac with working VideoToolbox hardware decoding and Metal, also run this offline acceptance
test from the repository root (no GFN account is needed):

```sh
cargo test --manifest-path native/opennow-streamer/Cargo.toml \
  -p opennow-streamer-platform-macos \
  hardware_decode_to_embedded_metal_survives_surface_retirement -- --ignored --nocapture
```

It decodes a synthetic black H.264 frame in hardware, imports the IOSurface on an adopted Metal
device, and retires/recreates graphics resources before GPU completion. Live gameplay still needs
the Qt windowed/fullscreen, overlay, reconnect, and input checks in `docs/qt-acceptance.md`.

Both Apple architectures can be type-checked from a non-macOS host when Rust's targets are
installed. A real build still requires the Apple SDK and a matching C compiler because vendored
libopus is compiled for the target:

```sh
cargo check --target aarch64-apple-darwin
cargo check --target x86_64-apple-darwin
```
