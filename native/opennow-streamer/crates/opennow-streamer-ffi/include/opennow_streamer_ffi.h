#ifndef OPENNOW_STREAMER_FFI_H
#define OPENNOW_STREAMER_FFI_H

#include <stddef.h>
#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define OPENNOW_STREAMER_FFI_ABI_VERSION 3u
#define OPENNOW_STREAMER_GRAPHICS_CONTEXT_VERSION 1u
#define OPENNOW_STREAMER_RENDER_COMMAND_VERSION 1u

#define OPENNOW_STREAMER_GRAPHICS_API_D3D11 1u
#define OPENNOW_STREAMER_GRAPHICS_API_VULKAN 2u
#define OPENNOW_STREAMER_GRAPHICS_API_METAL 3u

#define OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8 1u
#define OPENNOW_STREAMER_TEXTURE_FORMAT_RGB10A2 2u

#define OPENNOW_STREAMER_LOCAL_ACTION_GUIDE 1u
#define OPENNOW_STREAMER_LOCAL_ACTION_SCREENSHOT 2u
#define OPENNOW_STREAMER_LOCAL_ACTION_RECORDING_TOGGLE 3u

typedef struct OpenNowStreamer OpenNowStreamer;
typedef struct OpenNowStreamerFrame OpenNowStreamerFrame;

typedef void (*OpenNowStreamerCallback)(
    const uint8_t *bytes,
    size_t length,
    void *user_data);

typedef void (*OpenNowStreamerFrameAvailableCallback)(void *user_data);

typedef struct OpenNowStreamerConfig {
    uint32_t abi_version;
    size_t struct_size;
    size_t command_queue_capacity;
    size_t response_queue_capacity;
    size_t event_queue_capacity;
    size_t max_command_bytes;
    OpenNowStreamerCallback response_callback;
    OpenNowStreamerCallback event_callback;
    OpenNowStreamerFrameAvailableCallback frame_available_callback;
    OpenNowStreamerCallback cursor_callback;
    void *user_data;
} OpenNowStreamerConfig;

/*
 * Borrowed native objects from one live QRhi. The shell retains ownership.
 *
 * D3D11: device is ID3D11Device and queue is its immediate ID3D11DeviceContext.
 * Vulkan: instance, physical_device, device, queue, and queue_family_index are required.
 * Metal: device is id<MTLDevice> and queue is id<MTLCommandQueue>.
 */
typedef struct OpenNowStreamerGraphicsContext {
    uint32_t version;
    size_t struct_size;
    uint32_t graphics_api;
    void *instance;
    void *physical_device;
    void *device;
    void *queue;
    uint32_t queue_family_index;
} OpenNowStreamerGraphicsContext;

/*
 * One shell-owned in-flight slot and the current QRhi native command buffer.
 * record_frame writes conversion/synchronization commands into this command stream; it never
 * creates, commits, submits, or waits for another command buffer.
 */
typedef struct OpenNowStreamerRecordCommand {
    uint32_t version;
    size_t struct_size;
    void *command_buffer;
    uint32_t frame_slot;
} OpenNowStreamerRecordCommand;

typedef struct OpenNowStreamerFrameInfo {
    uint32_t width;
    uint32_t height;
    uint64_t sequence;
    uint64_t presentation_time_ns;
} OpenNowStreamerFrameInfo;

/*
 * One GPU texture populated by record_frame. texture_format identifies RGBA8 or RGB10A2. On D3D
 * and Metal, resource is the native texture pointer encoded as uint64_t and resource_view is zero.
 * On Vulkan, resource is VkImage and resource_view is VkImageView. The producer owns both handles;
 * the frame token retains their backing slot until release.
 */
typedef struct OpenNowStreamerRecordedFrame {
    uint64_t resource;
    uint64_t resource_view;
    uint32_t graphics_api;
    uint32_t texture_format;
    uint32_t width;
    uint32_t height;
    uint32_t frame_slot;
    uint64_t generation;
    uint64_t presentation_time_ns;
} OpenNowStreamerRecordedFrame;

