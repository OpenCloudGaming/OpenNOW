# OpenNOW Native Streamer

This crate is the native process boundary for OpenNOW's experimental native streamer.

The binary implements the JSON-lines protocol used by the Electron main process and reports a `stub` backend when it is built without optional media support. It validates session context, prepares incoming GFN offers with the same server-IP fix used by the browser client, and contains tested Rust ports of the SDP/NVST helpers and input packet encoder.

When built with `--features gstreamer`, the `gstreamer` backend initializes GStreamer, creates a `webrtcbin` pipeline, validates remote SDP syntax with GStreamer's SDP parser, performs offer/answer negotiation, emits local ICE candidates asynchronously, accepts remote ICE candidates, creates the browser-compatible input data channels, parses the server input handshake, sends input heartbeats, and owns pipeline shutdown plus heartbeat thread shutdown.

Incoming RTP video is linked through an explicit low-latency decoder path when the required platform plugins are installed:

- Windows: D3D12, then D3D11, then software fallback.
- macOS: VideoToolbox, then software fallback.
- Linux x64: VAAPI, then V4L2, then software fallback.
- Raspberry Pi / Linux arm64: V4L2 stateless, then VAAPI, then software fallback.

Set `OPENNOW_NATIVE_VIDEO_API` to `d3d12`, `d3d11`, `videotoolbox`, `vaapi`, `v4l2`, or `software` to force one path for diagnostics. The streamer logs the selected backend, decoder, renderer, and memory mode at stream startup.

Native OS-level input capture is currently implemented on Windows. On macOS, Linux, and Raspberry Pi, the native streamer keeps the input data channels active and Electron input forwarding remains the supported fallback.

Backend selection is controlled by OpenNOW settings and forwarded to the process with `OPENNOW_NATIVE_STREAMER_BACKEND`. Valid values are `stub` and, when the crate is built with `--features gstreamer`, `gstreamer`. Leaving the setting on auto omits the environment variable so the binary can choose the safest compiled default. If the requested backend is unavailable, the `ready` response includes `requestedBackend` and `fallbackReason` so Electron can fail early and fall back to the web streamer with a specific message.

Build for local development:

```powershell
cargo build --manifest-path native/opennow-streamer/Cargo.toml
```

Build the GStreamer backend for local streaming tests:

```powershell
cargo build --manifest-path native/opennow-streamer/Cargo.toml --features gstreamer
```

Run native tests:

```powershell
cargo test --manifest-path native/opennow-streamer/Cargo.toml
```

Run GStreamer feature tests:

```powershell
cargo test --manifest-path native/opennow-streamer/Cargo.toml --features gstreamer
```

For Electron packaging, run the OpenNOW native build script from `opennow-stable`; it copies the release binary into `native/opennow-streamer/bin`. Set `OPENNOW_NATIVE_STREAMER_FEATURES=gstreamer` when the packaging environment has the GStreamer development packages installed and should ship the GStreamer backend. On Windows x64 release builders, also set `OPENNOW_BUNDLE_GSTREAMER_RUNTIME=1` to copy a private GStreamer runtime next to the platform-specific streamer binary before Electron packages `extraResources`.

The build script also writes a platform-specific copy under `native/opennow-streamer/bin/<platform>-<arch>/`, for example `darwin-arm64`, `linux-x64`, or `linux-arm64`. Electron checks that directory first in packaged apps and still supports the flat `bin/opennow-streamer` development path.

Runtime dependency notes:

- Windows: install the GStreamer MSVC runtime/development packages with webrtc, d3d11, d3d12, libav, and codecs plugins. Packaged x64 releases can bundle these files under `native/opennow-streamer/win32-x64/gstreamer`, and Electron will point only the native child process at that private runtime.
- macOS: install GStreamer with webrtc, applemedia/VideoToolbox, gl, libav, and codec parser plugins.
- Linux x64: install GStreamer base/good/bad/ugly/libav plus VAAPI plugins for hardware decode.
- Raspberry Pi 64-bit Linux: install GStreamer base/good/bad/ugly/libav plus v4l2 stateless decoder plugins where the distro provides them.

If the requested native backend or a hardware decoder is unavailable, OpenNOW logs the missing capability and falls back to the next available path or to the web streamer.
