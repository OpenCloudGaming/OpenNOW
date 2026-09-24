#include "core/CoreClient.h"
#include "media/MediaPaths.h"

#include <QSignalSpy>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QElapsedTimer>
#include <QRegularExpression>
#include <QScopeGuard>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>
#include <algorithm>
#include <utility>

namespace {
QString fakeCorePath()
{
    auto path = QDir(QCoreApplication::applicationDirPath()).filePath(
        QStringLiteral("opennow-fake-core"));
#ifdef Q_OS_WIN
    path += QStringLiteral(".exe");
#endif
    return path;
}
}

class CoreClientTest final : public QObject
{
    Q_OBJECT

private slots:
    void acknowledgesOnlyAcceptedCreateResponses()
    {
        CoreClient client;
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        responses.clear();
        const auto accepted = client.request(QStringLiteral("session.create"));
        QTRY_VERIFY_WITH_TIMEOUT(std::any_of(responses.begin(), responses.end(), [&](const auto &response) {
            return response.at(0).toString() == accepted;
        }), 2'000);
        const auto cancelled = client.request(QStringLiteral("session.create"), {{QStringLiteral("delayReceipt"), true}});
        QVERIFY(client.cancel(cancelled));
        const auto query = client.request(QStringLiteral("test.create-receipts"));
        QTRY_VERIFY_WITH_TIMEOUT(std::any_of(responses.begin(), responses.end(), [&](const auto &response) {
            return response.at(0).toString() == query;
        }), 2'000);
        for (const auto &response : responses) {
            QVERIFY(response.at(0).toString() != cancelled);
            if (response.at(0).toString() == query)
                QCOMPARE(response.at(1).toJsonObject().value(QStringLiteral("receipts")).toInt(), 1);
        }
    }

#ifdef Q_OS_LINUX
    void passesResolvedPicturesRootToCore_data()
    {
        QTest::addColumn<QByteArray>("flatpakId");
        QTest::addColumn<QByteArray>("picturesOverride");
        const QByteArray opennowFlatpakId("io.github.opencloudgaming.OpenNOW");
        const QByteArray explicitOverride("/custom/captures");
        const QByteArray emptyOverride("");
        QTest::newRow("native-xdg-pictures") << QByteArray{} << QByteArray{};
        QTest::newRow("native-explicit-override") << QByteArray{} << explicitOverride;
        QTest::newRow("native-empty-override") << QByteArray{} << emptyOverride;
        QTest::newRow("flatpak-xdg-pictures") << opennowFlatpakId << QByteArray{};
        QTest::newRow("flatpak-explicit-override") << opennowFlatpakId << explicitOverride;
        QTest::newRow("flatpak-empty-override") << opennowFlatpakId << emptyOverride;
    }

    void passesResolvedPicturesRootToCore()
    {
        QFETCH(QByteArray, flatpakId);
        QFETCH(QByteArray, picturesOverride);
        const auto previousFlatpakId = qgetenv("FLATPAK_ID");
        const auto previousPictures = qgetenv("OPENNOW_PICTURES_DIR");
        const auto previousConfig = qgetenv("XDG_CONFIG_HOME");
        const auto restoreEnvironment = qScopeGuard([&] {
            for (const auto &[name, value] : {
                     std::pair{"FLATPAK_ID", previousFlatpakId},
                     std::pair{"OPENNOW_PICTURES_DIR", previousPictures},
                     std::pair{"XDG_CONFIG_HOME", previousConfig}}) {
                if (value.isNull()) qunsetenv(name);
                else qputenv(name, value);
            }
        });
        QTemporaryDir config;
        QVERIFY(config.isValid());
        const auto customPictures = config.filePath(QStringLiteral("Bilder und Aufnahmen"));
        QFile userDirs(config.filePath(QStringLiteral("user-dirs.dirs")));
        QVERIFY(userDirs.open(QIODevice::WriteOnly));
        const auto content = "XDG_PICTURES_DIR=\"" + customPictures.toUtf8() + "\"\n";
        QCOMPARE(userDirs.write(content), content.size());
        userDirs.close();
        qputenv("XDG_CONFIG_HOME", config.path().toUtf8());
        if (flatpakId.isNull()) qunsetenv("FLATPAK_ID");
        else qputenv("FLATPAK_ID", flatpakId);
        if (picturesOverride.isNull()) qunsetenv("OPENNOW_PICTURES_DIR");
        else qputenv("OPENNOW_PICTURES_DIR", picturesOverride);
        QCOMPARE(QStandardPaths::writableLocation(QStandardPaths::PicturesLocation), customPictures);

        const auto expected = picturesOverride.isNull() ? customPictures
                                                         : QString::fromUtf8(picturesOverride);
        QCOMPARE(mediaPicturesRoot(), expected);
        if (!expected.isEmpty()) QVERIFY(QDir::isAbsolutePath(expected));

        CoreClient client;
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        responses.clear();
        client.request(QStringLiteral("test.app-context"));
        QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 2'000);
        const auto context = qvariant_cast<QJsonObject>(responses.first().at(1));
        QCOMPARE(context.value(QStringLiteral("picturesDirectory")).toString(), expected);
        QCOMPARE(context.value(QStringLiteral("hasPicturesDirectory")).toBool(), true);
    }

    void reappliesPicturesRootWhenCoreRestarts()
    {
        QTemporaryDir firstOverride;
        QTemporaryDir secondOverride;
        QVERIFY(firstOverride.isValid() && secondOverride.isValid());
        const auto previousPictures = qgetenv("OPENNOW_PICTURES_DIR");
        const auto restoreEnvironment = qScopeGuard([&] {
            if (previousPictures.isNull()) qunsetenv("OPENNOW_PICTURES_DIR");
            else qputenv("OPENNOW_PICTURES_DIR", previousPictures);
        });
        qputenv("OPENNOW_PICTURES_DIR", firstOverride.path().toUtf8());

        CoreClient client;
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QSignalSpy failures(&client, &CoreClient::requestFailed);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        responses.clear();
        client.request(QStringLiteral("test.app-context"));
        QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 2'000);
        QCOMPARE(qvariant_cast<QJsonObject>(responses.first().at(1))
                     .value(QStringLiteral("picturesDirectory")).toString(),
                 firstOverride.path());

        qputenv("OPENNOW_PICTURES_DIR", secondOverride.path().toUtf8());
        QVERIFY(!client.request(QStringLiteral("test.exit")).isEmpty());
        QTRY_VERIFY_WITH_TIMEOUT(!failures.isEmpty(), 2'000);
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 4'000);
        responses.clear();
        client.request(QStringLiteral("test.app-context"));
        QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 2'000);
        QCOMPARE(qvariant_cast<QJsonObject>(responses.first().at(1))
                     .value(QStringLiteral("picturesDirectory")).toString(),
                 secondOverride.path());
    }
#endif

    void realCoreSharesTheResolvedPicturesRoot()
    {
        QTemporaryDir overrideRoot;
        QTemporaryDir dataDir;
        QVERIFY(overrideRoot.isValid() && dataDir.isValid());
        const auto previousPictures = qgetenv("OPENNOW_PICTURES_DIR");
        const auto restoreEnvironment = qScopeGuard([&] {
            if (previousPictures.isNull()) qunsetenv("OPENNOW_PICTURES_DIR");
            else qputenv("OPENNOW_PICTURES_DIR", previousPictures);
        });
        const auto program = QString::fromUtf8(OPENNOW_TEST_CORE_PATH);
        QVERIFY2(QFileInfo(program).isExecutable(), qPrintable(program));

        qputenv("OPENNOW_PICTURES_DIR", overrideRoot.path().toUtf8());
        CoreClient client;
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QVERIFY(client.start(program, {QStringLiteral("--data-dir"), dataDir.path()}));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 5'000);
        responses.clear();
        client.request(QStringLiteral("media.root.get"));
        QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 5'000);
        const auto root = qvariant_cast<QJsonObject>(responses.first().at(1))
                              .value(QStringLiteral("path")).toString();
        QCOMPARE(QDir::cleanPath(root),
                 QDir::cleanPath(QDir(overrideRoot.path()).filePath(QStringLiteral("OpenNOW"))));
        QCOMPARE(QDir::cleanPath(mediaRecordingsDirectory()),
                 QDir::cleanPath(QDir(root).filePath(QStringLiteral("Recordings"))));
        QCOMPARE(QDir::cleanPath(mediaScreenshotsDirectory()),
                 QDir::cleanPath(QDir(root).filePath(QStringLiteral("Screenshots"))));
        client.stop();
    }

    void realCoreTreatsAnEmptyPicturesMarkerAsUnavailable()
    {
        QTemporaryDir dataDir;
        QVERIFY(dataDir.isValid());
        const auto previousPictures = qgetenv("OPENNOW_PICTURES_DIR");
        const auto restoreEnvironment = qScopeGuard([&] {
            if (previousPictures.isNull()) qunsetenv("OPENNOW_PICTURES_DIR");
            else qputenv("OPENNOW_PICTURES_DIR", previousPictures);
        });
        const auto program = QString::fromUtf8(OPENNOW_TEST_CORE_PATH);
        QVERIFY2(QFileInfo(program).isExecutable(), qPrintable(program));

        qputenv("OPENNOW_PICTURES_DIR", "");
        if (!qEnvironmentVariableIsSet("OPENNOW_PICTURES_DIR"))
            QSKIP("This platform cannot set an empty environment variable in-process");

        CoreClient client;
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QSignalSpy failures(&client, &CoreClient::requestFailed);
        QVERIFY(client.start(program, {QStringLiteral("--data-dir"), dataDir.path()}));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 5'000);
        responses.clear();
        client.request(QStringLiteral("media.list"));
        QTRY_VERIFY_WITH_TIMEOUT(!failures.isEmpty(), 5'000);
        QCOMPARE(failures.last().at(1).toString(), QStringLiteral("media_list_failed"));
        QVERIFY(responses.isEmpty());
        client.stop();
    }

    void readsGraphicsPreferencesBeforeStartingTheCore()
    {
        QCOMPARE(CoreClient::graphicsPreference(fakeCorePath()), QStringLiteral("fixture-gpu"));
        QCOMPARE(CoreClient::graphicsPreference({}), QString{});
    }

    void rejectsInvalidGraphicsPreferences_data()
    {
        QTest::addColumn<QByteArray>("payload");
        QTest::newRow("version") << QByteArray(R"({"version":2,"windowsGpuDeviceId":"gpu"})");
        QTest::newRow("type") << QByteArray(R"({"version":1,"windowsGpuDeviceId":42})");
        QTest::newRow("nul") << QByteArray(R"({"version":1,"windowsGpuDeviceId":"gpu\u0000"})");
        QTest::newRow("malformed") << QByteArray("not json");
        QTest::newRow("oversized") << QByteArray(8193, 'x');
    }

    void rejectsInvalidGraphicsPreferences()
    {
        QFETCH(QByteArray, payload);
        qputenv("OPENNOW_TEST_GPU_BOOTSTRAP", payload);
        QTest::ignoreMessage(QtWarningMsg, QRegularExpression(QStringLiteral("Graphics preference bootstrap.*Automatic")));
        const auto preference = CoreClient::graphicsPreference(fakeCorePath());
        qunsetenv("OPENNOW_TEST_GPU_BOOTSTRAP");
        QVERIFY(preference.isEmpty());
    }

    void boundsGraphicsPreferenceStartupTime()
    {
        qputenv("OPENNOW_TEST_GPU_BOOTSTRAP", "delay");
        QTest::ignoreMessage(QtWarningMsg, "Graphics preference bootstrap failed; using Automatic");
        QElapsedTimer elapsed;
        elapsed.start();
        const auto preference = CoreClient::graphicsPreference(fakeCorePath());
        qunsetenv("OPENNOW_TEST_GPU_BOOTSTRAP");
        QVERIFY(preference.isEmpty());
        QVERIFY(elapsed.elapsed() < 4'000);
    }

    void acknowledgesUpdateOnlyAfterUiAndCoreAreReady_data()
    {
        QTest::addColumn<bool>("uiFirst");
        QTest::newRow("ui-first") << true;
        QTest::newRow("core-first") << false;
    }

    void acknowledgesUpdateOnlyAfterUiAndCoreAreReady()
    {
        QFETCH(bool, uiFirst);
        qputenv("OPENNOW_UPDATE_PLAN", "/fixture/update/plan.json");
        qputenv("OPENNOW_UPDATE_NONCE", "fixture-nonce");
        CoreClient client;
        QVERIFY(!qEnvironmentVariableIsSet("OPENNOW_UPDATE_PLAN"));
        QVERIFY(!qEnvironmentVariableIsSet("OPENNOW_UPDATE_NONCE"));
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        if (uiFirst) client.markUiReady();
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        if (!uiFirst) {
            responses.clear();
            const auto id = client.request(QStringLiteral("test.app-context"));
            QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 2'000);
            QCOMPARE(responses.first().at(0).toString(), id);
            const auto context = qvariant_cast<QJsonObject>(responses.first().at(1));
            QCOMPARE(context.value(QStringLiteral("startupAcknowledgements")).toInt(), 0);
            QVERIFY(context.value(QStringLiteral("hasUpdateEnvironment")).toBool());
            client.markUiReady();
        }
        QTRY_VERIFY_WITH_TIMEOUT(std::any_of(responses.begin(), responses.end(), [](const auto &response) {
            return qvariant_cast<QJsonObject>(response.at(1)).value(QStringLiteral("acknowledged")).toBool();
        }), 2'000);
        client.stop();
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        responses.clear();
        client.request(QStringLiteral("test.app-context"));
        QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 2'000);
        const auto context = qvariant_cast<QJsonObject>(responses.first().at(1));
        QCOMPARE(context.value(QStringLiteral("startupAcknowledgements")).toInt(), 0);
        QVERIFY(!context.value(QStringLiteral("hasUpdateEnvironment")).toBool());
    }

    void passesCanonicalApplicationIdentityToCore()
    {
        CoreClient client;
        client.markUiReady();
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        responses.clear();
        const auto id = client.request(QStringLiteral("test.app-context"));
        QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 2'000);
        QCOMPARE(responses.first().at(0).toString(), id);
        const auto context = qvariant_cast<QJsonObject>(responses.first().at(1));
        QCOMPARE(context.value(QStringLiteral("executable")).toString(),
                 QFileInfo(QCoreApplication::applicationFilePath()).canonicalFilePath());
        QCOMPARE(context.value(QStringLiteral("pid")).toString(),
                 QString::number(QCoreApplication::applicationPid()));
        QCOMPARE(context.value(QStringLiteral("startupAcknowledgements")).toInt(), 0);
    }

    void retriesAdmissionRejectionsWithoutFailingTheCaller()
    {
        CoreClient client;
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QSignalSpy failures(&client, &CoreClient::requestFailed);
        QSignalSpy events(&client, &CoreClient::eventReceived);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        responses.clear();
        const QJsonObject params{{QStringLiteral("searchQuery"), QStringLiteral("Library game")},
                                 {QStringLiteral("limit"), 1000}};
        const auto id = client.request(QStringLiteral("test.busy"), params, 2'000);
        QTRY_COMPARE_WITH_TIMEOUT(events.size(), 3, 2'000);
        QCOMPARE(failures.size(), 0);
        QCOMPARE(responses.size(), 1);
        QCOMPARE(responses.first().at(0).toString(), id);
        QCOMPARE(responses.first().at(1).toJsonObject().value(QStringLiteral("params")).toObject(), params);
    }

    void busyRetriesKeepTheOriginalDeadline()
    {
        CoreClient client;
        QSignalSpy failures(&client, &CoreClient::requestFailed);
        QSignalSpy events(&client, &CoreClient::eventReceived);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        const auto id = client.request(QStringLiteral("test.busy-forever"), {}, 500);
        QTRY_COMPARE_WITH_TIMEOUT(failures.size(), 1, 1'500);
        QCOMPARE(failures.first().at(0).toString(), id);
        QCOMPARE(failures.first().at(1).toString(), QStringLiteral("deadline_exceeded"));
        QVERIFY(events.size() >= 2);
        const auto attempts = events.size();
        QTest::qWait(400);
        QCOMPARE(events.size(), attempts);
        QCOMPARE(failures.size(), 1);
    }

    void cancellationAndStopDiscardBusyRetries_data()
    {
        QTest::addColumn<bool>("stop");
        QTest::newRow("cancel") << false;
        QTest::newRow("stop") << true;
    }

    void cancellationAndStopDiscardBusyRetries()
    {
        QFETCH(bool, stop);
        CoreClient client;
        QSignalSpy failures(&client, &CoreClient::requestFailed);
        QSignalSpy events(&client, &CoreClient::eventReceived);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        const auto id = client.request(QStringLiteral("test.busy-forever"), {}, 2'000);
        QTRY_COMPARE_WITH_TIMEOUT(events.size(), 1, 1'000);
        if (stop)
            client.stop();
        else
            QVERIFY(client.cancel(id));
        QCOMPARE(failures.size(), 1);
        QCOMPARE(failures.first().at(0).toString(), id);
        QCOMPARE(failures.first().at(1).toString(), stop ? QStringLiteral("core_stopping") : QStringLiteral("cancelled"));
        QTest::qWait(400);
        QCOMPARE(events.size(), 1);
        QCOMPARE(failures.size(), 1);
    }

    void startsStopped()
    {
        CoreClient client;
        QCOMPARE(client.state(), QStringLiteral("stopped"));
        QCOMPARE(client.protocolVersion(), 5);
        QVERIFY(client.lastError().isEmpty());
    }

    void rejectsOldCoreBeforeSendingCatalogRequests()
    {
        const auto previous = qgetenv("OPENNOW_TEST_OLD_CORE");
        const auto restore = qScopeGuard([previous] {
            if (previous.isNull()) qunsetenv("OPENNOW_TEST_OLD_CORE");
            else qputenv("OPENNOW_TEST_OLD_CORE", previous);
        });
        qputenv("OPENNOW_TEST_OLD_CORE", "1");
        QStringList errors;
        CoreClient client;
        connect(&client, &CoreClient::lastErrorChanged, &client, [&] { errors.append(client.lastError()); });
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("failed"), 2'000);
        QVERIFY(errors.contains(QStringLiteral("Core protocol version is incompatible")));
        QVERIFY(client.request(QStringLiteral("catalog.library.list")).isEmpty());
        QVERIFY(responses.isEmpty());
    }

    void rejectsCoreWithoutQueueCapabilityBeforeSendingProductRequests()
    {
        const auto previous = qgetenv("OPENNOW_TEST_NO_QUEUE_CAPABILITY");
        const auto restore = qScopeGuard([previous] {
            if (previous.isNull()) qunsetenv("OPENNOW_TEST_NO_QUEUE_CAPABILITY");
            else qputenv("OPENNOW_TEST_NO_QUEUE_CAPABILITY", previous);
        });
        qputenv("OPENNOW_TEST_NO_QUEUE_CAPABILITY", "1");
        QStringList errors;
        CoreClient client;
        connect(&client, &CoreClient::lastErrorChanged, &client, [&] { errors.append(client.lastError()); });
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("failed"), 2'000);
        QVERIFY(errors.contains(QStringLiteral("The packaged core lacks a required capability: queue.servers.v1")));
        QVERIFY(client.request(QStringLiteral("queue.servers.list")).isEmpty());
        QVERIFY(client.request(QStringLiteral("catalog.library.list")).isEmpty());
        QVERIFY(responses.isEmpty());
    }

    void rejectsCatalogRequestsDuringHandshakeAndProtocolFailure()
    {
        const auto previous = qgetenv("OPENNOW_TEST_OLD_CORE");
        const auto restore = qScopeGuard([previous] {
            if (previous.isNull()) qunsetenv("OPENNOW_TEST_OLD_CORE");
            else qputenv("OPENNOW_TEST_OLD_CORE", previous);
        });
        qputenv("OPENNOW_TEST_OLD_CORE", "1");
        QStringList observedStates;
        QStringList admittedStates;
        CoreClient client;
        connect(&client, &CoreClient::stateChanged, &client, [&] {
            const auto state = client.state();
            if (state != QStringLiteral("handshaking") && state != QStringLiteral("failed")) return;
            observedStates.append(state);
            if (!client.request(QStringLiteral("catalog.library.list")).isEmpty())
                admittedStates.append(state);
        });
        QVERIFY(client.start(fakeCorePath()));
        QTRY_VERIFY_WITH_TIMEOUT(observedStates.contains(QStringLiteral("failed")), 2'000);
        QVERIFY(observedStates.contains(QStringLiteral("handshaking")));
        QVERIFY2(admittedStates.isEmpty(), qPrintable(admittedStates.join(QStringLiteral(", "))));
    }

    void rejectsInvalidStartAndRequest()
    {
        CoreClient client;
        QVERIFY(!client.start(QString()));
        QVERIFY(client.request(QStringLiteral("catalog.list")).isEmpty());
        QVERIFY(!client.cancel(QStringLiteral("999")));
    }

    void injectsTransientHdrOutputCapability()
    {
        CoreClient client;
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        for (const auto &method : {QStringLiteral("session.create"), QStringLiteral("streamer.prepare"),
                                  QStringLiteral("settings.choices.get")}) {
            for (bool supported : {false, true, false}) {
                responses.clear();
                client.setNativeHdrSupported(supported);
                const QJsonObject capabilities{{QStringLiteral("nativeHdrSupported"), !supported},
                                               {QStringLiteral("protocolVersion"), 7}};
                const QJsonObject params{{QStringLiteral("runtimeCapabilities"), capabilities},
                                         {QStringLiteral("appId"), QStringLiteral("123")}};
                QVERIFY(!client.request(method, params).isEmpty());
                QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 2'000);
                const auto actual = responses.first().at(1).toJsonObject()
                    .value(QStringLiteral("params")).toObject();
                QCOMPARE(actual.value(QStringLiteral("appId")), params.value(QStringLiteral("appId")));
                const auto runtime = actual.value(QStringLiteral("runtimeCapabilities")).toObject();
                QCOMPARE(runtime.value(QStringLiteral("nativeHdrSupported")).toBool(), supported);
                QVERIFY(!runtime.contains(QStringLiteral("nativeHdrDisplay")));
                QCOMPARE(runtime.value(QStringLiteral("protocolVersion")).toInt(), 7);
                QVERIFY(!actual.contains(QStringLiteral("settings")));
                QCOMPARE(params.value(QStringLiteral("runtimeCapabilities")).toObject(), capabilities);
            }
        }
        client.stop();
    }

    void injectsValidatedNativeHdrDisplayCapability()
    {
        CoreClient client;
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        for (const auto &method : {QStringLiteral("session.create"), QStringLiteral("streamer.prepare"),
                                  QStringLiteral("settings.choices.get")}) {
            responses.clear();
            CoreClient::NativeHdrDisplay display;
            display.available = true;
            display.minimumNits = 0.005;
            display.maximumNits = 620;
            display.maximumFullFrameNits = 400;
            display.chromaticity = HdrChromaticity{0.68, 0.32, 0.265, 0.69,
                                                   0.15, 0.06, 0.3127, 0.329};
            client.setNativeHdrDisplay(display);
            QVERIFY(!client.request(method, {{QStringLiteral("appId"), QStringLiteral("123")}}).isEmpty());
            QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 2'000);
            const auto runtime = responses.first().at(1).toJsonObject()
                .value(QStringLiteral("params")).toObject()
                .value(QStringLiteral("runtimeCapabilities")).toObject();
            const auto injected = runtime.value(QStringLiteral("nativeHdrDisplay")).toObject();
            QCOMPARE(injected.value(QStringLiteral("minimumNits")).toDouble(), 0.005);
            QCOMPARE(injected.value(QStringLiteral("maximumNits")).toDouble(), 620.0);
            QCOMPARE(injected.value(QStringLiteral("maximumFullFrameNits")).toDouble(), 400.0);
            QCOMPARE(injected.value(QStringLiteral("redX")).toDouble(), 0.68);
            QCOMPARE(injected.value(QStringLiteral("redY")).toDouble(), 0.32);
            QCOMPARE(injected.value(QStringLiteral("greenX")).toDouble(), 0.265);
            QCOMPARE(injected.value(QStringLiteral("greenY")).toDouble(), 0.69);
            QCOMPARE(injected.value(QStringLiteral("blueX")).toDouble(), 0.15);
            QCOMPARE(injected.value(QStringLiteral("blueY")).toDouble(), 0.06);
            QCOMPARE(injected.value(QStringLiteral("whiteX")).toDouble(), 0.3127);
            QCOMPARE(injected.value(QStringLiteral("whiteY")).toDouble(), 0.329);
            responses.clear();
            client.setNativeHdrSupported(true);
            client.setNativeHdrDisplay({});
            const QJsonObject stale{
                {QStringLiteral("runtimeCapabilities"),
                 QJsonObject{{QStringLiteral("nativeHdrDisplay"),
                              QJsonObject{{QStringLiteral("minimumNits"), 0.005},
                                          {QStringLiteral("maximumNits"), 620}}},
                             {QStringLiteral("protocolVersion"), 7}}}};
            QVERIFY(!client.request(method, stale).isEmpty());
            QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 2'000);
            const auto absent = responses.first().at(1).toJsonObject()
                .value(QStringLiteral("params")).toObject()
                .value(QStringLiteral("runtimeCapabilities")).toObject();
            QCOMPARE(absent.value(QStringLiteral("nativeHdrSupported")).toBool(), true);
            QVERIFY(!absent.contains(QStringLiteral("nativeHdrDisplay")));
        }
        client.stop();
    }

    void injectedHdrOutputControlsRealCoreColorDescriptors()
    {
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        CoreClient client;
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QSignalSpy failures(&client, &CoreClient::requestFailed);
        const auto program = QString::fromUtf8(OPENNOW_TEST_CORE_PATH);
        QVERIFY2(QFileInfo(program).isExecutable(), qPrintable(program));
        QVERIFY(client.start(program, {QStringLiteral("--data-dir"), directory.path()}));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 5'000);
        responses.clear();
        QVERIFY(!client.request(QStringLiteral("settings.set"),
            {{QStringLiteral("key"), QStringLiteral("enableHdr")}, {QStringLiteral("value"), true}}).isEmpty());
        QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 5'000);
        for (bool supported : {false, true, false}) {
            client.setNativeHdrSupported(supported);
            CoreClient::NativeHdrDisplay display;
            display.available = true;
            display.minimumNits = 0.005;
            display.maximumNits = 620;
            client.setNativeHdrDisplay(display);
            const auto capabilities = QJsonDocument::fromJson(R"({"protocolVersion":7,
                "videoBackends":[{"backend":"vaapi","available":true,"codecs":[
                    {"codec":"h265","available":true,"hdrSupported":true,
                     "colorQualities":["8bit_420","10bit_420"],"hdrColorQualities":["10bit_420"]}]}]})").object();
            auto callerCapabilities = capabilities;
            callerCapabilities.insert(QStringLiteral("nativeHdrSupported"), !supported);
            responses.clear();
            QVERIFY(!client.request(QStringLiteral("settings.choices.get"),
                {{QStringLiteral("runtimeCapabilities"), callerCapabilities}}).isEmpty());
            QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 5'000);
            const auto choices = responses.first().at(1).toJsonObject().value(QStringLiteral("colorQualities")).toArray();
            QCOMPARE(choices.size(), 4);
            for (const auto &entry : choices) {
                const auto choice = entry.toObject();
                const auto expected = supported && choice.value(QStringLiteral("value")).toString().endsWith(QStringLiteral("420"));
                QCOMPARE(choice.value(QStringLiteral("disabled")).toBool(), !expected);
            }
            QCOMPARE(callerCapabilities.value(QStringLiteral("nativeHdrSupported")).toBool(), !supported);
        }
        QVERIFY(failures.isEmpty());
        client.stop();
    }

    void negotiatesAndRoutesMessages()
    {
        CoreClient client;
        QSignalSpy responses(&client, &CoreClient::responseReceived);
        QSignalSpy failures(&client, &CoreClient::requestFailed);
        QSignalSpy events(&client, &CoreClient::eventReceived);
        QSignalSpy logs(&client, &CoreClient::coreLogReceived);
        const auto helper = fakeCorePath();

        QVERIFY(client.start(helper));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        responses.clear();

        const auto echoId = client.request(QStringLiteral("test.echo"));
        QVERIFY(!echoId.isEmpty());
        QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 2'000);
        QCOMPARE(responses.first().at(0).toString(), echoId);
        QCOMPARE(responses.first().at(1).toJsonObject().value(QStringLiteral("value")).toString(),
                 QStringLiteral("pong"));

        client.request(QStringLiteral("test.event"));
        QTRY_COMPARE_WITH_TIMEOUT(events.size(), 1, 2'000);
        QCOMPARE(events.first().at(0).toString(), QStringLiteral("catalog.changed"));
        QCOMPARE(events.first().at(1).toJsonObject().value(QStringLiteral("revision")).toInt(), 2);

        client.request(QStringLiteral("test.streamer-event"));
        QTRY_COMPARE_WITH_TIMEOUT(events.size(), 2, 2'000);
        QCOMPARE(events.last().at(0).toString(), QStringLiteral("streamer.changed"));
        const auto streamer = events.last().at(1).toJsonObject();
        QCOMPARE(streamer.value(QStringLiteral("status")).toString(), QStringLiteral("streaming"));
        QCOMPARE(streamer.value(QStringLiteral("firstFrameLatencyMs")).toInt(), 37);
        QCOMPARE(streamer.value(QStringLiteral("mediaBackend")).toString(), QStringLiteral("ffmpeg"));
        QCOMPARE(streamer.value(QStringLiteral("deviceRecoveryCount")).toInt(), 2);
        QCOMPARE(streamer.value(QStringLiteral("queueDropCount")).toInt(), 4);

        const auto errorId = client.request(QStringLiteral("test.error"));
        QTRY_VERIFY_WITH_TIMEOUT(!failures.isEmpty(), 2'000);
        QCOMPARE(failures.last().at(0).toString(), errorId);
        QCOMPARE(failures.last().at(1).toString(), QStringLiteral("expected"));

        const auto timeoutId = client.request(QStringLiteral("test.hang"), {}, 100);
        QTRY_VERIFY_WITH_TIMEOUT(failures.size() >= 2, 2'000);
        QCOMPARE(failures.last().at(0).toString(), timeoutId);
        QCOMPARE(failures.last().at(1).toString(), QStringLiteral("deadline_exceeded"));

        const auto responseCount = responses.size();
        const auto partialId = client.request(QStringLiteral("test.partial"));
        QTRY_COMPARE_WITH_TIMEOUT(responses.size(), responseCount + 1, 2'000);
        QCOMPARE(responses.last().at(0).toString(), partialId);
        QVERIFY(responses.last().at(1).toJsonObject().value(QStringLiteral("fragmented")).toBool());

        client.request(QStringLiteral("test.stderr"));
        QTRY_COMPARE_WITH_TIMEOUT(logs.size(), 1, 2'000);
        QCOMPARE(logs.first().at(0).toString(),
                 QStringLiteral("native-streamer: decoder diagnostic"));
    }

    void rejectsMessagesFollowingFatalProtocolError()
    {
        CoreClient client;
        QSignalSpy failures(&client, &CoreClient::requestFailed);
        QSignalSpy events(&client, &CoreClient::eventReceived);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        const auto requestId = client.request(QStringLiteral("test.malformed-event-batch"));
        QTRY_VERIFY_WITH_TIMEOUT(!failures.isEmpty(), 2'000);
        QCOMPARE(failures.first().at(0).toString(), requestId);
        QCOMPARE(failures.first().at(1).toString(), QStringLiteral("protocol_error"));
        client.stop();
        QCoreApplication::processEvents();
        QCOMPARE(events.size(), 0);
    }

    void stopDiscardsAlreadyQueuedEvents()
    {
        CoreClient client;
        QSignalSpy events(&client, &CoreClient::eventReceived);
        QVERIFY(client.start(fakeCorePath()));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        const auto requestId = client.request(QStringLiteral("test.event"));
        connect(&client, &CoreClient::responseReceived, &client,
                [&client, requestId](const QString &id, const QJsonObject &) {
                    if (id == requestId) client.stop();
                });
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("stopped"), 2'000);
        QCoreApplication::processEvents();
        QCOMPARE(events.size(), 0);
    }

    void restartsAfterUnexpectedExit()
    {
        CoreClient client;
        QSignalSpy failures(&client, &CoreClient::requestFailed);
        const auto helper = fakeCorePath();
        QVERIFY(client.start(helper));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        const auto requestId = client.request(QStringLiteral("test.exit"));
        QVERIFY(!requestId.isEmpty());
        QTRY_VERIFY_WITH_TIMEOUT(!failures.isEmpty(), 2'000);
        QCOMPARE(failures.last().at(0).toString(), requestId);
        QCOMPARE(failures.last().at(1).toString(), QStringLiteral("core_exited"));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("failed"), 2'000);
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 3'000);
    }

    void retriesWhenCoreBecomesAvailable()
    {
        QTemporaryDir temporary;
        QVERIFY(temporary.isValid());
        const auto helper = fakeCorePath();
        const auto delayed = QDir(temporary.path()).filePath(QFileInfo(helper).fileName());

        CoreClient client;
        QVERIFY(client.start(delayed));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("failed"), 2'000);
        QVERIFY(QFile::copy(helper, delayed));
        QVERIFY(QFile::setPermissions(
            delayed, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner
                | QFileDevice::ReadGroup | QFileDevice::ExeGroup | QFileDevice::ReadOther
                | QFileDevice::ExeOther));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 4'000);
    }

    void stopClosesStdinBeforeEscalating()
    {
        QTemporaryDir temporary;
        QVERIFY(temporary.isValid());
        const auto helper = fakeCorePath();
        const auto marker = QDir(temporary.path()).filePath(QStringLiteral("graceful.marker"));

        CoreClient client;
        QVERIFY(client.start(helper, {QStringLiteral("--eof-marker"), marker}));
        QTRY_COMPARE_WITH_TIMEOUT(client.state(), QStringLiteral("ready"), 2'000);
        client.stop();
        QCOMPARE(client.state(), QStringLiteral("stopped"));
        QFile markerFile(marker);
        QVERIFY(markerFile.open(QIODevice::ReadOnly));
        QCOMPARE(markerFile.readAll(), QByteArray("graceful"));
    }
};

QTEST_GUILESS_MAIN(CoreClientTest)
#include "tst_coreclient.moc"
