# OpenNOW native streamer

This workspace implements the native GeForce NOW NVST runtime. It owns RTSPS negotiation, the dedicated Mjolnir SRTP video socket, the NVST ICE/DTLS/SCTP bundle used for audio, RTCP and input, bounded media queues, platform decode/audio output, source-stream Matroska recording and GPU frame publication. It does not implement the browser WebRTC offer/answer or trickle-ICE protocol and it has no microphone upstream path.

The Qt application loads `opennow-streamer-ffi` as an in-process shared library. Qt supplies one complete CloudMatch session context; the embedded engine reserves its bundle and Mjolnir sockets and performs OPTIONS, DESCRIBE, SETUP, ANNOUNCE, PLAY, keepalive and TEARDOWN. The runtime does not load or redistribute NVIDIA client libraries.

## Crates

- `opennow-streamer-protocol`: versioned local command and session DTOs.
- `opennow-streamer-core`: NVST lifecycle, command routing, media feedback and recording.
- `opennow-streamer-transport`: Mjolnir SRTP plus the NVST-required ICE/DTLS/SCTP, RTCP and input implementation.
- `opennow-streamer-platform`: bounded media queues, decode/audio output, recording and GPU-frame publication.
- `opennow-streamer-platform-{windows,macos,linux}`: platform decoders and native GPU texture producers.
- `opennow-streamer-ffi`: bounded C ABI used in process by Qt.
- `opennow-streamer`: a development JSON-lines host for the same engine; Qt packages do not include it.

## Qt GPU integration

The Qt path does not create an SDL video window or a child streamer process. `NativeStreamRuntime` owns the embedded Rust handle, and `StreamVideoItem` drives the GPU-only FFI from Qt's render thread:

1. Qt lends the current QRhi native graphics objects to the runtime.
2. Platform decode publishes a native GPU frame into a one-frame, drop-stale mailbox.
3. Qt acquires the latest opaque frame token and asks Rust to record conversion and synchronization into the same QRhi command buffer that will render it.
4. Qt imports the returned RGBA8 native texture and samples it in the scene graph, so QML overlays compose above video normally.
5. Qt releases the token after GPU use and shuts down the scene-graph binding on the render thread before QRhi teardown.

The FFI exposes no CPU image, encoded-frame callback, swap chain, window or Qt object. See `crates/opennow-streamer-ffi/README.md` for ownership and threading details.

## Packaging

The Qt CMake build always compiles `opennow-streamer-ffi` with Cargo's release profile and links the resulting shared library to `opennow-qt`. CPack installs that library beside the Qt executable and `opennow-core`; there is no separate streamer executable, helper application or native video window in the Qt package. Linux packages enable the bundled FFmpeg fallback while optional GPU driver interfaces remain dynamically discovered.

The standalone `opennow-streamer` binary remains a development host and is not evidence for the packaged Qt presentation path.

## Checks

```sh
cargo fmt --manifest-path native/opennow-streamer/Cargo.toml --all -- --check
cargo clippy --manifest-path native/opennow-streamer/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path native/opennow-streamer/Cargo.toml --workspace
```

Release validation must additionally run authorized live sessions on Windows, Linux/X11, Linux/Wayland, Intel macOS and Apple Silicon to validate NVST interoperability, native GPU import, audio, input, recovery and device-loss behavior.
