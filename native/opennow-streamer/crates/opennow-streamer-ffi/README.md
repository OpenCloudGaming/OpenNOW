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

### HDR texture metadata (ABI 6)

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

### Vulkan enabled capabilities (ABI 4, graphics context 2)

Shutdown invalidates the context even if resource retirement returns `OPENNOW_STREAMER_RENDER_FAILED`. A lost Vulkan device can retire normally. If an idle wait fails for another reason, resources whose completion cannot be proven are deliberately abandoned instead of being destroyed by a later worker drop against a dead device; the error is returned to the host.

`enabled_capabilities` is an explicit logical-device contract, not physical-device discovery. Set `OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_IMPORT` only when the host created the device with external-memory, external-memory-fd, DMA-BUF, and DRM-modifier support enabled, including their prerequisites. Unknown bits and capabilities on non-Vulkan contexts are rejected; a zero mask disables DMA-BUF import while leaving explicitly CPU-backed NV12 presentation available.

Qt requests the extensions through `QT_VULKAN_DEVICE_EXTENSIONS` before any window/device creation. Qt 6.8's Vulkan backend enables each requested extension that the selected physical device advertises. The host contract checks both that this startup request was installed and that every non-core extension is requested and supported on the selected device, with Vulkan 1.1 or newer on the instance and physical device. Vulkan 1.1 provides the external-memory, bind-memory, memory-requirements, sampler-YCbCr, and maintenance prerequisites; the request also includes their extension names. `VK_KHR_image_format_list` must be enabled unless both instance and physical device provide Vulkan 1.2. Support is not inferred from advertisement alone. This contract applies to Qt-created devices, not arbitrary adopted devices.

Embedded Linux sessions reject FFmpeg's independent-device Vulkan decoder before opening it. Vulkan decode is available only through the ABI 5 shared owner described above. CUDA/NVDEC remains an explicit CPU-transfer backend, and downloaded/software frames carry CPU planes without foreign Vulkan metadata. No failed GPU import triggers readback in the embedded presenter.

## Integration boundary

The public C constructor creates the embedded engine used by Qt. GPU producers publish through the platform runtime's `GraphicsFramePublisher`; the FFI owns the mailbox, token epochs, thread checks, and C ownership boundary while platform decoders own native texture creation and same-command-stream conversion.

There is no callback cancellation or timeout. A callback that never returns will eventually backpressure responses and will make destroy wait indefinitely. Dropped unsolicited events are not yet summarized with an overflow event. The header is handwritten and must be validated by each C/C++ consumer's compile-time layout assertions.
