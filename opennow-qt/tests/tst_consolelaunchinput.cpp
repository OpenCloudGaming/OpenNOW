#include "app/AppController.h"
#include "input/ControllerInput.h"
#include "input/InputModeTracker.h"

#include <QGuiApplication>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QScopeGuard>
#include <QSignalSpy>
#include <QTest>

class LaunchInputSink final : public QObject
{
public:
    int presses = 0;
    int releases = 0;
    bool event(QEvent *event) override
    {
        if (event->type() == QEvent::KeyPress || event->type() == QEvent::MouseButtonPress) ++presses;
        if (event->type() == QEvent::KeyRelease || event->type() == QEvent::MouseButtonRelease) ++releases;
        return true;
    }
    bool eventFilter(QObject *, QEvent *event) override
    {
        if (event->type() == QEvent::KeyPress) ++presses;
        return false;
    }
};

class ConsoleLaunchInputTest final : public QObject
{
    Q_OBJECT
private slots:
    void keyboardSkipDrainsThroughFinish()
    {
        AppController controller;
        InputModeTracker tracker(&controller);
        LaunchInputSink sink;
        sink.installEventFilter(&tracker);
        controller.setInputMode(QStringLiteral("controller"));
        controller.setConsoleLaunchInputBlocked(true);
        QSignalSpy skip(&controller, &AppController::consoleLaunchSkipRequested);
        QKeyEvent press(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier);
        QCoreApplication::sendEvent(&sink, &press);
        QCOMPARE(skip.count(), 1);
        QVERIFY(controller.consoleLaunchInputDraining());
        QCOMPARE(controller.inputMode(), QStringLiteral("controller"));
        controller.setConsoleLaunchInputBlocked(false);
        QKeyEvent repeat(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier, {}, true);
        QCoreApplication::sendEvent(&sink, &repeat);
        QKeyEvent repeatRelease(QEvent::KeyRelease, Qt::Key_Return, Qt::NoModifier, {}, true);
        QCoreApplication::sendEvent(&sink, &repeatRelease);
        QVERIFY(controller.consoleLaunchInputDraining());
        QKeyEvent release(QEvent::KeyRelease, Qt::Key_Return, Qt::NoModifier);
        QCoreApplication::sendEvent(&sink, &release);
        QVERIFY(!controller.consoleLaunchInputDraining());
        QCOMPARE(sink.presses, 0);
        QCOMPARE(sink.releases, 0);
        QCoreApplication::sendEvent(&sink, &press);
        QCOMPARE(sink.presses, 1);
    }

