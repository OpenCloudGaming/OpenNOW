#include "streaming/NativeStreamRuntime.h"
#include "streaming/rendering/StreamFramePacer.h"

#include <QElapsedTimer>
#include <QFile>
#include <QScopeGuard>
#include <QTemporaryDir>
#include <QJsonDocument>
#include <QSignalSpy>
#include <QTest>
#include <QThread>

#include <chrono>
#include <condition_variable>
#include <mutex>
#include <thread>
#include <utility>

namespace {
struct FakeRuntime {
    OpenNowStreamerConfig config{};
    int destroyDelayMs = 0;
};

int nextDestroyDelayMs = 0;
OpenNowStreamerStatus sendStatus = OPENNOW_STREAMER_OK;
std::mutex graphicsCallMutex;
std::condition_variable graphicsCallChanged;
bool recordCallEntered = false;
bool allowRecordCallToFinish = false;
bool inputCallFinished = false;

OpenNowStreamerStatus fakeCreate(const OpenNowStreamerConfig *config,
                                 OpenNowStreamer **output)
{
    if (!config || !output) return OPENNOW_STREAMER_NULL_POINTER;
    auto *runtime = new FakeRuntime{*config, nextDestroyDelayMs};
    *output = reinterpret_cast<OpenNowStreamer *>(runtime);
    return OPENNOW_STREAMER_OK;
}

OpenNowStreamerStatus fakeSend(const OpenNowStreamer *handle, const std::uint8_t *bytes,
                               std::size_t length)
{
    if (!handle || (!bytes && length)) return OPENNOW_STREAMER_NULL_POINTER;
    if (sendStatus != OPENNOW_STREAMER_OK) return sendStatus;
    const auto *runtime = reinterpret_cast<const FakeRuntime *>(handle);
    const QByteArray command(reinterpret_cast<const char *>(bytes),
                             static_cast<qsizetype>(length));
    std::thread callback([runtime, command] {
        const auto object = QJsonDocument::fromJson(command).object();
        const auto response = QJsonDocument(QJsonObject{{QStringLiteral("id"),
                                                          object.value(QStringLiteral("id"))},
                                                         {QStringLiteral("type"),
                                                          QStringLiteral("ok")}})
                                  .toJson(QJsonDocument::Compact);
        runtime->config.response_callback(
            reinterpret_cast<const std::uint8_t *>(response.constData()),
            static_cast<std::size_t>(response.size()), runtime->config.user_data);
        if (runtime->config.frame_available_callback)
            runtime->config.frame_available_callback(runtime->config.user_data);
        if (runtime->config.cursor_callback) {
            const std::uint8_t cursor[] = {0, 12, 0, 0, 0, 0, 0};
            runtime->config.cursor_callback(cursor, sizeof(cursor), runtime->config.user_data);
        }
    });
    callback.join();
    return OPENNOW_STREAMER_OK;
}

OpenNowStreamerStatus fakeDestroy(OpenNowStreamer *handle)
{
    if (!handle) return OPENNOW_STREAMER_NULL_POINTER;
    auto *runtime = reinterpret_cast<FakeRuntime *>(handle);
    std::this_thread::sleep_for(std::chrono::milliseconds(runtime->destroyDelayMs));
    delete runtime;
    return OPENNOW_STREAMER_OK;
}

NativeStreamRuntime::Api fakeApi()
{
    return {&fakeCreate, &fakeSend, &fakeDestroy};
}

OpenNowStreamerStatus fakeAcquireLatestFrame(
    const OpenNowStreamer *handle, OpenNowStreamerFrame **frame,
    OpenNowStreamerFrameInfo *info)
{
    if (!handle || !frame || !info) return OPENNOW_STREAMER_NULL_POINTER;
    *frame = reinterpret_cast<OpenNowStreamerFrame *>(new int(1));
    *info = OpenNowStreamerFrameInfo{1920, 1080, 1, 0};
    return OPENNOW_STREAMER_OK;
}

OpenNowStreamerStatus fakeRecordFrame(
    const OpenNowStreamer *handle, const OpenNowStreamerFrame *frame,
    const OpenNowStreamerRecordCommand *, OpenNowStreamerRecordedFrame *recorded)
{
    if (!handle || !frame || !recorded) return OPENNOW_STREAMER_NULL_POINTER;
    std::unique_lock lock(graphicsCallMutex);
    recordCallEntered = true;
    graphicsCallChanged.notify_all();
    graphicsCallChanged.wait(lock, [] { return allowRecordCallToFinish; });
    *recorded = OpenNowStreamerRecordedFrame{
        1, 0, OPENNOW_STREAMER_GRAPHICS_API_D3D11,
        OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8, OPENNOW_STREAMER_COLOR_SPACE_SDR709,
        1920, 1080, 0, 1, 0};
    return OPENNOW_STREAMER_OK;
}

OpenNowStreamerStatus fakeReleaseFrame(OpenNowStreamerFrame *frame)
{
    if (!frame) return OPENNOW_STREAMER_NULL_POINTER;
    delete reinterpret_cast<int *>(frame);
    return OPENNOW_STREAMER_OK;
}

OpenNowStreamerStatus fakeSubmitKey(
    const OpenNowStreamer *handle, std::uint16_t, std::uint16_t, bool)
{
    if (!handle) return OPENNOW_STREAMER_NULL_POINTER;
    {
        const std::lock_guard lock(graphicsCallMutex);
        inputCallFinished = true;
    }
    graphicsCallChanged.notify_all();
    return OPENNOW_STREAMER_OK;
}

NativeStreamRuntime::Api blockingGraphicsApi()
{
    auto api = fakeApi();
    api.acquireLatestFrame = &fakeAcquireLatestFrame;
    api.recordFrame = &fakeRecordFrame;
    api.releaseFrame = &fakeReleaseFrame;
    api.submitKey = &fakeSubmitKey;
    return api;
}
}

