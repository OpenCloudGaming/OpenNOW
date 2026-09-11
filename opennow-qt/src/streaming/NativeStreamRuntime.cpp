#include "streaming/NativeStreamRuntime.h"
#include "diagnostics/DiagnosticsPaths.h"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QDateTime>
#include <QSysInfo>
#include <QJsonDocument>
#include <QJsonParseError>
#include <QMetaObject>
#include <QPointer>
#include <QQueue>
#include <QStandardPaths>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
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

// GUI-thread control-plane diagnostics only. Never log raw commands or callback
// payloads (they may contain session credentials), or write on each video frame.
void handshakeLog(const QString &detail)
{
    QDir directory(coreDiagnosticsDataRoot());
    if (!directory.mkpath(u"diagnostics"_s) || !directory.cd(u"diagnostics"_s)) return;
    const auto path = directory.filePath(u"qt-native.log"_s);
    QFile file(path);
    if (file.size() > 1024 * 1024) {
        QFile::remove(path + u".previous"_s);
        if (!QFile::rename(path, path + u".previous"_s)) return;
    }
    if (file.open(QIODevice::WriteOnly | QIODevice::Append))
        file.write((QString::number(QDateTime::currentMSecsSinceEpoch())
                    + u" qt-native "_s + detail + u"\n"_s).toUtf8());
}

QString handshakeSummary(const QJsonObject &object)
{
    QStringList fields;
    for (const auto *key : {"id", "type", "event", "status", "code", "protocolVersion", "phase"}) {
        const auto value = object.value(QLatin1String(key));
        const auto text = value.isString() ? value.toString()
                        : value.isDouble() ? QString::number(value.toDouble()) : QString{};
        if (!text.isEmpty() && text.size() <= 64
            && std::all_of(text.begin(), text.end(), [](QChar c) {
                return (c.isLetterOrNumber() && c.unicode() < 128)
                    || c == u'-' || c == u'_' || c == u'.';
            })) fields.append(QLatin1String(key) + u"="_s + text);
    }
    return fields.join(u' ');
}

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
        quint64 rumbleEpoch = 0;
    };

    std::mutex mutex;
    QQueue<Message> pending;
    QByteArray cursorCapture;
    QPointer<NativeStreamRuntime> target;
    int dropped = 0;
    bool accepting = true;
    bool drainScheduled = false;
    bool framePending = false;
    quint64 rumbleEpoch = 0;
};

struct NativeStreamRuntime::Private {
    explicit Private(Api runtimeApi)
        : api(runtimeApi)
    {
    }

    Api api;
    const OpenNowStreamerVulkanDevice *vulkanDevice = nullptr;
    quint64 windowsAdapterLuid = 0;
    // FFI input queues and render-thread graphics state are independent and thread-safe. An
    // exclusive mutex here made every GUI input/overlay call wait behind Media Foundation and
    // D3D work performed by the render thread. Keep lifetime/scene-graph transitions exclusive,
    // while ordinary commands, input, and frame recording share the live-handle lease.
    std::shared_mutex handleMutex;
    OpenNowStreamer *handle = nullptr;
    std::shared_ptr<CallbackState> callbackState;
    QString lastError;
    bool graphicsActive = false;
    bool firstNotification = false;
    std::atomic<quint64> presentationGeneration{0};
    std::atomic_bool presentationAllowed{false};
    std::atomic_bool inputAllowed{false};
    bool inputAuthorizationPending = false;
    bool serverCursorComposited = true;
    QString cursorStartId;
    QString presentationStartId;
    QString rumbleStartId;
    quint64 rumbleStartEpoch = 0;
};

NativeStreamRuntime::NativeStreamRuntime(QObject *parent,
                                         const OpenNowStreamerVulkanDevice *vulkanDevice,
                                         quint64 windowsAdapterLuid)
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
                              &opennow_streamer_set_log_file,
                              &opennow_streamer_submit_text}, parent, vulkanDevice, windowsAdapterLuid)
{
}

NativeStreamRuntime::NativeStreamRuntime(Api api, QObject *parent,
                                         const OpenNowStreamerVulkanDevice *vulkanDevice,
                                         quint64 windowsAdapterLuid)
    : QObject(parent)
    , d(std::make_unique<Private>(api))
{
    d->vulkanDevice = vulkanDevice;
    d->windowsAdapterLuid = windowsAdapterLuid;
}

