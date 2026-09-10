#include "app/platform/MacAwdlController.h"

#include <QSignalSpy>
#include <QTest>

#ifdef Q_OS_MACOS
#include <cstring>
#include <ifaddrs.h>
#include <net/if.h>
#endif

class FakeMacAwdlBackend final : public MacAwdlBackend
{
public:
    MacAwdlController::State currentState = MacAwdlController::Enabled;
    QString readError;
    QString program;
    QStringList arguments;
    int reads = 0;
    int starts = 0;
    int kills = 0;

    MacAwdlController::State readState(QString *error) override
    {
        ++reads;
        *error = readError;
        return currentState;
    }

    void start(const QString &path, const QStringList &args) override
    {
        ++starts;
        program = path;
        arguments = args;
    }

    void kill() override { ++kills; }
};

class MacAwdlControllerTest final : public QObject
{
    Q_OBJECT

private slots:
    void defaultBackendReadsStatusWithoutActions()
    {
        MacAwdlController controller;
        QSignalSpy busy(&controller, &MacAwdlController::busyChanged);
        controller.refresh();
        QVERIFY(!controller.busy());
        QCOMPARE(busy.count(), 0);
#ifdef Q_OS_MACOS
        ifaddrs *addresses = nullptr;
        auto expectedState = MacAwdlController::Unknown;
        if (getifaddrs(&addresses) == 0) {
            expectedState = MacAwdlController::Unavailable;
            for (const ifaddrs *entry = addresses; entry; entry = entry->ifa_next) {
                if (entry->ifa_name && std::strcmp(entry->ifa_name, "awdl0") == 0) {
                    expectedState = (entry->ifa_flags & IFF_UP) ? MacAwdlController::Enabled
                                                              : MacAwdlController::Disabled;
                    break;
                }
            }
            freeifaddrs(addresses);
        }
        QCOMPARE(controller.state(), expectedState);
        if (expectedState == MacAwdlController::Unknown)
            QVERIFY(!controller.error().isEmpty());
#else
        QCOMPARE(controller.state(), MacAwdlController::Unsupported);
#endif
    }

    void unsupportedPlatform()
    {
#ifndef Q_OS_MACOS
        MacAwdlController controller;
        QCOMPARE(controller.state(), MacAwdlController::Unsupported);
        controller.disable();
        controller.enable();
        QVERIFY(!controller.busy());
        QCOMPARE(controller.state(), MacAwdlController::Unsupported);
#endif
    }

    void nonActionableStates_data()
    {
        QTest::addColumn<MacAwdlController::State>("state");
        QTest::newRow("unsupported") << MacAwdlController::Unsupported;
        QTest::newRow("absent") << MacAwdlController::Unavailable;
        QTest::newRow("unknown") << MacAwdlController::Unknown;
    }

    void nonActionableStates()
    {
        QFETCH(MacAwdlController::State, state);
        auto backend = std::make_unique<FakeMacAwdlBackend>();
        auto *fake = backend.get();
        fake->currentState = state;
        if (state == MacAwdlController::Unknown)
            fake->readError = QStringLiteral("getifaddrs failed");
        MacAwdlController controller(std::move(backend));
        QCOMPARE(controller.state(), state);
        controller.disable();
        controller.enable();
        QCOMPARE(fake->starts, 0);
        QVERIFY(!controller.busy());
        QVERIFY(!controller.error().isEmpty());
        if (state == MacAwdlController::Unknown)
            QCOMPARE(controller.error(), fake->readError);
    }

    void refreshUsesOnlyStateRead()
    {
        auto backend = std::make_unique<FakeMacAwdlBackend>();
        auto *fake = backend.get();
        MacAwdlController controller(std::move(backend));
        QSignalSpy status(&controller, &MacAwdlController::statusChanged);
        controller.refresh();
        QCOMPARE(status.count(), 0);
        fake->currentState = MacAwdlController::Disabled;
        controller.refresh();
        QCOMPARE(controller.state(), MacAwdlController::Disabled);
        QCOMPARE(status.count(), 1);
        QCOMPARE(fake->starts, 0);
    }

