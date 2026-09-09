#pragma once

#include "opennow_streamer_ffi.h"

#include <QByteArray>
#include <QJsonObject>
#include <QObject>
#include <QString>

#include <cstddef>
#include <cstdint>
#include <memory>

class NativeStreamRuntime final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool running READ running NOTIFY runningChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY lastErrorChanged)

public:
    struct Api {
        using Create = OpenNowStreamerStatus (*)(const OpenNowStreamerConfig *,
                                                  OpenNowStreamer **);
        using SetLogFile = OpenNowStreamerStatus (*)(const char *);
        using Send = OpenNowStreamerStatus (*)(const OpenNowStreamer *,
                                                const std::uint8_t *, std::size_t);
        using Destroy = OpenNowStreamerStatus (*)(OpenNowStreamer *);
        using SetGraphicsContext = OpenNowStreamerStatus (*)(
            const OpenNowStreamer *, const OpenNowStreamerGraphicsContext *);
        using AcquireLatestFrame = OpenNowStreamerStatus (*)(
            const OpenNowStreamer *, OpenNowStreamerFrame **, OpenNowStreamerFrameInfo *);
        using RecordFrame = OpenNowStreamerStatus (*)(
            const OpenNowStreamer *, const OpenNowStreamerFrame *,
            const OpenNowStreamerRecordCommand *, OpenNowStreamerRecordedFrame *);
        using ReleaseFrame = OpenNowStreamerStatus (*)(OpenNowStreamerFrame *);
        using SceneGraphShutdown = OpenNowStreamerStatus (*)(const OpenNowStreamer *);
        using SubmitKey = OpenNowStreamerStatus (*)(const OpenNowStreamer *, std::uint16_t,
                                                     std::uint16_t, bool);
        using SubmitMouseRelative = OpenNowStreamerStatus (*)(const OpenNowStreamer *,
                                                               std::int16_t, std::int16_t);
        using SubmitMouseAbsolute = OpenNowStreamerStatus (*)(const OpenNowStreamer *,
                                                               std::uint16_t, std::uint16_t,
                                                               std::uint16_t, std::uint16_t);
        using SubmitMouseButton = OpenNowStreamerStatus (*)(const OpenNowStreamer *,
                                                             std::uint8_t, bool);
        using SubmitMouseWheel = OpenNowStreamerStatus (*)(const OpenNowStreamer *,
                                                            std::int16_t, std::int16_t);
        using SubmitGamepad = OpenNowStreamerStatus (*)(
            const OpenNowStreamer *, std::uint8_t, std::uint16_t, std::uint16_t,
            std::uint8_t, std::uint8_t, std::int16_t, std::int16_t, std::int16_t,
            std::int16_t);
        using SubmitLocalAction = OpenNowStreamerStatus (*)(const OpenNowStreamer *,
                                                             std::uint32_t);
        using SetCaptureActive = OpenNowStreamerStatus (*)(const OpenNowStreamer *, bool,
                                                            bool, std::uintptr_t, bool *);

        Create create = nullptr;
        Send send = nullptr;
        Destroy destroy = nullptr;
        SetGraphicsContext setGraphicsContext = nullptr;
        AcquireLatestFrame acquireLatestFrame = nullptr;
        RecordFrame recordFrame = nullptr;
        ReleaseFrame releaseFrame = nullptr;
        SceneGraphShutdown sceneGraphShutdown = nullptr;
        SubmitKey submitKey = nullptr;
        SubmitMouseRelative submitMouseRelative = nullptr;
        SubmitMouseAbsolute submitMouseAbsolute = nullptr;
        SubmitMouseButton submitMouseButton = nullptr;
        SubmitMouseWheel submitMouseWheel = nullptr;
        SubmitGamepad submitGamepad = nullptr;
        SubmitLocalAction submitLocalAction = nullptr;
        SetCaptureActive setCaptureActive = nullptr;
        SetLogFile setLogFile = nullptr;
        Send submitText = nullptr;
    };

    static constexpr int DefaultShutdownTimeoutMs = 1'500;
    static constexpr qsizetype MaximumCallbackBytes = 1024 * 1024;
    static constexpr qsizetype MaximumPendingCallbacks = 512;

    static void initializeDiagnostics(Api::SetLogFile setLogFile = &opennow_streamer_set_log_file);

    explicit NativeStreamRuntime(QObject *parent = nullptr,
                                 const OpenNowStreamerVulkanDevice *vulkanDevice = nullptr);
    explicit NativeStreamRuntime(Api api, QObject *parent = nullptr,
                                 const OpenNowStreamerVulkanDevice *vulkanDevice = nullptr);
    ~NativeStreamRuntime() override;

    [[nodiscard]] bool running() const;
    [[nodiscard]] QString lastError() const;
    [[nodiscard]] quint64 presentationGeneration() const;
    [[nodiscard]] bool presentationAllowed() const;
    [[nodiscard]] bool inputAllowed() const;
    [[nodiscard]] bool serverCursorComposited() const;
    [[nodiscard]] const OpenNowStreamerVulkanDevice *vulkanDevice() const;
    // Called at most once per presentation failure from the scene-graph thread.
    void reportPresentationError(const QString &message);
    void reportPresentationError(const QString &message, quint64 generation);

    Q_INVOKABLE bool start();
    Q_INVOKABLE bool send(const QJsonObject &command);
    bool sendBytes(const QByteArray &command);
    Q_INVOKABLE bool shutdown(int timeoutMs = DefaultShutdownTimeoutMs);

    OpenNowStreamerStatus setGraphicsContext(
        const OpenNowStreamerGraphicsContext &context);
    OpenNowStreamerStatus recordLatestFrame(
        const OpenNowStreamerRecordCommand &command,
        OpenNowStreamerFrameInfo *info,
        OpenNowStreamerRecordedFrame *recorded,
        OpenNowStreamerFrame **frame);
    OpenNowStreamerStatus releaseFrame(OpenNowStreamerFrame *frame);
    OpenNowStreamerStatus sceneGraphShutdown();
    OpenNowStreamerStatus submitKey(std::uint16_t virtualKey, std::uint16_t modifiers,
                                    bool pressed);
    OpenNowStreamerStatus submitText(const QByteArray &text);
    OpenNowStreamerStatus submitMouseRelative(std::int16_t deltaX, std::int16_t deltaY);
    OpenNowStreamerStatus submitMouseAbsolute(std::uint16_t x, std::uint16_t y,
                                              std::uint16_t width, std::uint16_t height);
    OpenNowStreamerStatus submitMouseButton(std::uint8_t button, bool pressed);
    OpenNowStreamerStatus submitMouseWheel(std::int16_t deltaX, std::int16_t deltaY);
    OpenNowStreamerStatus submitGamepad(std::uint8_t controllerId, std::uint16_t bitmap,
                                        std::uint16_t buttons, std::uint8_t leftTrigger,
                                        std::uint8_t rightTrigger, std::int16_t leftStickX,
                                        std::int16_t leftStickY, std::int16_t rightStickX,
                                        std::int16_t rightStickY);
    OpenNowStreamerStatus submitLocalAction(std::uint32_t action);
    OpenNowStreamerStatus setCaptureActive(bool active, bool relativeMouse,
                                           std::uintptr_t windowHandle,
                                           bool *rawInputActive);

