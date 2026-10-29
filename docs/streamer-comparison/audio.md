# Audio

## Scope

This comparison covers the supported Qt/native client. The earlier version of
this page described a pre-microphone, standalone WASAPI path and incorrectly
treated an Opus `payloadSize 20` log as a 20 ms frame. The October 13, 2026 static
parity audit establishes a 10 ms microphone frame. No GFN binary or NVIDIA/GFN
connection is used to validate the behavior described here.

## Network buffering

Official Windows logs enable `TimestampAudioBuffer` with an 80 ms initial target,
a 200 ms ceiling, and `underFlowIncreaseVal:20`. They also show a dynamic minimum
lowered from 60 to 45 ms and pruning from 80 to 50 ms. The predicate for reducing
the target is not established by the supplied evidence.

OpenNOW reorders authenticated NVST audio RTP by sender timestamp and sequence
before RED loss recovery and decode. The network target starts at 45 ms and grows
by 20 ms for a new late-arrival burst after playout starts, up to 200 ms. Downward
adaptation is deferred rather than inferred from isolated log values. Starting
at GFN's observed dynamic floor is a deliberate local policy: copying its 80 ms
startup value without its reduction path would leave every clean session at
that higher delay.

The buffer holds at most 80 packets and at most 200 ms of timestamp span. Payloads
are bounded by the existing eight-redundant-block RED receive limit. Duplicates
and stale originals are discarded. An SSRC change clears queued packets and
starts a new clock baseline. Sender timestamps, sequences, SSRCs, payloads, and
arrival metadata are preserved. `audio-jitter` diagnostics report the target,
queue depth, reordering, stale/overflow/invalid drops, source changes,
underflow observations, and playout resyncs once a second.

This introduces an explicit 45–200 ms network delay. The old 120 ms figure is a
PCM queue capacity, not a fixed playout delay; its gentle backlog catch-up target
is about 40 ms. That PCM behavior remains unchanged. Network, decoder, PCM, and
OS endpoint buffering are separate, so 200 ms is not an end-to-end latency cap.
Dev has no scheduled network audio delay. The new clean-network stage adds 45 ms
to the otherwise unchanged pipeline, not 5 ms relative to the PCM catch-up
target. After N distinct underflow bursts, its delay target is
`min(45 + 20*N, 200)` ms: 45/65/85/145/200 ms at N = 0/1/2/5/8. A source change
resets it to 45 ms; clean traffic alone does not decrease a raised target.

## Playback and loss recovery

Game audio is Opus RTP on the native ICE/DTLS bundle. The common GFN profile uses
Opus PT 111, RED PT 63, and a 48 kHz RTP clock. OpenNOW's receive path already
recovers missing frames from RFC 2198 redundant blocks; it does not merely strip
RED down to the primary. Recovery happens after reordering so a late original
within the jitter window is not prematurely counted as a loss.

The shared libopus decoder conceals one preceding packet duration for a reported
gap. The embedded Linux audio owner additionally measures gaps from sender RTP
timestamps and caps concealment at 100 ms. Recovered RED frames are not concealed
again. Encoded/PCM queues stay bounded under backpressure,
and audio backpressure does not terminate the stream. Playback ownership differs
by platform; the normal Qt Windows path uses SDL rather than the standalone
polled WASAPI presenter. This change does not replace platform playback or add
an A/V synchronization controller.

## Microphone

OpenNOW has a default-device SDL capture owner, a separate libopus encoder worker,
and an upstream native-bundle RTP path. Capture remains mono at 48 kHz with 10 ms
frames. The encoder uses 16 kbps, matching the observed Windows voice bitrate.
The Windows session's actual stereo encoder/capture conflicts with its merged
`mic.numChannels:1` config; this is not enough evidence for a universal stereo
policy, so OpenNOW keeps mono.

The uplink sends plain Opus on the bundle's fixed RTC payload type 111 and SSRC 1.
It does not take payload types from the DESCRIBE `m=mic` section: those describe
the legacy RTSP media plane, while Bifrost's RTC SDP hardcodes `a=rtpmap:111` and
`a=rtpmap:63 red/48000/2`, and downlink audio already arrives on that fixed
mapping. Upstream RFC 2198 RED is not sent. Windows logs show the official client
enabling mic RED level 3, but no live OpenNOW session has shown the seat decoding
OpenNOW's RED, and plain PT 111 is the format verified live by #1014 and
OpenNOW-Mac. Re-enable RED only after a live seat confirms it.

PCM, encoded, and transport queues each hold at most five frames with a 100 ms
age limit. Mute closes capture and clears pending audio. The original RTP clock
origin is retained across mute/unmute and capture-device reopening.

On SDL device disconnection, the media host closes the old capture endpoint and
retries the default input every 250 ms for up to one second. It flushes PCM,
encoded packets, and in-flight encoder generations before using the replacement.
Recovery failure disables only the microphone. No device opens while capture is
disabled. This is bounded local recovery, not a claim that GFN uses these retry
timings or that every default-device/format-change notification is handled.

Startup-muted negotiation and the controller/UI capability gates belong to the
session-negotiation change, not this audio-buffer/packetizer change. No runtime
RTSP renegotiation is added here.

## Echo cancellation

GFN's Windows capturer supplies decoded game audio to WebRTC APM. It pairs 5 ms
render frames into 10 ms reference frames and accepts 2/6/8 render channels. The
observed configuration enables AEC and the residual echo detector; NS and AGC are
off. macOS also has hardware-AEC and software reverse-stream branches.

OpenNOW still sends microphone PCM to libopus without AEC. A production AEC needs
a bounded reference queue, actual output/capture delay accounting, and reset
rules for mute, device changes, queue discontinuity, and session restart. No
partially validated processing stage is enabled by this change.

## Owners

- Network reordering: `opennow-streamer-transport/src/nvst_audio_jitter.rs`.
- NVST audio receive/RED recovery: `opennow-streamer-transport/src/nvst.rs`.
- Microphone queue: `opennow-streamer-transport/src/nvst_microphone.rs`.
- Capture, encoder, and device recovery: `opennow-streamer-platform/src/microphone.rs`.
- Decode/PLC: `opennow-streamer-platform/src/media.rs` and platform audio owners.
- PCM backlog handling: `opennow-streamer-platform/src/audio_playout.rs`.
- SDP/handoff producers: `opennow-streamer-core/src/nvst_rtsp.rs`.
