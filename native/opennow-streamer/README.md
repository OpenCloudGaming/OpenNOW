# OpenNOW Native Streamer

This crate is the native process boundary for OpenNOW's experimental native streamer.

The current binary implements the JSON-lines protocol used by the Electron main process and reports a `stub` backend by default. It validates session context, prepares incoming GFN offers with the same server-IP fix used by the browser client, and contains tested Rust ports of the SDP/NVST helpers and input packet encoder. When built with `--features gstreamer`, the `gstreamer` backend initializes GStreamer, creates a `webrtcbin` pipeline, validates remote SDP syntax with GStreamer's SDP parser, performs offer/answer negotiation, emits local ICE candidates asynchronously, accepts remote ICE candidates, creates the browser-compatible input data channels, parses the server input handshake, sends input heartbeats, links incoming RTP pads through `decodebin`, and routes decoded audio/video to platform auto sinks. It owns pipeline shutdown and heartbeat thread shutdown.

The next implementation step is to embed the native video render surface into the OpenNOW window and forward captured keyboard, mouse, and gamepad packets from Electron into the native process. The GStreamer backend currently opens platform-managed sinks rather than an Electron-owned child surface, and it sends heartbeat packets only after the input handshake.

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

For Electron packaging, run the OpenNOW native build script from `opennow-stable`; it copies the release binary into `native/opennow-streamer/bin`. Set `OPENNOW_NATIVE_STREAMER_FEATURES=gstreamer` when the packaging environment has the GStreamer development packages installed and should ship the GStreamer backend.