signals:
    void inputCaptureReset();
    void inputAllowedChanged();
    void presentationError(const QString &message);
    void runningChanged();
    void lastErrorChanged();
    void responseReceived(const QJsonObject &response);
    void eventReceived(const QJsonObject &event);
    void frameAvailable();
    void cursorUpdated(const QByteArray &bytes);
    void cursorCaptureChanged(bool composited);
    void cursorStateReset();
    void controllerRumbleRequested(quint8 controllerId, quint16 lowFrequency,
                                   quint16 highFrequency, quint32 durationMs);
    void controllerRumbleStopped();
    void callbacksDropped(int count);

private:
    struct CallbackState;
    struct Private;

    static void responseCallback(const std::uint8_t *bytes, std::size_t length,
                                 void *userData);
    static void eventCallback(const std::uint8_t *bytes, std::size_t length,
                              void *userData);
    static void frameAvailableCallback(void *userData);
    static void cursorCallback(const std::uint8_t *bytes, std::size_t length,
                               void *userData);
    static void enqueueCallback(CallbackState *state, const std::uint8_t *bytes,
                                std::size_t length, bool event);
    static void enqueueFrameAvailable(CallbackState *state);
    static void scheduleDrain(const std::shared_ptr<CallbackState> &state);
    void drainCallbacks(const std::shared_ptr<CallbackState> &state);
    void setLastError(const QString &error);
    void invalidatePresentation();
    void resetInputCapture();

    std::unique_ptr<Private> d;
};
