# Audio playback and microphone findings

The helper analysis decompiled ten substantive functions in `archive/libGsAudioWebRTC.dylib`, x86_64. Geronimo and Bifrost references in this document are static symbols and strings unless stated otherwise. Separate Bifrost microphone/RTP-sender findings appear in [transport-feedback.md](transport-feedback.md). No target was executed and no GFN connection was made. Runtime behavior remains unverified on this Linux host.

The helper implements local capture and audio processing. Its reverse-stream function supplies audio to an echo-cancellation processor, not to a speaker API. Geronimo has separate Audio Units renderer symbols. These findings do not establish the session's media transport.

## Evidence and address convention

See the sibling [artifact inventory](artifacts.md) and [method and evidence retention](method.md) for source identity and reproducibility.

`D` means a decompiled branch/call or decoded instruction. `S` means a symbol, string, or static pointer present in the artifact. `I` means analyst inference. `U` means unresolved. All function and data addresses below are thin Mach-O virtual addresses, not offsets into the original fat file. The three x86_64 slices begin at fat-container offset `16384`.

| Artifact | x86_64 slice SHA-256 |
| --- | --- |
| `archive/libGsAudioWebRTC.dylib` | `53858ed4421f6299a998e1957fbf33a07c14dd16e72f860f63a9919b81cbe36e` |
| `archive/libGeronimo.dylib` | `f564aa1910f1a366973ce3c0f0e6261122833bc41c530a669ec846a51d50d15a` |
| `archive/libBifrost2.dylib` | `e145b333045dd66652ffcc6e75a2d301bff54c74064b1193e57afe9d498971e9` |

The following aliases refer to retained complete REA records.

| Alias | REA evidence ID | Coverage |
| --- | --- | --- |
| A1 | `ev_69ca52aa499c84fdba102119e4c3844d7d50470fd4b2ed8491b8597e58a7fa89` | `0x5180`, `0x5c50`, `0x7180`, `0x76e0`, `0x7c50` |
| A2 | `ev_ebf71fc365f54dfa5eb54d70693760300998f4dfb2ae7bf035b7c2456da2f3df` | `0x14c90`, `0x19870`, `0x1a280`, `0x195b0`, `0x16060` |
| A3 | `ev_0e78e40946d37b1992a44279a1669bbbf57e8cadb41724eb1f89972b662a369e` | Incoming references to `0x7c50` |
| A4 | `ev_6ca9c5fff2536d003c88099c26ce832df7951d36d72dae46e3475de8db208841` | Incoming references to `0x7180` |
| A5 | `ev_11ce7e2a6eaa85dbd8c71943ce2459a03f4f82bd5e4869cf6fd70f0d3df566dd` | Incoming references to `0x195b0` |

Static observations have no REA evidence ID. Their retained entries include addresses and file offsets from the supplied inventory. Full REA evidence was exported before the isolated helper session was closed.

## Ten function findings

Unless another library is named, every function in this section belongs to `archive/libGsAudioWebRTC.dylib`, x86_64.

### 1. Opening creates a shared helper and a platform device module

**D, A1, `gsAudioWebRTC_OpenAudioDevice`, `0x5180`.** The function initializes a guarded static `mywebrtc::WebrtcInit(mode)::mwr` object, installs logging sinks, and allocates the device module and its audio buffer. The initial mode byte is zero. The branch for audio-layer value zero constructs `AudioDeviceMac`; a separate value-ten branch constructs `AudioDeviceDummy`. The latter is a mode branch, not a proven automatic fallback after a Mac-device failure.

The module's initialization call must return zero before the helper marks the device initialized, sets `mywebrtc::m_bValid`, and returns the static object's address. A nonzero result logs `AudioDeviceModule init failed with %d platform %d` and returns null. The returned object is shared static state, so this is not evidence of independent capture instances per open.

**U.** Repeated-open safety, the intended mode setter, hardware-AEC capability discovery, and destructor ordering were not traced. This pass did not decompile `gsAudioWebRTC_CloseAudioDevice` at `0x7060`.

### 2. Capture initialization distinguishes hardware AEC from software processing

**D, A1, `gsAudioWebRTC_InitVoiceCapture`, `0x5c50`.** Null userdata returns low 32-bit `0xffffffff`. An uninitialized-device flag returns `0xfffffffc`. The supplied device index is compared against `RecordingDevices()`; an index at or above the signed device count returns `0xfffffffe`. This comparison alone does not reject a negative index.

