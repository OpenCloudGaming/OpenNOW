#include "app/platform/GraphicsDeviceSelection.h"

#include <QTest>
#include <QSet>
#include <QVariantMap>

using namespace Qt::StringLiterals;

class GraphicsDeviceSelectionTest final : public QObject
{
    Q_OBJECT

private slots:
    void onlyMultiplePhysicalAdaptersExposeTheSelector()
    {
        const GraphicsDeviceSelection::Adapter integrated{u"integrated"_s, u"Integrated GPU"_s, 1, 0};
        const GraphicsDeviceSelection::Adapter discrete{u"discrete"_s, u"Discrete GPU"_s, 2, 8ULL << 30};
        const GraphicsDeviceSelection::Adapter software{u"warp"_s, u"Software adapter"_s, 3, 0, true};
        GraphicsDeviceSelection empty({}, {});
        QVERIFY(!empty.selectorVisible());
        QCOMPARE(empty.adapterLuid(), 0ULL);
        GraphicsDeviceSelection one({integrated, software, integrated}, {});
        QVERIFY(!one.selectorVisible());
        QCOMPARE(one.choices().size(), 2);
        GraphicsDeviceSelection two({integrated, software, discrete}, {});
        QVERIFY(two.selectorVisible());
        QCOMPARE(two.choices().size(), 3);
        QCOMPARE(two.adapterLuid(), 1ULL);
    }

    void stableIdentityResolvesAcrossRebootsAndDuplicateNames()
    {
        const QList<GraphicsDeviceSelection::Adapter> adapters{
            {u"first-path"_s, u"Identical GPU"_s, 0x8000000000000001ULL, 4ULL << 30},
            {u"second-path"_s, u"Identical GPU"_s, 0xffffffff00000002ULL, 4ULL << 30}};
        GraphicsDeviceSelection selected(adapters, u"second-path"_s);
        QCOMPARE(selected.adapterLuid(), 0xffffffff00000002ULL);
        QVERIFY(!selected.savedDeviceUnavailable());
        auto rebooted = adapters;
        rebooted[1].luid = 1234;
        GraphicsDeviceSelection afterReboot(rebooted, u"second-path"_s);
        QCOMPARE(afterReboot.adapterLuid(), 1234ULL);
        QCOMPARE(afterReboot.requestedDeviceId(), u"second-path"_s);
    }

    void missingSavedAdapterUsesAutomaticWithoutChangingThePreference()
    {
        GraphicsDeviceSelection selection({{u"available"_s, u"Available GPU"_s, 7}}, u"unplugged"_s);
        QVERIFY(!selection.selectorVisible());
        QVERIFY(selection.savedDeviceUnavailable());
        QCOMPARE(selection.adapterLuid(), 7ULL);
        QCOMPARE(selection.activeDeviceId(), u"available"_s);
        QCOMPARE(selection.requestedDeviceId(), u"unplugged"_s);
    }

    void automaticUsesTheFirstAdapterThatCanDecode()
    {
        const GraphicsDeviceSelection::Adapter discrete{
            u"mx110"_s, u"NVIDIA GeForce MX110"_s, 2, 2ULL << 30, false, false, false, false};
        const GraphicsDeviceSelection::Adapter integrated{
            u"hd620"_s, u"Intel(R) HD Graphics 620"_s, 1, 0, false, true, true, false};
        GraphicsDeviceSelection hybrid({discrete, integrated}, {});
        QCOMPARE(hybrid.adapterLuid(), 1ULL);
        QCOMPARE(hybrid.activeDeviceId(), u"hd620"_s);
        const auto choices = hybrid.choices();
        QCOMPARE(choices[0].toMap().value(u"codecs"_s).toStringList(),
                 QStringList({u"h264"_s, u"h265"_s}));
        QVERIFY(choices[0].toMap().value(u"detail"_s).toString().contains(u"HD Graphics 620"_s));
        QVERIFY(choices[0].toMap().value(u"detail"_s).toString().contains(u"H.264"_s));
        QCOMPARE(choices[1].toMap().value(u"codecs"_s).toStringList(), QStringList());
        QVERIFY(choices[1].toMap().value(u"detail"_s).toString().contains(u"No hardware decoder"_s));
        QVERIFY(choices[1].toMap().value(u"detail"_s).toString().contains(u"2.0 GB"_s));
        QCOMPARE(choices[2].toMap().value(u"codecs"_s).toStringList(),
                 QStringList({u"h264"_s, u"h265"_s}));
        QVERIFY(choices[2].toMap().value(u"detail"_s).toString().contains(u"H.265"_s));
        QVERIFY(choices[2].toMap().value(u"detail"_s).toString().contains(u"Shared graphics memory"_s));

        GraphicsDeviceSelection explicitDiscrete({discrete, integrated}, u"mx110"_s);
        QCOMPARE(explicitDiscrete.adapterLuid(), 2ULL);
        QCOMPARE(explicitDiscrete.activeDeviceId(), u"mx110"_s);
        QCOMPARE(explicitDiscrete.choices()[0].toMap().value(u"codecs"_s).toStringList(),
                 QStringList({u"h264"_s, u"h265"_s}));

        const GraphicsDeviceSelection::Adapter discreteDecoder{
            u"rtx"_s, u"NVIDIA GeForce RTX"_s, 5, 8ULL << 30, false, true, true, true};
        GraphicsDeviceSelection highPerformanceCanDecode({discreteDecoder, integrated}, {});
        QCOMPARE(highPerformanceCanDecode.adapterLuid(), 5ULL);
    }

    void unidentifiedHardwareDoesNotDisableAutomatic()
    {
        GraphicsDeviceSelection selection({{{}, u"Unidentified GPU"_s, 7}, {u"known"_s, u"Known GPU"_s, 8}}, {});
        QVERIFY(selection.selectorVisible());
        const auto options = selection.choices();
        QVERIFY(options[0].toMap().value(u"value"_s).toString().isEmpty());
        QVERIFY(!options[0].toMap().value(u"disabled"_s).toBool());
        QVERIFY(options[1].toMap().value(u"disabled"_s).toBool());
        QVERIFY(!options[1].toMap().value(u"value"_s).toString().isEmpty());
    }

    void hardwareEnumerationNeverReturnsSoftwareOrDuplicateLuids()
    {
        const auto adapters = GraphicsDeviceSelection::detectAdapters();
        qInfo("Detected %lld hardware graphics adapters", static_cast<long long>(adapters.size()));
        QSet<quint64> seen;
        for (const auto &adapter : adapters) {
            qInfo("Adapter: %s; persistent identity available: %d", qUtf8Printable(adapter.name), !adapter.id.isEmpty());
            QVERIFY(!adapter.software);
            QVERIFY(adapter.luid != 0);
            QVERIFY(!seen.contains(adapter.luid));
            seen.insert(adapter.luid);
        }
    }
};

QTEST_MAIN(GraphicsDeviceSelectionTest)
#include "tst_graphicsdeviceselection.moc"
