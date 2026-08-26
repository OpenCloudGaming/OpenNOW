# OpenNOW native macOS platform backend

This isolated crate implements the macOS media path without GStreamer or FFmpeg:

- H.264 Annex B or four-byte AVCC access units are copied into CoreMedia sample buffers and
  decoded asynchronously by VideoToolbox.
- VideoToolbox is asked for Metal-compatible, IOSurface-backed NV12 (`420v`) pixel buffers. A
  `CVMetalTextureCache` maps both planes into Metal without a CPU pixel copy, and a
  `CAMetalLayer` presents them through a small BT.601/BT.709 conversion shader.
- Opus packets are decoded to interleaved `f32` PCM by the reference libopus decoder. The
  dependency builds libopus statically on macOS, so it adds no runtime media-framework
  dependency. A default-output Audio Unit pulls PCM from a fixed-size SPSC ring in its real-time
  callback.

The crate is a member of the native-streamer workspace and is linked only on macOS.

## Integration API

Create `H264ParameterSets` from the current SPS and PPS and start the backend on AppKit's main
thread. OpenNOW's native host creates a standalone SDL/AppKit window and supplies its dedicated
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
    video: H264Format::new(parameter_sets, VideoColorSpace::Bt709),
    audio: AudioFormat::OPUS_STEREO_48KHZ,
    queues: QueueLimits::default(),
})?;

let sink = backend.sink();
// Move `sink` to the transport thread and call submit_h264/submit_opus there.
// Keep `backend` on the AppKit main thread.

backend.stop();
# Ok::<(), Box<dyn std::error::Error>>(())
```

The native streamer never passes Electron's `NSView` or `NSWindow` address to this Rust child. In
standalone mode it passes only the helper's own SDL `NSView`. The alternative passive-overlay API
uses absolute screen rectangles:

```rust,no_run
use opennow_streamer_platform_macos::ScreenRect;

backend.update_owned_overlay(ScreenRect::new(120.0, 80.0, 960.0, 540.0), true)?;
backend.update_owned_overlay(ScreenRect::new(120.0, 80.0, 960.0, 540.0), false)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Screen rectangles in overlay mode use Electron's top-left device-independent coordinates. The backend converts
them to AppKit's bottom-left coordinate space. Its concrete `NSPanel` is borderless and
non-activating, ignores mouse events, and orders without stealing focus. The helper runs from a
regular LaunchServices application bundle; bare command-line executables are not a supported
deployment shape because WindowServer does not reliably composite their cross-process windows. The
native streamer also compares the frontmost application with its Electron parent process on every
main-thread pump, ordering the panel out while another application is active. The separately owned
SDL window used by the software fallback applies the same parent/frontmost gate.

`StreamSink` is `Send + Sync`; `MacOsBackend` is intentionally neither. Video and audio can be
reconfigured through `StreamSink` while running. Video reconfiguration constructs a new
VideoToolbox session before replacing the old one. Audio reconfiguration stops the old Audio Unit
and decoder worker before constructing the new format, so a failed audio reconfiguration leaves
audio stopped rather than running under an ambiguous format.

Only H.264 is implemented. The workspace advertises this backend only when
`VTIsHardwareDecodeSupported` succeeds and Metal can create a device and command queue. This crate
makes no H.265, AV1, software-decoder, or non-macOS capability claim.

The workspace performs full backend construction after the first SPS/PPS arrive. If VideoToolbox,
Metal, CoreAudio, or surface construction fails at that point, the main-thread host destroys the
partial macOS output, initializes the existing SDL output, and hands the pending keyframe plus the
same bounded H.264/Opus queues to the OpenH264/software workers. Hardware selection is disabled for
later sessions in that process and later capability replies report VideoToolbox unavailable.

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
closed. A supplied window's passive child view is removed without modifying the BrowserWindow
content view or renderer layer.

## Safety invariants

`BorrowedNsView::from_raw` and `BorrowedNsWindow::from_raw` are the only public unsafe entry
points. Their pointers must be live objects of exactly the documented AppKit class and must be
created and consumed on AppKit's main thread. The backend retains the object for its lifetime. A
supplied view is treated as a dedicated presentation surface because its backing layer is replaced
until shutdown. A supplied window instead receives an owned child surface; geometry/visibility
updates, child insertion, and child removal all require AppKit's main thread.

The `NativeSurfaceHandle` pointers borrow the running backend. For a supplied window, `ns_view`
identifies the owned passive child rather than the BrowserWindow content view. The pointers are
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

Both Apple architectures can be type-checked from a non-macOS host when Rust's targets are
installed. A real build still requires the Apple SDK and a matching C compiler because vendored
libopus is compiled for the target:

```sh
cargo check --target aarch64-apple-darwin
cargo check --target x86_64-apple-darwin
```