If the hardware-AEC flag at object offset `0x7934` is set, the function calls `EnableBuiltInAEC(true)`. A nonzero result clears that flag. Software `AudioProcessingImpl` construction and `ApplyConfig` occur when the requested AEC byte is nonzero and the hardware flag is clear. Hardware enablement is attempted based on the hardware flag, even before that requested-AEC condition. The packed software configuration was not reconstructed into every named WebRTC setting.

The function saves the callback at offset `0x7938`, registers the helper as the audio callback, then calls `SetRecordingDevice(0)` and `InitRecording()`. The latter two return values are not checked in this wrapper. Despite validating the caller's index, the final selector argument is the literal zero. This observation applies to this supplied helper, not to all GFN versions or platforms.

**S supporting the indirect targets.** The module vtable address point is `0xb8830`. Slots `+0x50`, `+0x1d8`, `+0x28`, `+0x78`, and `+0xa8` contain `RecordingDevices` at `0x10e20`, `EnableBuiltInAEC` at `0x130f0`, `RegisterAudioCallback` at `0x10b30`, `SetRecordingDevice` at `0x11160`, and `InitRecording` at `0x114b0`. The processing vtable address point `0xb92a8`, slot `+0x38`, contains `ApplyConfig` at `0x231d0`.

**U.** The hardware flag's producer, current Mac reachability of the hardware branch, and the caller's device-selection policy need further tracing. Geronimo's `initVoiceCapture` is the best next query.

### 3. Reverse-stream processing is an AEC input, not speaker playout

**D, A1, `gsAudioWebRTC_ProcessReverseStream`, `0x7180`.** Null userdata or frame pointers return `0xfffffffe`. The integer at frame offset `24` must equal ten, otherwise the same status is returned. Under the helper mutex, an invalid helper returns `0xfffffff3`; an active hardware-AEC flag returns success without software processing. An unset software-processing flag returns `0xfffffffd`.

The code computes frames per channel as the integer at offset `28` divided by the product of the integer at offset `8` and the integer at offset `4` shifted right by three. It reads the data pointer at offset `16`, copies two bytes per sample into internal storage, and enforces a maximum of `0x1e00` total samples with `FatalLog`, not a recoverable return. A mono branch duplicates 16-bit values into two channels when its internal size condition permits. It constructs a stream configuration with sample count `rate / 100` and calls the short-sample `AudioProcessingImpl::ProcessReverseStream` at `0x24f10` through slot `+0x98` of vtable address point `0xb92a8`.

The wrapper returns `0xfffffffa` after a nonzero processing result. Error logging uses counters, allowing the first fifty actions and later periodic actions after more than fifty accumulated errors. The function does not call a speaker or playout API in this recovered body.

**I.** Frame offsets `0`, `4`, `8`, `16`, `24`, and `28` respectively appear to mean sample rate, bits per sample, channels, data pointer, duration in milliseconds, and byte count. The arithmetic and the `AudioFrame_t*` symbol support that interpretation, but the producer's struct definition and validation remain unknown. Do not treat this inferred helper struct as `NvstAudioFrame_t`.

**D.** The helper's internal reverse-stream timestamp advances by ten. There is no traced connection between that counter and sender-authored media timestamps. **U.** Zero divisors, invalid bit depths, negative byte counts, and input channel limits are not safely rejected by the visible wrapper. Caller-side guards were not inspected.

### 4. The direct capture-processing export also operates on internal storage

**D, A1, `gsAudioWebRTC_ProcessStream`, `0x76e0`.** This export repeats the pointer and ten-unit frame checks, requires the software-processing flag, computes the sample count from frame byte count, bit depth, and channels, and copies two bytes per sample into internal storage. It uses the same `0x1e00` fatal sample limit.

It sets stream delay to zero, sets analog level to `0x80`, and calls the short-sample `ProcessStream` at `0x24b00`, resolved through vtable slot `+0x88`. Processing succeeds with status zero and fails with low 32-bit `0xfffffff9`. Its timestamp advances by ten. The recovered body contains no copy of processed data back to the original frame pointer and no capture callback invocation.

**U.** Whether any application caller uses this export is unresolved. Geronimo's supplied static imports list `ProcessReverseStream`, but no `ProcessStream` import appeared in the inspected audio-symbol subset. That bounded static observation is not proof that dynamic lookup is absent.

