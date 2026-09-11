#include "core/CoreClient.h"

#include <QSignalSpy>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QElapsedTimer>
#include <QRegularExpression>
#include <QTemporaryDir>
#include <QTest>

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
        QCOMPARE(client.protocolVersion(), 1);
        QVERIFY(client.lastError().isEmpty());
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
        for (const auto &method : {QStringLiteral("session.create"), QStringLiteral("streamer.prepare")}) {
            for (bool supported : {false, true, false}) {
                responses.clear();
                client.setNativeHdrSupported(supported);
                const QJsonObject capabilities{{QStringLiteral("nativeHdrSupported"), !supported},
                                               {QStringLiteral("protocolVersion"), 6}};
                const QJsonObject params{{QStringLiteral("runtimeCapabilities"), capabilities},
                                         {QStringLiteral("appId"), QStringLiteral("123")}};
                QVERIFY(!client.request(method, params).isEmpty());
                QTRY_COMPARE_WITH_TIMEOUT(responses.size(), 1, 2'000);
                const auto actual = responses.first().at(1).toJsonObject()
                    .value(QStringLiteral("params")).toObject();
                QCOMPARE(actual.value(QStringLiteral("appId")), params.value(QStringLiteral("appId")));
                const auto runtime = actual.value(QStringLiteral("runtimeCapabilities")).toObject();
                QCOMPARE(runtime.value(QStringLiteral("nativeHdrSupported")).toBool(), supported);
                QCOMPARE(runtime.value(QStringLiteral("protocolVersion")).toInt(), 6);
                QVERIFY(!actual.contains(QStringLiteral("settings")));
                QCOMPARE(params.value(QStringLiteral("runtimeCapabilities")).toObject(), capabilities);
            }
        }
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
