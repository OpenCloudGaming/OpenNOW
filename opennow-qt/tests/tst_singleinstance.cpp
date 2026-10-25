#include "app/SingleInstance.h"
#include "app/InstanceLaunchOptions.h"
#include "diagnostics/DiagnosticsPaths.h"

#include <QSignalSpy>
#include <QTest>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QScopeGuard>

#include <atomic>
#include <thread>

namespace {
#ifdef Q_OS_UNIX
QString endpointName(const QString &directory)
{
    const auto scope = directory.toUtf8();
    const auto digest = QCryptographicHash::hash(scope, QCryptographicHash::Sha256).toHex().first(16);
    return QStringLiteral("opennow-%1").arg(QString::fromLatin1(digest));
}
#endif
}

class SingleInstanceTest final : public QObject
{
    Q_OBJECT

    QTemporaryDir m_directory;

private slots:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
        QVERIFY(m_directory.isValid());
    }

    void launchOptions_data()
    {
        QTest::addColumn<QStringList>("arguments");
        QTest::addColumn<bool>("valid");
        QTest::newRow("default") << QStringList{"opennow"} << true;
        QTest::newRow("profile") << QStringList{"opennow", "--profile", "player-two"} << true;
        QTest::newRow("controller") << QStringList{"opennow", "--profile=player_two", "--controller=42"} << true;
        QTest::newRow("profile-missing") << QStringList{"opennow", "--profile"} << false;
        QTest::newRow("profile-empty") << QStringList{"opennow", "--profile="} << false;
        QTest::newRow("traversal") << QStringList{"opennow", "--profile", "../default"} << false;
        QTest::newRow("absolute") << QStringList{"opennow", "--profile", "/tmp/player"} << false;
        QTest::newRow("too-long") << QStringList{"opennow", "--profile", QString(65, 'a')} << false;
        QTest::newRow("duplicate-profile") << QStringList{"opennow", "--profile=a", "--profile=b"} << false;
        QTest::newRow("default-controller") << QStringList{"opennow", "--controller=1"} << false;
        QTest::newRow("controller-zero") << QStringList{"opennow", "--profile=a", "--controller=0"} << false;
        QTest::newRow("controller-negative") << QStringList{"opennow", "--profile=a", "--controller=-1"} << false;
        QTest::newRow("controller-overflow") << QStringList{"opennow", "--profile=a", "--controller=4294967296"} << false;
        QTest::newRow("controller-text") << QStringList{"opennow", "--profile=a", "--controller=pad"} << false;
    }

    void launchOptions()
    {
        QFETCH(QStringList, arguments);
        QFETCH(bool, valid);
        const auto options = InstanceLaunchOptions::parse(arguments);
        QCOMPARE(options.error.isEmpty(), valid);
        if (!valid) return;
        const auto restarted = InstanceLaunchOptions::parse(
            QStringList{"opennow"} + options.restartArguments());
        QVERIFY(restarted.error.isEmpty());
        QCOMPARE(restarted.profile, options.profile);
        QCOMPARE(restarted.controllerId, quint32(0));
    }

    void inheritedProfileIsValidated()
    {
        QVERIFY(!InstanceLaunchOptions::parse({"opennow"}, "../default").error.isEmpty());
        const auto options = InstanceLaunchOptions::parse({"opennow", "--controller=2"}, "player-two");
        QVERIFY(options.error.isEmpty());
        QCOMPARE(options.profile, QStringLiteral("player-two"));
        QCOMPARE(options.controllerId, quint32(2));
    }

    void profilePathsPreserveDefaultStorage()
    {
        const auto oldProfile = qgetenv("OPENNOW_PROFILE");
        const auto oldRoot = qgetenv("OPENNOW_DATA_DIR");
        const auto restore = qScopeGuard([&] {
            if (oldProfile.isNull()) qunsetenv("OPENNOW_PROFILE");
            else qputenv("OPENNOW_PROFILE", oldProfile);
            if (oldRoot.isNull()) qunsetenv("OPENNOW_DATA_DIR");
            else qputenv("OPENNOW_DATA_DIR", oldRoot);
        });
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        qputenv("OPENNOW_DATA_DIR", directory.path().toUtf8());
        qunsetenv("OPENNOW_PROFILE");
        QCOMPARE(coreDiagnosticsDataRoot(), directory.path());
        qputenv("OPENNOW_PROFILE", "player-two");
        QCOMPARE(coreDiagnosticsDataRoot(), directory.filePath("profiles/player-two"));
#ifdef Q_OS_LINUX
        const auto primary = directory.filePath("OpenNOW");
        const auto legacy = directory.filePath("opennow");
        QVERIFY(QDir().mkpath(legacy));
        qputenv("OPENNOW_DATA_DIR", primary.toUtf8());
        qunsetenv("OPENNOW_PROFILE");
        QCOMPARE(coreDiagnosticsDataRoot(), legacy);
        qputenv("OPENNOW_PROFILE", "player-two");
        QCOMPARE(coreDiagnosticsDataRoot(), primary + QStringLiteral("/profiles/player-two"));
        QVERIFY(QDir().mkpath(coreDiagnosticsDataRoot()));
        qunsetenv("OPENNOW_PROFILE");
        QCOMPARE(coreDiagnosticsDataRoot(), legacy);
        QFile settings(primary + QStringLiteral("/settings.json"));
        QVERIFY(settings.open(QIODevice::WriteOnly));
        settings.close();
        QCOMPARE(coreDiagnosticsDataRoot(), primary);
#endif
    }

    void distinctProfilesHaveDistinctEndpoints()
    {
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        SingleInstance defaultInstance;
        SingleInstance first;
        SingleInstance second;
        QVERIFY(QDir().mkpath(directory.filePath("one")));
        QVERIFY(QDir().mkpath(directory.filePath("two")));
        QCOMPARE(defaultInstance.acquire({"opennow"}, m_directory.path()), SingleInstance::Acquisition::Primary);
        QCOMPARE(first.acquire({"opennow", "--profile=one"}, directory.filePath("one")),
                 SingleInstance::Acquisition::Primary);
        QCOMPARE(second.acquire({"opennow", "--profile=two"}, directory.filePath("two")),
                 SingleInstance::Acquisition::Primary);
    }

    void missingProfileDirectoryCannotAdmitAPrimary()
    {
        SingleInstance instance;
        QCOMPARE(instance.acquire({"opennow"}, m_directory.filePath("missing")),
                 SingleInstance::Acquisition::Failed);
    }

    void missingEndpointCannotAdmitAnotherPrimary()
    {
#ifdef Q_OS_UNIX
        SingleInstance primary;
        QCOMPARE(primary.acquire({QStringLiteral("opennow")}, m_directory.path()), SingleInstance::Acquisition::Primary);
        QVERIFY(QLocalServer::removeServer(endpointName(m_directory.path())));
        SingleInstance secondary;
        QCOMPARE(secondary.acquire({QStringLiteral("opennow")}, m_directory.path()), SingleInstance::Acquisition::Failed);
#else
        QSKIP("Filesystem-backed local sockets are a Unix contract");
#endif
    }

    void listenFailureCannotAdmitAPrimary()
    {
#ifdef Q_OS_UNIX
        const auto endpoint = QDir::temp().filePath(endpointName(m_directory.path()));
        QVERIFY(QDir().mkdir(endpoint));
        SingleInstance instance;
        const auto result = instance.acquire({QStringLiteral("opennow")}, m_directory.path());
        QVERIFY(QDir().rmdir(endpoint));
        QCOMPARE(result, SingleInstance::Acquisition::Failed);
#else
        QSKIP("Filesystem-backed local sockets are a Unix contract");
#endif
    }

    void forwardsArgumentsToPrimary()
    {
        SingleInstance primary;
        QCOMPARE(primary.acquire({QStringLiteral("opennow"), QStringLiteral("opennow://launch/42")}, m_directory.path()), SingleInstance::Acquisition::Primary);
        QSignalSpy activation(&primary, &SingleInstance::activationRequested);

        std::atomic_bool secondaryCompleted = false;
        std::atomic<SingleInstance::Acquisition> secondaryAcquisition = SingleInstance::Acquisition::Primary;
        std::thread secondary([&] {
            SingleInstance instance;
            secondaryAcquisition = instance.acquire(
                {QStringLiteral("opennow"), QStringLiteral("opennow://launch/99")}, m_directory.path() + QStringLiteral("/."));
            secondaryCompleted = true;
        });
        QTRY_COMPARE_WITH_TIMEOUT(activation.size(), 1, 2'000);
        QTRY_VERIFY_WITH_TIMEOUT(secondaryCompleted.load(), 2'000);
        secondary.join();
        QCOMPARE(secondaryAcquisition.load(), SingleInstance::Acquisition::Forwarded);
        const auto arguments = activation.first().first().toStringList();
        QCOMPARE(arguments.value(1), QStringLiteral("opennow://launch/99"));
    }

    void sameProfileForwardsOnlyToItsOwner()
    {
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        SingleInstance first;
        SingleInstance other;
        QVERIFY(QDir().mkpath(directory.filePath("one")));
        QVERIFY(QDir().mkpath(directory.filePath("two")));
        QCOMPARE(first.acquire({"opennow"}, directory.filePath("one")), SingleInstance::Acquisition::Primary);
        QCOMPARE(other.acquire({"opennow"}, directory.filePath("two")), SingleInstance::Acquisition::Primary);
        QSignalSpy activation(&first, &SingleInstance::activationRequested);
        QSignalSpy otherActivation(&other, &SingleInstance::activationRequested);
        std::atomic_bool completed = false;
        std::atomic<SingleInstance::Acquisition> result = SingleInstance::Acquisition::Failed;
        std::thread secondary([&] {
            SingleInstance instance;
            result = instance.acquire({"opennow", "opennow://launch/99"}, directory.filePath("one"));
            completed = true;
        });
        QTRY_COMPARE_WITH_TIMEOUT(activation.size(), 1, 2'000);
        QTRY_VERIFY_WITH_TIMEOUT(completed.load(), 2'000);
        secondary.join();
        QCOMPARE(result.load(), SingleInstance::Acquisition::Forwarded);
        QCOMPARE(otherActivation.size(), 0);
    }
};

QTEST_GUILESS_MAIN(SingleInstanceTest)
#include "tst_singleinstance.moc"
