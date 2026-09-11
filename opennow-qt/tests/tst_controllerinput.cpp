#include "input/ControllerInput.h"

#include <QCoreApplication>
#include <QEvent>
#include <QKeyEvent>
#include <QSignalSpy>
#include <QTest>

class ControllerKeySink final : public QObject
{
public:
    int leftPresses = 0;
    int repeatedLeftPresses = 0;

protected:
    bool eventFilter(QObject *watched, QEvent *event) override
    {
        if (event->type() == QEvent::KeyPress) {
            const auto *key = static_cast<QKeyEvent *>(event);
            if (key->key() == Qt::Key_Left
                    && key->nativeScanCode() == ControllerInput::syntheticControllerScanCode) {
                ++leftPresses;
                if (key->isAutoRepeat()) ++repeatedLeftPresses;
            }
        }
        return QObject::eventFilter(watched, event);
    }
};

class ControllerInputTest final : public QObject
{
    Q_OBJECT

private slots:
    void suspendedInputNeutralizesAndStopsBackgroundGameplay()
    {
        ControllerInput input;
        QSignalSpy snapshots(&input, &ControllerInput::gamepadSnapshot);
        QSignalSpy activity(&input, &ControllerInput::controllerActivity);
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.button_mask = 1u << SDL_GAMEPAD_BUTTON_SOUTH;
        descriptor.name = "OpenNOW focus-loss controller";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 1, 2000);
        input.setShellCaptureEnabled(false);
        auto *joystick = SDL_GetJoystickFromID(id);
        QVERIFY(joystick);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        SDL_UpdateJoysticks();
        QTRY_VERIFY_WITH_TIMEOUT(snapshots.last().at(2).toUInt() == 0x1000, 1000);
        input.setInputSuspended(true);
        QCOMPARE(snapshots.last().at(2).toUInt(), 0u);
        auto count = snapshots.size();
        QTest::qWait(250);
        QCOMPARE(snapshots.size(), count);
        input.setShellCaptureEnabled(true);
        count = snapshots.size();
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));
        SDL_UpdateJoysticks();
        QTest::qWait(150);
        QCOMPARE(snapshots.size(), count);
        QCOMPARE(activity.size(), 0);
        input.setShellCaptureEnabled(false);
        input.setInputSuspended(false);
        QCOMPARE(snapshots.last().at(2).toUInt(), 0u);
        input.setInputSuspended(true);
        QVERIFY(SDL_DetachVirtualJoystick(id));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 0, 2000);
        QCOMPARE(snapshots.last().at(1).toUInt(), 0u);
        QCOMPARE(snapshots.last().at(2).toUInt(), 0u);
    }

    void idleMetadataDoesNotInvalidateTheUi()
    {
        ControllerInput input;
        QSignalSpy changes(&input, &ControllerInput::controllersChanged);
        const auto initial = input.controllers();
        QTest::qWait(2150);
        QCOMPARE(input.controllers(), initial);
        QCOMPARE(changes.size(), 0);
    }

    void virtualGamepadHotplugHysteresisAndRepeat()
    {
        ControllerInput input;
        const auto initialCount = input.controllerCount();
        auto *pollTimer = input.findChild<QTimer *>(QStringLiteral("controllerPollTimer"));
        QVERIFY(pollTimer);
        QCOMPARE(pollTimer->interval(), initialCount ? 16 : 100);
        QSignalSpy activity(&input, &ControllerInput::controllerActivity);
        QSignalSpy detailedActivity(&input, &ControllerInput::controllerActivityDetailed);
        ControllerKeySink sink;
        QCoreApplication::instance()->installEventFilter(&sink);

        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.axis_mask = 1u << SDL_GAMEPAD_AXIS_LEFTX;
        descriptor.button_mask = 1u << SDL_GAMEPAD_BUTTON_SOUTH;
        descriptor.name = "OpenNOW virtual validation controller";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), initialCount + 1, 2'000);
        QCOMPARE(pollTimer->interval(), 16);
        input.setShellCaptureEnabled(false);
        QCOMPARE(pollTimer->interval(), 4);
        QCOMPARE(pollTimer->timerType(), Qt::PreciseTimer);
        input.setShellCaptureEnabled(true);
        QCOMPARE(pollTimer->interval(), 16);

        auto *joystick = SDL_GetJoystickFromID(id);
        QVERIFY(joystick);
        // Hotplug and ordinary stick drift must not select console mode.
        QCOMPARE(activity.size(), 0);
        for (const auto drift : {1500, -2500, 0, 8000, -8000}) {
            QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTX, drift));
            SDL_UpdateJoysticks();
            QTest::qWait(15);
        }
        QCOMPARE(activity.size(), 0);
        QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTX, -19'000));
        SDL_UpdateJoysticks();
        QTRY_COMPARE_WITH_TIMEOUT(sink.leftPresses, 1, 1'000);
        QCOMPARE(activity.size(), 1);
        QCOMPARE(detailedActivity.size(), 1);
        QVERIFY(detailedActivity.at(0).at(0).toString().contains(QString::fromUtf8(descriptor.name)));
        QCOMPARE(detailedActivity.at(0).at(1).toString(), QStringLiteral("axis:0"));
        QCOMPARE(detailedActivity.at(0).at(2).toInt(), -19'000);

        // The 12k/18k hysteresis band must not emit a second navigation step.
        QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTX, -15'000));
        SDL_UpdateJoysticks();
        QTest::qWait(100);
        QCOMPARE(sink.leftPresses, 1);

        // A held direction starts repeat after the 280ms delay.
        QTRY_VERIFY_WITH_TIMEOUT(sink.repeatedLeftPresses >= 1, 1'000);
        QCOMPARE(activity.size(), 1); // Held repeats are not new device intent.

        // Releasing below 12k and crossing 18k again creates one fresh step.
        QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTX, -11'000));
        SDL_UpdateJoysticks();
        QTest::qWait(30);
        const auto beforeSecondPress = sink.leftPresses;
        QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTX, -19'000));
        SDL_UpdateJoysticks();
        QTRY_COMPARE_WITH_TIMEOUT(sink.leftPresses, beforeSecondPress + 1, 1'000);

        QVERIFY(SDL_DetachVirtualJoystick(id));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), initialCount, 2'000);
        QCoreApplication::instance()->removeEventFilter(&sink);
    }

    void publishesFullSnapshotsKeepaliveAndNeutralOwnershipTransition()
    {
        ControllerInput input;
        QSignalSpy snapshots(&input, &ControllerInput::gamepadSnapshot);

        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.axis_mask = (1u << SDL_GAMEPAD_AXIS_LEFTX)
            | (1u << SDL_GAMEPAD_AXIS_LEFT_TRIGGER);
        descriptor.button_mask = 1u << SDL_GAMEPAD_BUTTON_SOUTH;
        descriptor.name = "OpenNOW snapshot controller";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 1, 2'000);

        input.setShellCaptureEnabled(false);
        QTRY_VERIFY_WITH_TIMEOUT(!snapshots.isEmpty(), 1'000);
        QCOMPARE(snapshots.last().at(0).toUInt(), 0u);
        QCOMPARE(snapshots.last().at(1).toUInt(), 0x0101u);

        auto *joystick = SDL_GetJoystickFromID(id);
        QVERIFY(joystick);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTX, 24'000));
        QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFT_TRIGGER, 32'767));
        SDL_UpdateJoysticks();
        QTRY_VERIFY_WITH_TIMEOUT(snapshots.last().at(2).toUInt() == 0x1000u, 1'000);
        QVERIFY(snapshots.last().at(5).toInt() > 0);

        const auto beforeKeepalive = snapshots.size();
        QTRY_VERIFY_WITH_TIMEOUT(snapshots.size() > beforeKeepalive, 500);

        input.setShellCaptureEnabled(true);
        QCOMPARE(snapshots.last().at(0).toUInt(), 0u);
        QCOMPARE(snapshots.last().at(2).toUInt(), 0u);
        QCOMPARE(snapshots.last().at(3).toUInt(), 0u);
        QCOMPARE(snapshots.last().at(5).toInt(), 0);

        QVERIFY(SDL_DetachVirtualJoystick(id));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 0, 2'000);
    }

    void preservesExistingSlotsAcrossHotplug()
    {
        ControllerInput input;
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.name = "OpenNOW slot controller";
        const auto first = SDL_AttachVirtualJoystick(&descriptor);
        const auto second = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY(first != 0);
        QVERIFY(second != 0);
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 2, 2'000);
        QCOMPARE(input.controllers().at(0).toMap().value(QStringLiteral("slot")).toInt(), 1);
        QCOMPARE(input.controllers().at(1).toMap().value(QStringLiteral("slot")).toInt(), 2);

        QVERIFY(SDL_DetachVirtualJoystick(first));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 1, 2'000);
        QCOMPARE(input.controllers().at(0).toMap().value(QStringLiteral("slot")).toInt(), 2);
        const auto replacement = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY(replacement != 0);
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 2, 2'000);
        QCOMPARE(input.controllers().at(0).toMap().value(QStringLiteral("slot")).toInt(), 1);
        QCOMPARE(input.controllers().at(1).toMap().value(QStringLiteral("slot")).toInt(), 2);

        QVERIFY(SDL_DetachVirtualJoystick(second));
        QVERIFY(SDL_DetachVirtualJoystick(replacement));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 0, 2'000);
    }
};

QTEST_MAIN(ControllerInputTest)

#include "tst_controllerinput.moc"
