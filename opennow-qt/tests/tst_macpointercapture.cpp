#include "input/platform/MacPointerCapture.h"

#include <QGuiApplication>
#include <QMoveEvent>
#include <QPlatformSurfaceEvent>
#include <QSignalSpy>
#include <QResizeEvent>
#include <QTest>
#include <QWindow>
#include <limits>

#if defined(Q_OS_MACOS)
#include <CoreGraphics/CoreGraphics.h>
#endif

namespace {
struct NativeState
{
    QStringList calls;
    QString failAt;
    QWindow *window = nullptr;
    QRect region;
    std::function<void(QPointF)> callback;

    QString call(const QString &name)
    {
        calls.append(name);
        return name == failAt ? name + QStringLiteral(" failed") : QString();
    }

    void move(QPointF delta)
    {
        const auto deliver = callback;
        if (deliver) deliver(delta);
    }
};

class FakeNativeOperations final : public MacPointerCapture::NativeOperations
{
public:
    explicit FakeNativeOperations(std::shared_ptr<NativeState> state) : state(std::move(state)) {}
    QString associate(bool associated) override
    {
        return state->call(associated ? QStringLiteral("associate") : QStringLiteral("disassociate"));
    }
    QString center(QWindow *window, const QRect &region) override
    {
        state->window = window;
        state->region = region;
        return state->call(QStringLiteral("center"));
    }
    QString setHidden(bool hidden) override
    {
        return state->call(hidden ? QStringLiteral("hide") : QStringLiteral("show"));
    }
    QString startMotion(QWindow *, std::function<void(QPointF)> callback) override
    {
        state->callback = std::move(callback);
        return state->call(QStringLiteral("monitor"));
    }
    void stopMotion() override
    {
        if (state->callback) state->calls.append(QStringLiteral("stop"));
        state->callback = {};
    }
private:
    std::shared_ptr<NativeState> state;
};
}

class MacPointerCaptureTest final : public QObject
{
    Q_OBJECT
private slots:
    void nativeCocoaCaptureRestoresCursor()
    {
#if defined(Q_OS_MACOS)
        if (!MacPointerCapture::isSupported()) QSKIP("Requires the Cocoa platform plugin");
        QWindow window;
        window.resize(640, 480);
        auto capture = std::make_unique<MacPointerCapture>();
        for (const bool fullscreen : {false, true}) {
            if (fullscreen) window.showFullScreen();
            else window.showNormal();
            window.requestActivate();
            QTRY_VERIFY(window.isActive());
            const bool wasVisible = CGCursorIsVisible();
            capture->setCapture(&window, true, QRect(QPoint(), window.size()));
            QVERIFY2(capture->locked(), qPrintable(capture->error()));
            QTRY_VERIFY_WITH_TIMEOUT(!CGCursorIsVisible(), 1000);
            capture->release();
            QVERIFY2(capture->error().isEmpty(), qPrintable(capture->error()));
            QVERIFY(!capture->locked());
            QTRY_COMPARE_WITH_TIMEOUT(bool(CGCursorIsVisible()), wasVisible, 1000);
            capture->setCapture(&window, true, QRect(QPoint(), window.size()));
            QVERIFY2(capture->locked(), qPrintable(capture->error()));
            QTRY_VERIFY_WITH_TIMEOUT(!CGCursorIsVisible(), 1000);
            window.hide();
            QTRY_VERIFY(!capture->locked());
            QTRY_COMPARE_WITH_TIMEOUT(bool(CGCursorIsVisible()), wasVisible, 1000);
        }
        window.showNormal();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        const bool wasVisible = CGCursorIsVisible();
        capture->setCapture(&window, true, QRect(QPoint(), window.size()));
        QVERIFY2(capture->locked(), qPrintable(capture->error()));
        QTRY_VERIFY_WITH_TIMEOUT(!CGCursorIsVisible(), 1000);
        capture.reset();
        QTRY_COMPARE_WITH_TIMEOUT(bool(CGCursorIsVisible()), wasVisible, 1000);
#else
        QSKIP("Requires native macOS CoreGraphics");
#endif
    }

