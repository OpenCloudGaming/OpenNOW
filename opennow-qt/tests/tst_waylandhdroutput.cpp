#include "streaming/rendering/WaylandHdrOutput.h"

#include <QGuiApplication>
#include <QTest>
#include <QWindow>
#include <limits>

class WaylandHdrOutputTest : public QObject
{
    Q_OBJECT
    using Description = WaylandHdrOutput::Description;

    static Description hdrDescription()
    {
        Description value;
        value.ready = value.complete = value.primaries = value.pq = true;
        value.luminances = value.targetLuminance = true;
        value.minimum = value.targetMinimum = 0.005;
        value.maximum = 10000;
        value.white = 203;
        value.targetMaximum = 1000;
        value.primariesValue = {0.68, 0.32, 0.265, 0.69, 0.15, 0.06, 0.3127, 0.329};
        return value;
    }

private slots:
    void configuredPqWithHeadroom()
    {
        auto value = hdrDescription();
        for (double white : {80.0, 203.0, 500.0}) {
            value.white = white;
            const auto state = WaylandHdrOutput::stateForDescription(value);
            QVERIFY(state.supported);
            QCOMPARE(state.whiteNits, float(white));
        }
    }

    void incompleteDescriptionsFailClosed()
    {
        QVERIFY(!WaylandHdrOutput::stateForDescription({}).supported);
        for (bool Description::*field : {&Description::ready, &Description::complete,
                 &Description::primaries, &Description::pq, &Description::luminances,
                 &Description::targetLuminance}) {
            auto value = hdrDescription();
            value.*field = false;
            const auto state = WaylandHdrOutput::stateForDescription(value);
            QVERIFY(!state.supported);
            QCOMPARE(state.whiteNits, 203.0f);
        }
    }

    void sdrAndIccAreNotHdrEvidence()
    {
        auto value = hdrDescription();
        value.pq = false;
        QVERIFY(!WaylandHdrOutput::stateForDescription(value).supported);
        value = hdrDescription();
        value.power = true;
        QVERIFY(!WaylandHdrOutput::stateForDescription(value).supported);
        value = hdrDescription();
        value.icc = true;
        QVERIFY(!WaylandHdrOutput::stateForDescription(value).supported);
    }

    void headroomUsesTargetNotEncodingMaximum()
    {
        auto value = hdrDescription();
        for (double peak : {80.0, 203.0}) {
            value.targetMaximum = peak;
            QVERIFY(!WaylandHdrOutput::stateForDescription(value).supported);
        }
    }

    void invalidLuminancesFailClosed()
    {
        const double nan = std::numeric_limits<double>::quiet_NaN();
        const double infinity = std::numeric_limits<double>::infinity();
        for (double Description::*field : {&Description::white, &Description::minimum,
                 &Description::maximum, &Description::targetMinimum, &Description::targetMaximum}) {
            for (double invalid : {nan, infinity, -1.0}) {
                auto value = hdrDescription();
                value.*field = invalid;
                QVERIFY(!WaylandHdrOutput::stateForDescription(value).supported);
            }
        }
        for (double white : {0.0, 79.0, 501.0}) {
            auto value = hdrDescription();
            value.white = white;
            QVERIFY(!WaylandHdrOutput::stateForDescription(value).supported);
        }
        auto value = hdrDescription();
        value.targetMaximum = 10001;
        QVERIFY(!WaylandHdrOutput::stateForDescription(value).supported);
    }

    void publishedLuminancesAndTargetPrimariesCarryMeasuredValues()
    {
        auto value = hdrDescription();
        value.minimum = 0.005;
        value.maximum = 10000;
        value.white = 203;
        value.targetMinimum = 0.0005;
        value.targetMaximum = 620;
        const auto state = WaylandHdrOutput::stateForDescription(value);
        QVERIFY(state.supported);
        QCOMPARE(state.whiteNits, 203.0f);
        QCOMPARE(state.minimumNits, 0.005f);
        QCOMPARE(state.maximumNits, 10000.0f);
        QCOMPARE(state.targetMinimumNits, 0.0005f);
        QCOMPARE(state.targetMaximumNits, 620.0f);
        QCOMPARE(state.targetPrimaries[0], 0.68);
        QCOMPARE(state.targetPrimaries[7], 0.329);
    }

    void targetPrimariesOverrideTheEncodingVolume()
    {
        auto value = hdrDescription();
        value.targetPrimaries = true;
        value.targetPrimariesValue = {0.708, 0.292, 0.17, 0.797, 0.131, 0.046, 0.3127, 0.329};
        const auto state = WaylandHdrOutput::stateForDescription(value);
        QVERIFY(state.supported);
        QCOMPARE(state.targetPrimaries[0], 0.708);
        auto withoutTarget = hdrDescription();
        const auto encodingVolume = WaylandHdrOutput::stateForDescription(withoutTarget);
        QVERIFY(encodingVolume.supported);
        QCOMPARE(encodingVolume.targetPrimaries[0], 0.68);
    }

    void invalidPrimariesFailClosed()
    {
        const double nan = std::numeric_limits<double>::quiet_NaN();
        const double infinity = std::numeric_limits<double>::infinity();
        for (size_t index = 0; index < 8; ++index) {
            for (double invalid : {nan, infinity, -0.1, 1.5}) {
                auto value = hdrDescription();
                value.targetPrimaries = true;
                value.targetPrimariesValue[index] = invalid;
                QVERIFY(!WaylandHdrOutput::stateForDescription(value).supported);
                auto encoding = hdrDescription();
                encoding.primariesValue[index] = invalid;
                QVERIFY(!WaylandHdrOutput::stateForDescription(encoding).supported);
            }
        }
    }

    void unsupportedDescriptionsPublishNoMeasuredValues()
    {
        const auto state = WaylandHdrOutput::stateForDescription({});
        QVERIFY(!state.supported);
        QCOMPARE(state.minimumNits, 0.0f);
        QCOMPARE(state.maximumNits, 0.0f);
        QCOMPARE(state.targetMinimumNits, 0.0f);
        QCOMPARE(state.targetMaximumNits, 0.0f);
        for (double coordinate : state.targetPrimaries) QCOMPARE(coordinate, 0.0);
    }

    void nonWaylandAndWindowLifecycleFailClosed()
    {
        QVERIFY(!QGuiApplication::platformName().startsWith(QStringLiteral("wayland")));
        WaylandHdrOutput observer;
        QVERIFY(!observer.state().supported);
        {
            QWindow window;
            observer.attach(&window);
            window.show();
            QCoreApplication::processEvents();
            QVERIFY(!observer.state().supported);
            window.resize(100, 100);
            window.destroy();
            QCoreApplication::processEvents();
            QVERIFY(!observer.state().supported);
            window.create();
            QCoreApplication::processEvents();
            QVERIFY(!observer.state().supported);
        }
        QCoreApplication::processEvents();
        observer.attach(nullptr);
        QVERIFY(!observer.state().supported);
        QCOMPARE(observer.state().whiteNits, 203.0f);
    }
};

QTEST_MAIN(WaylandHdrOutputTest)
#include "tst_waylandhdroutput.moc"
