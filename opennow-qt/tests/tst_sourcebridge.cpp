#include "app/SourceBridge.h"
#include "core/CoreClient.h"
#include "streaming/NativeStreamRuntime.h"

#include <QCoreApplication>
#include <QDir>
#include <QJsonDocument>
#include <QScopeGuard>
#include <QSignalSpy>
#include <QTest>

#include <algorithm>
#include <mutex>
#include <thread>

namespace {
struct FakeRuntime {
    OpenNowStreamerConfig config{};
};

FakeRuntime *activeRuntime = nullptr;
std::mutex commandMutex;
QList<QJsonObject> commands;
bool failStart = false;
bool inputBeforeOk = false;
bool okWithoutLease = false;
bool wrongLease = false;
QString lastStartId;

void deliver(const FakeRuntime *runtime, bool event, const QJsonObject &message)
{
    const auto bytes = QJsonDocument(message).toJson(QJsonDocument::Compact);
    std::thread callback([runtime, event, bytes] {
        const auto target = event ? runtime->config.event_callback : runtime->config.response_callback;
        target(reinterpret_cast<const std::uint8_t *>(bytes.constData()), static_cast<std::size_t>(bytes.size()),
               runtime->config.user_data);
    });
    callback.join();
}

OpenNowStreamerStatus fakeCreate(const OpenNowStreamerConfig *config, OpenNowStreamer **output)
{
    activeRuntime = new FakeRuntime{*config};
    *output = reinterpret_cast<OpenNowStreamer *>(activeRuntime);
    return OPENNOW_STREAMER_OK;
}

OpenNowStreamerStatus fakeSend(const OpenNowStreamer *handle, const std::uint8_t *bytes, std::size_t length)
{
    const auto *runtime = reinterpret_cast<const FakeRuntime *>(handle);
    const auto command = QJsonDocument::fromJson(
        QByteArray(reinterpret_cast<const char *>(bytes), static_cast<qsizetype>(length))).object();
    {
        const std::lock_guard lock(commandMutex);
        commands.append(command);
    }
    const auto id = command.value(QStringLiteral("id"));
    const auto type = command.value(QStringLiteral("type")).toString();
    if (type == QStringLiteral("media-offer")) {
        deliver(runtime, false, {{QStringLiteral("id"), id}, {QStringLiteral("type"), QStringLiteral("media-offer")},
            {QStringLiteral("offer"), QJsonObject{{QStringLiteral("offerId"), QStringLiteral("offer-1")}}},
            {QStringLiteral("runtimeCapabilities"), QJsonObject{{QStringLiteral("protocolVersion"), 8}}}});
    } else if (type == QStringLiteral("media-status")) {
        deliver(runtime, false, {{QStringLiteral("id"), id}, {QStringLiteral("type"), QStringLiteral("media-status")},
            {QStringLiteral("nativeIdle"), true}, {QStringLiteral("legacyActive"), false},
            {QStringLiteral("active"), QJsonValue::Null}});
    } else if (type == QStringLiteral("start") && failStart) {
        deliver(runtime, false, {{QStringLiteral("id"), id}, {QStringLiteral("type"), QStringLiteral("error")},
            {QStringLiteral("code"), QStringLiteral("worker-rejected")}});
    } else if (type == QStringLiteral("start")) {
        lastStartId = id.toString();
        if (inputBeforeOk)
            deliver(runtime, true, {{QStringLiteral("type"), QStringLiteral("input")}, {QStringLiteral("ready"), true},
                {QStringLiteral("startId"), id}, {QStringLiteral("leaseId"), QStringLiteral("lease-1")}});
        QJsonObject ok{{QStringLiteral("id"), id}, {QStringLiteral("type"), QStringLiteral("ok")},
            {QStringLiteral("transport"), QStringLiteral("provider-worker")}, {QStringLiteral("inputReady"), false}};
        if (!okWithoutLease) ok.insert(QStringLiteral("leaseId"), wrongLease ? QStringLiteral("other-lease") : QStringLiteral("lease-1"));
        deliver(runtime, false, ok);
    } else if (type == QStringLiteral("media-cancel-offer")) {
        deliver(runtime, false, {{QStringLiteral("id"), id}, {QStringLiteral("type"), QStringLiteral("ok")}});
    } else if (type == QStringLiteral("stop")) {
        deliver(runtime, false, {{QStringLiteral("id"), id}, {QStringLiteral("type"), QStringLiteral("ok")}});
        deliver(runtime, true, {{QStringLiteral("type"), QStringLiteral("status")}, {QStringLiteral("status"), QStringLiteral("stopped")},
            {QStringLiteral("startId"), lastStartId}, {QStringLiteral("leaseId"), QStringLiteral("lease-1")}});
    }
    return OPENNOW_STREAMER_OK;
}

OpenNowStreamerStatus fakeDestroy(OpenNowStreamer *handle)
{
    if (reinterpret_cast<FakeRuntime *>(handle) == activeRuntime) activeRuntime = nullptr;
    delete reinterpret_cast<FakeRuntime *>(handle);
    return OPENNOW_STREAMER_OK;
}

QList<QJsonObject> sent(const QString &type)
{
    const std::lock_guard lock(commandMutex);
    QList<QJsonObject> matches;
    for (const auto &command : commands)
        if (command.value(QStringLiteral("type")).toString() == type) matches.append(command);
    return matches;
}

int coreCounter(CoreClient &client, QSignalSpy &responses, const QString &method, const QString &key)
{
    const auto id = client.request(method);
    int value = -1;
    static_cast<void>(QTest::qWaitFor([&] {
        for (const auto &response : responses)
            if (response.at(0).toString() == id)
                value = response.at(1).toJsonObject().value(key).toInt();
        return value >= 0;
    }, 2'000));
    return value;
}
}

class SourceBridgeTest final : public QObject
{
    Q_OBJECT

private slots:
    void init()
    {
        const std::lock_guard lock(commandMutex);
        commands.clear();
        failStart = false;
        inputBeforeOk = false;
        okWithoutLease = false;
        wrongLease = false;
        lastStartId.clear();
    }

    void runsThePrivateSessionFlowWithoutPublishingPrivateData()
    {
        const auto previous = qgetenv("OPENNOW_TEST_SOURCES");
        const auto restore = qScopeGuard([previous] {
            if (previous.isNull()) qunsetenv("OPENNOW_TEST_SOURCES");
            else qputenv("OPENNOW_TEST_SOURCES", previous);
        });
        qputenv("OPENNOW_TEST_SOURCES", "1");
        CoreClient core;
        QSignalSpy coreResponses(&core, &CoreClient::responseReceived);
        QVERIFY(core.start(QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("opennow-fake-core"))));
        QTRY_COMPARE_WITH_TIMEOUT(core.state(), QStringLiteral("ready"), 2'000);

        NativeStreamRuntime::Api api;
        api.create = fakeCreate;
        api.send = fakeSend;
        api.destroy = fakeDestroy;
        NativeStreamRuntime runtime(api);
        QSignalSpy nativeResponses(&runtime, &NativeStreamRuntime::responseReceived);
        QVERIFY(runtime.start());

        SourceBridge bridge(core, runtime);
        QSignalSpy created(&bridge, &SourceBridge::created);
        QSignalSpy failed(&bridge, &SourceBridge::failed);
        const auto provider = QStringLiteral("org.opennow.example.provider");
        const QJsonObject intent{{QStringLiteral("scope"), QJsonValue::Null},
            {QStringLiteral("target"), QJsonObject{{QStringLiteral("game"), QStringLiteral("game-1")},
                                                   {QStringLiteral("variant"), QStringLiteral("default")}}},
            {QStringLiteral("catalogRevision"), QStringLiteral("rev-1")}};
        const auto createId = bridge.create(provider, intent);
        QVERIFY(!createId.isEmpty());
        QTRY_COMPARE_WITH_TIMEOUT(created.size(), 1, 2'000);
        QCOMPARE(created.first().at(0).toString(), createId);
        const auto reply = created.first().at(1).toJsonObject();
        QCOMPARE(reply.value(QStringLiteral("result")).toObject().value(QStringLiteral("sessionHandle")).toString(),
                 QStringLiteral("op-1"));
        const auto offers = sent(QStringLiteral("media-offer"));
        QCOMPARE(offers.size(), 1);
        QCOMPARE(offers.first().value(QStringLiteral("protocolVersion")).toInt(), 8);
        QVERIFY(offers.first().value(QStringLiteral("context")).toObject()
                    .value(QStringLiteral("localPolicy")).toObject().contains(QStringLiteral("maxBitrateMbps")));
        QCOMPARE(coreCounter(core, coreResponses, QStringLiteral("test.create-receipts"), QStringLiteral("receipts")), 1);

        const QJsonObject session{{QStringLiteral("account"), QJsonValue::Null}, {QStringLiteral("remoteId"), QStringLiteral("s1")}};
        const auto startId = bridge.start(provider, session, QStringLiteral("op-1"));
        QVERIFY(!startId.isEmpty());
        QVERIFY(bridge.start(provider, session, QStringLiteral("op-1")).isEmpty());
        QTRY_COMPARE_WITH_TIMEOUT(sent(QStringLiteral("start")).size(), 1, 2'000);
        const auto start = sent(QStringLiteral("start")).first();
        QCOMPARE(start.value(QStringLiteral("id")).toString(), startId);
        QCOMPARE(start.value(QStringLiteral("protocolVersion")).toInt(), 8);
        QCOMPARE(start.value(QStringLiteral("context")).toObject().value(QStringLiteral("lease")).toObject()
                     .value(QStringLiteral("leaseId")).toString(), QStringLiteral("lease-1"));
        QTRY_COMPARE_WITH_TIMEOUT(coreCounter(core, coreResponses, QStringLiteral("test.create-receipts"),
                                              QStringLiteral("receipts")), 2, 2'000);
        QCOMPARE(sent(QStringLiteral("media-offer")).size(), 2);
        const auto profile = bridge.activeProfile();
        QCOMPARE(profile.value(QStringLiteral("startId")).toString(), startId);
        QCOMPARE(profile.value(QStringLiteral("sourceId")).toString(), provider);
        QCOMPARE(profile.value(QStringLiteral("width")).toInt(), 320);
        QCOMPARE(profile.value(QStringLiteral("height")).toInt(), 240);
        QCOMPARE(profile.value(QStringLiteral("fps")).toInt(), 50);
        QCOMPARE(profile.value(QStringLiteral("codec")).toString(), QStringLiteral("h264"));
        QCOMPARE(profile.value(QStringLiteral("colorQuality")).toString(), QStringLiteral("8bit_420"));
        QCOMPARE(profile.value(QStringLiteral("colorRange")).toString(), QStringLiteral("limited"));
        const auto exposed = QJsonDocument(QJsonObject::fromVariantMap(profile)).toJson();
        for (const auto *privateText : {"bootstrap", "private", "versionRoot", "leaseId", "offerId", "note", "secretTag", "<b>"})
            QVERIFY2(!exposed.contains(privateText), privateText);
        QVERIFY(runtime.presentationAllowed());
        QVERIFY(!runtime.inputAllowed());
        const auto input = [&](const QString &lease, bool ready) {
            deliver(activeRuntime, true, {{QStringLiteral("type"), QStringLiteral("input")}, {QStringLiteral("ready"), ready},
                {QStringLiteral("startId"), startId}, {QStringLiteral("leaseId"), lease}});
            QTest::qWait(50);
        };
        input(QStringLiteral("lease-other"), true);
        QVERIFY(!runtime.inputAllowed());
        input(QStringLiteral("lease-1"), true);
        QTRY_VERIFY_WITH_TIMEOUT(runtime.inputAllowed(), 1'000);
        input(QStringLiteral("lease-1"), false);
        QTRY_VERIFY_WITH_TIMEOUT(!runtime.inputAllowed(), 1'000);

        QVERIFY(runtime.send({{QStringLiteral("id"), QStringLiteral("stop-1")}, {QStringLiteral("type"), QStringLiteral("stop")}}));
        QTRY_COMPARE_WITH_TIMEOUT(coreCounter(core, coreResponses, QStringLiteral("test.source-releases"),
                                              QStringLiteral("releases")), 1, 2'000);
        QVERIFY(bridge.activeProfile().isEmpty());
        QTRY_VERIFY_WITH_TIMEOUT(coreCounter(core, coreResponses, QStringLiteral("test.source-releases"),
                                             QStringLiteral("reconciles")) >= 1, 2'000);
        QVERIFY(!bridge.start(provider, session, QStringLiteral("op-1")).isEmpty());
        QTRY_COMPARE_WITH_TIMEOUT(sent(QStringLiteral("start")).size(), 2, 2'000);

        for (const auto &response : nativeResponses) {
            const auto object = response.first().toJsonObject();
            QVERIFY(object.value(QStringLiteral("type")).toString() != QStringLiteral("media-offer"));
            QVERIFY(object.value(QStringLiteral("type")).toString() != QStringLiteral("media-status"));
        }
        for (const auto &response : coreResponses) {
            const auto text = QJsonDocument(response.at(1).toJsonObject()).toJson();
            QVERIFY(!text.contains("private-bootstrap-secret") && !text.contains("localPolicy"));
        }
        QVERIFY(failed.isEmpty());
        QVERIFY(runtime.shutdown());
    }

    void acceptedProfileWhitelistsTechnicalFields()
    {
        const auto lease = [](const QJsonObject &video) {
            return QJsonObject{{QStringLiteral("media"), QJsonObject{{QStringLiteral("prepared"), QJsonObject{
                {QStringLiteral("bootstrap"), QStringLiteral("secret")},
                {QStringLiteral("accepted"), QJsonObject{{QStringLiteral("video"), video}}}}}}}};
        };
        QVERIFY(SourceBridge::acceptedVideoProfile(lease({{QStringLiteral("encoding"), QStringLiteral("vp9")},
            {QStringLiteral("width"), 320}, {QStringLiteral("height"), 240}})).isEmpty());
        QVERIFY(SourceBridge::acceptedVideoProfile(lease({{QStringLiteral("encoding"), QStringLiteral("av1-obu")},
            {QStringLiteral("width"), 0}, {QStringLiteral("height"), 240}})).isEmpty());
        const auto profile = SourceBridge::acceptedVideoProfile(lease({{QStringLiteral("encoding"), QStringLiteral("hevc-annex-b")},
            {QStringLiteral("width"), 1440}, {QStringLiteral("height"), 1080}, {QStringLiteral("fps"), 60.5},
            {QStringLiteral("bitDepth"), 12}, {QStringLiteral("chroma"), QStringLiteral("yuv420")},
            {QStringLiteral("color"), QJsonObject{{QStringLiteral("range"), QStringLiteral("Full Range!")}}}}));
        QCOMPARE(profile.value(QStringLiteral("codec")).toString(), QStringLiteral("h265"));
        QCOMPARE(profile.value(QStringLiteral("width")).toInt(), 1440);
        QVERIFY(!profile.contains(QStringLiteral("fps")));
        QVERIFY(!profile.contains(QStringLiteral("bitDepth")) && !profile.contains(QStringLiteral("colorQuality")));
        QVERIFY(!profile.contains(QStringLiteral("colorRange")));
        QCOMPARE(profile.size(), 3);
    }

    void createOffersAreReleased_data()
    {
        QTest::addColumn<QString>("outcome");
        QTest::newRow("success") << QStringLiteral("success");
        QTest::newRow("failure") << QStringLiteral("failure");
        QTest::newRow("cancel-before-offer") << QStringLiteral("cancel-before-offer");
        QTest::newRow("cancel-after-dispatch") << QStringLiteral("cancel-after-dispatch");
    }

    void createOffersAreReleased()
    {
        QFETCH(QString, outcome);
        const auto previous = qgetenv("OPENNOW_TEST_SOURCES");
        const auto restore = qScopeGuard([previous] {
            if (previous.isNull()) qunsetenv("OPENNOW_TEST_SOURCES");
            else qputenv("OPENNOW_TEST_SOURCES", previous);
        });
        qputenv("OPENNOW_TEST_SOURCES", "1");
        CoreClient core;
        QSignalSpy events(&core, &CoreClient::eventReceived);
        QVERIFY(core.start(QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("opennow-fake-core"))));
        QTRY_COMPARE_WITH_TIMEOUT(core.state(), QStringLiteral("ready"), 2'000);
        NativeStreamRuntime::Api api;
        api.create = fakeCreate;
        api.send = fakeSend;
        api.destroy = fakeDestroy;
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        SourceBridge bridge(core, runtime);
        QSignalSpy created(&bridge, &SourceBridge::created);
        QSignalSpy failed(&bridge, &SourceBridge::failed);
        const auto provider = outcome == QStringLiteral("failure") ? QStringLiteral("invalid")
            : outcome == QStringLiteral("cancel-after-dispatch") ? QStringLiteral("pending")
            : QStringLiteral("org.opennow.example.provider");
        const QJsonObject intent{{QStringLiteral("scope"), QJsonValue::Null},
            {QStringLiteral("target"), QJsonObject{{QStringLiteral("game"), QStringLiteral("game-1")},
                                                   {QStringLiteral("variant"), QStringLiteral("default")}}},
            {QStringLiteral("catalogRevision"), QStringLiteral("rev-1")}};
        const auto requestId = bridge.create(provider, intent);
        QVERIFY(!requestId.isEmpty());
        if (outcome == QStringLiteral("cancel-after-dispatch")) {
            QTRY_VERIFY_WITH_TIMEOUT(std::any_of(events.cbegin(), events.cend(), [](const auto &event) {
                return event.at(0).toString() == QStringLiteral("test.source-create-pending");
            }), 2'000);
        }
        if (outcome.startsWith(QStringLiteral("cancel-"))) QVERIFY(bridge.cancel(requestId));
        if (outcome == QStringLiteral("success")) {
            QTRY_COMPARE_WITH_TIMEOUT(created.size(), 1, 2'000);
            QCOMPARE(failed.size(), 0);
        } else {
            QTRY_COMPARE_WITH_TIMEOUT(failed.size(), 1, 2'000);
            QCOMPARE(failed.first().at(0).toString(), requestId);
            QCOMPARE(created.size(), 0);
        }
        QTRY_COMPARE_WITH_TIMEOUT(sent(QStringLiteral("media-cancel-offer")).size(), 1, 2'000);
        QCOMPARE(sent(QStringLiteral("media-cancel-offer")).first().value(QStringLiteral("offerId")).toString(),
                 QStringLiteral("offer-1"));
        QVERIFY(!bridge.cancel(requestId));
        QVERIFY(runtime.shutdown());
    }

    void rejectedNativeStartSignalsFailure_data()
    {
        QTest::addColumn<QString>("outcome");
        QTest::newRow("error") << QStringLiteral("error");
        QTest::newRow("missing-lease") << QStringLiteral("missing-lease");
        QTest::newRow("wrong-lease") << QStringLiteral("wrong-lease");
    }

    void cancelledPrepareReleasesOffer()
    {
        const auto previous = qgetenv("OPENNOW_TEST_SOURCES");
        const auto restore = qScopeGuard([previous] {
            if (previous.isNull()) qunsetenv("OPENNOW_TEST_SOURCES");
            else qputenv("OPENNOW_TEST_SOURCES", previous);
        });
        qputenv("OPENNOW_TEST_SOURCES", "1");
        CoreClient core;
        QSignalSpy events(&core, &CoreClient::eventReceived);
        QVERIFY(core.start(QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("opennow-fake-core"))));
        QTRY_COMPARE_WITH_TIMEOUT(core.state(), QStringLiteral("ready"), 2'000);
        NativeStreamRuntime::Api api;
        api.create = fakeCreate;
        api.send = fakeSend;
        api.destroy = fakeDestroy;
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        SourceBridge bridge(core, runtime);
        QSignalSpy failed(&bridge, &SourceBridge::failed);
        const QJsonObject session{{QStringLiteral("account"), QJsonValue::Null}, {QStringLiteral("remoteId"), QStringLiteral("s1")}};
        const auto startId = bridge.start(QStringLiteral("org.opennow.example.provider"), session, QStringLiteral("pending"));
        QVERIFY(!startId.isEmpty());
        QTRY_VERIFY_WITH_TIMEOUT(std::any_of(events.cbegin(), events.cend(), [](const auto &event) {
            return event.at(0).toString() == QStringLiteral("test.source-prepare-pending");
        }), 2'000);
        QVERIFY(bridge.cancel(startId));
        QTRY_COMPARE_WITH_TIMEOUT(failed.size(), 1, 2'000);
        QCOMPARE(failed.first().at(0).toString(), startId);
        QCOMPARE(failed.first().at(1).toString(), QStringLiteral("cancelled"));
        QCOMPARE(sent(QStringLiteral("media-cancel-offer")).size(), 1);
        QCOMPARE(sent(QStringLiteral("media-cancel-offer")).first().value(QStringLiteral("offerId")).toString(),
                 QStringLiteral("offer-1"));
        QVERIFY(sent(QStringLiteral("start")).isEmpty());
        QVERIFY(!bridge.cancel(startId));
        QVERIFY(runtime.shutdown());
    }

    void rejectedNativeStartSignalsFailure()
    {
        QFETCH(QString, outcome);
        const auto previous = qgetenv("OPENNOW_TEST_SOURCES");
        const auto restore = qScopeGuard([previous] {
            if (previous.isNull()) qunsetenv("OPENNOW_TEST_SOURCES");
            else qputenv("OPENNOW_TEST_SOURCES", previous);
        });
        qputenv("OPENNOW_TEST_SOURCES", "1");
        failStart = outcome == QStringLiteral("error");
        okWithoutLease = outcome == QStringLiteral("missing-lease");
        wrongLease = outcome == QStringLiteral("wrong-lease");
        CoreClient core;
        QSignalSpy coreResponses(&core, &CoreClient::responseReceived);
        QVERIFY(core.start(QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("opennow-fake-core"))));
        QTRY_COMPARE_WITH_TIMEOUT(core.state(), QStringLiteral("ready"), 2'000);
        NativeStreamRuntime::Api api;
        api.create = fakeCreate;
        api.send = fakeSend;
        api.destroy = fakeDestroy;
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        SourceBridge bridge(core, runtime);
        QSignalSpy failed(&bridge, &SourceBridge::failed);
        const auto provider = QStringLiteral("org.opennow.example.provider");
        const QJsonObject session{{QStringLiteral("account"), QJsonValue::Null}, {QStringLiteral("remoteId"), QStringLiteral("s1")}};
        const auto startId = bridge.start(provider, session, QStringLiteral("op-1"));
        QVERIFY(!startId.isEmpty());
        QTRY_COMPARE_WITH_TIMEOUT(failed.size(), 1, 2'000);
        QCOMPARE(failed.first().at(0).toString(), startId);
        QCOMPARE(failed.first().at(1).toString(), failStart ? QStringLiteral("worker-rejected") : QStringLiteral("invalid_source_lease"));
        QVERIFY(!failed.first().at(2).toString().isEmpty());
        QVERIFY(bridge.activeProfile().isEmpty());
        QCOMPARE(sent(QStringLiteral("media-cancel-offer")).size(), 1);
        QCOMPARE(coreCounter(core, coreResponses, QStringLiteral("test.create-receipts"), QStringLiteral("receipts")), 0);
        QVERIFY(!bridge.start(provider, session, QStringLiteral("op-1")).isEmpty());
        QVERIFY(runtime.shutdown());
    }

    void rejectedNativeStartDeclinesThePreparedLease()
    {
        const auto previous = qgetenv("OPENNOW_TEST_SOURCES");
        const auto restore = qScopeGuard([previous] {
            if (previous.isNull()) qunsetenv("OPENNOW_TEST_SOURCES");
            else qputenv("OPENNOW_TEST_SOURCES", previous);
        });
        qputenv("OPENNOW_TEST_SOURCES", "1");
        failStart = true;
        CoreClient core;
        QSignalSpy coreResponses(&core, &CoreClient::responseReceived);
        QVERIFY(core.start(QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("opennow-fake-core"))));
        QTRY_COMPARE_WITH_TIMEOUT(core.state(), QStringLiteral("ready"), 2'000);
        NativeStreamRuntime::Api api;
        api.create = fakeCreate;
        api.send = fakeSend;
        api.destroy = fakeDestroy;
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        SourceBridge bridge(core, runtime);
        const QJsonObject session{{QStringLiteral("account"), QJsonValue::Null}, {QStringLiteral("remoteId"), QStringLiteral("s1")}};
        QVERIFY(!bridge.start(QStringLiteral("org.opennow.example.provider"), session, QStringLiteral("op-1")).isEmpty());
        QTRY_COMPARE_WITH_TIMEOUT(sent(QStringLiteral("start")).size(), 1, 2'000);
        QTest::qWait(200);
        QCOMPARE(coreCounter(core, coreResponses, QStringLiteral("test.create-receipts"), QStringLiteral("receipts")), 0);
        QVERIFY(!bridge.start(QStringLiteral("org.opennow.example.provider"), session, QStringLiteral("op-1")).isEmpty());
        QVERIFY(runtime.shutdown());
    }

    void foreignLeaseIsNeverStarted()
    {
        const auto previous = qgetenv("OPENNOW_TEST_SOURCES");
        const auto restore = qScopeGuard([previous] {
            if (previous.isNull()) qunsetenv("OPENNOW_TEST_SOURCES");
            else qputenv("OPENNOW_TEST_SOURCES", previous);
        });
        qputenv("OPENNOW_TEST_SOURCES", "1");
        CoreClient core;
        QVERIFY(core.start(QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("opennow-fake-core"))));
        QTRY_COMPARE_WITH_TIMEOUT(core.state(), QStringLiteral("ready"), 2'000);
        NativeStreamRuntime::Api api;
        api.create = fakeCreate;
        api.send = fakeSend;
        api.destroy = fakeDestroy;
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        SourceBridge bridge(core, runtime);
        QSignalSpy failed(&bridge, &SourceBridge::failed);
        const QJsonObject other{{QStringLiteral("account"), QJsonValue::Null}, {QStringLiteral("remoteId"), QStringLiteral("s2")}};
        const auto startId = bridge.start(QStringLiteral("org.opennow.example.provider"), other, QStringLiteral("op-1"));
        QTRY_COMPARE_WITH_TIMEOUT(failed.size(), 1, 2'000);
        QCOMPARE(failed.first().at(0).toString(), startId);
        QCOMPARE(failed.first().at(1).toString(), QStringLiteral("invalid_source_lease"));
        QVERIFY(sent(QStringLiteral("start")).isEmpty());
        QVERIFY(runtime.shutdown());
    }
    void correlationIsExactForInputAcceptanceAndRetirement()
    {
        const auto previous = qgetenv("OPENNOW_TEST_SOURCES");
        const auto restore = qScopeGuard([previous] {
            if (previous.isNull()) qunsetenv("OPENNOW_TEST_SOURCES");
            else qputenv("OPENNOW_TEST_SOURCES", previous);
        });
        qputenv("OPENNOW_TEST_SOURCES", "1");
        CoreClient core;
        QSignalSpy coreResponses(&core, &CoreClient::responseReceived);
        QVERIFY(core.start(QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("opennow-fake-core"))));
        QTRY_COMPARE_WITH_TIMEOUT(core.state(), QStringLiteral("ready"), 2'000);
        NativeStreamRuntime::Api api;
        api.create = fakeCreate;
        api.send = fakeSend;
        api.destroy = fakeDestroy;
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        SourceBridge bridge(core, runtime);
        const auto provider = QStringLiteral("org.opennow.example.provider");
        const QJsonObject session{{QStringLiteral("account"), QJsonValue::Null}, {QStringLiteral("remoteId"), QStringLiteral("s1")}};
        const auto counter = [&](const QString &key) {
            return coreCounter(core, coreResponses,
                key == QStringLiteral("receipts") ? QStringLiteral("test.create-receipts") : QStringLiteral("test.source-releases"), key);
        };

        okWithoutLease = true;
        QVERIFY(!bridge.start(provider, session, QStringLiteral("op-1")).isEmpty());
        QTRY_COMPARE_WITH_TIMEOUT(sent(QStringLiteral("start")).size(), 1, 2'000);
        QTest::qWait(100);
        QCOMPARE(counter(QStringLiteral("receipts")), 0);
        QVERIFY(runtime.send({{QStringLiteral("id"), QStringLiteral("stop-0")}, {QStringLiteral("type"), QStringLiteral("stop")}}));
        QTest::qWait(100);

        okWithoutLease = false;
        inputBeforeOk = true;
        const auto startId = bridge.start(provider, session, QStringLiteral("op-1"));
        QVERIFY(!startId.isEmpty());
        QTRY_COMPARE_WITH_TIMEOUT(counter(QStringLiteral("receipts")), 1, 2'000);
        QTRY_VERIFY_WITH_TIMEOUT(runtime.inputAllowed(), 1'000);

        deliver(activeRuntime, true, {{QStringLiteral("type"), QStringLiteral("status")}, {QStringLiteral("status"), QStringLiteral("stopped")},
            {QStringLiteral("leaseId"), QStringLiteral("lease-1")}});
        deliver(activeRuntime, true, {{QStringLiteral("type"), QStringLiteral("status")}, {QStringLiteral("status"), QStringLiteral("stopped")},
            {QStringLiteral("startId"), QStringLiteral("legacy-start")}, {QStringLiteral("leaseId"), QStringLiteral("lease-1")}});
        QTest::qWait(200);
        QCOMPARE(counter(QStringLiteral("releases")), 0);

        QVERIFY(runtime.shutdown());
        QTest::qWait(200);
        QCOMPARE(counter(QStringLiteral("releases")), 0);
        QVERIFY(bridge.start(provider, session, QStringLiteral("op-1")).isEmpty());
        QVERIFY(runtime.start());
        QTRY_COMPARE_WITH_TIMEOUT(counter(QStringLiteral("releases")), 1, 2'000);
        QVERIFY(!bridge.start(provider, session, QStringLiteral("op-1")).isEmpty());
        QVERIFY(runtime.shutdown());
    }

    void failedReleaseIsRetainedAndRetried()
    {
        const auto previous = qgetenv("OPENNOW_TEST_SOURCES");
        const auto restore = qScopeGuard([previous] {
            if (previous.isNull()) qunsetenv("OPENNOW_TEST_SOURCES");
            else qputenv("OPENNOW_TEST_SOURCES", previous);
        });
        qputenv("OPENNOW_TEST_SOURCES", "1");
        CoreClient core;
        QSignalSpy coreResponses(&core, &CoreClient::responseReceived);
        QVERIFY(core.start(QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("opennow-fake-core"))));
        QTRY_COMPARE_WITH_TIMEOUT(core.state(), QStringLiteral("ready"), 2'000);
        NativeStreamRuntime::Api api;
        api.create = fakeCreate;
        api.send = fakeSend;
        api.destroy = fakeDestroy;
        NativeStreamRuntime runtime(api);
        QVERIFY(runtime.start());
        SourceBridge bridge(core, runtime);
        const auto provider = QStringLiteral("org.opennow.example.provider");
        const QJsonObject session{{QStringLiteral("account"), QJsonValue::Null}, {QStringLiteral("remoteId"), QStringLiteral("s1")}};
        QVERIFY(!bridge.start(provider, session, QStringLiteral("op-1")).isEmpty());
        QTRY_COMPARE_WITH_TIMEOUT(coreCounter(core, coreResponses, QStringLiteral("test.create-receipts"),
                                              QStringLiteral("receipts")), 1, 2'000);
        coreCounter(core, coreResponses, QStringLiteral("test.fail-next-release"), QStringLiteral("armed"));
        QVERIFY(runtime.send({{QStringLiteral("id"), QStringLiteral("stop-1")}, {QStringLiteral("type"), QStringLiteral("stop")}}));
        QTRY_COMPARE_WITH_TIMEOUT(coreCounter(core, coreResponses, QStringLiteral("test.source-releases"),
                                              QStringLiteral("releaseAttempts")), 1, 2'000);
        QCOMPARE(coreCounter(core, coreResponses, QStringLiteral("test.source-releases"), QStringLiteral("releases")), 0);
        QVERIFY(bridge.start(provider, session, QStringLiteral("op-1")).isEmpty());
        QTRY_COMPARE_WITH_TIMEOUT(coreCounter(core, coreResponses, QStringLiteral("test.source-releases"),
                                              QStringLiteral("releases")), 1, 4'000);
        QVERIFY(!bridge.start(provider, session, QStringLiteral("op-1")).isEmpty());
        QVERIFY(runtime.shutdown());
    }
};

QTEST_GUILESS_MAIN(SourceBridgeTest)
#include "tst_sourcebridge.moc"