    void ineligibleWindowDoesNotTouchNativeState()
    {
        QWindow window;
        window.resize(640, 480);
        const auto native = std::make_shared<NativeState>();
        MacPointerCapture capture(std::make_unique<FakeNativeOperations>(native));
        capture.setCapture(&window, true, QRect(0, 0, 640, 480));
        QVERIFY(!capture.locked());
        QVERIFY(native->calls.isEmpty());
        window.show();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        capture.setCapture(&window, true, QRect());
        capture.setCapture(&window, true, QRect(1000, 1000, 100, 100));
        capture.setCapture(&window, false, QRect(0, 0, 640, 480));
        QVERIFY(!capture.locked());
        QVERIFY(native->calls.isEmpty());
    }

    void unsupportedBackendDoesNothing()
    {
        if (MacPointerCapture::isSupported()) QSKIP("The native Cocoa backend is available.");
        QWindow window;
        window.resize(640, 480);
        window.show();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        MacPointerCapture capture;
        capture.setCapture(&window, true, QRect(0, 0, 640, 480));
        QVERIFY(!capture.locked());
        QVERIFY(capture.error().isEmpty());
    }

    void lifecycleAndGeometry()
    {
        QWindow window;
        window.resize(640, 480);
        window.show();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        const auto native = std::make_shared<NativeState>();
        MacPointerCapture capture(std::make_unique<FakeNativeOperations>(native));
        QSignalSpy changes(&capture, &MacPointerCapture::stateChanged);
        const QRect initial(10, 20, 600, 400);
        capture.setCapture(&window, true, initial);
        QVERIFY(capture.locked());
        QCOMPARE(changes.count(), 1);
        QCOMPARE(native->calls, QStringList({"disassociate", "center", "hide", "monitor"}));
        QCOMPARE(native->window, &window);
        QCOMPARE(native->region, initial);
        capture.setCapture(&window, true, initial);
        QCOMPARE(native->calls.count("center"), 1);
        const QRect resized(30, 40, 500, 300);
        capture.setCapture(&window, true, resized);
        QCOMPARE(native->region, resized);
        QCOMPARE(native->calls.count("center"), 2);
        for (auto type : {QEvent::Move, QEvent::Resize, QEvent::ScreenChangeInternal,
                          QEvent::DevicePixelRatioChange}) {
            const auto before = native->calls.count("center");
            std::unique_ptr<QEvent> event;
            if (type == QEvent::Move)
                event = std::make_unique<QMoveEvent>(window.position(), window.position());
            else if (type == QEvent::Resize)
                event = std::make_unique<QResizeEvent>(window.size(), window.size());
            else
                event = std::make_unique<QEvent>(type);
            QCoreApplication::sendEvent(&window, event.get());
            QCOMPARE(native->calls.count("center"), before + 1);
            QVERIFY(capture.locked());
        }
        QCOMPARE(native->calls.count("disassociate"), 1);
        QCOMPARE(native->calls.count("hide"), 1);
        capture.release();
        QVERIFY(!capture.locked());
        QCOMPARE(native->calls.last(3), QStringList({"stop", "associate", "show"}));
        const auto calls = native->calls;
        capture.release();
        QCOMPARE(native->calls, calls);
        QCOMPARE(changes.count(), 2);
    }

    void acquisitionFailureRollsBack_data()
    {
        QTest::addColumn<QString>("failure");
        QTest::addColumn<bool>("restoresAssociation");
        QTest::addColumn<bool>("restoresVisibility");
        QTest::newRow("disassociation") << QString("disassociate") << false << false;
        QTest::newRow("centering") << QString("center") << true << false;
        QTest::newRow("cursor-hide") << QString("hide") << true << false;
        QTest::newRow("monitor") << QString("monitor") << true << true;
    }

    void acquisitionFailureRollsBack()
    {
        QFETCH(QString, failure);
        QFETCH(bool, restoresAssociation);
        QFETCH(bool, restoresVisibility);
        QWindow window;
        window.resize(640, 480);
        window.show();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        const auto native = std::make_shared<NativeState>();
        native->failAt = failure;
        MacPointerCapture capture(std::make_unique<FakeNativeOperations>(native));
        QSignalSpy changes(&capture, &MacPointerCapture::stateChanged);
        capture.setCapture(&window, true, QRect(0, 0, 640, 480));
        QVERIFY(!capture.locked());
        QVERIFY(capture.error().contains(failure));
        QVERIFY(!native->callback);
        QCOMPARE(native->calls.contains("associate"), restoresAssociation);
        QCOMPARE(native->calls.contains("show"), restoresVisibility);
        const auto error = capture.error();
        const auto calls = native->calls;
        capture.setCapture(&window, true, QRect(0, 0, 640, 480));
        QCOMPARE(capture.error(), error);
        QCOMPARE(native->calls, calls);
        QCOMPARE(changes.count(), 1);
        native->failAt.clear();
        capture.release();
        capture.setCapture(&window, true, QRect(0, 0, 640, 480));
        QVERIFY(capture.locked());
        QVERIFY(capture.error().isEmpty());
    }

