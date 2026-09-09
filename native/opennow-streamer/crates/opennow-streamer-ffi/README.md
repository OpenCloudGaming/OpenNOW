# OpenNOW streamer FFI

This crate exposes `opennow-streamer-core::Engine` as a C-compatible in-process library. It is an integration boundary only: it does not start a child process, read or write standard streams, create an SDL/native window, or depend on Qt.

## Contract

- Initialize every `OpenNowStreamerConfig` field, set `abi_version` to `OPENNOW_STREAMER_FFI_ABI_VERSION`, and set `struct_size` to `sizeof(OpenNowStreamerConfig)`.
- `opennow_streamer_create` writes one owned opaque handle to `output`. On failure it writes `NULL` when the supplied config header is readable.
- `opennow_streamer_send` copies one serialized JSON protocol command before returning. `OPENNOW_STREAMER_OK` means the bounded command queue accepted it, not that the protocol command succeeded. Parse errors and protocol results arrive at `response_callback`.
- Values returned directly by `Engine::handle` go to `response_callback`. Unsolicited engine events go to `event_callback`. Each byte slice is UTF-8 JSON and is valid only for the duration of that callback.
- `frame_available_callback` is separate from the protocol callbacks. It only asks the shell to schedule a render; it carries no frame or protocol payload and may run on a decoder thread.
- Response and event callbacks each run serially on their own dispatcher thread, so the two callbacks may overlap. They must return promptly and must not re-enter this handle. `user_data` must remain valid until `opennow_streamer_destroy` returns.
- `opennow_streamer_destroy` consumes the handle exactly once, waits for the engine and both callback queues to drain, and then returns. No call may race with destroy. A null handle is rejected; reusing a destroyed pointer is caller-side undefined behavior.
- Every exported function catches Rust panics before they can unwind through the C ABI. A worker-thread panic closes the command queue.

### Clipboard text (ABI 9)

Rebuild Qt and the native library together with ABI version 9. Existing structure
layouts and the graphics render-command version are unchanged. The new export is
`opennow_streamer_submit_text(const OpenNowStreamer *, const uint8_t *, size_t)`;
`OPENNOW_STREAMER_MAX_TEXT_BYTES` is 65,536.

The caller supplies nonempty, strictly valid UTF-8 without embedded NUL or a trailing
terminator. The library copies the complete text before returning; it never truncates,
normalizes newlines, interprets text as key combinations, or logs its contents.
Null pointers return `NULL_POINTER`, empty/invalid/NUL-containing text returns
`INVALID_CONFIG`, and lengths over the bound return `MESSAGE_TOO_LARGE` before the
buffer is read. An oversized length takes precedence over a null text pointer.

Admission requires active embedded capture and negotiated session input. Otherwise
it returns `CLOSED`. One paste occupies a capture-queue entry and retains a single-flight
reservation through the ordered transport command queue. A second paste or a full
256-entry capture queue returns `QUEUE_FULL` without admitting any text, evicting
gameplay input, or triggering the control-overflow shutdown. `OK` acknowledges whole
local admission, not delivery to or insertion by the remote application.

Capture loss, input unavailability, and session teardown cancel text still pending
in capture or transport. Readiness changes are generation-scoped, so stale session
events cannot enable or cancel text belonging to a replacement session. A paste
already submitted to SCTP cannot be recalled.
Transport checks capacity for the complete encoded batch across all eight negotiated
SCTP channels before writing any chunk. This matches str0m 0.23's 128-KiB aggregate
send-buffer limit; the receive worker owns all writes and does not poll the network
between chunks. Insufficient capacity rejects the whole batch and emits a payload-free
diagnostic rather than retrying a paste that might duplicate text. Connection failure
can interrupt remote delivery; the protocol provides no paste-level acknowledgement
or rollback.

Each packet uses ordered reliable control command `0x0206` and remote-input type 23,
with raw UTF-8 bodies of at most `0x3f8` (1,016) bytes. Overflow cuts back to the nearest
code-point boundary (at most three bytes). The RI header is a big-endian 32-bit length
of `4 + body bytes`, then little-endian type 23. Following the verified
`NvstRemoteInput.utf8TextPackets` / `sendFramedRemoteInput` reference contract, inner
packets up to 1,007 bytes receive a type-14 envelope with zero padding to
`((inner_length + 8) / 8) * 8 + 8` bytes and one little-endian 64-bit capture timestamp.
Larger inner packets (text bodies of 1,000–1,016 bytes) are sent bare. The older
serialized ASCII text command retains its key-stroke conversion for compatibility;
the typed clipboard path never enters that codec or its raw-input diagnostics.