class NativeStreamRuntimeTest final : public QObject
{
    Q_OBJECT

private slots:
    void rumbleCallbacksValidateFieldsAndRejectReplacedSessions()
    {
        static OpenNowStreamerConfig callbackConfig;
        auto api = fakeApi();
        api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
            callbackConfig = *config;
            return fakeCreate(config, output);
        };
        NativeStreamRuntime runtime(api);
        QSignalSpy rumble(&runtime, &NativeStreamRuntime::controllerRumbleRequested);
        QSignalSpy stopped(&runtime, &NativeStreamRuntime::controllerRumbleStopped);
        QVERIFY(runtime.start());
        const auto start = [&](const QString &id) {
            return runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                                 {QStringLiteral("id"), id}});
        };
        const auto deliver = [](const QJsonObject &event) {
            const auto bytes = QJsonDocument(event).toJson(QJsonDocument::Compact);
            std::thread callback([bytes] {
                callbackConfig.event_callback(
                    reinterpret_cast<const std::uint8_t *>(bytes.constData()),
                    static_cast<std::size_t>(bytes.size()), callbackConfig.user_data);
            });
            callback.join();
        };
        QJsonObject event{{QStringLiteral("type"), QStringLiteral("controller-rumble")},
                          {QStringLiteral("startId"), QStringLiteral("first")},
                          {QStringLiteral("controllerId"), 3},
                          {QStringLiteral("lowFrequency"), 65535},
                          {QStringLiteral("highFrequency"), 12345},
                          {QStringLiteral("durationMs"), 65535}};
        QVERIFY(start(QStringLiteral("first")));
        QTRY_VERIFY(runtime.inputAllowed());
        bool guiThread = false;
        connect(&runtime, &NativeStreamRuntime::controllerRumbleRequested, &runtime,
                [&] { guiThread = QThread::currentThread() == runtime.thread(); });
        deliver(event);
        QTRY_COMPARE(rumble.size(), 1);
        QVERIFY(guiThread);
        QCOMPARE(rumble[0][0].value<quint8>(), quint8(3));
        QCOMPARE(rumble[0][1].value<quint16>(), quint16(65535));
        QCOMPARE(rumble[0][2].value<quint16>(), quint16(12345));
        QCOMPARE(rumble[0][3].value<quint32>(), quint32(65535));
        for (const auto &field : {QStringLiteral("controllerId"), QStringLiteral("lowFrequency"),
                                  QStringLiteral("highFrequency"), QStringLiteral("durationMs")}) {
            for (const auto &invalid : {QJsonValue(-1), QJsonValue(65536), QJsonValue(0.5),
                                        QJsonValue(QStringLiteral("2")), QJsonValue()}) {
                auto malformed = event;
                malformed[field] = invalid;
                deliver(malformed);
            }
        }
        auto invalidSlot = event;
        invalidSlot[QStringLiteral("controllerId")] = 4;
        deliver(invalidSlot);
        QCoreApplication::processEvents();
        QCOMPARE(rumble.size(), 1);
        deliver(event);
        QVERIFY(start(QStringLiteral("second")));
        QTRY_VERIFY(runtime.inputAllowed());
        QCOMPARE(rumble.size(), 1);
        const auto stopsBeforeStale = stopped.size();
        deliver(event);
        QCoreApplication::processEvents();
        QCOMPARE(stopped.size(), stopsBeforeStale);
        QCOMPARE(rumble.size(), 1);
        event[QStringLiteral("startId")] = QStringLiteral("second");
        event[QStringLiteral("lowFrequency")] = 0;
        event[QStringLiteral("highFrequency")] = 0;
        deliver(event);
        QTRY_COMPARE(rumble.size(), 2);
        QCOMPARE(rumble[1][1].value<quint16>(), quint16(0));
        QCOMPARE(rumble[1][2].value<quint16>(), quint16(0));
        QSignalSpy delivered(&runtime, &NativeStreamRuntime::eventReceived);
        for (int i = 0; i <= NativeStreamRuntime::MaximumPendingCallbacks; ++i)
            deliver(event);
        QTRY_COMPARE(stopped.size(), stopsBeforeStale + 1);
        deliver({{QStringLiteral("type"), QStringLiteral("drain-marker")}});
        QTRY_COMPARE(delivered.size(), 1);
        QCOMPARE(rumble.size(), 2);
        deliver(event);
        QTRY_COMPARE(rumble.size(), 3);
        deliver({{QStringLiteral("type"), QStringLiteral("status")},
                 {QStringLiteral("status"), QStringLiteral("error")}});
        QTRY_COMPARE(stopped.size(), stopsBeforeStale + 2);
        deliver(event);
        QCoreApplication::processEvents();
        QCOMPARE(rumble.size(), 3);
        QVERIFY(runtime.shutdown());
        QCOMPARE(stopped.size(), stopsBeforeStale + 3);
    }

    void metalUpscalingPreservesDecodedSourceGeometry()
    {
        auto api = fakeApi();
        api.acquireLatestFrame = &fakeAcquireLatestFrame;
        api.releaseFrame = &fakeReleaseFrame;
        api.recordFrame = [](const OpenNowStreamer *, const OpenNowStreamerFrame *,
                               const OpenNowStreamerRecordCommand *command,
                               OpenNowStreamerRecordedFrame *recorded) {
            if (command->version != OPENNOW_STREAMER_RENDER_COMMAND_VERSION
                || command->upscale_width != 2560 || command->upscale_height != 1440)
                return OPENNOW_STREAMER_INVALID_CONFIG;
            *recorded = {1, 0, OPENNOW_STREAMER_GRAPHICS_API_METAL,
                         OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8, OPENNOW_STREAMER_COLOR_SPACE_SDR709,
                         2560, 1440, command->frame_slot, 1, 123456789};
            return OPENNOW_STREAMER_OK;
        };
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        OpenNowStreamerRecordCommand command{};
        command.version = OPENNOW_STREAMER_RENDER_COMMAND_VERSION;
        command.struct_size = sizeof(command);
        command.upscale_width = 2560;
        command.upscale_height = 1440;
        OpenNowStreamerFrameInfo info{};
        OpenNowStreamerRecordedFrame recorded{};
        OpenNowStreamerFrame *frame = nullptr;
        QCOMPARE(runtime.recordLatestFrame(command, &info, &recorded, &frame), OPENNOW_STREAMER_OK);
        const auto release = qScopeGuard([&] { runtime.releaseFrame(frame); });
        QCOMPARE(info.width, 1920U);
        QCOMPARE(info.height, 1080U);
        QCOMPARE(info.sequence, 1U);
        QCOMPARE(info.presentation_time_ns, recorded.presentation_time_ns);
        QCOMPARE(recorded.width, 2560U);
        QCOMPARE(recorded.height, 1440U);
    }

    void recordedFrameMetadataReplacesProvisionalNotificationMetadata()
    {
        static std::uint64_t sequence;
        sequence = 0;
        auto api = fakeApi();
        api.acquireLatestFrame = [](const OpenNowStreamer *handle,
                                     OpenNowStreamerFrame **frame,
                                     OpenNowStreamerFrameInfo *info) {
            const auto status = fakeAcquireLatestFrame(handle, frame, info);
            if (status == OPENNOW_STREAMER_OK)
                *info = {1920, 1088, ++sequence, 1'000'000'000};
            return status;
        };
        api.recordFrame = [](const OpenNowStreamer *, const OpenNowStreamerFrame *,
                               const OpenNowStreamerRecordCommand *,
                               OpenNowStreamerRecordedFrame *recorded) {
            *recorded = {1, 0, OPENNOW_STREAMER_GRAPHICS_API_D3D11,
                         OPENNOW_STREAMER_TEXTURE_FORMAT_RGBA8, OPENNOW_STREAMER_COLOR_SPACE_SDR709,
                         1920, 1080, 0,
                         sequence, 1'000'000'000 + (sequence - 1) * 16'666'667};
            return OPENNOW_STREAMER_OK;
        };
        api.releaseFrame = &fakeReleaseFrame;
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        StreamFramePacer pacer;
        for (std::uint64_t index = 1; index <= 10; ++index) {
            OpenNowStreamerRecordCommand command{};
            OpenNowStreamerFrameInfo info{};
            OpenNowStreamerRecordedFrame recorded{};
            OpenNowStreamerFrame *frame = nullptr;
            QCOMPARE(runtime.recordLatestFrame(command, &info, &recorded, &frame),
                     OPENNOW_STREAMER_OK);
            const auto release = qScopeGuard([&] { runtime.releaseFrame(frame); });
            QVERIFY(frame);
            QCOMPARE(info.width, 1920U);
            QCOMPARE(info.height, 1080U);
            QCOMPARE(info.sequence, index);
            QCOMPARE(info.presentation_time_ns, recorded.presentation_time_ns);
            QCOMPARE(pacer.source(info.sequence, info.presentation_time_ns,
                                  (index - 1) * 16'666'667, 144),
                     index == 1 ? StreamFramePacer::Result::WarmingUp
                                : StreamFramePacer::Result::Interpolate);
        }
    }

    void bootstrapAndRuntimeShareTheInjectedDiagnosticsSink()
    {
        QTemporaryDir data;
        QVERIFY(data.isValid());
        const bool hadOverride = qEnvironmentVariableIsSet("OPENNOW_DATA_DIR");
        const auto oldOverride = qgetenv("OPENNOW_DATA_DIR");
        const auto restore = qScopeGuard([&] {
            if (hadOverride) qputenv("OPENNOW_DATA_DIR", oldOverride);
            else qunsetenv("OPENNOW_DATA_DIR");
        });
        qputenv("OPENNOW_DATA_DIR", data.path().toUtf8());
        static QStringList paths;
        static bool configuredBeforeCreate;
        paths.clear();
        configuredBeforeCreate = false;
        auto api = fakeApi();
        api.setLogFile = [](const char *path) {
            paths.append(QString::fromUtf8(path));
            return OPENNOW_STREAMER_OK;
        };
        api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
            configuredBeforeCreate = paths.size() == 2;
            return fakeCreate(config, output);
        };
        NativeStreamRuntime::initializeDiagnostics(api.setLogFile);
        QCOMPARE(paths, QStringList{data.filePath(QStringLiteral("diagnostics/native-streamer.log"))});
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        QVERIFY(configuredBeforeCreate);
        QCOMPARE(paths.size(), 2);
        QCOMPARE(paths.first(), paths.last());
        NativeStreamRuntime::initializeDiagnostics(nullptr);
        QCOMPARE(paths.size(), 2);
    }

    void passesOptionalVulkanOwnerAcrossRestarts()
    {
        static OpenNowStreamerConfig captured;
        auto api = fakeApi();
        api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
            captured = *config;
            return fakeCreate(config, output);
        };
        const auto *owner = reinterpret_cast<const OpenNowStreamerVulkanDevice *>(1);
        NativeStreamRuntime runtime(api, nullptr, owner);
        QCOMPARE(runtime.vulkanDevice(), owner);
        for (int attempt = 0; attempt < 2; ++attempt) {
            QVERIFY(runtime.start());
            QCOMPARE(captured.abi_version, OPENNOW_STREAMER_FFI_ABI_VERSION);
            QCOMPARE(captured.struct_size, sizeof(OpenNowStreamerConfig));
            QCOMPARE(captured.vulkan_device, owner);
            QVERIFY(runtime.shutdown());
        }
        NativeStreamRuntime fallback(api);
        QVERIFY(fallback.start());
        QVERIFY(!fallback.vulkanDevice());
        QVERIFY(!captured.vulkan_device);
    }

    void preservesWindowsAdapterSelectionAcrossRuntimeRestarts()
    {
        static OpenNowStreamerConfig captured;
        auto api = fakeApi();
        api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
            captured = *config;
            return fakeCreate(config, output);
        };
        NativeStreamRuntime selected(api, nullptr, nullptr, 0xffffffff00000042ULL);
        for (int attempt = 0; attempt < 2; ++attempt) {
            QVERIFY(selected.start());
            QCOMPARE(captured.windows_adapter_luid, 0xffffffff00000042ULL);
            QVERIFY(selected.shutdown());
        }
        NativeStreamRuntime automatic(api);
        QVERIFY(automatic.start());
        QCOMPARE(captured.windows_adapter_luid, 0ULL);
    }

    void rejectedSessionCommandsRestorePreviousInputAuthorization()
    {
        NativeStreamRuntime runtime(fakeApi());
        QVERIFY(runtime.start());
        sendStatus = OPENNOW_STREAMER_QUEUE_FULL;
        QVERIFY(!runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                               {QStringLiteral("id"), QStringLiteral("rejected-initial")}}));
        QVERIFY(!runtime.inputAllowed());
        sendStatus = OPENNOW_STREAMER_OK;
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("authorized-session")}}));
        QTRY_VERIFY(runtime.inputAllowed());
        QSignalSpy resets(&runtime, &NativeStreamRuntime::inputCaptureReset);
        QSignalSpy authorization(&runtime, &NativeStreamRuntime::inputAllowedChanged);
        for (const auto status : {OPENNOW_STREAMER_QUEUE_FULL, OPENNOW_STREAMER_CLOSED}) {
            for (const auto &type : {QStringLiteral("start"), QStringLiteral("stop")}) {
                sendStatus = status;
                QVERIFY(!runtime.send({{QStringLiteral("type"), type},
                                       {QStringLiteral("id"), QStringLiteral("rejected-command")}}));
                QVERIFY(runtime.presentationAllowed());
                QVERIFY(runtime.inputAllowed());
            }
        }
        QCOMPARE(resets.size(), 4);
        QCOMPARE(authorization.size(), 8);
    }

    void sessionTransitionsCloseInputBeforeSendingOrDestroying()
    {
        static QStringList calls;
        static OpenNowStreamerConfig callbackConfig;
        calls.clear();
        auto api = fakeApi();
        api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
            callbackConfig = *config;
            return fakeCreate(config, output);
        };
        api.setCaptureActive = [](const OpenNowStreamer *, bool active, bool,
                                  std::uintptr_t, bool *raw) {
            calls.append(active ? QStringLiteral("open") : QStringLiteral("close"));
            *raw = false;
            return OPENNOW_STREAMER_OK;
        };
        api.send = [](const OpenNowStreamer *handle, const std::uint8_t *bytes, std::size_t size) {
            calls.append(QStringLiteral("send"));
            return fakeSend(handle, bytes, size);
        };
        api.destroy = [](OpenNowStreamer *handle) {
            calls.append(QStringLiteral("destroy"));
            return fakeDestroy(handle);
        };
        NativeStreamRuntime runtime(api);
        connect(&runtime, &NativeStreamRuntime::inputCaptureReset, &runtime,
                [] { calls.append(QStringLiteral("release")); });
        QVERIFY(runtime.start());
        bool raw = false;
        for (const auto &type : {QStringLiteral("start"), QStringLiteral("stop")}) {
            calls.clear();
            QCOMPARE(runtime.setCaptureActive(true, false, 0, &raw), OPENNOW_STREAMER_OK);
            QVERIFY(runtime.send({{QStringLiteral("type"), type}}));
            QCOMPARE(calls, QStringList({QStringLiteral("open"), QStringLiteral("release"), QStringLiteral("close"),
                                        QStringLiteral("send")}));
        }
        calls.clear();
        const QByteArray terminal = R"({"type":"status","status":"error"})";
        callbackConfig.event_callback(reinterpret_cast<const std::uint8_t *>(terminal.constData()),
                                      terminal.size(), callbackConfig.user_data);
        QTRY_COMPARE(calls, QStringList({QStringLiteral("release"), QStringLiteral("close")}));
        calls.clear();
        QVERIFY(runtime.shutdown());
        QCOMPARE(calls, QStringList({QStringLiteral("release"), QStringLiteral("close"), QStringLiteral("destroy")}));
    }

    void init()
    {
        nextDestroyDelayMs = 0;
        sendStatus = OPENNOW_STREAMER_OK;
        const std::lock_guard lock(graphicsCallMutex);
        recordCallEntered = false;
        allowRecordCallToFinish = false;
        inputCallFinished = false;
    }

    void presentationFailuresAreMarshalledToTheQtThread()
    {
        NativeStreamRuntime runtime;
        QSignalSpy errors(&runtime, &NativeStreamRuntime::presentationError);
        std::thread render([&runtime] {
            runtime.reportPresentationError(QStringLiteral("Texture import failed"));
        });
        render.join();
        QCOMPARE(errors.size(), 0);
        QTRY_COMPARE(errors.size(), 1);
        QCOMPARE(runtime.lastError(), QStringLiteral("Texture import failed"));
    }

    void failedGraphicsRetirementStillAllowsRuntimeShutdown()
    {
        auto api = fakeApi();
        api.setGraphicsContext = [](const OpenNowStreamer *, const OpenNowStreamerGraphicsContext *) {
            return OPENNOW_STREAMER_OK;
        };
        api.sceneGraphShutdown = [](const OpenNowStreamer *) {
            return OPENNOW_STREAMER_RENDER_FAILED;
        };
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        OpenNowStreamerGraphicsContext context{};
        context.version = OPENNOW_STREAMER_GRAPHICS_CONTEXT_VERSION;
        context.struct_size = sizeof(context);
        QCOMPARE(runtime.setGraphicsContext(context), OPENNOW_STREAMER_OK);
        QCOMPARE(runtime.sceneGraphShutdown(), OPENNOW_STREAMER_RENDER_FAILED);
        QVERIFY(runtime.shutdown());
    }

    void presentationIsInvalidatedBetweenNativeSessions()
    {
        NativeStreamRuntime runtime(fakeApi());
        QVERIFY(runtime.start());
        QVERIFY(!runtime.presentationAllowed());
        const auto first = runtime.presentationGeneration();
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("start-1")}}));
        QVERIFY(runtime.presentationGeneration() > first);
        QVERIFY(!runtime.presentationAllowed());
        QTRY_VERIFY(runtime.presentationAllowed());
        const auto running = runtime.presentationGeneration();
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("stop")},
                              {QStringLiteral("id"), QStringLiteral("stop-1")}}));
        QVERIFY(!runtime.presentationAllowed());
        QVERIFY(runtime.presentationGeneration() > running);
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("start-2")}}));
        QVERIFY(!runtime.presentationAllowed());
        QTRY_VERIFY(runtime.presentationAllowed());
        runtime.reportPresentationError(QStringLiteral("device lost"));
        QTRY_VERIFY(!runtime.presentationAllowed());
        QVERIFY(runtime.shutdown());
    }

    void copiesAndMarshalsWorkerCallbacksToTheQtThread()
    {
        NativeStreamRuntime runtime(fakeApi());
        QSignalSpy responses(&runtime, &NativeStreamRuntime::responseReceived);
        QSignalSpy frames(&runtime, &NativeStreamRuntime::frameAvailable);
        QSignalSpy cursors(&runtime, &NativeStreamRuntime::cursorUpdated);
        QVERIFY(runtime.start());
        QVERIFY(runtime.send(QJsonObject{{QStringLiteral("id"), QStringLiteral("hello")},
                                         {QStringLiteral("type"), QStringLiteral("hello")}}));

        QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 1'000);
        QCOMPARE(responses.first().first().toJsonObject().value(QStringLiteral("id")).toString(),
                 QStringLiteral("hello"));
        QCOMPARE(responses.first().first().toJsonObject().value(QStringLiteral("type")).toString(),
                 QStringLiteral("ok"));
        QTRY_COMPARE_WITH_TIMEOUT(frames.size(), 1, 1'000);
        QTRY_COMPARE_WITH_TIMEOUT(cursors.size(), 1, 1'000);
        QCOMPARE(cursors.first().first().toByteArray(), QByteArray::fromHex("000c0000000000"));
        QVERIFY(runtime.shutdown());
    }

    void discardsPresentationErrorsFromAnEarlierSession()
    {
        NativeStreamRuntime runtime(fakeApi());
        QSignalSpy errors(&runtime, &NativeStreamRuntime::presentationError);
        QVERIFY(runtime.start());
        runtime.reportPresentationError(QStringLiteral("old device lost"));
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("new-session")}}));
        QTRY_VERIFY(runtime.presentationAllowed());
        QCOMPARE(errors.size(), 0);
        QVERIFY(runtime.lastError().isEmpty());
        QVERIFY(runtime.shutdown());
    }

    void cursorCompositionIsValidatedCachedAndResetForAcceptedStarts()
    {
        static OpenNowStreamerConfig callbacks;
        auto api = fakeApi();
        api.create = [](const OpenNowStreamerConfig *config, OpenNowStreamer **output) {
            callbacks = *config;
            return fakeCreate(config, output);
        };
        NativeStreamRuntime runtime(api);
        QSignalSpy composition(&runtime, &NativeStreamRuntime::cursorCaptureChanged);
        QSignalSpy resets(&runtime, &NativeStreamRuntime::cursorStateReset);
        QVERIFY(runtime.start());
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("initial")}}));
        resets.clear();
        QVERIFY(runtime.serverCursorComposited());
        const auto deliver = [](const QByteArray &bytes) {
            callbacks.event_callback(reinterpret_cast<const std::uint8_t *>(bytes.constData()),
                                     bytes.size(), callbacks.user_data);
        };
        deliver(R"({"type":"cursor-capture","startId":"initial","composited":false})");
        QTRY_VERIFY(!runtime.serverCursorComposited());
        QCOMPARE(composition.size(), 1);
        for (const auto &bytes : {R"({"type":"cursor-capture","startId":"initial"})",
                                 R"({"type":"cursor-capture","startId":"initial","composited":"true"})",
                                 R"({"type":"cursor-capture","startId":"initial","composited":1})",
                                 R"({"type":"cursor-capture","startId":"initial","composited":false})"}) {
            deliver(bytes);
        }
        QCoreApplication::processEvents();
        QVERIFY(!runtime.serverCursorComposited());
        QCOMPARE(composition.size(), 1);
        sendStatus = OPENNOW_STREAMER_QUEUE_FULL;
        const auto restore = qScopeGuard([] { sendStatus = OPENNOW_STREAMER_OK; });
        QVERIFY(!runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                               {QStringLiteral("id"), QStringLiteral("rejected")}}));
        QVERIFY(!runtime.serverCursorComposited());
        QCOMPARE(resets.size(), 0);
        sendStatus = OPENNOW_STREAMER_OK;
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("accepted")}}));
        QVERIFY(runtime.serverCursorComposited());
        QCOMPARE(resets.size(), 1);
        deliver(R"({"type":"cursor-capture","startId":"initial","composited":false})");
        QCoreApplication::processEvents();
        QVERIFY(runtime.serverCursorComposited());
        deliver(R"({"type":"cursor-capture","startId":"accepted","composited":false})");
        QTRY_VERIFY(!runtime.serverCursorComposited());
        deliver(R"({"type":"cursor-capture","startId":"accepted","composited":true})");
        QTRY_VERIFY(runtime.serverCursorComposited());
        QCOMPARE(composition.size(), 3);
        QSignalSpy dropped(&runtime, &NativeStreamRuntime::callbacksDropped);
        for (qsizetype i = 0; i < NativeStreamRuntime::MaximumPendingCallbacks + 1; ++i)
            deliver(R"({"type":"telemetry"})");
        deliver(R"({"type":"cursor-capture","startId":"accepted","composited":true})");
        deliver(R"({"type":"cursor-capture","startId":"accepted","composited":false})");
        QTRY_VERIFY(!runtime.serverCursorComposited());
        QCOMPARE(composition.size(), 4);
        QTRY_VERIFY(!dropped.isEmpty());
        QVERIFY(runtime.shutdown());
    }

    void lateRenderFailureKeepsItsOriginalSessionGeneration()
    {
        NativeStreamRuntime runtime(fakeApi());
        QSignalSpy errors(&runtime, &NativeStreamRuntime::presentationError);
        QVERIFY(runtime.start());
        const auto oldGeneration = runtime.presentationGeneration();
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("replacement-session")}}));
        QTRY_VERIFY(runtime.presentationAllowed());
        runtime.reportPresentationError(QStringLiteral("retired renderer"), oldGeneration);
        QCoreApplication::processEvents();
        QCOMPARE(errors.size(), 0);
        QVERIFY(runtime.presentationAllowed());
        QVERIFY(runtime.lastError().isEmpty());
        QVERIFY(runtime.shutdown());
    }

    void rejectedSessionCommandsPreservePresentation()
    {
        NativeStreamRuntime runtime(fakeApi());
        QVERIFY(runtime.start());
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("start")},
                              {QStringLiteral("id"), QStringLiteral("active-session")}}));
        QTRY_VERIFY(runtime.presentationAllowed());
        const auto generation = runtime.presentationGeneration();
        sendStatus = OPENNOW_STREAMER_QUEUE_FULL;
        for (const auto &type : {QStringLiteral("stop"), QStringLiteral("start")}) {
            QVERIFY(!runtime.send({{QStringLiteral("type"), type},
                                   {QStringLiteral("id"), QStringLiteral("rejected")}}));
            QVERIFY(runtime.presentationAllowed());
            QCOMPARE(runtime.presentationGeneration(), generation);
        }
        const auto oversized = QJsonDocument(QJsonObject{
            {QStringLiteral("type"), QStringLiteral("stop")},
            {QStringLiteral("padding"), QString(NativeStreamRuntime::MaximumCallbackBytes, 'x')}
        }).toJson(QJsonDocument::Compact);
        QVERIFY(!runtime.sendBytes(oversized));
        QVERIFY(runtime.presentationAllowed());
        QCOMPARE(runtime.presentationGeneration(), generation);
        QVERIFY(runtime.shutdown());
    }

    void discardsDrainedCallbacksWhenASignalRestartsTheRuntime()
    {
        NativeStreamRuntime runtime(fakeApi());
        QSignalSpy responses(&runtime, &NativeStreamRuntime::responseReceived);
        QSignalSpy cursors(&runtime, &NativeStreamRuntime::cursorUpdated);
        QVERIFY(runtime.start());
        bool restarted = false;
        connect(&runtime, &NativeStreamRuntime::frameAvailable, &runtime, [&] {
            if (restarted) return;
            restarted = true;
            QVERIFY(runtime.shutdown());
            QVERIFY(runtime.start());
        });
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("hello")},
                              {QStringLiteral("id"), QStringLiteral("old-runtime")}}));
        QTRY_VERIFY(restarted);
        QCOMPARE(responses.size(), 0);
        QCOMPARE(cursors.size(), 0);
        QVERIFY(runtime.send({{QStringLiteral("type"), QStringLiteral("hello")},
                              {QStringLiteral("id"), QStringLiteral("new-runtime")}}));
        QTRY_COMPARE(responses.size(), 1);
        QCOMPARE(responses.first().first().toJsonObject().value(QStringLiteral("id")).toString(),
                 QStringLiteral("new-runtime"));
        QTRY_COMPARE(cursors.size(), 1);
        QVERIFY(runtime.shutdown());
    }

    void callbackHandlerCanDeleteTheRuntime()
    {
        auto *runtime = new NativeStreamRuntime(fakeApi());
        QSignalSpy responses(runtime, &NativeStreamRuntime::responseReceived);
        QSignalSpy cursors(runtime, &NativeStreamRuntime::cursorUpdated);
        QVERIFY(runtime->start());
        connect(runtime, &NativeStreamRuntime::responseReceived, this, [&] {
            delete std::exchange(runtime, nullptr);
        });
        QVERIFY(runtime->send({{QStringLiteral("type"), QStringLiteral("hello")},
                               {QStringLiteral("id"), QStringLiteral("delete-runtime")}}));
        QTRY_VERIFY(!runtime);
        QCOMPARE(responses.size(), 1);
        QCOMPARE(cursors.size(), 0);
    }

    void boundsShutdownWhenTheFfiDestroyCallStalls()
    {
        nextDestroyDelayMs = 300;
        NativeStreamRuntime runtime(fakeApi());
        QVERIFY(runtime.start());

        QElapsedTimer timer;
        timer.start();
        QVERIFY(!runtime.shutdown(20));
        QVERIFY2(timer.elapsed() < 200, "shutdown blocked past its bounded deadline");
        QVERIFY(!runtime.running());
        QVERIFY(runtime.lastError().contains(QStringLiteral("Timed out")));
        QTest::qWait(350);
    }

    void inputDoesNotQueueBehindAStalledRenderCall()
    {
        NativeStreamRuntime runtime(blockingGraphicsApi());
        QVERIFY(runtime.start());
        OpenNowStreamerRecordCommand command{};
        OpenNowStreamerFrameInfo info{};
        OpenNowStreamerRecordedFrame recorded{};
        OpenNowStreamerFrame *frame = nullptr;
        OpenNowStreamerStatus recordStatus = OPENNOW_STREAMER_CLOSED;
        std::thread render([&] {
            recordStatus = runtime.recordLatestFrame(command, &info, &recorded, &frame);
        });
        bool recordStarted = false;
        {
            std::unique_lock lock(graphicsCallMutex);
            recordStarted = graphicsCallChanged.wait_for(
                lock, std::chrono::seconds(1), [] { return recordCallEntered; });
            if (!recordStarted) allowRecordCallToFinish = true;
        }
        graphicsCallChanged.notify_all();
        if (!recordStarted) {
            render.join();
            QVERIFY2(recordStarted, "graphics FFI call did not start");
            return;
        }

        OpenNowStreamerStatus inputStatus = OPENNOW_STREAMER_CLOSED;
        std::thread input([&] { inputStatus = runtime.submitKey(0x57, 0, true); });
        bool inputWasConcurrent = false;
        {
            std::unique_lock lock(graphicsCallMutex);
            inputWasConcurrent = graphicsCallChanged.wait_for(
                lock, std::chrono::milliseconds(200), [] { return inputCallFinished; });
            allowRecordCallToFinish = true;
        }
        graphicsCallChanged.notify_all();
        input.join();
        render.join();

        QVERIFY2(inputWasConcurrent, "gameplay input waited behind the graphics FFI call");
        QCOMPARE(inputStatus, OPENNOW_STREAMER_OK);
        QCOMPARE(recordStatus, OPENNOW_STREAMER_OK);
        QCOMPARE(runtime.releaseFrame(frame), OPENNOW_STREAMER_OK);
        QVERIFY(runtime.shutdown());
    }

    void roundTripsThroughTheLinkedRustFfi()
    {
        QTemporaryDir data;
        QVERIFY(data.isValid());
        const bool hadOverride = qEnvironmentVariableIsSet("OPENNOW_DATA_DIR");
        const auto oldOverride = qgetenv("OPENNOW_DATA_DIR");
        const auto restore = qScopeGuard([&] {
            if (hadOverride) qputenv("OPENNOW_DATA_DIR", oldOverride);
            else qunsetenv("OPENNOW_DATA_DIR");
        });
        qputenv("OPENNOW_DATA_DIR", data.path().toUtf8());
        NativeStreamRuntime runtime;
        QSignalSpy responses(&runtime, &NativeStreamRuntime::responseReceived);
        QVERIFY2(runtime.start(), qPrintable(runtime.lastError()));
        QVERIFY2(runtime.send(QJsonObject{{QStringLiteral("id"), QStringLiteral("abi-hello")},
                                          {QStringLiteral("type"), QStringLiteral("hello")},
                                          {QStringLiteral("protocolVersion"), 6}}),
                 qPrintable(runtime.lastError()));
        QTRY_VERIFY_WITH_TIMEOUT(!responses.isEmpty(), 5'000);
        QCOMPARE(responses.first().first().toJsonObject().value(QStringLiteral("id")).toString(),
                 QStringLiteral("abi-hello"));
        QVERIFY2(runtime.shutdown(), qPrintable(runtime.lastError()));
        QFile log(data.filePath(QStringLiteral("diagnostics/native-streamer.log")));
        QVERIFY2(log.open(QIODevice::ReadOnly), "native log must share the core diagnostics directory");
        const auto nativeText = log.readAll();
        QVERIFY(nativeText.contains("file log configured"));
        QVERIFY(nativeText.contains("qt-to-native id=abi-hello type=hello"));
        QVERIFY(nativeText.contains("native-to-qt id=abi-hello"));
        QFile qtLog(data.filePath(QStringLiteral("diagnostics/qt-native.log")));
        QVERIFY(qtLog.open(QIODevice::ReadOnly));
        const auto qtText = qtLog.readAll();
        QVERIFY(qtText.contains("build=diagnostics-v2"));
        QVERIFY(qtText.contains("send id=abi-hello type=hello"));
        QVERIFY(qtText.contains("delivered id=abi-hello"));
    }
};

QTEST_MAIN(NativeStreamRuntimeTest)
#include "tst_nativestreamruntime.moc"
