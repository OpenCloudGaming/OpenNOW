#include "input/ControllerInput.h"

#include <QSignalSpy>
#include <QTest>

#include <memory>

class TuningPad final
{
public:
    TuningPad()
    {
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.axis_mask = (1u << SDL_GAMEPAD_AXIS_COUNT) - 1;
        descriptor.button_mask = (1u << SDL_GAMEPAD_BUTTON_COUNT) - 1;
        descriptor.name = "OpenNOW controller tuning test";
        descriptor.userdata = this;
        descriptor.Rumble = [](void *context, Uint16 low, Uint16 high) -> bool {
            auto *pad = static_cast<TuningPad *>(context);
            pad->lowFrequency = low;
            pad->highFrequency = high;
            ++pad->rumbleCalls;
            return true;
        };
        id = SDL_AttachVirtualJoystick(&descriptor);
    }

    ~TuningPad() { if (id) SDL_DetachVirtualJoystick(id); }

    bool axis(SDL_GamepadAxis axis, Sint16 value)
    {
        return SDL_SetJoystickVirtualAxis(SDL_GetJoystickFromID(id), axis, value);
    }

    SDL_JoystickID id = 0;
    Uint16 lowFrequency = 0;
    Uint16 highFrequency = 0;
    int rumbleCalls = 0;
};

class ControllerTuningTest final : public QObject
{
    Q_OBJECT

private slots:
    void radialDeadzone_data()
    {
        QTest::addColumn<int>("deadzone");
        QTest::addColumn<int>("x");
        QTest::addColumn<int>("y");
        QTest::addColumn<int>("expectedX");
        QTest::addColumn<int>("expectedY");
        QTest::newRow("zero-centre") << 0 << 0 << 0 << 0 << 0;
        QTest::newRow("zero-passthrough") << 0 << 12000 << -9000 << 12000 << 9000;
        QTest::newRow("resting-drift") << 24 << 5000 << 5000 << 0 << 0;
        QTest::newRow("radial-not-axial") << 24 << 7000 << 7000 << 1894 << -1894;
        QTest::newRow("linear-outside-zone") << 50 << 24575 << 0 << 16383 << 0;
        QTest::newRow("full-positive") << 24 << 32767 << 0 << 32767 << 0;
        QTest::newRow("full-negative") << 24 << -32768 << 0 << -32767 << 0;
        QTest::newRow("full-diagonal") << 27 << 32767 << 32767 << 32767 << -32767;
        QTest::newRow("negative-diagonal") << 27 << -32768 << -32768 << -32767 << 32767;
    }

    void radialDeadzone()
    {
        QFETCH(int, deadzone);
        QFETCH(int, x);
        QFETCH(int, y);
        QFETCH(int, expectedX);
        QFETCH(int, expectedY);
        ControllerInput input;
        TuningPad pad;
        QVERIFY(pad.id);
        QTRY_COMPARE(input.controllerCount(), 1);
        input.setLeftStickDeadzone(deadzone);
        input.setRightStickDeadzone(deadzone);
        QSignalSpy snapshots(&input, &ControllerInput::gamepadSnapshot);
        QVERIFY(pad.axis(SDL_GAMEPAD_AXIS_LEFTX, x));
        QVERIFY(pad.axis(SDL_GAMEPAD_AXIS_LEFTY, y));
        QVERIFY(pad.axis(SDL_GAMEPAD_AXIS_RIGHTX, x));
        QVERIFY(pad.axis(SDL_GAMEPAD_AXIS_RIGHTY, y));
        SDL_UpdateJoysticks();
        input.setShellCaptureEnabled(false);
        QTRY_VERIFY(!snapshots.isEmpty());
        for (const auto index : {5, 7}) {
            QVERIFY(qAbs(snapshots.last().at(index).toInt() - expectedX) <= 1);
            QVERIFY(qAbs(snapshots.last().at(index + 1).toInt() - expectedY) <= 1);
        }
    }

    void independentSettingsApplyWithoutNewAxisEvents()
    {
        ControllerInput input;
        TuningPad pad;
        QVERIFY(pad.id);
        QTRY_COMPARE(input.controllerCount(), 1);
        QCOMPARE(input.leftStickDeadzone(), 24);
        QCOMPARE(input.rightStickDeadzone(), 27);
        QVERIFY(pad.axis(SDL_GAMEPAD_AXIS_LEFTX, 8192));
        QVERIFY(pad.axis(SDL_GAMEPAD_AXIS_RIGHTX, 8192));
        SDL_UpdateJoysticks();
        QSignalSpy snapshots(&input, &ControllerInput::gamepadSnapshot);
        input.setShellCaptureEnabled(false);
        QVERIFY(snapshots.last().at(5).toInt() > 0);
        QCOMPARE(snapshots.last().at(7).toInt(), 0);
        input.setLeftStickDeadzone(50);
        QCOMPARE(snapshots.last().at(5).toInt(), 0);
        input.setRightStickDeadzone(0);
        QCOMPARE(snapshots.last().at(7).toInt(), 8192);
        input.setLeftStickDeadzone(-1);
        QCOMPARE(input.leftStickDeadzone(), 0);
        input.setRightStickDeadzone(100);
        QCOMPARE(input.rightStickDeadzone(), 50);
        input.setInputSuspended(true);
        const auto count = snapshots.count();
        input.setLeftStickDeadzone(24);
        QCOMPARE(snapshots.count(), count);
    }

