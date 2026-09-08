# OpenNOW native streamer

This workspace implements the native GeForce NOW NVST runtime. It owns RTSPS negotiation, the dedicated Mjolnir SRTP video socket, the NVST ICE/DTLS/SCTP bundle used for audio, microphone, RTCP and input, bounded media queues, platform decode/audio output, source-stream Matroska recording and GPU frame publication. It does not implement the browser WebRTC offer/answer or trickle-ICE protocol.

The Qt application loads `opennow-streamer-ffi` as an in-process shared library. Qt supplies one complete CloudMatch session context; the embedded engine reserves its bundle and Mjolnir sockets and performs OPTIONS, DESCRIBE, SETUP, ANNOUNCE, PLAY, keepalive and TEARDOWN. The runtime does not load or redistribute NVIDIA client libraries.

## Crates

### Hardware selection in the Qt app

Stream settings default to `nativeVideoBackend: auto`. On Windows the embedded presenter
currently uses D3D11; its picker offers an explicit DX11 override and shows DX12/Vulkan as
unavailable. Standalone D3D12 support does **not** imply embedded D3D12 texture interop.
Unsupported forced backends fail explicitly; they never silently select a different API.
On Linux supported decoder overrides are passed per session, without changing process-global
environment variables. Qt's compositor still uses the platform-native presentation API.

Windows codec detection requires both a usable Media Foundation transform and a matching
DXVA decoder profile/configuration on the probed GPU. Decoder startup repeats the check on
Qt's adopted device with the actual codec, dimensions, bit depth and chroma. Installing an
AV1 codec package on a GPU without AV1 hardware support no longer makes it hardware-capable.
The core resolves Auto against the embedded capability report before CloudMatch allocation.
This is capability selection, not an automatic reconnect or codec change inside a live stream.

GPU import/shader failures are reported to the shell and Qt log. To diagnose an individual
black-screen report, collect `diagnostics/native-streamer.log` plus the Qt log and distinguish
an entirely black application window from a stream-only black video surface.

### Workspace components

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

### Audio output selection

Protocol 5 accepts the additive request `{"type":"audioDevices","id":"audio-1"}`
and returns `{"type":"audioDevices","id":"audio-1","devices":[{"id":"...","name":"..."}]}`.
Failures return a correlated `error` with code `audio-devices-unavailable`; an empty
successful list is not an enumeration failure. Queries run on the existing native
host worker with a two-second response timeout and at most one outstanding query.
Lists exceeding 512 KiB return an error rather than exceeding Qt's callback bound.
Queries neither open a playback device
nor pause, recreate, or stop the active session.

The optional session-context `settings.audioOutputDevice` string is an opaque ID
from this response. An absent or empty string uses the system default and preserves
existing backend fallback behavior. Nonempty IDs are used only when the next session
starts, must contain no NUL and be at most 1024 UTF-8 bytes, and must identify exactly
one available output. Missing or ambiguous fixed outputs fail explicitly rather than
opening the default. Clients persist IDs, never list positions, device indices, or
display names. Duplicate identities are excluded from enumeration.

The Qt embedded backends use their actual playback identities:

- Windows uses exact SDL2 playback device names. SDL playback and enumeration share
  the existing embedded host worker; Qt does not create a second audio owner.
- Linux uses `pipewire:<node.name>` or `alsa:<PCM hint name>`. PipeWire enumeration
  requires `pw-dump` and is bounded to 1.5 seconds and 4 MiB; playback uses `pw-cat`.
  ALSA enumeration uses the playback PCM hints from the same dynamically loaded
  libasound backend. Default-routing aliases are excluded from fixed ALSA choices.
  Fixed outputs disable cross-backend fallback; PipeWire streams also set
  `node.dont-fallback` and `node.dont-reconnect`.
- macOS uses `coreaudio:<device UID>` and passes that UID through native backend
  startup and audio recovery rather than persisting an AudioDeviceID.

The development standalone host uses SDL identities for SDL playback and CoreAudio
identities for native macOS playback. Standalone Windows native WASAPI output does
not support fixed SDL device selection; use the Qt embedded path instead.
The startup selection guarantee does not promise that the operating system will
never reroute an already-playing stream after device loss.

This extension does not change protocol or FFI ABI versions: old clients omit the
setting and keep default behavior, and new clients must tolerate `unknown-command`
from older runtimes that do not implement enumeration.

## Packaging