    void releaseFailureIsReportedAndRetried_data()
    {
        QTest::addColumn<QString>("failure");
        QTest::newRow("association") << QString("associate");
        QTest::newRow("visibility") << QString("show");
    }

    void changedGeometryAllowsRetryAfterFailure_data()
    {
        QTest::addColumn<bool>("changeRegion");
        QTest::newRow("viewport") << true;
        QTest::newRow("window-position") << false;
    }

    void changedGeometryAllowsRetryAfterFailure()
    {
        QFETCH(bool, changeRegion);
        QWindow window;
        window.resize(640, 480);
        window.show();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        const auto native = std::make_shared<NativeState>();
        native->failAt = QStringLiteral("center");
        MacPointerCapture capture(std::make_unique<FakeNativeOperations>(native));
        QRect region(0, 0, 640, 480);
        capture.setCapture(&window, true, region);
        QVERIFY(!capture.locked());
        native->failAt.clear();
        capture.setCapture(&window, true, region);
        QVERIFY(!capture.locked());
        if (changeRegion) region.adjust(10, 10, -10, -10);
        else window.setPosition(window.position() + QPoint(10, 10));
        capture.setCapture(&window, true, region);
        QVERIFY(capture.locked());
        QVERIFY(capture.error().isEmpty());
    }

    void releaseFailureIsReportedAndRetried()
    {
        QFETCH(QString, failure);
        QWindow window;
        window.resize(640, 480);
        window.show();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        const auto native = std::make_shared<NativeState>();
        MacPointerCapture capture(std::make_unique<FakeNativeOperations>(native));
        capture.setCapture(&window, true, QRect(0, 0, 640, 480));
        native->failAt = failure;
        capture.release();
        QVERIFY(!capture.locked());
        QVERIFY(capture.error().contains(failure));
        QVERIFY(native->calls.contains("show"));
        QVERIFY(native->calls.contains("associate"));
        const auto secondNative = std::make_shared<NativeState>();
        MacPointerCapture second(std::make_unique<FakeNativeOperations>(secondNative));
        second.setCapture(&window, true, QRect(0, 0, 640, 480));
        QVERIFY(!second.locked());
        QVERIFY(secondNative->calls.isEmpty());
        native->failAt.clear();
        capture.release();
        QCOMPARE(native->calls.count(failure), 2);
        second.release();
        second.setCapture(&window, true, QRect(0, 0, 640, 480));
        QVERIFY(second.locked());
    }

    void lifecycleLossRestoresCursor_data()
    {
        QTest::addColumn<int>("type");
        QTest::newRow("focus") << int(QEvent::FocusOut);
        QTest::newRow("deactivation") << int(QEvent::WindowDeactivate);
        QTest::newRow("hide") << int(QEvent::Hide);
        QTest::newRow("close") << int(QEvent::Close);
        QTest::newRow("surface") << int(QEvent::PlatformSurface);
        QTest::newRow("application") << -1;
        QTest::newRow("window-destruction") << -2;
        QTest::newRow("owner-destruction") << -3;
    }

