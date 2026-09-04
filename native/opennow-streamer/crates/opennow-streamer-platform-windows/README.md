# OpenNOW Windows media backend

This isolated crate provides the Windows media end of the native streamer. It accepts complete
H.264, HEVC/H.265, or AV1 access units and interleaved `f32` PCM that has already been decoded from
Opus.

The backend uses only Windows system APIs:

- Media Foundation hardware MFTs for H.264/HEVC/AV1 decode into NV12 or P010. Hardware selection
  accepts only adapter-matched hardware transforms. A separate software mode accepts only
  synchronous/asynchronous software MFTs that are D3D11-aware and publish D3D11-backed output;
  the two modes are never silently mixed.
- A D3D11 video processor and a two-buffer flip-model DXGI swap chain for NV12 conversion, aspect-correct scaling, and presentation. The swap chain can target a caller-owned HWND, or the backend can create and own a child or top-level HWND.
- WASAPI shared-mode rendering for interleaved IEEE-float PCM. Windows performs endpoint format conversion when the active mix format differs.

`WindowsBackend::probe_for` creates a hidden presentation path using either D3D11 or a D3D12-backed
D3D11-on-12 device, independently probes adapter-matched hardware transforms and registered
software transforms for H.264, HEVC and AV1, and starts a 48 kHz stereo WASAPI client. A codec
capability is reported only when its selected decoder class and the complete presentation/audio
operation succeed. `WindowsBackend::probe` remains the D3D11 compatibility entry point.
Non-Windows builds expose the same typed API but report the backend as unavailable.

## Integration API

```rust,no_run
use std::num::NonZeroU32;
use opennow_streamer_platform_windows::{
    AudioFormat, BackendConfig, Bounds, OwnedWindow, SurfaceTarget, VideoCodec, VideoFormat,
    WindowsBackend,
};

let backend = WindowsBackend::start(BackendConfig {
    video: VideoFormat {
        codec: VideoCodec::H264,
        width: 1920,
        height: 1080,
        frame_rate_numerator: NonZeroU32::new(120).unwrap(),
        frame_rate_denominator: NonZeroU32::new(1).unwrap(),
        average_bitrate: 75_000_000,
    },
    audio: AudioFormat {
        sample_rate: 48_000,
        channels: 2,
    },
    surface: SurfaceTarget::Owned(OwnedWindow {
        parent: None,
        bounds: Bounds {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        },
        visible: true,
    }),
    video_queue_capacity: 3,
    audio_queue_capacity: 8,
})?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`submit_video` and `submit_audio` never wait for the media thread. When a configured queue is full, the oldest packet is discarded and the method returns `PushOutcome::DroppedOldest`. The WASAPI staging buffer is independently capped at two endpoint buffers. Events use a bounded drop-oldest queue as well, so a stalled event consumer cannot create unbounded memory growth.

### Existing and parented surfaces

`SurfaceTarget::Existing` is only for a dedicated HWND that the caller reserves for D3D presentation. Do not pass the shell's interactive top-level HWND as an existing surface because the swap chain and window input policy would then share that interactive window.

An embedded shell integration must use `SurfaceTarget::Owned` with the shell HWND in `OwnedWindow::parent`. The backend creates a child with `WS_EX_NOACTIVATE | WS_EX_TRANSPARENT`, returns `HTTRANSPARENT` from `WM_NCHITTEST`, and rejects mouse activation. The child therefore cannot take focus or consume pointer input from the shell. `OwnedWindow::bounds` are parent-client coordinates relative to the shell. Calling `set_surface` with the same parent updates those bounds and `visible` state in place with `SWP_NOACTIVATE`; changing the parent recreates the owned child and its swap chain.

Video and audio reconfiguration flush their respective queues. D3D11 or WASAPI failures move the backend to `Recovering`, clear stale packets, and retry for five seconds. Successful D3D recovery emits `KeyFrameRequired`; exhausted recovery emits a fatal device-loss event. `stop` closes both queues and joins the media thread, including when recovery is in progress.

## Checks

The crate is a member of the native streamer workspace and can also be checked directly:

```sh
cargo test --manifest-path native/opennow-streamer/crates/opennow-streamer-platform-windows/Cargo.toml
cargo check --manifest-path native/opennow-streamer/crates/opennow-streamer-platform-windows/Cargo.toml --all-targets --target x86_64-pc-windows-msvc
cargo check --manifest-path native/opennow-streamer/crates/opennow-streamer-platform-windows/Cargo.toml --all-targets --target aarch64-pc-windows-msvc
```

The cross-target checks validate the Win32 bindings for x64 and ARM64. Hardware decode, presentation, audio output, device removal, and endpoint switching still require tests on real Windows hardware.
