#include "app/SingleInstance.h"

#include <QSignalSpy>
#include <QTest>
#include <QCryptographicHash>
#include <QDir>
#include <QStandardPaths>

#include <atomic>
#include <thread>

namespace {
#ifdef Q_OS_UNIX
QString endpointName()
{
    const auto scope = QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation).toUtf8();
    const auto digest = QCryptographicHash::hash(scope, QCryptographicHash::Sha256).toHex().first(16);
    return QStringLiteral("opennow-%1").arg(QString::fromLatin1(digest));
}
#endif
}

class SingleInstanceTest final : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void missingEndpointCannotAdmitAnotherPrimary()
    {
#ifdef Q_OS_UNIX
        SingleInstance primary;
        QCOMPARE(primary.acquire({QStringLiteral("opennow")}), SingleInstance::Acquisition::Primary);
        QVERIFY(QLocalServer::removeServer(endpointName()));
        SingleInstance secondary;
        QCOMPARE(secondary.acquire({QStringLiteral("opennow")}), SingleInstance::Acquisition::Failed);
#else
        QSKIP("Filesystem-backed local sockets are a Unix contract");
#endif
    }

    void listenFailureCannotAdmitAPrimary()
    {
#ifdef Q_OS_UNIX
        const auto endpoint = QDir::temp().filePath(endpointName());
        QVERIFY(QDir().mkdir(endpoint));
        SingleInstance instance;
        const auto result = instance.acquire({QStringLiteral("opennow")});
        QVERIFY(QDir().rmdir(endpoint));
        QCOMPARE(result, SingleInstance::Acquisition::Failed);
#else
        QSKIP("Filesystem-backed local sockets are a Unix contract");
#endif
    }

    void forwardsArgumentsToPrimary()
    {
        SingleInstance primary;
        QCOMPARE(primary.acquire({QStringLiteral("opennow"), QStringLiteral("opennow://launch/42")}), SingleInstance::Acquisition::Primary);
        QSignalSpy activation(&primary, &SingleInstance::activationRequested);

        std::atomic_bool secondaryCompleted = false;
        std::atomic<SingleInstance::Acquisition> secondaryAcquisition = SingleInstance::Acquisition::Primary;
        std::thread secondary([&] {
            SingleInstance instance;
            secondaryAcquisition = instance.acquire(
                {QStringLiteral("opennow"), QStringLiteral("opennow://launch/99")});
            secondaryCompleted = true;
        });
        QTRY_COMPARE_WITH_TIMEOUT(activation.size(), 1, 2'000);
        QTRY_VERIFY_WITH_TIMEOUT(secondaryCompleted.load(), 2'000);
        secondary.join();
        QCOMPARE(secondaryAcquisition.load(), SingleInstance::Acquisition::Forwarded);
        const auto arguments = activation.first().first().toStringList();
        QCOMPARE(arguments.value(1), QStringLiteral("opennow://launch/99"));
    }
};

QTEST_GUILESS_MAIN(SingleInstanceTest)
#include "tst_singleinstance.moc"
