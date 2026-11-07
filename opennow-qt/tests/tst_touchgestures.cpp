#include <QtTest>

#include "streaming/StreamTouchGestures.h"

using Mode = StreamTouchGestures::Mode;

class TouchGesturesTest final : public QObject
{
    Q_OBJECT

    QStringList m_log;
    bool m_timerArmed = false;

    StreamTouchGestures gestures()
    {
        m_log.clear();
        m_timerArmed = false;
        return StreamTouchGestures(StreamTouchGestures::Sink{
            [this](QPointF point) {
                m_log << QStringLiteral("to %1,%2").arg(point.x()).arg(point.y());
            },
            [this](QPointF delta) {
                m_log << QStringLiteral("by %1,%2").arg(delta.x()).arg(delta.y());
            },
            [this](int button, bool pressed) {
                m_log << QStringLiteral("button %1 %2").arg(button).arg(pressed ? u"down" : u"up");
            },
            [this](int x, int y) { m_log << QStringLiteral("wheel %1,%2").arg(x).arg(y); },
            [this](bool armed) { m_timerArmed = armed; },
            [this] { m_log << QStringLiteral("switch mode"); }});
    }

private slots:
    void directTapMovesThenClicks()
    {
        auto touch = gestures();
        touch.pressed(1, {100, 200}, 0, Mode::Direct);
        touch.released(1, {101, 201}, 80);
        QCOMPARE(m_log, QStringList({"to 100,200", "to 101,201", "button 1 down", "button 1 up"}));
    }

    void trackpadReleaseMovementIsAMoveNotATap()
    {
        auto touch = gestures();
        touch.pressed(1, {100, 100}, 0, Mode::Trackpad);
        touch.released(1, {250, 100}, 60);
        QCOMPARE(m_log, QStringList({"by 150,0"}));
    }

    void directDragKeepsItsLastSegmentOnRelease()
    {
        auto touch = gestures();
        touch.pressed(1, {100, 200}, 0, Mode::Direct);
        touch.moved(1, {150, 200}, 10);
        touch.released(1, {180, 220}, 20);
        QCOMPARE(m_log, QStringList({"to 100,200", "button 1 down", "to 150,200", "to 180,220",
                                     "button 1 up"}));
    }

    void directHoldRightClicks()
    {
        auto touch = gestures();
        touch.pressed(1, {100, 200}, 0, Mode::Direct);
        touch.holdTimeout();
        touch.released(1, {100, 200}, 700);
        QCOMPARE(m_log, QStringList({"to 100,200", "button 3 down", "button 3 up"}));
    }

    void directDragHoldsTheLeftButton()
    {
        auto touch = gestures();
        touch.pressed(1, {100, 200}, 0, Mode::Direct);
        touch.moved(1, {105, 200}, 10);
        touch.moved(1, {150, 200}, 20);
        touch.moved(1, {180, 220}, 30);
        touch.released(1, {180, 220}, 40);
        QCOMPARE(m_log, QStringList({"to 100,200", "button 1 down", "to 150,200", "to 180,220",
                                     "button 1 up"}));
    }

    void trackpadTapClicksAtTheCursor()
    {
        auto touch = gestures();
        touch.pressed(1, {500, 500}, 0, Mode::Trackpad);
        touch.released(1, {500, 500}, 90);
        QCOMPARE(m_log, QStringList({"button 1 down", "button 1 up"}));
    }

    void trackpadMoveIsRelativeAndKeepsShortMoves()
    {
        auto touch = gestures();
        touch.pressed(1, {500, 500}, 0, Mode::Trackpad);
        touch.moved(1, {505, 500}, 10);
        touch.moved(1, {530, 510}, 20);
        touch.moved(1, {540, 510}, 30);
        touch.released(1, {540, 510}, 40);
        QCOMPARE(m_log, QStringList({"by 30,10", "by 10,0"}));
    }

    void trackpadHoldThenMoveDrags()
    {
        auto touch = gestures();
        touch.pressed(1, {500, 500}, 0, Mode::Trackpad);
        touch.holdTimeout();
        touch.moved(1, {560, 500}, 600);
        touch.released(1, {560, 500}, 700);
        QCOMPARE(m_log, QStringList({"button 1 down", "by 60,0", "button 1 up"}));
    }

    void twoFingerTapRightClicks()
    {
        auto touch = gestures();
        touch.pressed(1, {500, 500}, 0, Mode::Trackpad);
        touch.pressed(2, {600, 500}, 30, Mode::Trackpad);
        touch.released(1, {500, 500}, 120);
        touch.released(2, {600, 500}, 130);
        QCOMPARE(m_log, QStringList({"button 3 down", "button 3 up"}));
    }

    void threeFingerTapSwitchesModeWithoutClicking()
    {
        auto touch = gestures();
        touch.pressed(1, {500, 500}, 0, Mode::Trackpad);
        touch.pressed(2, {600, 500}, 20, Mode::Trackpad);
        touch.pressed(3, {700, 500}, 40, Mode::Trackpad);
        touch.released(1, {500, 500}, 150);
        touch.released(2, {600, 500}, 160);
        touch.released(3, {700, 500}, 170);
        QCOMPARE(m_log, QStringList({"switch mode"}));
    }

    void slowThreeFingerContactDoesNothing()
    {
        auto touch = gestures();
        touch.pressed(1, {500, 500}, 0, Mode::Trackpad);
        touch.pressed(2, {600, 500}, 20, Mode::Trackpad);
        touch.pressed(3, {700, 500}, 40, Mode::Trackpad);
        touch.released(1, {500, 500}, 900);
        touch.released(2, {600, 500}, 910);
        touch.released(3, {700, 500}, 920);
        QVERIFY(m_log.isEmpty());
    }

    void twoFingerDragScrolls()
    {
        auto touch = gestures();
        touch.pressed(1, {500, 500}, 0, Mode::Direct);
        touch.pressed(2, {600, 500}, 20, Mode::Direct);
        touch.moved(1, {500, 540}, 40);
        touch.moved(2, {600, 540}, 40);
        touch.released(1, {500, 540}, 200);
        touch.released(2, {600, 540}, 210);
        int total = 0;
        for (const auto &entry : std::as_const(m_log)) {
            QVERIFY(!entry.startsWith(u"button"));
            if (entry.startsWith(u"wheel")) total += entry.section(u',', 1).toInt();
        }
        QCOMPARE(total, 120);
    }

    void cancelReleasesAHeldButtonAndTheTimer()
    {
        auto touch = gestures();
        touch.pressed(1, {100, 100}, 0, Mode::Direct);
        touch.moved(1, {200, 100}, 10);
        touch.cancel();
        QCOMPARE(m_log, QStringList({"to 100,100", "button 1 down", "to 200,100", "button 1 up"}));
        QVERIFY(!m_timerArmed);
    }

    void leftoverFingerDoesNothing()
    {
        auto touch = gestures();
        touch.pressed(1, {500, 500}, 0, Mode::Trackpad);
        touch.pressed(2, {600, 500}, 10, Mode::Trackpad);
        touch.released(2, {600, 500}, 400);
        touch.moved(1, {700, 500}, 450);
        touch.released(1, {700, 500}, 500);
        QVERIFY(m_log.isEmpty());
    }
};

QTEST_APPLESS_MAIN(TouchGesturesTest)
#include "tst_touchgestures.moc"
