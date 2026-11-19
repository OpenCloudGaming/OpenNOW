# GFN Windows client: audio playback and microphone

Evidence tags as in architecture.md: **S** static, **D** decompiled, **L** log-confirmed, **I** inferred, **U** unknown. `Bifrost2.dll` is now available. The Opus/RED and jitter observations below remain log-confirmed unless an individual Windows function is cited; they are not upgraded just because the DLL arrived.

## Summary

- Speaker audio arrives as RTP on the WebRTC native bundle with RFC 2198 RED (level 2) and 5 ms Opus frames. Bifrost decodes it and runs a timestamp-based adaptive jitter buffer. It hands 16-bit 48 kHz PCM to Geronimo's `WASAPIAudioRenderer`. **L**
- `WASAPIAudioRenderer` uses shared mode, is event-driven, asks for `AUDCLNT_STREAMFLAGS_RATEADJUST`, coerces the mix format to 16-bit PCM, disables ducking, and has its own channel mixer and resampler. It rebuilds on default-device or format change. **D/L**
- The client supports 2, 6, and 8 channels end to end (bitrate tables, AEC reverse-stream guard, channel mixer). This session negotiated stereo because the default device was stereo. **D/L**
- The microphone path uses WebRTC's Windows Core Audio device module and APM inside `GsAudioWebRTC.dll`. Capture is 48 kHz stereo 16-bit in 10 ms chunks with the echo canceller on and NS/AGC/HPF off. The AEC reference is the actual decoded game audio. Bifrost encodes the mic as Opus (20 ms, 2 ch, 16 kbps) with RED level 3 and sends it on the WebRTC bundle. **D/L**
- OpenNOW's Qt embedded path has bounded SDL2 playback and backlog catch-up resampling, but not this timestamp-adaptive depth. It decodes mono or stereo to stereo output and captures mono mic without AEC or upstream RED. Downstream RED decoding already exists. See [opennow-gaps.md](opennow-gaps.md).

## 1. Playback

### 1.1 Who creates what

- **S** `platformCreateAudioRenderer` (`0x18015bb50`) returns the WASAPI renderer. `SDLAudio` also exists as an `AudioRenderer` subclass (`??_7SDLAudio@@6BAudioRenderer@@@`, `SDLAudio::renderAudio`), and Geronimo imports `SDL_OpenAudioDevice`, `SDL_GetDefaultAudioInfo`. **U** No SDL audio log lines appear in either `geronimo.log`, so the SDL renderer was not used in these sessions.
- **L** `debug.log` 19:45:11.620 `simple_grid_app.cc(183)] Creating audio renderer.` then `Initializing audio renderer.`, before `QUERY_GFN_START`. Audio resumes in `onStreamingBegin` (19:45:30.830 `onStreamingBegin: AudioRenderer resumed`).
- **L** `geronimo.log.bak` 19:45:29 `[BifrostClient: SessionController] Channel count in audio renderer = 2` and `[GridApp] Channel count supported 2` (19:45:11). The renderer's channel count is reported to Bifrost through `nvbConfigureAudioChannelCount` (**S** import; error string `nvbConfigureAudioChannelCount Error %s (%d)`).

### 1.2 WASAPI initialization — **D** `WASAPIAudioRenderer::initWin32CoreAudio` `0x180255eb0`

