#include "NativeStreamRuntime.h"

#include <QCoreApplication>
#include <QDir>
#include <QJsonDocument>
#include <QJsonParseError>
#include <QMetaObject>
#include <QPointer>
#include <QQueue>
#include <QStandardPaths>

#include <algorithm>
#include <chrono>
#include <condition_variable>
#include <limits>
#include <mutex>
#include <shared_mutex>
#include <thread>
#include <utility>

using namespace Qt::StringLiterals;

namespace {
constexpr std::size_t CommandQueueCapacity = 128;
constexpr std::size_t ResponseQueueCapacity = 128;
constexpr std::size_t EventQueueCapacity = 256;

QString statusText(OpenNowStreamerStatus status)
{
    switch (status) {
    case OPENNOW_STREAMER_OK:
        return {};
    case OPENNOW_STREAMER_NULL_POINTER:
        return u"The embedded streamer rejected a null pointer."_s;
    case OPENNOW_STREAMER_INVALID_CONFIG:
        return u"The embedded streamer rejected its ABI configuration."_s;
    case OPENNOW_STREAMER_MESSAGE_TOO_LARGE:
        return u"The embedded streamer command is too large."_s;
    case OPENNOW_STREAMER_QUEUE_FULL:
        return u"The embedded streamer command queue is full."_s;
    case OPENNOW_STREAMER_CLOSED:
        return u"The embedded streamer is closed."_s;
    case OPENNOW_STREAMER_NO_FRAME:
        return u"The embedded streamer has no decoded frame available."_s;
    case OPENNOW_STREAMER_GRAPHICS_UNAVAILABLE:
        return u"The embedded streamer graphics context is unavailable."_s;
    case OPENNOW_STREAMER_WRONG_THREAD:
        return u"The embedded streamer graphics call ran on the wrong thread."_s;
    case OPENNOW_STREAMER_STALE_FRAME:
        return u"The embedded streamer rejected a stale frame."_s;
    case OPENNOW_STREAMER_RENDER_FAILED:
        return u"The embedded streamer could not record the frame."_s;
    case OPENNOW_STREAMER_SCENE_GRAPH_ACTIVE:
        return u"The embedded streamer scene graph is still active."_s;
    case OPENNOW_STREAMER_FRAME_ALREADY_RECORDED:
        return u"The embedded streamer frame was already recorded."_s;
    case OPENNOW_STREAMER_PANIC:
        return u"The embedded streamer caught an internal panic."_s;
    }
    return u"The embedded streamer returned an unknown status (%1)."_s
        .arg(static_cast<int>(status));
}
}

struct NativeStreamRuntime::CallbackState final
    : std::enable_shared_from_this<NativeStreamRuntime::CallbackState>
{
    struct Message {
        QByteArray bytes;
        bool event = false;
        bool cursor = false;
    };

    std::mutex mutex;
    QQueue<Message> pending;
    QPointer<NativeStreamRuntime> target;
    int dropped = 0;
    bool accepting = true;
    bool drainScheduled = false;
    bool framePending = false;
};

struct NativeStreamRuntime::Private {
    explicit Private(Api runtimeApi)
        : api(runtimeApi)
    {
    }

    Api api;
    // FFI input queues and render-thread graphics state are independent and thread-safe. An
    // exclusive mutex here made every GUI input/overlay call wait behind Media Foundation and
    // D3D work performed by the render thread. Keep lifetime/scene-graph transitions exclusive,
    // while ordinary commands, input, and frame recording share the live-handle lease.
    std::shared_mutex handleMutex;
    OpenNowStreamer *handle = nullptr;
    std::shared_ptr<CallbackState> callbackState;
    QString lastError;
    bool graphicsActive = false;
};

NativeStreamRuntime::NativeStreamRuntime(QObject *parent)
    : NativeStreamRuntime(Api{&opennow_streamer_create,
                              &opennow_streamer_send,
                              &opennow_streamer_destroy,
                              &opennow_streamer_set_graphics_context,
                              &opennow_streamer_acquire_latest_frame,
                              &opennow_streamer_record_frame,
                              &opennow_streamer_release_frame,
                              &opennow_streamer_scene_graph_shutdown,
                              &opennow_streamer_submit_key,
                              &opennow_streamer_submit_mouse_relative,
                              &opennow_streamer_submit_mouse_absolute,
                              &opennow_streamer_submit_mouse_button,
                              &opennow_streamer_submit_mouse_wheel,
                              &opennow_streamer_submit_gamepad,
                              &opennow_streamer_submit_local_action,
                              &opennow_streamer_set_capture_active,
                              &opennow_streamer_set_log_file}, parent)
{
}