### 5. Device capture callbacks normalize channels and optionally resample

**D, A1, `mywebrtc::RecordedDataIsAvailable`, `0x7c50`.** A stopped-capture flag causes an early success return without forwarding. After locking, an invalid helper also returns without invoking the client callback. The body copies 16-bit samples, branches on channel values one, two, and four, rejects other values with `0xffffffff`, duplicates mono samples, and includes a four-channel reduction branch with pairwise arithmetic. The intended normalized path uses two channels.

When the sample-rate argument differs from `48000`, a resampling path uses scratch storage at helper offset `0x7940`, a `48000 / inputRate` ratio, and persistent `Resample(...)::info` state. The scalar and SIMD arithmetic is substantial. Exact rounding, continuity across restart, clipping, and the four-channel channel-order mapping are not established here.

If the processing pointer at `0x7928` is present, the callback calls `set_stream_delay_ms` and short-sample `ProcessStream`. A nonzero processing result logs `ProcessStream %d`, returns `0xffffffff`, and skips client delivery. Otherwise it invokes the callback object at `0x7938`, passing processed sample storage and a byte length computed as `channels * frames * 2`. The callback occurs before the helper mutex is unlocked. Client callback latency therefore matters to this critical section.

**U and decompiler warning.** The generated signature has imperfect ABI typing, including `CONCAT44(in_register_00000084,param_5)`, overlapping globals, and two removed unreachable blocks. Do not derive exact callback parameter positions for delay, drift, level, or key-press flags from that signature. The outer callback target remains an unresolved indirect call. Its Geronimo candidate is listed below.

**D/S, A3.** Incoming references include the `mywebrtc` vtable data pointer at `0xb85c0`, not a direct code caller. Computed dispatch does not appear as a fully resolved call chain in this reference record. Registration plus vtable presence supports the callback relationship, but its complete delivery path was not decompiled.

### 6. CoreAudio capture converts native device format to fixed-rate short samples

**D, A2, `webrtc::AudioDeviceMac::InitRecording`, `0x14c90`.** The function locks device state, rejects unsuitable state, and returns success immediately if recording is already initialized. It obtains the input `AudioStreamBasicDescription` through `AudioObjectGetPropertyData`, requires format ID `0x6c70636d`, rejects channel counts above sixty-four, and rejects a computed input block size larger than the stored ring-buffer capacity at device offset `0x1970`.

Output channels become two only when the input has at least two channels and the requested recording-channel byte equals two. Otherwise output is mono. The audio buffer receives sample rate `48000` and the selected channel count. The desired converter format contains sample rate `48000`, sixteen bits per channel, one frame per packet, and two bytes per sample per channel. `AudioConverterNew` performs the native-to-desired conversion.

The function gets and sets device buffer size, obtains latency and stream properties, registers property listeners, and creates either a shared device IOProc or a separate input IOProc according to device state. It marks recording initialized only after the setup path reaches completion. API failures take logged failure returns. Failure-path cleanup of every earlier allocation/listener was not proven.

**U.** The decompiler reports `Type propagation algorithm not settling`. Do not use its recovered temporary types as authoritative CoreAudio declarations. The actual ring-buffer allocation size, preferred device selection, and initiation of the capture thread were not covered by this function.

### 7. The capture worker delivers complete 480-frame chunks

**D, A2, `webrtc::AudioDeviceMac::CaptureWorkerThread`, `0x19870`.** The worker requests `0x1e0`, or 480, frames from `AudioConverterFillComplexBuffer`, using `inConverterProc` at `0x19ac0`. It delivers only when the converter returns zero and produces exactly 480 frames. A successful short conversion does not deliver that chunk, though the worker returns one.

With a valid audio buffer, the function calls `SetRecordedBuffer` at `0x1adc0`, then `SetVQEData` at `0x1b190`, and finally `DeliverRecordedData` at `0x1b1b0`. The targets are resolved from audio-buffer vtable address point `0xb9088`, slots `+0x10`, `+0x18`, and `+0x20`. Delay values are rounded from accumulated microsecond quantities to milliseconds. The body also samples key states and writes a changed-key flag before delivery.

A null audio buffer logs an error and returns zero. Converter status one returns zero without the general converter-error log. Other nonzero converter statuses log and return zero. **U.** The worker-loop owner's response to zero, the meaning of converter status one, and queue-empty waits in `inConverterProc` were not traced. This is not proof of an automatic retry or stop policy.