1. `CoInitialize`, then `CoCreateInstance(MMDeviceEnumerator)`, then `GetDefaultAudioEndpoint` (the slot-0x20 call). The renderer always uses the default render endpoint; there is no device-ID selection.
2. Ducking is turned off. It gets the session manager through `IMMDevice::Activate(IAudioSessionManager)`, then `GetAudioSessionControl`, `QueryInterface(IAudioSessionControl2)`, and calls slot `0x80` with `TRUE`, which is `SetDuckingPreference(TRUE)`. **L** `Successfully disabled ducking`.
3. It activates `IAudioClient` and calls `GetMixFormat`. It coerces the format to 16-bit integer PCM: `WAVE_FORMAT_IEEE_FLOAT` (tag 3) is rewritten to tag 1 with 16 bits, and `WAVE_FORMAT_EXTENSIBLE` with the float subformat GUID is rewritten to the PCM subformat with 16 valid bits. Other tags log `Cannot coerce WAVEFORMATEX with wFormatTag = 0x%08x to int-16`.
4. `IsFormatSupported(SHARED, ...)`. If it returns `S_FALSE` with a closest match, the closest match replaces the format. For a 4-channel extensible format it records the channel mask (`Channel Mask = %d`).
5. `IAudioClient::Initialize(AUDCLNT_SHAREMODE_SHARED, 0x140000, 0, ...)`, where `0x140000` is `EVENTCALLBACK | RATEADJUST` and the buffer duration is 0 (engine default). On failure it logs `IAudioClient Initialize with RATEADJUST failed (0x%x), retrying without rate adjust` and retries with `0x40000` (`EVENTCALLBACK` only).
6. `CreateEventA` and `SetEventHandle`. The render thread is event-driven.
7. If rate adjust is active it uses `IAudioClockAdjustment::SetSampleRate` for clock-drift correction. If that fails it logs `SetSampleRate failed, hr: %x, using WebRtC resampler` (string at decompiled line ~970 of the function).

**L** `geronimo.log` (every session): `Initializing render client with, nChannels: 2, bitsPerSample: 16, samplesPerSec: 48000`, then `Audio core initialized successfully`. **S** Other diagnostics: `Recommended sample frame count is %d (=%f ms), allocating %u for audio channel count %d`, `Default audio device name: %s, channel count: %u, sample rate: %u, samples: %u, audio buffer size: %u`.

**S** The `FullScreenExclusive` string is a display-mode string (`Unable to set fullscreen exclusive mode`), not audio. No `AUDCLNT_SHAREMODE_EXCLUSIVE` path was found in this function. **I** Audio exclusive mode is not used.

### 1.3 Per-frame rendering — **D** `WASAPIAudioRenderer::renderAudio` `0x180259dd0`

- Bifrost calls it with an `NvstAudioFrame_t`. The renderer requires 16 bits per sample and 48000 Hz. Anything else raises `GERONIMO_ERROR 0xc0100001 GERONIMO_WASAPIAUDIO_RENDER_AUDIO_PARAMS: Incompatible audio stream.`
- If the core-audio state is flagged dirty it reinitializes (`Reinitializing core audio` / `Failed to initialize core audio`).
- A volume ramp is applied: a target gain is interpolated linearly over a nanosecond time window (fields at `+0x140`..`+0x168`). This backs `setVolume(double,double)` and `QUERY_GFN_SET_STREAM_VOLUME`.
- When the incoming channel count or rate differs from the device format it builds a channel mixer and resampler (`Initializing channel mixer and resampler`; on failure `Failed to initialize channel mixer and resampler`, `Failed to initialize upmixer as output channel count is not supported`). The source is upmixed or downmixed to the endpoint layout.
- **S** `Audio channel count changed from %d to %d` and `Streaming channel count is %d , incoming channel count is %d` show it handles mid-stream channel-count changes (`audio.enableDynamicAudioConfig: 1`, **L**).
- **I** The rendered frame is also fed to the mic AEC as a reverse stream (`WebRTCAudioCapturer::processReverseStream`, section 2.4). This is how the render-side callback is registered: **L** `Registering audiocapture callback for echo cancellation at render end`.

### 1.4 Device and format changes

- **S** `WASAPIAudioRenderer` inherits `DefaultAudioDeviceChangeHandler` and `StreamFormatChangeHandler` (vtables `0x1814e6a28`, `0x1814e6a38`). **D** `onDefaultDeviceChanged` (`0x1802597e0`) logs `Audio renderer default device changed` and schedules a rebuild. `onStreamFormatChanged` (`0x180259820`) does the same for format changes. `overrideDeviceChangeEvent`/`overrideFormatChangeEvent` exist for testing.
- **L** `[DefaultAudioDeviceHelper] Callback not registered` appears 4 times at shutdown. **I** It is a harmless unregister after the renderer is already gone.
- **L** `ERROR [WASAPIAudioRenderer] Stream not paused before resuming` appears once. **I** It is a benign state-machine warning on resume.

### 1.5 Bifrost receive path (from Bifrost log lines, Windows evidence; addresses unknown)