The Qt CMake build always compiles `opennow-streamer-ffi` with Cargo's release profile and links the resulting shared library to `opennow-qt`. CPack installs that library beside the Qt executable and `opennow-core`; there is no separate streamer executable, helper application or native video window in the Qt package. Linux packages enable the bundled FFmpeg fallback while optional GPU driver interfaces remain dynamically discovered.

The standalone `opennow-streamer` binary remains a development host and is not evidence for the packaged Qt presentation path.

### Embedded latency and resource bounds

- Windows decoder workers wait for actionable input or a bounded 1 ms output
  poll; queued frames do not wake a worker that has no decoder input credits.
  Closing the queue interrupts either wait immediately.
- Windows conversion targets are allocated lazily for the QRhi slots actually
  used, with an eight-slot upper bound. Format changes discard the old slot set.
- Stereo SDL playout retains the 120 ms overflow bound. A backlog above 60 ms
  lasting 200 ms enables allocation-free catch-up resampling (at most 2% faster)
  until the queued tail reaches 40 ms. Normal playback is bit-exact; clearing
  the buffer resets recovery. This controls the application queue, not latency
  inside the OS audio device, and the temporary correction can slightly alter pitch.
- Qt samples the converted surface directly in its scene pass; it does not
  allocate a second video-sized intermediate render target. Native conversion
  and Qt drawing remain on the same graphics command stream.

## Checks

### Microphone upstream

Microphone capture defaults off. The `voice-activity` settings value is exposed as
**Open microphone**: it sends continuous audio, not a voice-activity detector or
push-to-talk implementation. Select it before starting a session; NVST does not
renegotiate microphone support during a stream. Capture uses the system default
input device. A nonempty `microphoneDeviceId` is rejected rather than silently
capturing a different device.

The native RTSP path requires the server's DESCRIBE to advertise
`x-nv-general.rtcMicOnNativeBundle:1`. Only then, and with microphone opt-in,
ANNOUNCE includes that flag and `x-nv-mic.micSsrcConfig.senderSsrc:1`. The existing
str0m bundle sends Opus payload type 111 on SSRC 1, independently of downlink
audio. This follows the native bundle findings documented in OpenNOW-Mac's
`docs/StreamTransportArchitecture.md` (reference revision `88a09bd68598651b367aa4744e7528da9c074d28`).
Legacy RTSP/UDP microphone carriage is not implemented.

Capture produces mono 48 kHz PCM and an off-callback encoder produces 20 ms Opus
frames at 32 kbps. PCM, encoded audio, and transport queues each hold at most five
frames and drop old data under load. Mute closes capture and clears pending audio;
unmute preserves the RTP clock and transport sequence lifetime. Capture or uplink
failure disables only the microphone, not game audio/video. Session termination
also closes capture. Queue drops are reported in microphone diagnostics without
logging audio contents.

Embedded capture devices are owned by the existing embedded runtime host, sharing
SDL ownership with Windows playback and output-device enumeration. Engine keeps
only the microphone session's shared control; the uplink worker receives encoded
packets without owning SDL. Mute pauses hardware synchronously, and the host
reclaims stopped capture on its next bounded poll. The host sleeps without a
polling timer when no capture is active. Output enumeration and input-pause
commands never open, mute, or restart capture. The actual audio-device-aware
stream-start entry point resets the microphone clock; mute/unmute does not.

The local JSON protocol is version 6; the C ABI is unchanged. `hello` exposes
`supportsMicrophone` for runtime capture support, while the `start` response
reports the negotiated session capability. `microphone-set` requires a boolean
`enabled`; `microphone-toggle` toggles the current capture state. Both require an
active, opted-in, negotiated session. `microphone-state` events contain `state`
(`disabled`, `muted`, `ready`, `unavailable`, or `error`), `enabled`, and an optional
`message`. `ready` confirms local capture/queueing, not that a remote game has
selected the microphone.

The `start` command accepts an optional boolean `microphoneEnabled`. Setting it
to false reserves negotiated microphone support but does not open capture; Qt
uses this to retain mute during same-session recovery. It cannot override a
disabled microphone mode or a server that does not support bundle capture.

Live release acceptance must confirm remote voice reception and uninterrupted
game audio, then mute/unmute, disconnect the input device, end/restart the session,
and deny OS permission. Use headphones: acoustic echo cancellation and noise
suppression are not provided by this capture path.

### Embedded session diagnostics

Protocol 5 telemetry includes optional `jitterMs` (RTP interarrival jitter on
the video 90 kHz clock) and `packetLossPercent` (cumulative authenticated RTP
reception loss since stream start). Values are null before a stream is known.
Reading these measurements does not advance RTCP report intervals. Qt forwards
measured values without converting nulls into zeros.

