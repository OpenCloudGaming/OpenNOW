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