NativeStreamRuntime::NativeStreamRuntime(Api api, QObject *parent)
    : QObject(parent)
    , d(std::make_unique<Private>(api))
{
}

NativeStreamRuntime::~NativeStreamRuntime()
{
    if (shutdown()) return;
    std::shared_ptr<CallbackState> callbacks;
    {
        const std::lock_guard lock(d->handleMutex);
        d->handle = nullptr;
        callbacks = std::exchange(d->callbackState, {});
    }
    if (!callbacks) return;
    {
        const std::lock_guard lock(callbacks->mutex);
        callbacks->accepting = false;
        callbacks->target.clear();
        callbacks->pending.clear();
        callbacks->framePending = false;
    }
    new std::shared_ptr<CallbackState>(std::move(callbacks));
}

bool NativeStreamRuntime::running() const
{
    const std::shared_lock lock(d->handleMutex);
    return d->handle != nullptr;
}

QString NativeStreamRuntime::lastError() const
{
    return d->lastError;
}

bool NativeStreamRuntime::start()
{
    if (!d->api.create || !d->api.send || !d->api.destroy) {
        setLastError(u"The embedded streamer FFI is incomplete."_s);
        return false;
    }

    {
        const std::lock_guard lock(d->handleMutex);
        if (d->handle) return true;
    }

    // Point the embedded file log at the diagnostics folder before creating
    // the engine. Packaged builds never spawn the legacy child streamer, so
    // without this the video pipeline logs nowhere.
    if (d->api.setLogFile) {
        const auto dataRoot =
            QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
        if (!dataRoot.isEmpty()) {
            QDir directory(dataRoot);
            if (directory.mkpath(u"diagnostics"_s) && directory.cd(u"diagnostics"_s)) {
                const auto logPath =
                    directory.filePath(u"native-streamer.log"_s).toUtf8();
                d->api.setLogFile(logPath.constData());
            }
        }
    }

    auto callbacks = std::make_shared<CallbackState>();
    callbacks->target = this;
    OpenNowStreamerConfig config{};
    config.abi_version = OPENNOW_STREAMER_FFI_ABI_VERSION;
    config.struct_size = sizeof(config);
    config.command_queue_capacity = CommandQueueCapacity;
    config.response_queue_capacity = ResponseQueueCapacity;
    config.event_queue_capacity = EventQueueCapacity;
    config.max_command_bytes = static_cast<std::size_t>(MaximumCallbackBytes);
    config.response_callback = &NativeStreamRuntime::responseCallback;
    config.event_callback = &NativeStreamRuntime::eventCallback;
    config.frame_available_callback = &NativeStreamRuntime::frameAvailableCallback;
    config.cursor_callback = &NativeStreamRuntime::cursorCallback;
    config.user_data = callbacks.get();

    OpenNowStreamer *handle = nullptr;
    const auto status = d->api.create(&config, &handle);
    if (status != OPENNOW_STREAMER_OK || !handle) {
        setLastError(status == OPENNOW_STREAMER_OK
                         ? u"The embedded streamer did not return a runtime handle."_s
                         : statusText(status));
        return false;
    }

    {
        const std::lock_guard lock(d->handleMutex);
        d->callbackState = std::move(callbacks);
        d->handle = handle;
    }
    setLastError({});
    emit runningChanged();
    return true;
}

bool NativeStreamRuntime::send(const QJsonObject &command)
{
    return sendBytes(QJsonDocument(command).toJson(QJsonDocument::Compact));
}

bool NativeStreamRuntime::sendBytes(const QByteArray &command)
{
    if (command.size() > MaximumCallbackBytes) {
        setLastError(statusText(OPENNOW_STREAMER_MESSAGE_TOO_LARGE));
        return false;
    }

    OpenNowStreamerStatus status = OPENNOW_STREAMER_CLOSED;
    {
        const std::shared_lock lock(d->handleMutex);
        if (d->handle) {
            status = d->api.send(
                d->handle, reinterpret_cast<const std::uint8_t *>(command.constData()),
                static_cast<std::size_t>(command.size()));
        }
    }
    if (status != OPENNOW_STREAMER_OK) {
        setLastError(statusText(status));
        return false;
    }
    setLastError({});
    return true;
}