typedef enum OpenNowStreamerStatus {
    OPENNOW_STREAMER_OK = 0,
    OPENNOW_STREAMER_NULL_POINTER = 1,
    OPENNOW_STREAMER_INVALID_CONFIG = 2,
    OPENNOW_STREAMER_MESSAGE_TOO_LARGE = 3,
    OPENNOW_STREAMER_QUEUE_FULL = 4,
    OPENNOW_STREAMER_CLOSED = 5,
    OPENNOW_STREAMER_NO_FRAME = 6,
    OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE = 7,
    OPENNOW_STREAMER_WRONG_THREAD = 8,
    OPENNOW_STREAMER_STALE_FRAME = 9,
    OPENNOW_STREAMER_RENDER_FAILED = 10,
    OPENNOW_STREAMER_SCENE_GRAPH_ACTIVE = 11,
    OPENNOW_STREAMER_FRAME_ALREADY_RECORDED = 12,
    OPENNOW_STREAMER_PANIC = 255
} OpenNowStreamerStatus;

OpenNowStreamerStatus opennow_streamer_create(
    const OpenNowStreamerConfig *config,
    OpenNowStreamer **output);

/* Points the embedded file log at a UTF-8 path (rotating past 2 MiB).
 * The Qt shell passes its diagnostics native-streamer.log here so packaged
 * builds log the video pipeline without spawning the legacy child streamer.
 * Additive since ABI v3; safe to call more than once. Never fails streaming. */
OpenNowStreamerStatus opennow_streamer_set_log_file(const char *path);

OpenNowStreamerStatus opennow_streamer_send(
    const OpenNowStreamer *handle,
    const uint8_t *bytes,
    size_t length);

OpenNowStreamerStatus opennow_streamer_submit_key(
    const OpenNowStreamer *handle,
    uint16_t virtual_key,
    uint16_t modifiers,
    bool pressed);

OpenNowStreamerStatus opennow_streamer_submit_mouse_relative(
    const OpenNowStreamer *handle,
    int16_t delta_x,
    int16_t delta_y);

OpenNowStreamerStatus opennow_streamer_submit_mouse_absolute(
    const OpenNowStreamer *handle,
    uint16_t x,
    uint16_t y,
    uint16_t width,
    uint16_t height);

OpenNowStreamerStatus opennow_streamer_submit_mouse_button(
    const OpenNowStreamer *handle,
    uint8_t button,
    bool pressed);

OpenNowStreamerStatus opennow_streamer_submit_mouse_wheel(
    const OpenNowStreamer *handle,
    int16_t delta_x,
    int16_t delta_y);

OpenNowStreamerStatus opennow_streamer_submit_gamepad(
    const OpenNowStreamer *handle,
    uint8_t controller_id,
    uint16_t bitmap,
    uint16_t buttons,
    uint8_t left_trigger,
    uint8_t right_trigger,
    int16_t left_stick_x,
    int16_t left_stick_y,
    int16_t right_stick_x,
    int16_t right_stick_y);

OpenNowStreamerStatus opennow_streamer_submit_local_action(
    const OpenNowStreamer *handle,
    uint32_t action);

OpenNowStreamerStatus opennow_streamer_set_capture_active(
    const OpenNowStreamer *handle,
    bool active,
    bool relative_mouse,
    uintptr_t window_handle,
    bool *raw_input_active);

OpenNowStreamerStatus opennow_streamer_set_graphics_context(
    const OpenNowStreamer *handle,
    const OpenNowStreamerGraphicsContext *context);

OpenNowStreamerStatus opennow_streamer_acquire_latest_frame(
    const OpenNowStreamer *handle,
    OpenNowStreamerFrame **output,
    OpenNowStreamerFrameInfo *info);

/*
 * Call after QRhi has opened its frame and supplied the native command buffer, but BEFORE
 * QQuickRhiItem begins its render pass. The conversion may open its own offscreen pass or encode
 * barriers, which is invalid inside the item's pass. After this returns, import/sample output's
 * GPU texture in the QQuickRhiItem pass on the same command stream.
 */
OpenNowStreamerStatus opennow_streamer_record_frame(
    const OpenNowStreamer *handle,
    const OpenNowStreamerFrame *frame,
    const OpenNowStreamerRecordCommand *command,
    OpenNowStreamerRecordedFrame *output);

OpenNowStreamerStatus opennow_streamer_release_frame(OpenNowStreamerFrame *frame);

/*
 * Call from the render thread during sceneGraphInvalidated/releaseResources, after releasing all
 * frame tokens and before QRhi destroys the native objects from the graphics context.
 */
OpenNowStreamerStatus opennow_streamer_scene_graph_shutdown(
    const OpenNowStreamer *handle);

OpenNowStreamerStatus opennow_streamer_destroy(OpenNowStreamer *handle);

#ifdef __cplusplus
}
#endif

#endif