    void rumbleRoutesSelectedControllerAndStopsOnOwnershipChanges()
    {
        ControllerInput input;
        TuningPad first;
        TuningPad second;
        QVERIFY(first.id && second.id);
        QTRY_COMPARE(input.controllerCount(), 2);
        input.playRumble(0, 20000, 40000, 1000);
        QCOMPARE(first.lowFrequency, 0);
        input.setShellCaptureEnabled(false);
        input.playRumble(1, 20000, 40000, 1000);
        QCOMPARE(first.lowFrequency, 0);
        QCOMPARE(second.lowFrequency, 20000);
        QCOMPARE(second.highFrequency, 40000);
        input.setInputControllerId(second.id);
        QCOMPARE(second.lowFrequency, 0);
        input.playRumble(1, 10000, 10000, 1000);
        QCOMPARE(second.lowFrequency, 0);
        input.setVibrationIntensity(50);
        input.playRumble(0, 20000, 40000, 1000);
        QCOMPARE(first.lowFrequency, 0);
        QCOMPARE(second.lowFrequency, 10000);
        QCOMPARE(second.highFrequency, 20000);
        input.setInputSuspended(true);
        QCOMPARE(second.lowFrequency, 0);
        input.playRumble(0, 20000, 40000, 1000);
        QCOMPARE(second.lowFrequency, 0);
        input.setInputSuspended(false);
        input.playRumble(0, 20000, 40000, 1000);
        QCOMPARE(second.lowFrequency, 10000);
        input.setShellCaptureEnabled(true);
        QCOMPARE(second.lowFrequency, 0);
        input.setShellCaptureEnabled(false);
        input.playRumble(0, 20000, 40000, 1000);
        input.setVibrationIntensity(0);
        QCOMPARE(second.lowFrequency, 0);
        input.playRumble(0, 20000, 40000, 1000);
        QCOMPARE(second.lowFrequency, 0);
        input.setVibrationIntensity(150);
        QCOMPARE(input.vibrationIntensity(), 100);
        input.playRumble(255, 20000, 40000, 1000);
        QCOMPARE(first.lowFrequency, 0);
        QCOMPARE(second.lowFrequency, 0);
    }

    void rumbleExpiresRefreshesAndStopsExplicitly()
    {
        ControllerInput input;
        TuningPad pad;
        QVERIFY(pad.id);
        QTRY_COMPARE(input.controllerCount(), 1);
        input.setShellCaptureEnabled(false);
        input.playRumble(0, 5000, 9000, 30);
        QCOMPARE(pad.lowFrequency, 5000);
        input.playRumble(0, 6000, 10000, 250);
        QTest::qWait(60);
        QCOMPARE(pad.lowFrequency, 6000);
        QTRY_COMPARE_WITH_TIMEOUT(pad.lowFrequency, 0, 1000);
        QCOMPARE(pad.highFrequency, 0);
        input.playRumble(0, 5000, 9000, 1000);
        input.playRumble(0, 0, 0, 0);
        QCOMPARE(pad.lowFrequency, 0);
        QCOMPARE(pad.highFrequency, 0);
        input.playRumble(0, 5000, 9000, 1000);
        input.stopRumble();
        QCOMPARE(pad.lowFrequency, 0);
    }

    void disconnectedSelectedControllerDoesNotRumbleReplacement()
    {
        ControllerInput input;
        auto pad = std::make_unique<TuningPad>();
        QVERIFY(pad->id);
        QTRY_COMPARE(input.controllerCount(), 1);
        input.setInputControllerId(pad->id);
        input.setShellCaptureEnabled(false);
        input.playRumble(0, 5000, 9000, 1000);
        QCOMPARE(pad->lowFrequency, 5000);
        pad.reset();
        QTRY_COMPARE(input.controllerCount(), 0);
        TuningPad replacement;
        QVERIFY(replacement.id);
        QTRY_COMPARE(input.availableControllers().size(), 1);
        input.playRumble(0, 5000, 9000, 1000);
        QCOMPARE(replacement.lowFrequency, 0);
    }
};

QTEST_MAIN(ControllerTuningTest)
#include "tst_controllertuning.moc"