### Controller rumble events

The additive `controller-rumble` JSON event uses the existing `event_callback` (no C ABI
layout change): `startId` is the originating start command's ID, `controllerId` is a wire
slot from 0 through 3, `lowFrequency` and `highFrequency` are unsigned 16-bit motor
amplitudes, and `durationMs` is an integer from 1 through 65535. Hosts must discard
events whose `startId` does not match the currently authorized start, and stop motors
on session replacement, stop, failure, capture loss, and device removal.

NVST control command `0x010b` uses a little-endian `u16 code, u16 payloadLength`
envelope. Its payload begins with `u16 kind, u16 blobLength`. Kind 1 contains 6-byte
records (`u16 slot, u16 left, u16 right`); kind 2 contains 8-byte records with an
additional `u16 durationMs`. Absent or zero durations become 1000 ms, matching the
reference client. Zero motor amplitudes are stop commands. Unknown kinds and slots
above 3 are ignored. Complete records within the smaller of the declared blob length
and available payload are accepted; truncated outer envelopes are not dispatched.

Transport reception coalesces into four session-owned latest-value slots, including
zero-amplitude stops. The engine polls these alongside input and forwards events
nonblockingly through the bounded FFI queue. If that queue is full, four latest-value
slots retain commands for retry, so stops are not permanently lost. Coalesced records
are included in the periodic queue-drop diagnostics. Qt also bounds callback storage,
validates every numeric field and start ID, and emits `controllerRumbleRequested` on
the GUI thread. Its `controllerRumbleStopped` signal accompanies presentation
invalidation. Callback overflow stops motors and discards rumble queued before the
overflow, without discarding status or response callbacks. The shell owns
physical-device routing and shutdown. Individual rumble events are
not logged on the high-frequency callback path.

### Optional MetalFX spatial upscaling (ABI 8, render command 3)

ABI 8 requires rebuilding the Qt shell and native runtime together. Version 3 of
`OpenNowStreamerRecordCommand` appends `upscale_sharpness` and `upscale_denoise`
after the version-2 `upscale_width` and `upscale_height` fields. Earlier command
versions and truncated layouts are rejected. Initialize sharpness to 10 and denoise
to 0; valid inclusive ranges are 0–15 and 0–20 respectively. Invalid controls are
rejected even when scaling is disabled. Other graphics backends ignore valid controls.
Set both target dimensions to zero for normal scaling. On Metal, nonzero dimensions request spatial
MetalFX at the video viewport's physical pixel size, excluding letterboxing.
Dimensions must both be zero or each be in `1..=16384`. Other graphics backends
ignore the target. Unsupported MetalFX configurations fall back to the original
converted texture without stopping the session.

The Mac producer encodes scaling after YUV-to-RGB conversion in the same borrowed
command buffer. Clarity and denoise filter source-resolution RGB in that conversion
pass, before MetalFX; they are not MetalFX descriptor settings. The four cardinal
neighbors form `blur`, with `sharpness = value / 10` and
`denoise = min(value / 10 * 0.65, 1)`. The shader computes
`denoised = mix(center, blur, denoise)`, then clamps
`denoised + (denoised - blur) * sharpness` to 0–1, matching OpenNOW-Mac's native
spatial shader. Zero controls skip the neighborhood samples. Changes update
uniforms without rebuilding the scaler or restarting the session.
New control values apply when the next decoded frame is recorded; a retained output
keeps its existing processing until that frame arrives. Token ownership and the
single-record contract remain unchanged.
The returned texture dimensions describe the actual output;
`OpenNowStreamerFrameInfo` still describes the decoded source. Source sequence,
timestamps, color precision, frame-slot ownership, and scene-graph retirement
remain unchanged. Qt must refresh the target after resize, fullscreen, or display
scale changes, and must not infer source video geometry from upscaled textures.

### HDR texture metadata (introduced in ABI 6)

ABI 6 adds `color_space` to `OpenNowStreamerRecordedFrame` and rejects clients compiled for earlier ABI versions. Rebuild the native runtime and Qt shell together. `texture_format` now accepts `RGBA16F` in addition to `RGBA8` and `RGB10A2`.

The color-space constants identify the encoded RGB signal: `SDR709` is the existing SDR output, `PQ2020` is ST 2084 with BT.2020 primaries, and `HLG2020` is HLG with BT.2020 primaries. A 10-bit texture does not imply HDR. Windows converts HDR decoder surfaces to PQ BT.2020 RGB10A2; Linux preserves PQ/HLG BT.2020 in RGBA16F. HDR in RGBA8 is rejected at the FFI boundary. The shell must transform the signal to its actual swapchain color space, or explicitly tone-map HDR for SDR output.

