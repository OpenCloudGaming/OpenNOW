#include "input/platform/WaylandPointerCapture.h"

#include <QSignalSpy>
#include <QTest>
#include <QWindow>
#include <QBackingStore>
#include <QPainter>
#include <limits>

class InputTestWindow final : public QWindow
{
    QBackingStore m_buffer{this};
    void exposeEvent(QExposeEvent *) override
    {
        if (!isExposed()) return;
        m_buffer.resize(size());
        const QRegion region(QRect(QPoint(), size()));
        m_buffer.beginPaint(region);
        QPainter painter(m_buffer.paintDevice());
        painter.fillRect(QRect(QPoint(), size()), Qt::darkBlue);
        painter.end();
        m_buffer.endPaint();
        m_buffer.flush(region);
    }
};

class WaylandPointerCaptureTest final : public QObject
{
    Q_OBJECT
private slots:
    void relativeMotionIsBoundedAndPreservesSubpixelRemainders()
    {
        QPointF remainder;
        QCOMPARE(WaylandPointerCapture::boundedDelta(remainder, {0.75, -0.75}), QPoint());
        QCOMPARE(WaylandPointerCapture::boundedDelta(remainder, {0.75, -0.75}), QPoint(1, -1));
        QCOMPARE(remainder, QPointF(0.5, -0.5));
        QCOMPARE(WaylandPointerCapture::boundedDelta(remainder, {1e9, -1e9}), QPoint(32767, -32768));
        QCOMPARE(remainder, QPointF());
        QCOMPARE(WaylandPointerCapture::boundedDelta(remainder,
            {std::numeric_limits<double>::infinity(), 1}), QPoint());
    }

    void inactiveWindowNeverLocks()
    {
        WaylandPointerCapture capture;
        QWindow window;
        capture.setCapture(&window, true, QRect(0, 0, 640, 480));
        QVERIFY(!capture.locked());
        capture.release();
        capture.release();
        QVERIFY(!capture.locked());
    }

    void compositorCaptureLifecycle()
    {
        if (!WaylandPointerCapture::isWayland()
                || !qEnvironmentVariableIsSet("OPENNOW_TEST_WAYLAND_CAPTURE"))
            QSKIP("Requires a Wayland compositor with an interactive pointer seat");
        InputTestWindow window;
        window.setTitle(QStringLiteral("OpenNOW Wayland input validation"));
        window.resize(640, 480);
        window.show();
        WaylandPointerCapture capture;
        QSignalSpy motion(&capture, &WaylandPointerCapture::relativeMotion);
        QTRY_VERIFY_WITH_TIMEOUT(window.isActive(), 10000);
        auto acquire = [&] {
            capture.setCapture(&window, true, QRect(QPoint(), window.size()));
            return capture.locked();
        };
        QTRY_VERIFY_WITH_TIMEOUT(acquire(), 10000);
        QTRY_VERIFY_WITH_TIMEOUT(!motion.isEmpty(), 15000);
        for (const auto fullscreen : {false, true}) {
            if (fullscreen) window.showFullScreen();
            else window.showNormal();
            QTest::qWait(250);
            QTRY_VERIFY_WITH_TIMEOUT(acquire(), 10000);
            capture.release();
            QVERIFY(!capture.locked());
            QTRY_VERIFY_WITH_TIMEOUT(acquire(), 10000);
        }
        window.hide();
        QTRY_VERIFY(!capture.locked());
    }
};

QTEST_MAIN(WaylandPointerCaptureTest)
#include "tst_waylandpointercapture.moc"