### 8. The device IO callback queues native samples and wakes capture work

**D, A2, `webrtc::AudioDeviceMac::implInDeviceIOProc`, `0x1a280`.** While recording is active, the callback computes capture-delay accounting from ring indices, native rate/channel values, and host timestamps. It calls `PaUtil_WriteRingBuffer` on device ring pointer `0x1958`, using the incoming buffer pointer and a native-format-derived element count, then calls `semaphore_signal_all`.

The `PaUtil_WriteRingBuffer` return value is not checked in this body. Semaphore errors are logged. **U.** Partial-write or overflow behavior inside `PaUtil_WriteRingBuffer`, ring element size, and recovery after truncated writes remain unresolved. No bounded queue-loss counter was identified in this function.

If the stop-request byte at device offset `0x188a` is set, the callback stops and destroys the input IOProc under the device mutex, clears the request, sets the completion event, broadcasts its condition variable, and returns. This branch connects to the separate-input stop path below.

### 9. Device removal invalidates device state but does not itself reopen capture

**D, A2, `webrtc::AudioDeviceMac::HandleDeviceChange`, `0x195b0`.** The function conditionally queries input and output device-alive properties. API result `0x21646576` or a zero alive value clears the corresponding atomic state at `0x1884` or `0x1880` and clears microphone or speaker mixer state under its mutex. Other property-query errors are logged. No reopen, replacement-device selection, or capture restart call appears in this body.

**S supporting the conditions.** The gates are vtable calls to `MicrophoneIsInitialized` at `0x16cd0` and `SpeakerIsInitialized` at `0x16a10`, slots `+0xd8` and `+0xc8` of address point `0xb8e80`. They are not proven `Recording()` and `Playing()` tests.

**D, A5.** `objectListenerProc` at `0x193d0` directly calls this function at instruction address `0x19433`. Its surrounding property-selection branch was not decompiled, so callback ordering relative to other property changes is unknown.

### 10. Capture shutdown has a bounded IOProc handshake, not a fully bounded shutdown

**D, A2, `webrtc::AudioDeviceMac::StopRecording`, `0x16060`.** An uninitialized recording state returns zero. For an active separate input device, the function clears the recording-active byte, sets stop-request byte `0x188a`, releases the device mutex, and waits `2000` milliseconds on the completion event. The shared-device stop branch uses a separate request and event with the same wait. If the wait times out, the function directly calls `AudioDeviceStop` and `AudioDeviceDestroyIOProcID`, then clears the request.

The separate-input completion is implemented in `implInDeviceIOProc` above. Shared-IOProc completion was not traced. The subsequent teardown clears atomic capture state, joins the capture thread, disposes the audio converter, removes format/overload listeners, and clears recording flags. Most OS errors are logged and the function still returns zero. A thread-join failure reaches `FatalLog`.

**U.** `pthread_join` has no visible timeout. The two-second IOProc handshake therefore does not establish a maximum duration for the complete shutdown. Whether the converter's semaphore wait always exits after removal remains unknown without `inConverterProc` and the thread-loop owner.

## Playout boundary and small decoded callbacks

**D, decoded x86_64 bytes, helper `0x8e80` and `0x8e90`.** `mywebrtc::NeedMorePlayData` contains `push rbp; mov rbp,rsp; xor eax,eax; pop rbp; ret`. It returns zero without filling the supplied audio destination or sample-count output. `mywebrtc::PullRenderData` contains a prologue followed immediately by an epilogue and return, with no render work. These are two small stubs beyond the ten substantive functions. The raw bytes are respectively `554889e531c05dc3` and `554889e55dc3`.

**I.** These stubs, the reverse-processing body at `0x7180`, and Geronimo's renderer symbols support separating AEC reference input from actual speaker playout. They do not prove that no other object uses the bundled WebRTC audio-device playout implementation.

**S, Geronimo x86_64.** `AudioUnitsAudioRenderer::renderAudio` at `0x10d6e0`, `queueAudio` at `0x1122e0`, `AudioRingBuffer::read` at `0x112640`, and `AudioRingBuffer::write` at `0x112710` are present. These names identify a separate playback queue/callback boundary, but their behavior and application reachability remain unknown until decompiled.

## Cross-library strings and next queries

