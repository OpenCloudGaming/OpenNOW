# Provider media worker example

This native binary is the media role of the local OpenNOW SDK demo. It generates
video and audio, consumes accepted input events, and demonstrates the canonical
media-worker protocol. It does not connect to a cloud gaming service or render a
window. The host owns decoding and presentation.

Build from the repository root:

```sh
export CARGO_TARGET_DIR="$HOME/.capy/work/opennow-provider-v2/provider-worker-target"
cargo build --release --manifest-path examples/provider-media-worker/Cargo.toml
```

The resulting binary is `$CARGO_TARGET_DIR/release/provider-media-worker`, or
`provider-media-worker.exe` on Windows. The packager in `examples/provider-plugin`
includes this binary as the verified media role. This worker has no independent
manifest or package command. OpenH264 and the repository's vendored Opus are
compiled into the binary; FFmpeg is not a runtime dependency.

## Host contract

The host supplies a bounded, newline-terminated `WorkerBootstrap` on stdin and
sets `OPENNOW_PLUGIN_DATA_DIR` to the demo control process's private data root.
The worker uses the shared `opennow_sdk_demo::authorization` library to validate
the private provider bootstrap against the real durable journal before opening
its control socket or emitting media. It does not accept a data-root path inside
the provider bootstrap.

The authorized session must equal the host's `WorkerBinding.session`, including
its account scope, and `WorkerBinding.sourceId` must be the demo's canonical
plugin ID. A valid grant for another session in the same journal does not permit
media attachment to the host-selected session.

The worker connects only to `127.0.0.1:controlPort`, disables Nagle, sends `Hello`,
requires a matching host `Attached`, and sends `Ready` with the accepted input
capabilities. The wire uses the shared length-prefixed control messages and
56-byte `ONW1` media headers. Stdout contains only media headers and payloads.

The implemented video format is H.264 Annex B, 320×180 at 50 fps, SDR 8-bit
YUV 4:2:0, limited-range BT.709 with left chroma siting. RGB patterns are converted
using the actual BT.709 coefficients; metadata is not relabeled BT.601. The center
patch spans x=40..279 and y=30..149. It alternates blue and red every second.
Audio, when accepted, is a 440 Hz stereo Opus tone at 48000 Hz in 20 ms packets.
An absent accepted audio format produces no audio packets. Other formats fail
instead of silently substituting a different stream.

The video sender ID starts at `u32::MAX + 1000`, and the audio sender ID starts
at `u32::MAX + 2000`. Both advance by one. Video timestamps start at 90000 and
advance by 1800 at 90000 Hz; audio timestamps start at 48000 and advance by 960
at 48000 Hz. Worker-authored SSRCs are 1234 and 5678. `SourceStamp` remains typed,
including optional sender identity and SSRC. Normal builds produce continuous
video, including across explicitly requested keyframes. A periodic IDR every
100 frames bounds the wait for a mid-stream recording start to two seconds
without requiring feedback. Keyframe flags reflect actual IDR output from
OpenH264.

The optional Cargo feature `fault-injection` marks every fiftieth video frame as
discontinuous to exercise recovery. It changes no wire or bootstrap fields and is
disabled by default. Do not enable it when packaging the normal recording demo.

Accepted keyboard, relative/absolute pointer, mouse buttons, wheel, text, and
gamepad events update bounded local fixture state and receive input
acknowledgements. They are not injected into the host operating system. Neutral
clears that input state. Text is assembled as one paste of at most 65536 UTF-8
bytes, from chunks of at most 8192 bytes with an exact paste ID and byte offset.
Each chunk is acknowledged, but text is applied only after the final chunk.
Neutral and Stop discard an incomplete paste and release held input.
Frame progress retains the latest source stamp for each
track and stage, without narrowing IDs. When rumble is explicitly accepted, a
gamepad press echoes a short trigger-strength rumble with the same controller
and incarnation. The control provider chooses which subset to advertise.

Control handling runs independently of the bulk stdout writer. There is no media
queue; one encoded unit at a time can block on stdout. Payloads are checked against
both fixture bounds and accepted limits. Stop receives an acknowledgement and an
Ended event. Shutdown allows 250 ms for media completion; process exit closes a
blocked pipe, so the host must discard any partial final media unit.

The worker rechecks the durable session state every 250 ms even when stdout is
blocked. A terminal session stops media. An attach-token expiry, token rotation,
or control-process restart does not terminate an already authorized active
session. A corrupt or unavailable journal fails closed.

## Verification

```sh
cargo test --manifest-path examples/provider-media-worker/Cargo.toml --all-targets
cargo test --manifest-path examples/provider-media-worker/Cargo.toml --all-targets --features fault-injection
cargo clippy --manifest-path examples/provider-media-worker/Cargo.toml --all-targets --no-deps -- -D warnings
```

Process tests perform real demo pairing, session allocation, receipt acceptance,
and preparation through `DemoProvider`. They use its actual authorization journal,
not a bypass. Tests cover full-width provenance, controls under observed stdout
backpressure, keyframe feedback, forged authorization, restart/expiry/rotation,
and terminal-session shutdown. The same recovery tests check explicit
discontinuities with `fault-injection` enabled. A default-mode regression decodes
121 consecutive frames, verifies every continuity flag, and requests an IDR
mid-GOP without breaking continuity. Another test starts a fresh decoder at the
first periodic IDR after a simulated late recording subscription. Unit tests also decode generated audio and
exercise all typed input variants.

With `ffprobe` installed, also check the encoded SPS metadata:

```sh
cargo test --manifest-path examples/provider-media-worker/Cargo.toml --all-targets -- --include-ignored
```