| Fact | Evidence |
| --- | --- |
| Audio is received on the WebRTC native bundle, with RTP from `WebRtcAudioRtpReceiver` | **L** SDP `a=x-nv-general.rtcAudioOnNativeBundle:1`; `[NVST:RtpAudioPlayer] WebRtcAudioRtpReceiver is the audio packet source`; `[NVST:SignalingHandler] [webrtc-create] SignalingHandler creates audio receiver: streamName=Audio` |
| Video is **not** on the native bundle in this session, but audio, mic, and data channel are | **L** `general.rtcVideoOnNativeBundle: 0`, `rtcAudioOnNativeBundle: 1`, `rtcMicOnNativeBundle: 1`, `rtcDataChannelOnNativeBundle: 1` |
| Downstream frame duration is 5 ms | **L** `[NVST:RtpAudioPlayer] Setting audio frame duration: 5` |
| Downstream RED is RFC 2198, level 2 | **L** `a=x-nv-aqos.enableRedundancy:1`, `a=x-nv-aqos.redundancyLevel:2`; `[NVST:ClientSession] Audio redundancy enabled using RFC2198 (RED)`; `Audio redundancy enabled using RFC2198 (RED), redundancy level is 2`; `[NVST:WebRtcAudioRtpReceiver] Audio RED decode fail count: 0` |
| Codec is Opus, not in "Opus mapping mode" | **L** `m_bInOpusMappingMode FALSE`. **U** The 6/8-channel Opus decoder topology has not been traced or exercised by these stereo sessions. |
| SRTP for audio is off | **L** `a=x-nv-runtime.audioSrtp:0` |
| 48 kHz, stereo negotiated this session; surround disabled | **L** `audio.samplesPerSecond: 48000`, `audio.surround.enable: 0`, `audio.surround.numChannels: 2`, `audio.surround.channelMask: 3`, `audio.surround.version: 2` |
| Surround bitrates (kbps) | **L** `audioBitrate.remote2chMin/Max 96/128`, `remote6chMin/Max 256/512`, `remote8chMin/Max 450/768`, `local2ch 96/510`, `local6ch 256/1284`, `local8ch 450/1794`; adaptive bitrate off |
| Timestamp jitter buffer on | **L** `audio.enableTimestampAudioBuffer: 1`, `audio.enableNativeBufferImplementation: 0` |
| Jitter-buffer config | **L** `audio.jbConfig: initialThreshold 80, thresholdBase 25, maxThreshold 200, underFlowIncreaseVal 20, burstAbsenceDuration 120`. `TsJbConf` defaults (`tsjb.conf` absent): `JITTER_BUFFER_INITIAL_THRESHOLD 80`, `MIN 60`, `MAX 200`, `UNDERFLOW_THRESHOLD 40`, `OVERFLOW_THRESHOLD 220`, `ALPHA_JITTER 0.9`, `BURST_FACTOR 1.25`, `MIN_ADAPTATION_TIMEGAP 30000`, `INTOLERABLE_SILENCE_PLAYED 5` |
| Runtime adaptation | **L** 19:45:36 `Setting JITTER_BUFFER_MIN_THRESHOLD: 60 -> 45`, `Setting underflow threshold: 40 -> 10`, `Setting new threshold: 50`, `Pruning: 80 -> 50`; 20:06:44 `Pruning: was 105 vs 100 (ts: ... )`; teardown `Packets dropped at input due to Stale 0, Overflow 0, Maxsize 0` |
| Audio stats cadence | **L** `aqos.enableAudioStats: 1`, `aqos.audioStatsUpdateIntervalMs: 70` |
| Session request | **L** `debug.log` 19:45:11.637 `AudioMode used for Start stream session: 2`; device capability JSON includes `"audioChannelCount" : 2` (`[ModeSelectionUtil] Device Capability is = {...}`) |

**I** The "jitter buffer" is in milliseconds of audio. It starts at 80 ms, adapts between a dynamic floor (lowered to 45 here) and 200 ms, and prunes when it runs above target. OpenNOW's 120 ms value is maximum PCM capacity, not a fixed delay. Its embedded output consumes immediately and applies up to 2% catch-up when backlog exceeds 60 ms for 200 ms, stopping at 40 ms (`audio_playout.rs:63-98`, frozen `7ce3cd54`). That backlog policy is not a timestamp-adaptive jitter target.

## 2. Microphone

