# OpenNOW Native Streamer v2

This workspace is the clean replacement for the former GStreamer-based native streamer.

The workspace owns the local process protocol, lifecycle state machine, standards-based WebRTC transport, and media output path. The baseline backend links OpenH264, Opus, and SDL from source, so it does not require a GStreamer or FFmpeg runtime.

The executable retains the versioned JSON-lines process contract used by OpenNOW while the app shell is migrated away from Electron. It does not load or redistribute NVIDIA client libraries.

## Crates

- `opennow-streamer-protocol`: versioned local IPC DTOs.
- `opennow-streamer-core`: session lifecycle and command routing.
- `opennow-streamer-transport`: ICE, DTLS-SRTP, RTP/RTCP, and SCTP data channels.
- `opennow-streamer-platform`: bounded media queues, OpenH264/Opus decode, SDL audio/video output, and platform-native Electron surface ownership.
- `opennow-streamer`: process entry point.

## Checks

```sh
cargo test --manifest-path native/opennow-streamer/Cargo.toml
cargo build --manifest-path native/opennow-streamer/Cargo.toml --release
```

The Electron app's build wrapper also checks protocol-version parity, copies the executable into the platform package directory, and runs a JSON-lines `hello`/`stop` process smoke test:

```sh
npm --prefix opennow-stable run native:build
```

## Platform integration

The SDL presentation window is created hidden on the process main thread. Windows and X11/XWayland reparent it as an input-transparent child of the Electron native window. macOS keeps it as a non-activating, mouse-ignoring child `NSWindow` and converts renderer-relative surface rectangles through the Electron `NSView`. The executable runs `MainThreadHost` on its real main thread; stdin, WebRTC, and codec work run on named workers. This is required by AppKit and is checked with `pthread_main_np()` on macOS.

Wayland does not provide a portable foreign-surface parenting protocol, so native presentation requires Electron's X11/XWayland mode on Linux. Hardware decoder entries remain unavailable until their implementations are linked; the built software backend advertises H.264 only.

## macOS validation

Run the checks natively on each architecture rather than treating a Linux cross-check as macOS proof:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
OPENNOW_NATIVE_STREAMER_TARGET="$(rustc -vV | sed -n 's/^host: //p')" \
OPENNOW_NATIVE_STREAMER_PLATFORM_KEY="darwin-$(uname -m | sed 's/arm64/arm64/;s/x86_64/x64/')" \
npm --prefix opennow-stable run native:build
```

The CI package matrix runs this build on both Apple Silicon and Intel macOS runners before producing the DMG and ZIP. Release validation must additionally inspect the app bundle, verify its signature, launch the packaged child executable, and confirm fallback behavior on real hardware.

## Runtime validation

Unit tests synthesize and decode H.264 and Opus frames, verify drop-oldest queue behavior, and exercise pause/stop lifecycle. Release validation must additionally run an authorized live session on Windows, Linux/XWayland, Intel macOS, and Apple Silicon to validate WebRTC interoperability, A/V timing, Electron child-surface stacking, and device-specific audio output.