    void refreshClearsRecoveredReadError()
    {
        auto backend = std::make_unique<FakeMacAwdlBackend>();
        auto *fake = backend.get();
        fake->currentState = MacAwdlController::Unknown;
        fake->readError = QStringLiteral("getifaddrs failed");
        MacAwdlController controller(std::move(backend));
        QCOMPARE(controller.error(), fake->readError);
        controller.disable();
        QCOMPARE(controller.error(), fake->readError);
        QSignalSpy errors(&controller, &MacAwdlController::errorChanged);
        fake->currentState = MacAwdlController::Enabled;
        fake->readError.clear();
        controller.refresh();
        QCOMPARE(controller.state(), MacAwdlController::Enabled);
        QVERIFY(controller.error().isEmpty());
        QCOMPARE(errors.count(), 1);
        QCOMPARE(fake->starts, 0);
    }

    void recoveredReadPreservesActionError()
    {
        auto backend = std::make_unique<FakeMacAwdlBackend>();
        auto *fake = backend.get();
        MacAwdlController controller(std::move(backend));
        controller.disable();
        emit fake->standardErrorReady("execution error: User canceled. (-128)\n");
        emit fake->finished(1, QProcess::NormalExit);
        const auto actionError = controller.error();
        QVERIFY(actionError.contains(QStringLiteral("(-128)")));
        fake->currentState = MacAwdlController::Unknown;
        fake->readError = QStringLiteral("getifaddrs failed");
        controller.refresh();
        QVERIFY(controller.error().contains(actionError));
        QVERIFY(controller.error().contains(fake->readError));
        fake->currentState = MacAwdlController::Enabled;
        fake->readError.clear();
        controller.refresh();
        QCOMPARE(controller.error(), actionError);
        QCOMPARE(fake->starts, 1);
    }

    void fixedCommandsAndIdempotence()
    {
        auto backend = std::make_unique<FakeMacAwdlBackend>();
        auto *fake = backend.get();
        MacAwdlController controller(std::move(backend));
        QSignalSpy busy(&controller, &MacAwdlController::busyChanged);
        controller.enable();
        QCOMPARE(fake->starts, 0);
        controller.disable();
        QVERIFY(controller.busy());
        QCOMPARE(fake->program, QStringLiteral("/usr/bin/osascript"));
        QCOMPARE(fake->arguments, QStringList({QStringLiteral("-e"), QStringLiteral("do shell script \"/sbin/ifconfig awdl0 down\" with administrator privileges")}));
        fake->currentState = MacAwdlController::Disabled;
        emit fake->finished(0, QProcess::NormalExit);
        QCOMPARE(controller.state(), MacAwdlController::Disabled);
        QVERIFY(!controller.busy());
        QVERIFY(controller.error().isEmpty());
        QCOMPARE(busy.count(), 2);
        controller.disable();
        QCOMPARE(fake->starts, 1);
        controller.enable();
        QCOMPARE(fake->program, QStringLiteral("/usr/bin/osascript"));
        QCOMPARE(fake->arguments, QStringList({QStringLiteral("-e"), QStringLiteral("do shell script \"/sbin/ifconfig awdl0 up\" with administrator privileges")}));
        fake->currentState = MacAwdlController::Enabled;
        emit fake->finished(0, QProcess::NormalExit);
        QVERIFY(controller.error().isEmpty());
        QCOMPARE(controller.state(), MacAwdlController::Enabled);
    }

    void staleStateIsReadBeforeAction()
    {
        auto backend = std::make_unique<FakeMacAwdlBackend>();
        auto *fake = backend.get();
        MacAwdlController controller(std::move(backend));
        fake->currentState = MacAwdlController::Disabled;
        controller.disable();
        QCOMPARE(fake->starts, 0);
        QCOMPARE(controller.state(), MacAwdlController::Disabled);
    }

    void duplicateRequestsAreIgnored()
    {
        auto backend = std::make_unique<FakeMacAwdlBackend>();
        auto *fake = backend.get();
        MacAwdlController controller(std::move(backend));
        controller.disable();
        const int reads = fake->reads;
        controller.disable();
        controller.enable();
        controller.refresh();
        QCOMPARE(fake->starts, 1);
        QCOMPARE(fake->reads, reads);
        fake->currentState = MacAwdlController::Disabled;
        emit fake->finished(0, QProcess::NormalExit);
        QCOMPARE(fake->reads, reads + 1);
        QCOMPARE(fake->starts, 1);
    }