### 2.1 Ownership and bring-up

- **L** The capturer is created only after streaming begins and only if permission is allowed: `debug.log` 19:45:30.834 `Register mic callback`, `Initialize audio capturer as permission is allowed.`, `Creating audio capturer.`, `Creating MicStateHandler`, `Calling registerMicStateHandler`.
- **D** `GeForceNOW.exe` `FUN_14017f5f0` (SimpleGridApp) logs `Initialize the audio capturer` and returns early with `Could not initialize the mic as the user mic preference is off. Will be initialized when mic is toggled.` when the preference byte is clear. The JS controls this through `QUERY_GFN_SET_MIC_STATE`, `QUERY_GFN_GET_MIC_STATE`, `QUERY_SHOULD_DEFAULT_ENABLE_MIC`, `QUERY_GFN_REGISTER_MIC_CALLBACK`, `QUERY_GFN_AUTH_AV_DEVICES`.
- **L** Mic state events to the UI: `Sending Mic State Event: 2` and `Mic state change detected. New state: 3` (12 to 18 occurrences each in `debug.log`). **D** `WebRTCAudioCapturer::sendMicState` `0x1802c33c0` logs `Updating mic state : %d` from a `DEVICE_ERROR` code. **U** The full numeric `MicState` enum was not mapped. 2 and 3 are the two values seen.
- **L** Bifrost: `[NVST:ConfigHelper] Application requested for 1 video streams, 1 audio streams, 1 input streams, 1 mic streams`; `[BifrostClient: Streamer] Created mic stream <ptr>`.

### 2.2 `GsAudioWebRTC.dll` — **D**