The optional `pingMs` field measures round-trip time on the active session. It prefers
the nominated ICE candidate pair's measured RTT, then authenticated STUN/NATT replies
on the dedicated video socket or control/audio bundle, then a timed RTSPS keepalive.
STUN replies must match an outstanding transaction, the peer address, fingerprint, and
message integrity. Each receiver tracks at most 64 probes. ICE statistics only refresh
the sample when the pair's response count changes; rereading old statistics cannot keep
an old measurement alive.

The control fallback follows [OpenNOW-Mac's live-tested keepalive method](https://github.com/OpenCloudGaming/OpenNOW-Mac/blob/90627114383501dd18ef165baa005d9ea603fdf3/GFN/NVST/Rtsp/NvstRtspConnection.swift#L161-L234):
send `GET_PARAMETER` with the RTSP Session header every two seconds over the existing
RTSPS WebSocket, and time the matching response. A `551 Option Not Supported` response
still measures a real round trip. Some seats do not answer STUN/ICE probes and close the
connection on client WebSocket pings, so those pings are replaced by session-scoped
RTSP keepalives. Server-initiated WebSocket pings are still answered with pongs.

Measurements expire after five seconds, and new sessions start without a sample.
Missing or expired ping is emitted as null. No discovery-server ping, remote-clock
assumptions, stale handshake timing, or synthesized ICE replies supply this metric.
Ping is network/control-path RTT, not one-way video or input-to-display latency. Decode duration
and end-to-end latency remain unavailable without a measurement source; Qt hides those
two metrics when unavailable in the panel, compact bar, and copied statistics.

Queue discard events retain `type: "log"`, `event: "queue-dropped"`, `media`, and
the per-source delta `count`. The additive `unit` field is `frames` for video
decode/presentation sources, `samples` for `audio-output`, `packets` for compressed
audio or queued PCM blocks, and `items` for unknown sources. `audio-output` also
includes `sampleRate: 48000` and `channels: 2`; its interleaved sample count converts
to milliseconds as `count * 1000 / (sampleRate * channels)`. Audio packet/block
duration is unknown and must not be inferred from its count. Never sum unlike
units or treat these queue-stage counters as unique lost video frames or network
packet loss.

Pending deltas flush every second, including when no further drops occur. Full
event queues retain pending counts for retry. Shutdown joins the media producers
and drains their remaining drop feedback before final best-effort delivery.
Final summaries never wait for event-queue capacity: undeliverable deltas are
written directly to the native log with `delivery=event-queue-unavailable`, then
removed. These deltas will be missing from callback/UI totals under saturation.
Each delivered or directly logged final delta is removed, so it is never counted twice.
Native-to-Qt file logs retain every delivered per-source delta and its unit,
independently of periodic telemetry logging. Routine progress/file telemetry and
repeated keyframe-attempt logs use ten-second intervals; errors, warnings and
recovery transitions remain visible. Callback telemetry and wire feedback/retry
cadences are unchanged.

During Windows decoder recreation, the worker retains one pending recovery
keyframe outside the bounded input queue. Already-queued descendants remain in
FIFO order instead of being silently cleared. This adds at most one owned access
unit while the replacement MFT waits for input credits; it adds no presenter or
CPU pixel-copy path. Regression coverage verifies reference-chain ordering.

```sh
cargo fmt --manifest-path native/opennow-streamer/Cargo.toml --all -- --check
cargo clippy --manifest-path native/opennow-streamer/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path native/opennow-streamer/Cargo.toml --workspace
```

Controlled output tests can also run without physical audio hardware:

```sh
SDL_AUDIODRIVER=dummy cargo test --manifest-path native/opennow-streamer/Cargo.toml -p opennow-streamer-platform selected_sdl_output -- --ignored
ALSA_CONFIG_PATH="$PWD/native/opennow-streamer/crates/opennow-streamer-platform-linux/tests/fixtures/audio-null.conf" cargo test --manifest-path native/opennow-streamer/Cargo.toml -p opennow-streamer-platform-linux selected_alsa_output -- --ignored
```

The ignored `selected_pipewire_output` test requires an isolated PipeWire server
and session manager with an `Audio/Sink` node named `opennow_test_output`. Point
`XDG_RUNTIME_DIR` at that server's private runtime directory when running the test.

Release validation must additionally run authorized live sessions on Windows, Linux/X11, Linux/Wayland, Intel macOS and Apple Silicon to validate NVST interoperability, native GPU import, audio, input, recovery and device-loss behavior.