    void pointerSkipDoesNotChangeMode()
    {
        AppController controller;
        InputModeTracker tracker(&controller);
        LaunchInputSink sink;
        sink.installEventFilter(&tracker);
        controller.setInputMode(QStringLiteral("controller"));
        controller.setConsoleLaunchInputBlocked(true);
        QSignalSpy skip(&controller, &AppController::consoleLaunchSkipRequested);
        QMouseEvent press(QEvent::MouseButtonPress, QPointF(10, 10), QPointF(10, 10),
                          Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
        QCoreApplication::sendEvent(&sink, &press);
        QCOMPARE(skip.count(), 1);
        controller.setConsoleLaunchInputBlocked(false);
        QVERIFY(controller.consoleLaunchInputDraining());
        QMouseEvent release(QEvent::MouseButtonRelease, QPointF(10, 10), QPointF(10, 10),
                            Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
        QCoreApplication::sendEvent(&sink, &release);
        QVERIFY(!controller.consoleLaunchInputDraining());
        QCOMPARE(sink.presses, 0);
        QCOMPARE(sink.releases, 0);
        QCOMPARE(controller.inputMode(), QStringLiteral("controller"));
    }

    void shortcutOverrideIsConsumed()
    {
        AppController controller;
        InputModeTracker tracker(&controller);
        LaunchInputSink sink;
        sink.installEventFilter(&tracker);
        controller.setConsoleLaunchInputBlocked(true);
        QKeyEvent shortcut(QEvent::ShortcutOverride, Qt::Key_F10, Qt::NoModifier);
        shortcut.ignore();
        QCoreApplication::sendEvent(&sink, &shortcut);
        QVERIFY(shortcut.isAccepted());
    }

    void deactivationReleasesDrainWithoutLeakingHeldRepeat()
    {
        AppController controller;
        InputModeTracker tracker(&controller);
        LaunchInputSink sink;
        sink.installEventFilter(&tracker);
        controller.setConsoleLaunchInputBlocked(true);
        QKeyEvent press(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier);
        QCoreApplication::sendEvent(&sink, &press);
        QVERIFY(controller.consoleLaunchInputDraining());
        QEvent deactivate(QEvent::ApplicationDeactivate);
        QCoreApplication::sendEvent(&sink, &deactivate);
        controller.setConsoleLaunchInputBlocked(false);
        QVERIFY(!controller.consoleLaunchInputDraining());
        QKeyEvent repeat(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier, {}, true);
        QCoreApplication::sendEvent(&sink, &repeat);
        QKeyEvent release(QEvent::KeyRelease, Qt::Key_Return, Qt::NoModifier);
        QCoreApplication::sendEvent(&sink, &release);
        QCOMPARE(sink.presses, 0);
        QCOMPARE(sink.releases, 0);
        QCoreApplication::sendEvent(&sink, &press);
        QCOMPARE(sink.presses, 1);
    }

    void touchSkipDrainsThroughFinish()
    {
        AppController controller;
        InputModeTracker tracker(&controller);
        LaunchInputSink sink;
        sink.installEventFilter(&tracker);
        controller.setConsoleLaunchInputBlocked(true);
        QSignalSpy skip(&controller, &AppController::consoleLaunchSkipRequested);
        QEvent begin(QEvent::TouchBegin);
        QCoreApplication::sendEvent(&sink, &begin);
        QCOMPARE(skip.count(), 1);
        QVERIFY(controller.consoleLaunchInputDraining());
        controller.setConsoleLaunchInputBlocked(false);
        QEvent end(QEvent::TouchEnd);
        QCoreApplication::sendEvent(&sink, &end);
        QVERIFY(!controller.consoleLaunchInputDraining());
    }

    void queuedControllerKeyDoesNotCreateKeyboardDrain()
    {
        AppController controller;
        InputModeTracker tracker(&controller);
        LaunchInputSink sink;
        sink.installEventFilter(&tracker);
        controller.setConsoleLaunchInputBlocked(true);
        QKeyEvent press(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier,
                        ControllerInput::syntheticControllerScanCode, 0, 0);
        QCoreApplication::sendEvent(&sink, &press);
        QVERIFY(!controller.consoleLaunchInputDraining());
        controller.setConsoleLaunchInputBlocked(false);
        QCOMPARE(sink.presses, 0);
    }

    void controllerSkipDrainsThroughFinish_data()
    {
        QTest::addColumn<bool>("axis");
        QTest::addColumn<bool>("disconnect");
        QTest::newRow("cross-release") << false << false;
        QTest::newRow("stick-neutral") << true << false;
        QTest::newRow("cross-disconnect") << false << true;
        QTest::newRow("stick-disconnect") << true << true;
    }

    void controllerSkipDrainsThroughFinish()
    {
        QFETCH(bool, axis);
        QFETCH(bool, disconnect);
        ControllerInput input;
        LaunchInputSink sink;
        QCoreApplication::instance()->installEventFilter(&sink);
        const auto removeFilter = qScopeGuard([&] { QCoreApplication::instance()->removeEventFilter(&sink); });
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.name = "OpenNOW launch drain regression";
        auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        const auto cleanup = qScopeGuard([&] { if (id) SDL_DetachVirtualJoystick(id); });
        QTRY_COMPARE(input.controllerCount(), 1);
        auto *joystick = SDL_GetJoystickFromID(id);
        QVERIFY(joystick);
        input.setShellInputBlocked(true);
        QSignalSpy skip(&input, &ControllerInput::shellInputSkipRequested);
        if (axis) QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTX, 24000));
        else QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QTRY_VERIFY(skip.count() > 0);
        QVERIFY(input.shellInputDraining());
        input.setShellInputBlocked(false);
        QTest::qWait(600);
        QCOMPARE(sink.presses, 0);
        QVERIFY(input.shellInputDraining());
        if (disconnect) {
            QVERIFY(SDL_DetachVirtualJoystick(id));
            id = 0;
        } else if (axis) {
            QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTX, 0));
        } else {
            QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));
        }
        QTRY_VERIFY(!input.shellInputDraining());
        QCOMPARE(sink.presses, 0);
        if (!disconnect) {
            QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
            QTRY_COMPARE(sink.presses, 1);
        }
    }
};

QTEST_MAIN(ConsoleLaunchInputTest)
#include "tst_consolelaunchinput.moc"