- `gsAudioWebRTC_OpenAudioDevice` (`0x180001240`) calls `FUN_180002ce0(mode 0)`. Mode 0 builds a `webrtc::AudioDeviceModule` (WebRTC's ADM) and calls its `Init`. Mode 1 builds only the APM (`FUN_180003320`), and other modes log `Unsupported gsAudioWebRTC mode`. **L** `Windows Core Audio APIs will be utilized`, `AudioDeviceWindowsCore created`, `current platform is Win32`.
- `gsAudioWebRTC_InitVoiceCapture(handle, deviceIndex, callback, enableAEC)` (`0x180001230`):
  - It validates the index against `RecordingDevices()` (`invalid device id:%d recording devices:%d`) and logs `enableAEC: %d`.
  - If hardware AEC is flagged it logs `Hardware Aec is enabled` and skips software APM. Otherwise, when AEC is requested, it builds the APM.
  - It registers the audio callback and calls `SetRecordingDevice(0xfffffffe)` through ADM vtable slot `0x70`, then `InitRecording` (slot `0xa0`).
  - **I** `-2` is WebRTC's `WindowsDeviceType::kDefaultDevice`, the console default capture endpoint, not the communications endpoint. So the validated `deviceIndex` is effectively ignored, as on macOS, where the analysis found a literal `0` selector instead.
  - **L** `Initializing voice capture on micindex 0, echoCancellationEnabled 1`; `RecordingDevices ... output: 2`; `Audio started successfully on default device (could not obtain device name)`.
- `gsAudioWebRTC_StartVoiceCapture`/`StopVoiceCapture` (`0x1800013a0`/`0x1800013b0`) check `RecordingIsInitialized`, then call `StartRecording`/`StopRecording`. Error codes: `-4` uninitialized device, `-8` recording not initialized, `-9` start failed, `-10` stop failed.
- `gsAudioWebRTC_ProcessReverseStream` and `gsAudioWebRTC_ProcessStream` reject frames whose duration field (`frame[6]`) is not 10 ms (`Framelength mismatch expected %d found %d`). This is the same 10-unit check the macOS helper has.
- APM configuration — **L** (more precise than the packed struct in `FUN_180003320`): `AudioProcessing::Config{ pipeline: {maximum_internal_processing_rate: 48000, multi_channel_render: 0, multi_channel_capture: 0}, pre_amplifier: {enabled: 0}, high_pass_filter: {enabled: 0}, echo_canceller: {enabled: 1, mobile_mode: 0, enforce_high_pass_filtering: 1}, noise_suppression: {enabled: 0, level: Moderate}, transient_suppression: {enabled: 0}, voice_detection: {enabled: 0}, gain_controller1: {enabled: 0, ...}, gain_controller2: {enabled: 0, ...}, residual_echo_detector: {enabled: 1}`. So only the echo canceller (with residual echo detector) is active. Noise suppression and AGC are off. This resolves a macOS unknown ("no claim about NS/AGC") for Windows.
- **L** Capture format: `SetRecordingSampleRate(48000)`, `SetRecordingChannels(2)`, `Size of recording buffer: 960` (samples, i.e. 480 frames × 2 ch = 10 ms). Periodic stats: `[REC : 10001msec, 48kHz] callbacks: 1000, samples: 480000, rate: 47995`. The recorder thread runs at 100 callbacks per second.

### 2.3 Geronimo capture queue — **D**

- `WebRTCAudioCapturer::gsAudioWebRTC_RecordedDataIsAvailableCb` (`0x1802c11d0`) copies the helper's bytes into a ring buffer. It signals the reader event once at least `0x780` = 1920 bytes are buffered. That is 10 ms of 48 kHz stereo s16. When the ring is full it logs `Voice data packets dropped`. A null buffer logs `Native WebRTC data buffer is NULL`.
- `WebRTCAudioCapturer::readVoiceData` (`0x1802c2af0`) waits on the event while fewer than 1920 bytes are buffered. It then copies at most the available bytes from the ring (handling wrap-around) under a critical section and advances the read index modulo the ring size.
- **L** On shutdown the blocked reader is released with nothing to read: `Voice data size not equal to requested size, requestedSize: 1920, readBytes: 0` (20:15:24, during `MicAudiocapturer shutdown`). The macOS research asked how partial reads are handled. On Windows a short read is logged and returned as is.
- **S/L** A separate `PushMicFrameThread` (RTTI) pushes frames to `GridApp::sendMicAudioFrame` (`0x1800ccdd0`), then `BifrostSDKExecutor::sendMicAudioFrame` (`0x1800d2b50`), then `nvbSendMicAudioFrame`. **L** At teardown the last frames fail with `nvbSendMicFrame Error NVB_R_STREAMER_UNINITIALIZED (341)` (20:15:24.279, 3 times overall). This is benign ordering: the streamer stops before the mic thread does.
- `reinitializeCaptureDevice` (`0x1802c2c60`) starts a new thread if needed (`Starting new thread`), then `Reinitializing capture device`. It is triggered by `onDefaultDeviceChanged` (`0x1802c25f0`) and `onStreamFormatChanged` (`0x1802c2650`). The capturer registers both notifiers. **L** at shutdown: `Device change notifier is unregistered`, `Stream format change notifier is unregistered`.

### 2.4 AEC reference (reverse stream) — **D** `WebRTCAudioCapturer::processReverseStream` `0x1802c2710`

- It requires voice capture to be initialized and AEC enabled. Otherwise it logs `AudioCapture not initialized or AEC is disabled VoiceCapInitDone:%d, AEC: %d`.
- It accepts render frames with 2, 6, or 8 channels. The guard is `(ch-2) & ~6 == 0 && ch != 4`. Anything else logs `Invalid reverse stream channel count: %u`.
- Render frames are 5 ms (section 1.5) but the APM needs 10 ms. The function copies each 5 ms frame into one half of a `channels × 960`-byte buffer, toggles a half flag, and calls `gsAudioWebRTC_ProcessReverseStream` on every second frame. A failure raises `GERONIMO_ERROR 0xc00f0003 GERONIMO_WEBRTCAUDIO_PROCESSREVERSESTREAM_FAILED`.
- **I** The AEC reference is the decoded game audio after Bifrost, not the post-mix WASAPI output. Volume changes and endpoint mixing are therefore not reflected in the reference.

### 2.5 Encode and transport (Bifrost log lines)

| Fact | Evidence |
| --- | --- |
| Mic stream setup bypasses Mjolnir and uses the WebRTC transport | **L** `[NVST:SignalingHandler] Bypassing 'Mjolnir' SETUP for the Mic stream. Deferring to WebRtcTransport setup`; `[NVST:RtpAudioPlayer] WebRtcTransport will employed for upstream audio transport.`; SDP `a=x-nv-general.rtcMicOnNativeBundle:1` |
| Opus encoder: 20 ms payload, 2 channels, 16 kbps | **L** `Opus encoder payloadSize 20 , channels 2, m_param.mVoiceBitrate 16000`; `Opus encoder payloadSize 20, m_micTimeStamp 0, micFrameLength 1920` |
| Mic RED is RFC 2198, level 3 | **L** `a=x-nv-aqos.enableRedundancyForMic:1`, `a=x-nv-aqos.redundancyLevelForMic:3`; `Mic redundancy enabled using RFC2198 (RED), redundancy level is 3`; `[NVST:WebRtcAudioRtpSender] Enabling audio redundancy` |
| Mic SRTP off | **L** `a=x-nv-runtime.micSrtp:0` |
| Shared encode wrapper | **L** `Encode Wrapper unification utilized`, `Created Audio Encoder` |

**I** Geronimo hands Bifrost 10 ms (1920-byte) chunks and Bifrost packs two chunks into each 20 ms Opus packet. **U** The macOS note `Microphone stream did not set the supportedFrameSize` hints that the frame size is negotiated. The server's mic `frameSize` value was not found in these Windows logs (OpenNOW cites `mic.frameSize:10`).

## 3. Comparison with macOS (PR #1161)

- Same helper API and the same 10 ms frame-duration guard. Windows uses `AudioDeviceWindowsCore`; macOS uses `AudioDeviceMac`. Both ignore the caller's device index (Windows passes `-2` default device; macOS passes literal `0`).
- macOS left NS/AGC unknown. On Windows the APM config log shows AEC on, NS off, AGC1/AGC2 off, HPF off.
- macOS asked how `processReverseStream` slices frames. On Windows: 5 ms render frames are paired into 10 ms reverse frames, and 2/6/8 channels are allowed.
- macOS has `AudioUnitsAudioRenderer` with an `AudioRingBuffer`. Windows uses WASAPI event mode with rate adjust and no separate Geronimo ring buffer for playback (Bifrost's TimestampAudioBuffer does the buffering).

## 4. Comparison with OpenNOW (frozen dev `7ce3cd54`)

| Area | GFN Windows | OpenNOW |
| --- | --- | --- |
| Playback API (Qt path) | WASAPI shared, event-driven, `RATEADJUST` + `IAudioClockAdjustment`, ducking off | SDL2 audio callback, f32, 10 ms buffer (`crates/opennow-streamer-platform/src/output.rs:38-41,1529-1537`). The custom WASAPI renderer is standalone-only, polled, with no event handle (`opennow-streamer-platform-windows/src/windows/audio.rs:110-148`) |
| Jitter buffer | Bifrost TimestampAudioBuffer, 80 ms initial, adaptive 45-200 ms, pruning | Bounded encoded and PCM queues, 120 ms maximum PCM capacity, immediate consumption and ≤2% backlog catch-up. No timestamp-adaptive depth on the embedded path (`audio_playout.rs:1-98`, `media.rs:32`) |
| Frame size | 5 ms downstream | Assumes up to 960 samples/ch (20 ms) for PLC bounds (`media.rs:2966-2978`) |
| RED | Level 2 down, level 3 up | Parses RED down (`nvst.rs:6253-6287`); no mic RED |
| Channels | 2/6/8 supported, negotiated from device | Mono/stereo decode to stereo output; no multichannel playback (`media.rs:3073-3081,3115-3135`); `channels:2` (`nvst_rtsp.rs:1262`) |
| Device change | IMMNotificationClient-style handlers rebuild renderer and capturer | No application-controlled migration in the embedded SDL owner; SDL backend behavior unverified. Separate WASAPI owner polls the default endpoint every 250 ms |
| Mic capture | WebRTC ADM (Core Audio), 48 kHz stereo, 10 ms | SDL2, 48 kHz mono, 10 ms (`microphone.rs:303-353`) |
| Mic processing | WebRTC APM AEC with the game audio as reference | None |
| Mic encode | Opus 20 ms, 2 ch, 16 kbps, RED 3 | Opus VoIP 10 ms, mono, 32 kbps, plain PT 111 (`microphone.rs:251-273`, `nvst.rs:7235-7251`) |
| Mic state to UI | Mic state events (2/3) plus permission gating | Local `microphone-state` events only |
