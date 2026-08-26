# OpenNOW Windows media backend

This isolated crate provides the Windows media end of the native streamer. It accepts complete H.264 Annex B access units and interleaved `f32` PCM that has already been decoded from Opus.

The backend uses only Windows system APIs:

- Media Foundation asynchronous hardware MFTs for H.264-to-NV12 decode. The backend requires a registered hardware decoder, D3D11 awareness, and D3D11-backed output samples. It does not silently select the Media Foundation software decoder.
- A D3D11 video processor and a two-buffer flip-model DXGI swap chain for NV12 conversion, aspect-correct scaling, and presentation. The swap chain can target a caller-owned HWND, or the backend can create and own a child or top-level HWND.
- WASAPI shared-mode rendering for interleaved IEEE-float PCM. Windows performs endpoint format conversion when the active mix format differs.

`WindowsBackend::probe_for` creates a hidden presentation path using either D3D11 or a D3D12-backed D3D11-on-12 device, configures an adapter-matched hardware H.264 transform, and starts a 48 kHz stereo WASAPI client. A capability is reported only when that complete operation succeeds. `WindowsBackend::probe` remains the D3D11 compatibility entry point. Non-Windows builds expose the same typed API but report the backend as unavailable.

## Integration API

```rust,no_run
use std::num::NonZeroU32;
use opennow_streamer_platform_windows::{
    AudioFormat, BackendConfig, Bounds, OwnedWindow, SurfaceTarget, VideoFormat,
    WindowsBackend,
};

let backend = WindowsBackend::start(BackendConfig {
    video: VideoFormat {
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

### Electron surfaces

`SurfaceTarget::Existing` is only for a dedicated HWND that the caller reserves for D3D presentation. Do not pass an Electron `BrowserWindow` HWND as an existing surface because the swap chain and window input policy would then share Electron's interactive window.

Electron integration must use `SurfaceTarget::Owned` with the Electron HWND in `OwnedWindow::parent`. The backend creates a child with `WS_EX_NOACTIVATE | WS_EX_TRANSPARENT`, returns `HTTRANSPARENT` from `WM_NCHITTEST`, and rejects mouse activation. The child therefore cannot take focus or consume pointer input from the renderer. `OwnedWindow::bounds` are parent-client coordinates relative to the Electron renderer. Calling `set_surface` with the same parent updates those bounds and `visible` state in place with `SWP_NOACTIVATE`; changing the parent recreates the owned child and its swap chain.

Video and audio reconfiguration flush their respective queues. D3D11 or WASAPI failures move the backend to `Recovering`, clear stale packets, and retry for five seconds. Successful D3D recovery emits `KeyFrameRequired`; exhausted recovery emits a fatal device-loss event. `stop` closes both queues and joins the media thread, including when recovery is in progress.

## Checks

The crate is a member of the native streamer workspace and can also be checked directly:

```sh
cargo test --manifest-path native/opennow-streamer/crates/opennow-streamer-platform-windows/Cargo.toml
cargo check --manifest-path native/opennow-streamer/crates/opennow-streamer-platform-windows/Cargo.toml --all-targets --target x86_64-pc-windows-msvc
cargo check --manifest-path native/opennow-streamer/crates/opennow-streamer-platform-windows/Cargo.toml --all-targets --target aarch64-pc-windows-msvc
```

The cross-target checks validate the Win32 bindings for x64 and ARM64. Hardware decode, presentation, audio output, device removal, and endpoint switching still require tests on real Windows hardware.