The following Geronimo strings are in real `__cstring`, whose thin virtual range begins at `0xdaa220`. They are `S` only, not executed branches. Each file offset is relative to the thin x86_64 file.

| Geronimo string | Address | Thin file offset | Question raised |
| --- | --- | --- | --- |
| `Registering audiocapture callback for echo cancellation at render end` | `0xdbf7c5` | `14415813` | Where is the render callback registered and removed? |
| `Voice data packets dropped` | `0xdbfa08` | `14416392` | Does the callback queue drop new data or old data, and what is its bound? |
| `Invalid reverse stream channel count: %u` | `0xdbfa86` | `14416518` | Which channels does Geronimo permit before calling the helper? |
| `Capture device changed successfully; %s using concealment filler` | `0xdbfe54` | `14417492` | What filler is generated and when does normal microphone data resume? |
| `No capture devices present, stopping recording` | `0xdbfe99` | `14417561` | Which layer stops, signals state, and retries after removal? |
| `Voice data size not equal to requested size, requestedSize: %d, readBytes: %d` | `0xdbfec8` | `14417608` | How are partial reads padded, retried, or discarded? |

All next-query targets below are `S`, named functions in the supplied x86_64 symbol tables. Their names are not proof of calls or runtime policy. Geronimo candidates are especially important because the helper accepts an outer callback that this pass did not resolve.

| Library | Address and function | Specific follow-up |
| --- | --- | --- |
| Geronimo | `0x15a880` `WebRTCAudioCapturer::initVoiceCapture(int,bool)` | Trace requested device index, AEC mode, helper errors, and render-reference registration. |
| Geronimo | `0x15baf0` `WebRTCAudioCapturer::gsAudioWebRTC_RecordedDataIsAvailableCb(void*,unsigned long)` | Resolve the helper's outer callback and its queue/drop behavior. |
| Geronimo | `0x15bbb0` `WebRTCAudioCapturer::Queue<unsigned char>::put` and `0x15b7b0` `get` | Recover queue capacity, synchronization, overflow, and partial-read policy. |
| Geronimo | `0x15b740` `WebRTCAudioCapturer::readVoiceData` | Determine the requested capture size and caller behavior on short reads. |
| Geronimo | `0x15bd60` `WebRTCAudioCapturer::processReverseStream(NvstAudioFrame_t const&)` | Recover the NVST-to-helper frame conversion, channel guards, and frame slicing. |
| Geronimo | `0x15c380` `WebRTCAudioCapturer::reinitializeCaptureDevice` | Determine device-change teardown, restart, and concealment behavior. Callbacks are `onDefaultDeviceChanged` at `0x15c490` and `onStreamFormatChanged` at `0x15c570`. |
| Geronimo | `0x15c5f0` `WebRTCAudioCapturer::getMicState(DEVICE_ERROR)` | Map permission and unavailable-device states from real branches, not string enums. |
| Geronimo | `0x10d6e0` `AudioUnitsAudioRenderer::renderAudio` and `0x1122e0` `queueAudio` | Recover speaker consumption, underflow handling, and queue ownership. |
| Geronimo | `0x111c40` `AudioUnitsAudioRenderer::reset(bool)` and `0x10d990` `reconnectAU` | Recover queue clearing and render-device recovery. Device-change callback is `0x10dac0`. |
| Geronimo | `0x9bad0` `Nsk::NskStreamClientImpl::sendMicAudioFrame` | Connect capture bytes to the typed microphone frame and transport boundary. |
| Bifrost | `0x12490` `NVB::BifrostClient::sendMicAudioFrame` and `0x56290` `NVB::SessionController::sendMicAudioFrame` | Trace microphone submission through the SDK and session state. |
| Bifrost | `0x8fad0` `NVB::Streamer::sendMicAudioFrame` and `0xffcf0` `ClientSession::sendMicAudioFrame` | Establish selected microphone sender and encoded format. |
| Bifrost | `0x7c7d0` `NVB::NvscWrapper::onRenderAudio` | Trace audio frame delivery to the renderer and ownership/release semantics. |
| Bifrost | `0x1bb2b0` `WebRtcTransport::CreateAudioSender` and `0x1bb6e0` `CreateAudioReceiver` | Trace reachability from the selected session transport before claiming these paths are used. |
| Bifrost | `0x1d4b00` `WebRtcAudioRtpSender::sendAudio`, `0x1d49f0` `ControlAudioRedundancy`, and `0x21a200` `RtpAudioPlayer::sendAudioPacket` | Recover codec, timestamp, loss/redundancy, and packet rules only after the transport branch is proven. |

