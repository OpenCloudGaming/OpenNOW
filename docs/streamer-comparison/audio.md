# Audio

## Overview

Both clients play stereo Opus at 48 kHz through WASAPI. Official GFN keeps a timestamped jitter buffer (`NVST:TimestampAudioBuffer`) and asks the server for RFC 2198 RED at redundancy level 2. OpenNOW advertises `aqos.enableRedundancy:0`, decodes with `fec=false`, and has no audio jitter buffer. Loss is a drop. Underrun is a WASAPI stop and reset, not silence.

Microphone is the other split. Official GFN has an Opus encoder wrapper and mic RED level 3. OpenNOW has no microphone capture or upstream transport path.

Game audio works. It will not conceal gaps the way official GFN does, and the mic button will not reach the session.

## Key concepts

**Bundle audio.** Video is Mjolnir. Audio is standard Opus RTP on the ICE/DTLS bundle. Payload type 111. RED payload type 63. Clock 48 kHz.

**RED.** RFC 2198 redundant encoding. Official GFN this session. `Audio redundancy enabled using RFC2198 (RED), redundancy level is 2`. In-band Opus FEC was disabled. OpenNOW ANNOUNCE sets `enableRedundancy:0` and `redundancyLevel:0`. If a RED packet still arrives, OpenNOW keeps the primary block and drops the rest.

**TimestampAudioBuffer.** Official adaptive jitter buffer. Logged defaults. Initial 80, min 60, max 200. This session sat at threshold 220 after frequent underruns, then flushed on overbuffer.

**Shared WASAPI.** OpenNOW uses `AUDCLNT_SHAREMODE_SHARED` plus `AUTOCONVERTPCM`. Not exclusive. No `IAudioClock`. Render is polled on the same `opennow-windows-media` thread as video.

## How settings reach the stream

CloudMatch hardcodes stereo. `audioMode: 2`, `requestedAudioFormat: 0`, metadata `surroundAudioInfo=2`. Native omits `audioChannelCount`.

There is no user audio bitrate or output-device setting. `maxBitrateMbps` is video.

Microphone settings:

| Setting | Default | Native NVST |
|---|---|---|
| `microphoneMode` | `disabled` | Refused |
| `microphoneDeviceId` | empty | Unused |
| shortcut | Ctrl+Shift+M | Unused |

Official persist. `audioMode: 2`. Share overlay `micmode.mode: "off"`. No user-facing audio quality slider in Mall i18n.

This official session still logged `Number of channels(2) is not valid for surround` because stereo plus `surroundAudioInfo: 2` is a messy pair on both clients.

## How OpenNOW plays audio

```
Opus RTP on the DTLS bundle
  → EncodedMediaFrame codec opus, 48 kHz
  → shared sync_channel(8) with video
  → encoded queue capacity 4, drop-oldest
  → libopus 1.3 decode_float(..., false)
  → mono duplicated to L/R
  → PCM queue capacity 4, drop-oldest
  → WASAPI shared, float32, 48 kHz, stereo
  → 20 ms requested buffer
  → preroll 15 ms, plus 5 ms per underrun, cap 60 ms
```

Timestamps and `contiguous` are ignored at decode. They exist for recording. A gap with `contiguous == false` aborts an in-progress recording.

NVST backpressure drops the audio packet and stays up. The comment says queued audio is worse than a short gap.

Software and SDL fallback. 48 kHz stereo, 480-frame callback, 120 ms ring, silence on underrun. That zero-fill is the opposite of the WASAPI path.

Device unplug. `begin_audio_recovery` clears PCM, retries every 250 ms, and throws incoming PCM away so audio cannot stall video. Default endpoint is polled every 250 ms. There is no output picker.

## How official GFN plays audio

From `geronimo.log`:

- `WASAPIAudioRenderer` 2 channels, 16-bit, 48000 Hz
- `NVST:OpusAudioDecoderWrapper` plus `NVST:RtpAudioPlayer`
- RED level 2 on playback, level 3 on mic
- Opus in-band FEC disabled
- Jitter buffer initial 80, min 60, max 200
- `burstAbsenceDuration = 120`, `thresholdBase = 25`, `maxThreshold = 200`
- First audio packet about 84 ms after start, before first video
- Underruns raise the threshold. Overbuffer flushes the queue
- Teardown this session. 17412 audio stale drops. RED decode fail count 0

Official has a dedicated audio renderer lifecycle. Pause, destroy render client, destroy audio client, destroy render event. OpenNOW tears WASAPI down on the video worker.

## Recovery

**Official.** Underruns increase `TimestampAudioBuffer` depth up to a cap. Overflow flushes. RED can fill a lost primary. This session used RED with zero decode failures and still saw thousands of stale drops.

**OpenNOW.** No PLC. No empty-packet decode. No audio NACK envelope. Encoded and PCM queues drop oldest. WASAPI `padding == 0` stops and resets the client, then waits for a larger preroll. That is a hole, not concealment.

There is no A/V sync. Video uses `PresentationClock`. Audio is decode-and-push.

## Microphone

OpenNOW's NVST ANNOUNCE compatibility profile still writes `rtcMicOnNativeBundle:1` and `clientPorts.mic:0`, but the runtime has no microphone capture, encoder, queue or send path. Persisted microphone settings migrate to disabled.

Official logs `NVST:OpusAudioEncoderWrapper` payload 20 ms, 2 channels, `mVoiceBitrate 16000`, in-band FEC disabled, mic RED 3.

## Where things live

- WASAPI. `native/opennow-streamer/crates/opennow-streamer-platform-windows/src/windows/audio.rs`
- Opus decode. `native/opennow-streamer/crates/opennow-streamer-platform/src/media.rs`
- NVST audio and RED strip. `native/opennow-streamer/crates/opennow-streamer-transport/src/nvst.rs`
- ANNOUNCE audio attrs. `native/opennow-streamer/crates/opennow-streamer-core/src/nvst_rtsp.rs`
- CloudMatch stereo. `native/opennow-core/src/cloudmatch.rs`

## Gotchas

Capability probe requires WASAPI. A machine that can decode D3D11 but cannot open the default render endpoint will fail the hardware backend probe.

Linux hardware sessions set `config.audio = None` and still use the shared Opus decode plus SDL path.

`microphoneDeviceId` is stored and unused on this path.
