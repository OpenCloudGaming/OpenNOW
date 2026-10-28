#include "input/platform/WindowsWindowMoveLoop.h"

#include <QGuiApplication>
#include <QScopeGuard>
#include <QTest>
#include <QWindow>

#if defined(Q_OS_WIN)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <commctrl.h>
#endif

class WindowsWindowMoveLoopTest final : public QObject
{
    Q_OBJECT

private slots:
    void nativeSurfaceLifetimeDoesNotCreateAnEarlyWindow()
    {
        struct State {
            quintptr handle = 0;
            int installs = 0;
            int removals = 0;
        } state;
        class Operations final : public WindowsWindowMoveLoop::NativeOperations {
        public:
            explicit Operations(State &state) : state(state) {}
            bool install(quintptr window, WindowsWindowMoveLoop *) override
            {
                state.handle = window;
                ++state.installs;
                return true;
            }
            void remove(quintptr, WindowsWindowMoveLoop *) override
            {
                state.handle = 0;
                ++state.removals;
            }
            qintptr forward(quintptr, quint32, quintptr, qintptr) override { return 0; }
            void setCursorPosition(int, int) override { QFAIL("No cursor movement expected"); }
            State &state;
        };
        QWindow window;
        WindowsWindowMoveLoop owner([] { return true; }, [](bool) {},
                                    std::make_unique<Operations>(state));
        owner.setWindow(&window);
        QVERIFY(!window.handle());
        QCOMPARE(state.installs, 0);
        window.create();
        QCOMPARE(state.handle, quintptr(window.winId()));
        QCOMPARE(state.installs, 1);
        window.destroy();
        QCOMPARE(state.handle, quintptr(0));
        QCOMPARE(state.removals, 1);
        window.create();
        QCOMPARE(state.handle, quintptr(window.winId()));
        QCOMPARE(state.installs, 2);
        owner.setWindow(nullptr);
        QCOMPARE(state.handle, quintptr(0));
        QCOMPARE(state.removals, 2);
        window.destroy();
        QCOMPARE(state.removals, 2);
    }

#if defined(Q_OS_WIN)
    void sentSystemCommandsSuspendBeforeQtDispatch_data()
    {
        QTest::addColumn<quintptr>("command");
        QTest::newRow("mouse-move") << quintptr(SC_MOVE | HTCAPTION);
        QTest::newRow("mouse-size") << quintptr(SC_SIZE | WMSZ_LEFT);
        QTest::newRow("keyboard-move") << quintptr(SC_MOVE);
        QTest::newRow("keyboard-size") << quintptr(SC_SIZE);
    }

    void sentSystemCommandsSuspendBeforeQtDispatch()
    {
        QFETCH(quintptr, command);
        QCOMPARE(QGuiApplication::platformName(), QStringLiteral("windows"));
        QWindow window;
        window.create();
        const auto hwnd = reinterpret_cast<HWND>(window.winId());
        QVERIFY(IsWindow(hwnd));
        QVERIFY(!IsWindowVisible(hwnd));
        struct Probe {
            bool enabled = true;
            bool captureActive = true;
            bool moveActive = false;
            QList<bool> transitions;
            int calls = 0;
            LPARAM position = 0;
        } probe;
        const auto procedure = +[](HWND window, UINT message, WPARAM wParam, LPARAM lParam,
                                    UINT_PTR, DWORD_PTR data) -> LRESULT {
            if (message == WM_SYSCOMMAND
                && ((wParam & 0xfff0) == SC_MOVE || (wParam & 0xfff0) == SC_SIZE)) {
                auto &probe = *reinterpret_cast<Probe *>(data);
                probe.moveActive = !probe.transitions.isEmpty() && probe.transitions.last();
                ++probe.calls;
                probe.position = lParam;
                return 42;
            }
            return DefSubclassProc(window, message, wParam, lParam);
        };
        QVERIFY(SetWindowSubclass(hwnd, procedure, 1, reinterpret_cast<DWORD_PTR>(&probe)));
        const auto removeProbe = qScopeGuard([&] { RemoveWindowSubclass(hwnd, procedure, 1); });
        POINT originalCursor{};
        const bool haveCursor = GetCursorPos(&originalCursor);
        const auto restoreCursor = qScopeGuard([&] {
            if (haveCursor) SetCursorPos(originalCursor.x, originalCursor.y);
        });
        WindowsWindowMoveLoop owner([&] { return probe.enabled; }, [&](bool active) {
            probe.transitions.append(active);
            if (active) probe.captureActive = false;
            else QMetaObject::invokeMethod(&window, [&] { probe.captureActive = true; },
                                            Qt::QueuedConnection);
        });
        owner.setWindow(&window);
        const auto position = MAKELPARAM(quint16(-1800), quint16(310));
        QCOMPARE(SendMessageW(hwnd, WM_SYSCOMMAND, command, position), LRESULT(42));
        QCOMPARE(probe.calls, 1);
        QVERIFY(probe.moveActive);
        QVERIFY(!probe.captureActive);
        QCOMPARE(probe.position, LPARAM(position));
        QCOMPARE(probe.transitions, (QList<bool>{true, false}));
        QTRY_VERIFY(probe.captureActive);

        probe.enabled = false;
        probe.captureActive = false;
        probe.transitions.clear();
        QCOMPARE(SendMessageW(hwnd, WM_SYSCOMMAND, command, position), LRESULT(42));
        QCOMPARE(probe.calls, 2);
        QVERIFY(!probe.moveActive);
        QVERIFY(!probe.captureActive);
        QVERIFY(probe.transitions.isEmpty());
    }
#endif
};

QTEST_MAIN(WindowsWindowMoveLoopTest)
#include "tst_windowswindowmoveloop.moc"