bool NativeStreamRuntime::shutdown(int timeoutMs)
{
    OpenNowStreamer *handle = nullptr;
    std::shared_ptr<CallbackState> callbacks;
    bool graphicsActive = false;
    {
        const std::lock_guard lock(d->handleMutex);
        graphicsActive = d->handle && d->graphicsActive;
        if (!graphicsActive) {
            handle = std::exchange(d->handle, nullptr);
            callbacks = std::exchange(d->callbackState, {});
        }
    }
    if (graphicsActive) {
        setLastError(u"The embedded streamer scene graph must be released on its render "
                     "thread before shutdown."_s);
        return false;
    }
    if (!handle) return true;
    emit runningChanged();

    if (callbacks) {
        const std::lock_guard lock(callbacks->mutex);
        callbacks->accepting = false;
        callbacks->target.clear();
        callbacks->pending.clear();
        callbacks->dropped = 0;
        callbacks->framePending = false;
    }

    struct Completion {
        std::mutex mutex;
        std::condition_variable changed;
        bool finished = false;
        OpenNowStreamerStatus status = OPENNOW_STREAMER_CLOSED;
    };
    auto completion = std::make_shared<Completion>();
    std::thread destroyer([api = d->api, handle, callbacks = std::move(callbacks), completion] {
        const auto status = api.destroy(handle);
        {
            const std::lock_guard lock(completion->mutex);
            completion->status = status;
            completion->finished = true;
        }
        completion->changed.notify_one();
    });

    const auto boundedTimeout = std::clamp(timeoutMs, 0, 30'000);
    std::unique_lock completionLock(completion->mutex);
    const auto finished = completion->changed.wait_for(
        completionLock, std::chrono::milliseconds(boundedTimeout),
        [&completion] { return completion->finished; });
    if (!finished) {
        completionLock.unlock();
        destroyer.detach();
        setLastError(u"Timed out waiting for the embedded streamer to stop; cleanup continues "
                     "off the Qt thread."_s);
        return false;
    }
    const auto status = completion->status;
    completionLock.unlock();
    destroyer.join();
    if (status != OPENNOW_STREAMER_OK) {
        setLastError(statusText(status));
        return false;
    }
    setLastError({});
    return true;
}

OpenNowStreamerStatus NativeStreamRuntime::setGraphicsContext(
    const OpenNowStreamerGraphicsContext &context)
{
    const std::unique_lock lock(d->handleMutex);
    if (!d->handle || !d->api.setGraphicsContext) return OPENNOW_STREAMER_CLOSED;
    const auto status = d->api.setGraphicsContext(d->handle, &context);
    if (status == OPENNOW_STREAMER_OK) d->graphicsActive = true;
    return status;
}

OpenNowStreamerStatus NativeStreamRuntime::recordLatestFrame(
    const OpenNowStreamerRecordCommand &command,
    OpenNowStreamerFrameInfo *info,
    OpenNowStreamerRecordedFrame *recorded,
    OpenNowStreamerFrame **frame)
{
    if (!info || !recorded || !frame) return OPENNOW_STREAMER_NULL_POINTER;
    *info = {};
    *recorded = {};
    *frame = nullptr;
    const std::shared_lock lock(d->handleMutex);
    if (!d->handle || !d->api.acquireLatestFrame || !d->api.recordFrame
        || !d->api.releaseFrame) {
        return OPENNOW_STREAMER_CLOSED;
    }

    auto status = d->api.acquireLatestFrame(d->handle, frame, info);
    if (status != OPENNOW_STREAMER_OK) return status;
    if (!*frame) return OPENNOW_STREAMER_NULL_POINTER;
    status = d->api.recordFrame(d->handle, *frame, &command, recorded);
    if (status != OPENNOW_STREAMER_OK) {
        d->api.releaseFrame(std::exchange(*frame, nullptr));
    }
    return status;
}

OpenNowStreamerStatus NativeStreamRuntime::releaseFrame(OpenNowStreamerFrame *frame)
{
    if (!frame) return OPENNOW_STREAMER_NULL_POINTER;
    // A frame token owns its backing resources and the release ABI deliberately does not take the
    // runtime handle. Avoid serializing this cheap drop with decoder/render work.
    return d->api.releaseFrame ? d->api.releaseFrame(frame) : OPENNOW_STREAMER_CLOSED;
}

OpenNowStreamerStatus NativeStreamRuntime::sceneGraphShutdown()
{
    const std::unique_lock lock(d->handleMutex);
    if (!d->handle || !d->api.sceneGraphShutdown) return OPENNOW_STREAMER_CLOSED;
    const auto status = d->api.sceneGraphShutdown(d->handle);
    if (status == OPENNOW_STREAMER_OK) d->graphicsActive = false;
    return status;
}

OpenNowStreamerStatus NativeStreamRuntime::submitKey(std::uint16_t virtualKey,
                                                     std::uint16_t modifiers, bool pressed)
{
    const std::shared_lock lock(d->handleMutex);
    return d->handle && d->api.submitKey
        ? d->api.submitKey(d->handle, virtualKey, modifiers, pressed)
        : OPENNOW_STREAMER_CLOSED;
}

OpenNowStreamerStatus NativeStreamRuntime::submitMouseRelative(std::int16_t deltaX,
                                                               std::int16_t deltaY)
{
    const std::shared_lock lock(d->handleMutex);
    return d->handle && d->api.submitMouseRelative
        ? d->api.submitMouseRelative(d->handle, deltaX, deltaY)
        : OPENNOW_STREAMER_CLOSED;
}

OpenNowStreamerStatus NativeStreamRuntime::submitMouseAbsolute(
    std::uint16_t x, std::uint16_t y, std::uint16_t width, std::uint16_t height)
{
    const std::shared_lock lock(d->handleMutex);
    return d->handle && d->api.submitMouseAbsolute
        ? d->api.submitMouseAbsolute(d->handle, x, y, width, height)
        : OPENNOW_STREAMER_CLOSED;
}

OpenNowStreamerStatus NativeStreamRuntime::submitMouseButton(std::uint8_t button,
                                                             bool pressed)
{
    const std::shared_lock lock(d->handleMutex);
    return d->handle && d->api.submitMouseButton
        ? d->api.submitMouseButton(d->handle, button, pressed)
        : OPENNOW_STREAMER_CLOSED;
}

OpenNowStreamerStatus NativeStreamRuntime::submitMouseWheel(std::int16_t deltaX,
                                                            std::int16_t deltaY)
{
    const std::shared_lock lock(d->handleMutex);
    return d->handle && d->api.submitMouseWheel
        ? d->api.submitMouseWheel(d->handle, deltaX, deltaY)
        : OPENNOW_STREAMER_CLOSED;
}

OpenNowStreamerStatus NativeStreamRuntime::submitGamepad(
    std::uint8_t controllerId, std::uint16_t bitmap, std::uint16_t buttons,
    std::uint8_t leftTrigger, std::uint8_t rightTrigger, std::int16_t leftStickX,
    std::int16_t leftStickY, std::int16_t rightStickX, std::int16_t rightStickY)
{
    const std::shared_lock lock(d->handleMutex);
    return d->handle && d->api.submitGamepad
        ? d->api.submitGamepad(d->handle, controllerId, bitmap, buttons,
                               leftTrigger, rightTrigger, leftStickX, leftStickY,
                               rightStickX, rightStickY)
        : OPENNOW_STREAMER_CLOSED;
}

OpenNowStreamerStatus NativeStreamRuntime::submitLocalAction(std::uint32_t action)
{
    const std::shared_lock lock(d->handleMutex);
    return d->handle && d->api.submitLocalAction
        ? d->api.submitLocalAction(d->handle, action) : OPENNOW_STREAMER_CLOSED;
}

OpenNowStreamerStatus NativeStreamRuntime::setCaptureActive(
    bool active, bool relativeMouse, std::uintptr_t windowHandle, bool *rawInputActive)
{
    if (!rawInputActive) return OPENNOW_STREAMER_NULL_POINTER;
    *rawInputActive = false;
    const std::shared_lock lock(d->handleMutex);
    return d->handle && d->api.setCaptureActive
        ? d->api.setCaptureActive(d->handle, active, relativeMouse, windowHandle,
                                  rawInputActive)
        : OPENNOW_STREAMER_CLOSED;
}

void NativeStreamRuntime::responseCallback(const std::uint8_t *bytes, std::size_t length,
                                           void *userData)
{
    enqueueCallback(static_cast<CallbackState *>(userData), bytes, length, false);
}

void NativeStreamRuntime::eventCallback(const std::uint8_t *bytes, std::size_t length,
                                        void *userData)
{
    enqueueCallback(static_cast<CallbackState *>(userData), bytes, length, true);
}

void NativeStreamRuntime::frameAvailableCallback(void *userData)
{
    enqueueFrameAvailable(static_cast<CallbackState *>(userData));
}

void NativeStreamRuntime::cursorCallback(const std::uint8_t *bytes, std::size_t length,
                                         void *userData)
{
    auto *state = static_cast<CallbackState *>(userData);
    if (!state || (!bytes && length != 0)
        || length > static_cast<std::size_t>(MaximumCallbackBytes)
        || length > static_cast<std::size_t>(std::numeric_limits<qsizetype>::max())) {
        return;
    }
    const auto sharedState = state->shared_from_this();
    bool shouldSchedule = false;
    {
        const std::lock_guard lock(state->mutex);
        if (!state->accepting) return;
        if (state->pending.size() >= MaximumPendingCallbacks) {
            ++state->dropped;
            return;
        }
        state->pending.enqueue(
            {QByteArray(reinterpret_cast<const char *>(bytes),
                        static_cast<qsizetype>(length)), false, true});
        if (!state->drainScheduled) {
            state->drainScheduled = true;
            shouldSchedule = true;
        }
    }
    if (shouldSchedule) scheduleDrain(sharedState);
}

void NativeStreamRuntime::enqueueCallback(CallbackState *state, const std::uint8_t *bytes,
                                          std::size_t length, bool event)
{
    if (!state || (!bytes && length != 0)
        || length > static_cast<std::size_t>(MaximumCallbackBytes)
        || length > static_cast<std::size_t>(std::numeric_limits<qsizetype>::max())) {
        return;
    }

    const auto sharedState = state->shared_from_this();
    bool shouldSchedule = false;
    {
        const std::lock_guard lock(state->mutex);
        if (!state->accepting) return;
        if (state->pending.size() >= MaximumPendingCallbacks) {
            ++state->dropped;
            return;
        }
        state->pending.enqueue(
            {QByteArray(reinterpret_cast<const char *>(bytes), static_cast<qsizetype>(length)),
             event});
        if (!state->drainScheduled) {
            state->drainScheduled = true;
            shouldSchedule = true;
        }
    }
    if (shouldSchedule) scheduleDrain(sharedState);
}

void NativeStreamRuntime::enqueueFrameAvailable(CallbackState *state)
{
    if (!state) return;
    const auto sharedState = state->shared_from_this();
    bool shouldSchedule = false;
    {
        const std::lock_guard lock(state->mutex);
        if (!state->accepting) return;
        state->framePending = true;
        if (!state->drainScheduled) {
            state->drainScheduled = true;
            shouldSchedule = true;
        }
    }
    if (shouldSchedule) scheduleDrain(sharedState);
}

void NativeStreamRuntime::scheduleDrain(const std::shared_ptr<CallbackState> &state)
{
    auto *application = QCoreApplication::instance();
    if (application && QMetaObject::invokeMethod(
            application,
            [state] {
                if (auto *target = state->target.data()) target->drainCallbacks(state);
            },
            Qt::QueuedConnection)) {
        return;
    }
    const std::lock_guard lock(state->mutex);
    state->drainScheduled = false;
    state->pending.clear();
}

void NativeStreamRuntime::drainCallbacks(const std::shared_ptr<CallbackState> &state)
{
    QQueue<CallbackState::Message> messages;
    int dropped = 0;
    bool framePending = false;
    bool reschedule = false;
    {
        const std::lock_guard lock(state->mutex);
        constexpr qsizetype BatchSize = 64;
        while (!state->pending.isEmpty() && messages.size() < BatchSize)
            messages.enqueue(state->pending.dequeue());
        dropped = std::exchange(state->dropped, 0);
        framePending = std::exchange(state->framePending, false);
        reschedule = !state->pending.isEmpty() || state->framePending;
        if (!reschedule) state->drainScheduled = false;
    }

    if (dropped > 0) emit callbacksDropped(dropped);
    if (framePending) emit frameAvailable();
    while (!messages.isEmpty()) {
        const auto message = messages.dequeue();
        if (message.cursor) {
            emit cursorUpdated(message.bytes);
            continue;
        }
        QJsonParseError error;
        const auto document = QJsonDocument::fromJson(message.bytes, &error);
        if (error.error != QJsonParseError::NoError || !document.isObject()) {
            setLastError(u"The embedded streamer callback contained malformed JSON."_s);
            continue;
        }
        if (message.event)
            emit eventReceived(document.object());
        else
            emit responseReceived(document.object());
    }
    if (reschedule) scheduleDrain(state);
}

void NativeStreamRuntime::setLastError(const QString &error)
{
    if (d->lastError == error) return;
    d->lastError = error;
    emit lastErrorChanged();
}