Color metadata belongs to each retained frame, so HDR/SDR transitions cannot reuse a previous frame's color interpretation. The texture ownership, bounded slots, generation, and sender presentation timestamps keep the same lifecycle.

### Shared Vulkan device (introduced in ABI 5)

ABI 5 appends `const OpenNowStreamerVulkanDevice *vulkan_device` to the engine config and rejects older config versions. Initialize this field to `NULL` on platforms that do not adopt a shared device.

On Linux, call `opennow_streamer_vulkan_device_create` before creating the Qt graphics device. Success returns one opaque owner; unsupported builds, unavailable Vulkan Video devices, and device-creation failures return `OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE` with a null output. `opennow_streamer_vulkan_device_info` writes the complete version-1 `OpenNowStreamerVulkanDeviceInfo`, including its size, instance, physical device, logical device, graphics queue, queue family and index, and API version. The output needs writable storage for the complete structure; it does not need preinitialization. These native objects are borrowed, not transferred to the caller.

Linux bootstrap failures retain the device-creation reason in a single line capped at 512 characters, sent to the native file log when configured and to stderr even before runtime logging is initialized. This diagnostic contains the typed device error, not native object addresses or session credentials. Stderr write failures do not change the FFI result.

Qt adopts these exact graphics objects and passes the owner in `OpenNowStreamerConfig`. Engine creation clones the native reference before starting its worker, and the runtime passes that same owner to every Linux decoder session. Keep the shell's owner alive until all adopted Qt graphics resources, windows, and Vulkan instance wrappers have been released. `opennow_streamer_vulkan_device_destroy` consumes only the shell's reference; it does not invalidate a reference retained by an engine or decoder. No call may race with owner destruction. Null device handles are rejected by info and destroy.

Embedded Vulkan capabilities come from the attached logical device's codec profiles rather than the standalone device probe. Session startup also validates the negotiated codec, dimensions, and color depth: 4:2:0 8-bit uses NV12, supported 4:2:0 10-bit uses P010, and 4:4:4 is rejected instead of silently downgraded. A null owner keeps embedded Vulkan decode unavailable and preserves the existing non-Vulkan fallback. Standalone backend selection and other operating systems are unchanged.

The protocol-6 hello report includes `videoBackends[].codecs[].colorQualities` for embedded Linux codecs. Vulkan lists only the attached device's supported `8bit_420`/`10bit_420` profiles. VAAPI lists `10bit_420` for HEVC/AV1 only when FFmpeg supports that decoder and a render node reports the corresponding decode profile and 10-bit render-target format. Other Linux paths remain 8-bit only. Windows reports additive `hdrSupported` flags from actual P010 decoder and PQ video-processor conversion probes without changing its existing SDR color-profile contract. Qt adds the current window's `nativeHdrSupported` to the transient runtime capabilities sent to core. Both Auto and manual codec choices are checked before CloudMatch allocation; the media-start check remains a second guard.

The shared decoder copies decoded images into bounded GPU snapshots isolated from FFmpeg's reference-picture pool. Completed snapshots remain immutable while Qt samples them. This path is GPU-only, with no CPU readback, but it is not zero-copy and does not advertise a zero-copy mode.

All three queues are bounded. Command submission returns `OPENNOW_STREAMER_QUEUE_FULL` rather than blocking. Responses backpressure the engine worker so an accepted command's response is retained. Unsolicited events use a drop-newest policy when their queue is full because the engine's event path cannot block latency-sensitive transport workers.

Queue-drop diagnostic deltas are retained per source when periodic event delivery
finds a full queue, then retried at the next one-second flush. The engine joins
media producers and drains remaining drop feedback before completing stop.
Final event delivery is also nonblocking; undeliverable final deltas are written
directly to the native log and omitted from callback/UI totals rather than waiting
for a stalled dispatcher. The dispatcher remains alive through engine shutdown. Callback owners must
keep accepting events through the stop response to receive these final deltas;
native file logging still records them if a host has already disabled callbacks.
This does not change media queues or normal event-path backpressure.

## GPU frame lifecycle

The graphics API is GPU-only. It exposes no window, swap chain, `QWindow`, CPU image, pixel buffer, or encoded-video callback.