    void lifecycleLossRestoresCursor()
    {
        QFETCH(int, type);
        auto window = std::make_unique<QWindow>();
        window->resize(640, 480);
        window->show();
        window->requestActivate();
        QTRY_VERIFY(window->isActive());
        const auto native = std::make_shared<NativeState>();
        auto capture = std::make_unique<MacPointerCapture>(std::make_unique<FakeNativeOperations>(native));
        capture->setCapture(window.get(), true, QRect(0, 0, 640, 480));
        QVERIFY(capture->locked());
        if (type == -1) {
            QVERIFY(QMetaObject::invokeMethod(qApp, "applicationStateChanged", Qt::DirectConnection,
                                              Q_ARG(Qt::ApplicationState, Qt::ApplicationInactive)));
        } else if (type == -2) {
            window.reset();
        } else if (type == -3) {
            capture.reset();
        } else if (type == QEvent::PlatformSurface) {
            QPlatformSurfaceEvent event(QPlatformSurfaceEvent::SurfaceAboutToBeDestroyed);
            QCoreApplication::sendEvent(window.get(), &event);
        } else {
            QEvent event{QEvent::Type(type)};
            QCoreApplication::sendEvent(window.get(), &event);
        }
        if (capture) QVERIFY(!capture->locked());
        QCOMPARE(native->calls.count("associate"), 1);
        QCOMPARE(native->calls.count("show"), 1);
        QVERIFY(!native->callback);
    }

    void relativeMotionSplitsAndPreservesFractions()
    {
        QWindow window;
        window.resize(640, 480);
        window.show();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        const auto native = std::make_shared<NativeState>();
        MacPointerCapture capture(std::make_unique<FakeNativeOperations>(native));
        QSignalSpy motion(&capture, &MacPointerCapture::relativeMotion);
        capture.setCapture(&window, true, QRect(0, 0, 640, 480));
        native->move({0.75, -0.75});
        QVERIFY(motion.isEmpty());
        native->move({70000.75, -70000.75});
        QCOMPARE(motion.count(), 3);
        int x = 0;
        int y = 0;
        for (const auto &event : motion) {
            x += event.at(0).value<qint16>();
            y += event.at(1).value<qint16>();
        }
        QCOMPARE(x, 70001);
        QCOMPARE(y, -70001);
        motion.clear();
        native->move({0.5, -0.5});
        QCOMPARE(motion.count(), 1);
        QCOMPARE(motion.first().at(0).value<qint16>(), 1);
        QCOMPARE(motion.first().at(1).value<qint16>(), -1);
        native->move({std::numeric_limits<double>::infinity(), 0});
        QVERIFY(!capture.locked());
        QVERIFY(!capture.error().isEmpty());
    }

    void releaseDuringMotionStopsRemainingChunks()
    {
        QWindow window;
        window.resize(640, 480);
        window.show();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        const auto native = std::make_shared<NativeState>();
        MacPointerCapture capture(std::make_unique<FakeNativeOperations>(native));
        QSignalSpy motion(&capture, &MacPointerCapture::relativeMotion);
        connect(&capture, &MacPointerCapture::relativeMotion, &capture, &MacPointerCapture::release);
        capture.setCapture(&window, true, QRect(0, 0, 640, 480));
        native->move({100000, -100000});
        QCOMPARE(motion.count(), 1);
        QVERIFY(!capture.locked());
    }

    void reacquisitionDuringMotionDoesNotForwardOldChunks()
    {
        QWindow window;
        window.resize(640, 480);
        window.show();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        const auto native = std::make_shared<NativeState>();
        MacPointerCapture capture(std::make_unique<FakeNativeOperations>(native));
        QSignalSpy motion(&capture, &MacPointerCapture::relativeMotion);
        connect(&capture, &MacPointerCapture::relativeMotion, &capture, [&] {
            capture.release();
            capture.setCapture(&window, true, QRect(0, 0, 640, 480));
        });
        capture.setCapture(&window, true, QRect(0, 0, 640, 480));
        native->move({100000.75, -100000.75});
        QCOMPARE(motion.count(), 1);
        QVERIFY(capture.locked());
        native->move({0.5, -0.5});
        QCOMPARE(motion.count(), 1);
    }

    void destructionRetriesFailedRestoreWithoutDoubleShowing()
    {
        QWindow window;
        window.resize(640, 480);
        window.show();
        window.requestActivate();
        QTRY_VERIFY(window.isActive());
        const auto native = std::make_shared<NativeState>();
        {
            MacPointerCapture capture(std::make_unique<FakeNativeOperations>(native));
            capture.setCapture(&window, true, QRect(0, 0, 640, 480));
            QVERIFY(capture.locked());
            native->failAt = QStringLiteral("associate");
        }
        QCOMPARE(native->calls.count("associate"), 2);
        QCOMPARE(native->calls.count("show"), 1);
        QVERIFY(!native->callback);
    }
};

QTEST_MAIN(MacPointerCaptureTest)
#include "tst_macpointercapture.moc"