    void processFailures_data()
    {
        QTest::addColumn<QString>("outcome");
        QTest::addColumn<QString>("expectedError");
        QTest::newRow("cancel") << QStringLiteral("cancel") << QStringLiteral("(-128)");
        QTest::newRow("nonzero") << QStringLiteral("nonzero") << QStringLiteral("exit code 7");
        QTest::newRow("launch") << QStringLiteral("launch") << QStringLiteral("Could not start");
        QTest::newRow("crash") << QStringLiteral("crash") << QStringLiteral("crashed");
        QTest::newRow("io") << QStringLiteral("io") << QStringLiteral("process error");
    }

    void processFailures()
    {
        QFETCH(QString, outcome);
        QFETCH(QString, expectedError);
        auto backend = std::make_unique<FakeMacAwdlBackend>();
        auto *fake = backend.get();
        MacAwdlController controller(std::move(backend));
        controller.disable();
        fake->currentState = MacAwdlController::Disabled;
        if (outcome == QStringLiteral("launch")) {
            emit fake->processError(QProcess::FailedToStart);
        } else if (outcome == QStringLiteral("crash")) {
            emit fake->processError(QProcess::Crashed);
            emit fake->finished(9, QProcess::CrashExit);
        } else if (outcome == QStringLiteral("io")) {
            emit fake->processError(QProcess::ReadError);
            QCOMPARE(fake->kills, 1);
            QVERIFY(controller.busy());
            emit fake->finished(9, QProcess::CrashExit);
        } else {
            if (outcome == QStringLiteral("cancel")) {
                emit fake->standardErrorReady(QByteArray(100'000, 'x'));
                emit fake->standardErrorReady("execution error: User canceled. (-128)\n");
            }
            emit fake->finished(7, QProcess::NormalExit);
        }
        QVERIFY(!controller.busy());
        QVERIFY2(controller.error().contains(expectedError), qPrintable(controller.error()));
        QVERIFY(controller.error().size() < 1024);
        QCOMPARE(controller.state(), MacAwdlController::Disabled);
        controller.refresh();
        QVERIFY(controller.error().contains(expectedError));
        controller.disable();
        QVERIFY(controller.error().isEmpty());
        QCOMPARE(fake->starts, 1);
    }

    void timeoutWaitsForExitAndRefreshes()
    {
        auto backend = std::make_unique<FakeMacAwdlBackend>();
        auto *fake = backend.get();
        MacAwdlController controller(std::move(backend), 1);
        controller.disable();
        QTRY_COMPARE(fake->kills, 1);
        QVERIFY(controller.busy());
        QVERIFY(controller.error().contains(QStringLiteral("timed out")));
        controller.enable();
        controller.disable();
        QCOMPARE(fake->starts, 1);
        fake->currentState = MacAwdlController::Disabled;
        emit fake->finished(9, QProcess::CrashExit);
        QVERIFY(!controller.busy());
        QCOMPARE(controller.state(), MacAwdlController::Disabled);
        QVERIFY(controller.error().contains(QStringLiteral("timed out")));
    }

    void unexpectedPostState_data()
    {
        QTest::addColumn<MacAwdlController::State>("state");
        QTest::newRow("reenabled") << MacAwdlController::Enabled;
        QTest::newRow("absent") << MacAwdlController::Unavailable;
        QTest::newRow("read-failure") << MacAwdlController::Unknown;
    }

    void unexpectedPostState()
    {
        QFETCH(MacAwdlController::State, state);
        auto backend = std::make_unique<FakeMacAwdlBackend>();
        auto *fake = backend.get();
        MacAwdlController controller(std::move(backend));
        controller.disable();
        fake->currentState = state;
        if (state == MacAwdlController::Unknown)
            fake->readError = QStringLiteral("getifaddrs failed");
        emit fake->finished(0, QProcess::NormalExit);
        QCOMPARE(controller.state(), state);
        QVERIFY(!controller.error().isEmpty());
        QVERIFY(!controller.busy());
        QCOMPARE(fake->starts, 1);
        controller.refresh();
        QCOMPARE(fake->starts, 1);
    }
};

QTEST_GUILESS_MAIN(MacAwdlControllerTest)
#include "tst_macawdlcontroller.moc"