1. On the QQuick render thread, call `opennow_streamer_set_graphics_context` with the versioned native objects borrowed from the current QRhi.
2. When `frame_available_callback` schedules a frame, call `opennow_streamer_acquire_latest_frame`. The bounded mailbox contains one pending frame: publishing a newer frame releases the stale pending frame. A successful acquisition transfers one retained reference into the opaque token.
3. After QRhi has opened the frame and provided its native command buffer, but **before** the Qt scene pass begins, call `opennow_streamer_record_frame`. Conversion and synchronization are encoded into that exact command stream. The function never creates, submits, commits, or waits for another command buffer. It returns one producer-owned RGBA8, RGB10A2, or RGBA16F GPU texture with explicit color-space metadata for the selected in-flight slot.
4. Import and sample that texture inside the item's render pass. Keep the token until QRhi has finished every GPU use of the slot, then call `opennow_streamer_release_frame`. A token records at most once and must be released exactly once.
5. At session-generation changes and scene-graph invalidation, call `QRhi::finish()` outside a render pass to submit and drain commands, release tokens and imported Qt texture wrappers, and drain Qt's deferred releases. Then call `opennow_streamer_scene_graph_shutdown` on the bound render thread before QRhi destroys its native objects. This explicitly retires the Linux producer's in-flight slots and imported resources even when the decoder/publisher still holds that producer. A session worker may clear the mailbox and stop decoding but never destroys the render-owned resource lease. Destroy rejects a still-active scene graph instead of dropping GPU state from the wrong thread.

Replacing the graphics device or session renderer requires explicit shutdown first. Shutdown clears the one-frame mailbox and advances its epoch. Previously acquired tokens stay releasable but become stale and cannot record against the replacement context. Graphics calls are bound to the thread that installed the active context; a new scene graph can bind a different thread after shutdown. Only one session renderer is retained, bounding ownership across repeated session restarts.

### Vulkan enabled capabilities (graphics context 3)

Shutdown invalidates the context even if resource retirement returns `OPENNOW_STREAMER_RENDER_FAILED`. A lost Vulkan device can retire normally. If an idle wait fails for another reason, resources whose completion cannot be proven are deliberately abandoned instead of being destroyed by a later worker drop against a dead device; the error is returned to the host.

`enabled_capabilities` is an explicit logical-device contract, not physical-device discovery. Set `OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_IMPORT` only when the host created the device with external-memory, external-memory-fd, DMA-BUF, and DRM-modifier support enabled, including their prerequisites. Unknown bits and capabilities on non-Vulkan contexts are rejected; a zero mask disables DMA-BUF import while leaving explicitly CPU-backed NV12 presentation available.

Graphics context version 3 adds the independent
`OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_BUFFER_IMPORT` capability. Set it only
for Vulkan 1.1+ instances/devices with external-memory-fd, DMA-BUF, and
`VK_EXT_queue_family_foreign` enabled on the logical device. The Pi SAND path
requires this capability, queries external TRANSFER_SRC buffer import support,
and transfers ownership from/to the foreign decoder around its GPU copy. It does
not require SAND image-modifier sampling support. Setting the image-import bit
alone does not enable this path; zero remains a safe default for both bits.

Qt requests the extensions through `QT_VULKAN_DEVICE_EXTENSIONS` before any window/device creation. Qt 6.8's Vulkan backend enables each requested extension that the selected physical device advertises. The host contract checks both that this startup request was installed and that every non-core extension is requested and supported on the selected device, with Vulkan 1.1 or newer on the instance and physical device. Vulkan 1.1 provides the external-memory, bind-memory, memory-requirements, sampler-YCbCr, and maintenance prerequisites; the request also includes their extension names. `VK_KHR_image_format_list` must be enabled unless both instance and physical device provide Vulkan 1.2. Support is not inferred from advertisement alone. This contract applies to Qt-created devices, not arbitrary adopted devices.

Embedded Linux sessions reject FFmpeg's independent-device Vulkan decoder before opening it. Vulkan decode is available only through the ABI 5 shared owner described above. CUDA/NVDEC remains an explicit CPU-transfer backend, and downloaded/software frames carry CPU planes without foreign Vulkan metadata. No failed GPU import triggers readback in the embedded presenter.

## Integration boundary

The public C constructor creates the embedded engine used by Qt. GPU producers publish through the platform runtime's `GraphicsFramePublisher`; the FFI owns the mailbox, token epochs, thread checks, and C ownership boundary while platform decoders own native texture creation and same-command-stream conversion.

There is no callback cancellation or timeout. A callback that never returns will eventually backpressure responses and will make destroy wait indefinitely. Dropped unsolicited events are not yet summarized with an overflow event. The header is handwritten and must be validated by each C/C++ consumer's compile-time layout assertions.