const OpenNowStreamerVulkanDevice *NativeStreamRuntime::vulkanDevice() const
{
    return d->vulkanDevice;
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
        callbacks->cursorCapture.clear();
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

quint64 NativeStreamRuntime::presentationGeneration() const
{
    return d->presentationGeneration.load(std::memory_order_acquire);
}

bool NativeStreamRuntime::presentationAllowed() const
{
    return d->presentationAllowed.load(std::memory_order_acquire);
}

bool NativeStreamRuntime::inputAllowed() const
{
    return d->inputAllowed.load(std::memory_order_acquire);
}

void NativeStreamRuntime::invalidatePresentation()
{
    d->presentationAllowed.store(false, std::memory_order_release);
    d->presentationGeneration.fetch_add(1, std::memory_order_acq_rel);
    d->presentationStartId.clear();
    d->rumbleStartId.clear();
    d->cursorStartId.clear();
    emit controllerRumbleStopped();
    emit frameAvailable(); // Repaint even when the dead transport sends no more frames.
}

void NativeStreamRuntime::reportPresentationError(const QString &message)
{
    reportPresentationError(message, presentationGeneration());
}

void NativeStreamRuntime::reportPresentationError(const QString &message, quint64 generation)
{
    QMetaObject::invokeMethod(this, [this, message, generation] {
        if (generation != presentationGeneration()) return;
        resetInputCapture();
        invalidatePresentation();
        setLastError(message);
        qWarning("Stream presentation: %s", qUtf8Printable(message));
        emit presentationError(message);
    }, Qt::QueuedConnection);
}

void NativeStreamRuntime::initializeDiagnostics(Api::SetLogFile setLogFile)
{
    if (!setLogFile) return;
    const auto dataRoot = coreDiagnosticsDataRoot();
    if (dataRoot.isEmpty()) return;
    QDir directory(dataRoot);
    if (!directory.mkpath(u"diagnostics"_s) || !directory.cd(u"diagnostics"_s)) return;
    const auto logPath = directory.filePath(u"native-streamer.log"_s).toUtf8();
    setLogFile(logPath.constData());
}

bool NativeStreamRuntime::start()
{
    handshakeLog(u"runtime.start build=diagnostics-v2 app=%1 qt=%2 arch=%3 abi=%4"_s
        .arg(QCoreApplication::applicationVersion(), QString::fromLatin1(qVersion()),
             QSysInfo::currentCpuArchitecture()).arg(OPENNOW_STREAMER_FFI_ABI_VERSION));
    if (!d->api.create || !d->api.send || !d->api.destroy) {
        setLastError(u"The embedded streamer FFI is incomplete."_s);
        return false;
    }

    {
        const std::lock_guard lock(d->handleMutex);
        if (d->handle) return true;
    }

    initializeDiagnostics(d->api.setLogFile);

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
    config.vulkan_device = d->vulkanDevice;
    config.windows_adapter_luid = d->windowsAdapterLuid;

    OpenNowStreamer *handle = nullptr;
    const auto status = d->api.create(&config, &handle);
    handshakeLog(u"runtime.create result=%1 handle_present=%2"_s.arg(int(status)).arg(handle != nullptr));
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

bool NativeStreamRuntime::serverCursorComposited() const
{
    return d->serverCursorComposited;
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
    const auto object = QJsonDocument::fromJson(command).object();
    const auto type = object.value(u"type"_s).toString();
    const bool changesSession = type == u"start"_s || type == u"stop"_s;
    const bool previousInputAllowed = inputAllowed();
    const bool previousInputPending = d->inputAuthorizationPending;
    if (changesSession) resetInputCapture();

    OpenNowStreamerStatus status = OPENNOW_STREAMER_CLOSED;
    {
        const std::shared_lock lock(d->handleMutex);
        if (d->handle) {
            status = d->api.send(
                d->handle, reinterpret_cast<const std::uint8_t *>(command.constData()),
                static_cast<std::size_t>(command.size()));
        }
    }
    handshakeLog(u"send %1 bytes=%2 queue_result=%3"_s
        .arg(handshakeSummary(object)).arg(command.size()).arg(int(status)));
    if (status != OPENNOW_STREAMER_OK) {
        if (changesSession) {
            d->inputAuthorizationPending = previousInputPending;
            if (d->inputAllowed.exchange(previousInputAllowed, std::memory_order_acq_rel)
                    != previousInputAllowed)
                emit inputAllowedChanged();
        }
        setLastError(statusText(status));
        return false;
    }
    if (type == u"start"_s || type == u"stop"_s) {
        invalidatePresentation();
        if (type == u"start"_s) {
            d->serverCursorComposited = true;
            d->firstNotification = false;
            d->presentationStartId = object.value(u"id"_s).toString();
            d->cursorStartId = d->presentationStartId;
            d->rumbleStartId = d->presentationStartId;
            if (d->callbackState) {
                const std::lock_guard lock(d->callbackState->mutex);
                d->rumbleStartEpoch = d->callbackState->rumbleEpoch;
            }
            d->inputAuthorizationPending = true;
            emit cursorStateReset();
        }
    }
    setLastError({});
    return true;
}

bool NativeStreamRuntime::shutdown(int timeoutMs)
{
    resetInputCapture();
    invalidatePresentation();
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
    } else {
        if (recorded->graphics_api != OPENNOW_STREAMER_GRAPHICS_API_METAL) {
            info->width = recorded->width;
            info->height = recorded->height;
        }
        info->presentation_time_ns = recorded->presentation_time_ns;
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
    if (status == OPENNOW_STREAMER_OK || status == OPENNOW_STREAMER_RENDER_FAILED)
        d->graphicsActive = false;
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

OpenNowStreamerStatus NativeStreamRuntime::submitText(const QByteArray &text)
{
    if (text.size() > OPENNOW_STREAMER_MAX_TEXT_BYTES) return OPENNOW_STREAMER_MESSAGE_TOO_LARGE;
    if (text.isEmpty() || text.contains('\0') || !text.isValidUtf8())
        return OPENNOW_STREAMER_INVALID_CONFIG;
    const std::shared_lock lock(d->handleMutex);
    return inputAllowed() && d->handle && d->api.submitText
        ? d->api.submitText(d->handle,
                           reinterpret_cast<const std::uint8_t *>(text.constData()),
                           static_cast<std::size_t>(text.size()))
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
            ++state->rumbleEpoch;
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

    const QByteArray payload(reinterpret_cast<const char *>(bytes), static_cast<qsizetype>(length));
    bool cursorCapture = false;
    if (event && payload.size() <= 1024 && payload.contains("\"cursor-capture\"")) {
        const auto object = QJsonDocument::fromJson(payload).object();
        cursorCapture = object.value(u"type"_s) == u"cursor-capture"_s
            && object.value(u"composited"_s).isBool()
            && object.value(u"startId"_s).isString();
    }
    const auto sharedState = state->shared_from_this();
    bool shouldSchedule = false;
    {
        const std::lock_guard lock(state->mutex);
        if (!state->accepting) return;
        if (cursorCapture) {
            state->cursorCapture = payload;
        } else if (state->pending.size() >= MaximumPendingCallbacks) {
            ++state->dropped;
            ++state->rumbleEpoch;
            return;
        } else {
            state->pending.enqueue({payload, event, false, state->rumbleEpoch});
        }
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
    state->cursorCapture.clear();
}

void NativeStreamRuntime::drainCallbacks(const std::shared_ptr<CallbackState> &state)
{
    const QPointer<NativeStreamRuntime> guard(this);
    const auto current = [&] {
        return guard && state->target == this && d->callbackState == state;
    };
    if (!current()) return;
    QQueue<CallbackState::Message> messages;
    int dropped = 0;
    bool framePending = false;
    bool reschedule = false;
    quint64 rumbleEpoch = 0;
    {
        const std::lock_guard lock(state->mutex);
        constexpr qsizetype BatchSize = 64;
        if (!state->cursorCapture.isEmpty()) {
            messages.enqueue({std::exchange(state->cursorCapture, {}), true});
        }
        while (!state->pending.isEmpty() && messages.size() < BatchSize)
            messages.enqueue(state->pending.dequeue());
        dropped = std::exchange(state->dropped, 0);
        rumbleEpoch = state->rumbleEpoch;
        framePending = std::exchange(state->framePending, false);
        reschedule = !state->pending.isEmpty() || state->framePending;
        if (!reschedule) state->drainScheduled = false;
    }

    if (dropped > 0) {
        if (!d->rumbleStartId.isEmpty() && rumbleEpoch != d->rumbleStartEpoch) {
            emit controllerRumbleStopped();
            if (!current()) return;
        }
        handshakeLog(u"callback_queue dropped=%1"_s.arg(dropped));
        emit callbacksDropped(dropped);
        if (!current()) return;
    }
    if (framePending) {
        if (!d->firstNotification) {
            handshakeLog(u"first frame notification delivered to Qt GUI (not yet presentation)"_s);
            d->firstNotification = true;
        }
        emit frameAvailable();
        if (!current()) return;
    }
    while (!messages.isEmpty()) {
        if (!current()) return;
        const auto message = messages.dequeue();
        if (message.cursor) {
            emit cursorUpdated(message.bytes);
            continue;
        }
        QJsonParseError error;
        const auto document = QJsonDocument::fromJson(message.bytes, &error);
        if (error.error != QJsonParseError::NoError || !document.isObject()) {
            handshakeLog(u"callback rejected: malformed JSON bytes=%1"_s.arg(message.bytes.size()));
            setLastError(u"The embedded streamer callback contained malformed JSON."_s);
            continue;
        }
        const auto kind = document.object().value(u"type"_s).toString();
        const auto status = document.object().value(u"status"_s).toString();
        if (message.event && kind == u"cursor-capture"_s) {
            if (d->cursorStartId.isEmpty()
                || document.object().value(u"startId"_s).toString() != d->cursorStartId) continue;
            const auto composited = document.object().value(u"composited"_s);
            if (composited.isBool() && d->serverCursorComposited != composited.toBool()) {
                d->serverCursorComposited = composited.toBool();
                emit cursorCaptureChanged(d->serverCursorComposited);
                if (!current()) return;
            }
        }
        if (message.event && kind == u"controller-rumble"_s) {
            const auto object = document.object();
            if (message.rumbleEpoch != rumbleEpoch || !inputAllowed() || d->rumbleStartId.isEmpty()
                || object.value(u"startId"_s).toString() != d->rumbleStartId) {
                continue;
            }
            const auto controller = object.value(u"controllerId"_s).toDouble(-1);
            const auto low = object.value(u"lowFrequency"_s).toDouble(-1);
            const auto high = object.value(u"highFrequency"_s).toDouble(-1);
            const auto duration = object.value(u"durationMs"_s).toDouble(-1);
            const auto valid = [](double value, double maximum) {
                return value >= 0 && value <= maximum && std::floor(value) == value;
            };
            if (valid(controller, 3) && valid(low, 65535) && valid(high, 65535)
                && valid(duration, 65535) && duration > 0) {
                emit controllerRumbleRequested(static_cast<quint8>(controller),
                    static_cast<quint16>(low), static_cast<quint16>(high),
                    static_cast<quint32>(duration));
            }
            continue;
        }
        if (message.event && (kind == u"error"_s
                || (kind == u"status"_s && (status == u"stopped"_s || status == u"error"_s)))) {
            resetInputCapture();
            invalidatePresentation();
            if (!current()) return;
        } else if (!message.event && !d->presentationStartId.isEmpty()
                && document.object().value(u"id"_s).toString() == d->presentationStartId) {
            d->presentationStartId.clear();
            d->presentationAllowed.store(kind == u"ok"_s, std::memory_order_release);
            const bool allowed = std::exchange(d->inputAuthorizationPending, false) && kind == u"ok"_s;
            if (d->inputAllowed.exchange(allowed, std::memory_order_acq_rel) != allowed) {
                emit inputAllowedChanged();
                if (!current()) return;
            }
            emit frameAvailable();
            if (!current()) return;
        }
        if (kind != u"telemetry"_s && kind != u"stats"_s && kind != u"log"_s)
            handshakeLog(u"delivered %1 bytes=%2"_s.arg(handshakeSummary(document.object()))
                         .arg(message.bytes.size()));
        if (message.event)
            emit eventReceived(document.object());
        else
            emit responseReceived(document.object());
    }
    if (current() && reschedule) scheduleDrain(state);
}

void NativeStreamRuntime::resetInputCapture()
{
    const bool wasAllowed = d->inputAllowed.exchange(false, std::memory_order_acq_rel);
    d->inputAuthorizationPending = false;
    emit inputCaptureReset();
    bool rawInput = false;
    setCaptureActive(false, false, 0, &rawInput);
    if (wasAllowed) emit inputAllowedChanged();
}

void NativeStreamRuntime::setLastError(const QString &error)
{
    if (d->lastError == error) return;
    d->lastError = error;
    emit lastErrorChanged();
}