**S, Bifrost x86_64.** Its real `__cstring` includes `Microphone stream did not set the supportedFrameSize` at `0xd0bb8f`, thin offset `13679503`; `supportedPacketsRetransmitCount` diagnostic at `0xd0bbc4`, offset `13679556`; and `supportedEncodeBitrates` diagnostic at `0xd0bc06`, offset `13679622`. These indicate negotiation fields worth tracing, not their values, encoding, or mandatory runtime use.

## Compatibility recommendation

**Recommendation, not a recovered implementation contract.** Keep local capture/AEC processing separate from speaker playout and media transport. Use explicit sample rate, channel count, sample type, and chunk duration at the conversion boundary. The helper's successful fixed-rate device path provides 480 frames at 48 kHz, but the caller's NVST format and packet duration must be traced rather than copied from that fact.

Bound and measure both playback and microphone queues, and make device-loss shutdown independent of a future IO callback. Reject malformed frame dimensions before reaching a fatal sample limit. Preserve sender-authored timestamps in transport and recording; the helper's ten-step processing counters are local processor bookkeeping, not a substitute. These recommendations follow the risks identified at helper `0x7180`, `0x7c50`, `0x1a280`, and `0x16060`, A1/A2, but they do not assert that the existing OpenNOW implementation has those faults.

## Exact remaining unknowns and limitations

- **U, helper `0x19ac0` and `0x19840`.** Queue-empty wait, ring capacity/overflow implementation, converter underflow status, and the capture worker's scheduling/exit loop remain untraced.
- **U, helper `0x6e00`, `0x6ee0`, and `0x7060`.** Start/stop/close wrapper status behavior and lifetime of the shared object remain untraced. Only the lower CoreAudio stop implementation was decompiled.
- **U, Geronimo `0x15bd60` and `0x15baf0`.** Which render audio reaches AEC, whether it reflects actual speaker volume/mixing, callback registration lifetime, capture-byte format, and the application's queue/drop policy remain untraced.
- **U, Geronimo `0x1122e0`, `0x10d6e0`, and `0x15c380`.** Playback underflow/silence, latency correction, channel layouts, default-device recovery, capture concealment, and hotplug retry timing remain untraced.
- **U, Bifrost `0xffcf0` and `0x1bb2b0`.** Session transport selection, codec, microphone packetization, reliability/retransmission policy, and frame/timestamp semantics remain untraced. Bundled WebRTC processing and WebRTC-named sender symbols do not prove a session uses a WebRTC transport.
- **U, helper `0x5c50`.** Packed APM configuration fields and AEC generation were not individually reconstructed. No claim is made about noise suppression, automatic gain control, or exact echo-canceller settings.
- Both batches reported all five requested entries successful and provided complete returned pseudocode strings. This is not proof that Ghidra recovered every original function branch. `InitRecording` has unsettled type propagation; the capture callback has removed unreachable blocks, ABI ambiguities, and overlapping-global warnings. No runtime test resolves those limitations.
- Incoming-reference queries cover Ghidra's recovered function bodies and entries. Unresolved computed flows are absent. A4 supplies data references to the exported reverse function, not a Geronimo caller. Cross-library calls and callback dispatch cannot be asserted from that absence.

## Queries and retention

The isolated session analyzed the thin x86_64 slice of `archive/libGsAudioWebRTC.dylib`. Successful analysis calls and their exact arguments are preserved in the accompanying replay manifest. They used a separate local MCP session, not a shared mutable REA target.

1. `batch_decompile` with addresses `0x5180`, `0x5c50`, `0x7180`, `0x76e0`, and `0x7c50`.
2. `batch_decompile` with addresses `0x14c90`, `0x19870`, `0x1a280`, `0x195b0`, and `0x16060`.
3. `procedure_references`, direction `incoming`, for `0x7c50`, `0x7180`, and `0x195b0`.
4. `export_evidence_bundle`, followed by `close_binary` after a successful export.

Raw results, full pseudocode, static subsets, and the static-pointer verifier remain private research evidence. This document contains behavioral summaries rather than bulk decompiled source. The helper was closed after export; no live analysis operation is required to read this report.
