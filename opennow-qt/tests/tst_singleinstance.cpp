#include "SingleInstance.h"

#include <QSignalSpy>
#include <QTest>

#include <atomic>
#include <thread>

class SingleInstanceTest final : public QObject
{
    Q_OBJECT

private slots:
    void forwardsArgumentsToPrimary()
    {
        SingleInstance primary;
        QVERIFY(primary.acquire({QStringLiteral("opennow"), QStringLiteral("opennow://launch/42")}));
        QSignalSpy activation(&primary, &SingleInstance::activationRequested);

        std::atomic_bool secondaryCompleted = false;
        std::atomic_bool secondaryBecamePrimary = true;
        std::thread secondary([&] {
            SingleInstance instance;
            secondaryBecamePrimary = instance.acquire(
                {QStringLiteral("opennow"), QStringLiteral("opennow://launch/99")});
            secondaryCompleted = true;
        });
        QTRY_COMPARE_WITH_TIMEOUT(activation.size(), 1, 2'000);
        QTRY_VERIFY_WITH_TIMEOUT(secondaryCompleted.load(), 2'000);
        secondary.join();
        QVERIFY(!secondaryBecamePrimary.load());
        const auto arguments = activation.first().first().toStringList();
        QCOMPARE(arguments.value(1), QStringLiteral("opennow://launch/99"));
    }
};

QTEST_GUILESS_MAIN(SingleInstanceTest)
#include "tst_singleinstance.moc"
