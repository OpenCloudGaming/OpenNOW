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

All three queues are bounded. Command submission returns `OPENNOW_STREAMER_QUEUE_FULL` rather than blocking. Responses backpressure the engine worker so an accepted command's response is retained. Unsolicited events use a drop-newest policy when their queue is full because the engine's event path cannot block latency-sensitive transport workers.

## GPU frame lifecycle

The graphics API is GPU-only. It exposes no window, swap chain, `QWindow`, CPU image, pixel buffer, or encoded-video callback.

1. On the QQuick render thread, call `opennow_streamer_set_graphics_context` with the versioned native objects borrowed from the current QRhi.
2. When `frame_available_callback` schedules a frame, call `opennow_streamer_acquire_latest_frame`. The bounded mailbox contains one pending frame: publishing a newer frame releases the stale pending frame. A successful acquisition transfers one retained reference into the opaque token.
3. After QRhi has opened the frame and provided its native command buffer, but **before** `QQuickRhiItem` calls `beginPass`, call `opennow_streamer_record_frame`. Conversion and synchronization are encoded into that exact command stream. The function never creates, submits, commits, or waits for another command buffer. It returns one producer-owned RGBA8 or RGB10A2 GPU texture for the selected in-flight slot, preserving a negotiated 10-bit stream on supported paths.
4. Import and sample that texture inside the item's render pass. Keep the token until QRhi has finished every GPU use of the slot, then call `opennow_streamer_release_frame`. A token records at most once and must be released exactly once.
5. During scene-graph invalidation, release tokens first and call `opennow_streamer_scene_graph_shutdown` on the bound render thread before QRhi destroys its native objects. Destroy rejects a still-active scene graph instead of dropping GPU state from the wrong thread.

Changing the graphics context clears the one-frame mailbox and advances its epoch. Previously acquired tokens stay releasable but become stale and cannot record against the replacement context. Graphics calls are bound to the thread that installed the active context; a new scene graph can bind a different thread after shutdown.

## Integration boundary

The public C constructor creates the embedded engine used by Qt. GPU producers publish through the platform runtime's `GraphicsFramePublisher`; the FFI owns the mailbox, token epochs, thread checks, and C ownership boundary while platform decoders own native texture creation and same-command-stream conversion.

There is no callback cancellation or timeout. A callback that never returns will eventually backpressure responses and will make destroy wait indefinitely. Dropped unsolicited events are not yet summarized with an overflow event. The header is handwritten and must be validated by each C/C++ consumer's compile-time layout assertions.
