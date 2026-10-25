#include "input/ControllerInput.h"

#include <QKeyEvent>
#include <QScopeGuard>
#include <QSignalSpy>
#include <QTest>

#include <memory>

class TuningPad final
{
public:
    explicit TuningPad(const char *name = "OpenNOW controller tuning test")
    {
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.axis_mask = (1u << SDL_GAMEPAD_AXIS_COUNT) - 1;
        descriptor.button_mask = (1u << SDL_GAMEPAD_BUTTON_COUNT) - 1;
        descriptor.name = name;
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

    bool button(SDL_GamepadButton button, bool pressed)
    {
        return SDL_SetJoystickVirtualButton(SDL_GetJoystickFromID(id), button, pressed);
    }

    SDL_JoystickID id = 0;
    Uint16 lowFrequency = 0;
    Uint16 highFrequency = 0;
    int rumbleCalls = 0;
};

class SyntheticKeyCounter final : public QObject
{
public:
    QList<int> pressed;

protected:
    bool eventFilter(QObject *watched, QEvent *event) override
    {
        if (event->type() == QEvent::KeyPress) {
            const auto *key = static_cast<QKeyEvent *>(event);
            if (key->nativeScanCode() == ControllerInput::syntheticControllerScanCode)
                pressed.append(key->key());
        }
        return QObject::eventFilter(watched, event);
    }
};

static QVariantMap metadataFor(const ControllerInput &input, SDL_JoystickID id)
{
    for (const auto &entry : input.availableControllers()) {
        const auto map = entry.toMap();
        if (map.value(QStringLiteral("instanceId")).toULongLong() == id) return map;
    }
    return {};
}

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

    void profileKeyPrefersSerialThenGuid()
    {
        SDL_GUID first{};
        SDL_GUID second{};
        first.data[0] = 0x03;
        second.data[0] = 0x05;
        QCOMPARE(ControllerInput::profileKey(0x054c, 0x0ce6, " A1B2-C3 ", first),
                 QStringLiteral("serial:054c:0ce6:A1B2-C3"));
        QCOMPARE(ControllerInput::profileKey(0x054c, 0x0ce6, "A1B2-C3", second),
                 QStringLiteral("serial:054c:0ce6:A1B2-C3"));
        QCOMPARE(ControllerInput::profileKey(0x045e, 0x0b13, nullptr, first),
                 QStringLiteral("guid:03000000000000000000000000000000"));
        QCOMPARE(ControllerInput::profileKey(0x045e, 0x0b13, "   ", first),
                 ControllerInput::profileKey(0x045e, 0x0b13, nullptr, first));
        QVERIFY(ControllerInput::profileKey(0x045e, 0x0b13, nullptr, first)
                != ControllerInput::profileKey(0x045e, 0x0b13, nullptr, second));
        QVERIFY(ControllerInput::profileKey(1, 2, QByteArray(400, 'x').constData(), first).size() <= 160);
    }

    void identicalPadsWithoutSerialShareOneProfile()
    {
        ControllerInput input;
        TuningPad first("OpenNOW twin pad");
        TuningPad second("OpenNOW twin pad");
        TuningPad other("OpenNOW other pad");
        QVERIFY(first.id && second.id && other.id);
        QTRY_COMPARE(input.controllerCount(), 3);
        const auto firstKey = metadataFor(input, first.id).value(QStringLiteral("profileKey")).toString();
        QVERIFY(firstKey.startsWith(QStringLiteral("guid:")));
        QVERIFY(!metadataFor(input, first.id).value(QStringLiteral("hasSerial")).toBool());
        QCOMPARE(metadataFor(input, second.id).value(QStringLiteral("profileKey")).toString(), firstKey);
        QVERIFY(metadataFor(input, other.id).value(QStringLiteral("profileKey")).toString() != firstKey);
        input.setControllerProfiles({{firstKey, QVariantMap{{QStringLiteral("leftStickDeadzone"), 31}}}});
        QCOMPARE(metadataFor(input, first.id).value(QStringLiteral("leftStickDeadzone")).toInt(), 31);
        QCOMPARE(metadataFor(input, second.id).value(QStringLiteral("leftStickDeadzone")).toInt(), 31);
        QCOMPARE(metadataFor(input, other.id).value(QStringLiteral("leftStickDeadzone")).toInt(), 5);
    }

    void profileFieldsFallBackToGlobalDefaults()
    {
        ControllerInput input;
        TuningPad pad;
        QVERIFY(pad.id);
        QTRY_COMPARE(input.controllerCount(), 1);
        const auto key = metadataFor(input, pad.id).value(QStringLiteral("profileKey")).toString();
        input.setLeftStickDeadzone(20);
        input.setControllerProfiles({{key, QVariantMap{{QStringLiteral("rightStickDeadzone"), 30.0},
                                                       {QStringLiteral("name"), QStringLiteral("Pad")}}}});
        auto metadata = metadataFor(input, pad.id);
        QCOMPARE(metadata.value(QStringLiteral("leftStickDeadzone")).toInt(), 20);
        QCOMPARE(metadata.value(QStringLiteral("rightStickDeadzone")).toInt(), 30);
        QCOMPARE(metadata.value(QStringLiteral("vibrationIntensity")).toInt(), 100);
        input.setLeftStickDeadzone(12);
        input.setRightStickDeadzone(44);
        input.setVibrationIntensity(60);
        metadata = metadataFor(input, pad.id);
        QCOMPARE(metadata.value(QStringLiteral("leftStickDeadzone")).toInt(), 12);
        QCOMPARE(metadata.value(QStringLiteral("rightStickDeadzone")).toInt(), 30);
        QCOMPARE(metadata.value(QStringLiteral("vibrationIntensity")).toInt(), 60);
        QCOMPARE(input.rightStickDeadzone(), 44);
        input.setControllerProfiles({{key, QVariantMap{{QStringLiteral("rightStickDeadzone"), 999},
                                                       {QStringLiteral("vibrationIntensity"), QStringLiteral("loud")}}}});
        metadata = metadataFor(input, pad.id);
        QCOMPARE(metadata.value(QStringLiteral("rightStickDeadzone")).toInt(), 50);
        QCOMPARE(metadata.value(QStringLiteral("vibrationIntensity")).toInt(), 60);
        input.setControllerProfiles({});
        QCOMPARE(metadataFor(input, pad.id).value(QStringLiteral("rightStickDeadzone")).toInt(), 44);
    }

    void deadzoneAppliesPerPad()
    {
        ControllerInput input;
        TuningPad tuned("OpenNOW tuned pad");
        TuningPad plain("OpenNOW plain pad");
        QVERIFY(tuned.id && plain.id);
        QTRY_COMPARE(input.controllerCount(), 2);
        const auto key = metadataFor(input, tuned.id).value(QStringLiteral("profileKey")).toString();
        const auto tunedSlot = metadataFor(input, tuned.id).value(QStringLiteral("slot")).toInt() - 1;
        const auto plainSlot = metadataFor(input, plain.id).value(QStringLiteral("slot")).toInt() - 1;
        input.setControllerProfiles({{key, QVariantMap{{QStringLiteral("leftStickDeadzone"), 40}}}});
        QVERIFY(tuned.axis(SDL_GAMEPAD_AXIS_LEFTX, 8192));
        QVERIFY(plain.axis(SDL_GAMEPAD_AXIS_LEFTX, 8192));
        SDL_UpdateJoysticks();
        QSignalSpy snapshots(&input, &ControllerInput::gamepadSnapshot);
        input.setShellCaptureEnabled(false);
        const auto latestLeftX = [&snapshots](int slot) {
            for (auto index = snapshots.size() - 1; index >= 0; --index) {
                if (snapshots.at(index).at(0).toInt() == slot) return snapshots.at(index).at(5).toInt();
            }
            return -1;
        };
        QCOMPARE(latestLeftX(tunedSlot), 0);
        QVERIFY(latestLeftX(plainSlot) > 0);
        input.setControllerProfiles({});
        QVERIFY(latestLeftX(tunedSlot) > 0);
        const auto sample = input.stickSample(tuned.id);
        QVERIFY(sample.value(QStringLiteral("leftX")).toDouble() > 0.24);
        QVERIFY(sample.value(QStringLiteral("leftOutX")).toDouble() > 0);
        QVERIFY(input.stickSample(0).isEmpty());
    }

    void leftStickNeverMovesFocusWhileNavigationIsOff()
    {
        ControllerInput input;
        TuningPad pad;
        QVERIFY(pad.id);
        QTRY_COMPARE(input.controllerCount(), 1);
        QVERIFY(input.shellCaptureEnabled());
        SyntheticKeyCounter keys;
        qApp->installEventFilter(&keys);
        input.setLeftStickNavigationEnabled(false);
        QVERIFY(pad.axis(SDL_GAMEPAD_AXIS_LEFTY, 30000));
        SDL_UpdateJoysticks();
        QTest::qWait(400);
        QVERIFY2(!keys.pressed.contains(Qt::Key_Down), "left stick moved focus on the controller page");
        QVERIFY(input.stickSample(pad.id).value(QStringLiteral("leftY")).toDouble() > 0.9);
        QVERIFY(pad.button(SDL_GAMEPAD_BUTTON_DPAD_DOWN, true));
        SDL_UpdateJoysticks();
        QTRY_VERIFY(keys.pressed.contains(Qt::Key_Down));
        QVERIFY(pad.button(SDL_GAMEPAD_BUTTON_DPAD_DOWN, false));
        QVERIFY(pad.axis(SDL_GAMEPAD_AXIS_LEFTY, 0));
        SDL_UpdateJoysticks();
        QTest::qWait(60);
        keys.pressed.clear();
        input.setLeftStickNavigationEnabled(true);
        QVERIFY(pad.axis(SDL_GAMEPAD_AXIS_LEFTY, 30000));
        SDL_UpdateJoysticks();
        QTRY_VERIFY(keys.pressed.contains(Qt::Key_Down));
        qApp->removeEventFilter(&keys);
    }

    void vibrationAppliesPerPadAndTestRumbleWorksInTheMenu()
    {
        ControllerInput input;
        TuningPad pad;
        QVERIFY(pad.id);
        QTRY_COMPARE(input.controllerCount(), 1);
        const auto key = metadataFor(input, pad.id).value(QStringLiteral("profileKey")).toString();
        input.setControllerProfiles({{key, QVariantMap{{QStringLiteral("vibrationIntensity"), 50}}}});
        QVERIFY(input.shellCaptureEnabled());
        QVERIFY(input.testRumble(pad.id));
        QCOMPARE(pad.lowFrequency, 0x3000);
        QCOMPARE(pad.highFrequency, 0x5000);
        input.stopRumble();
        input.setShellCaptureEnabled(false);
        input.playRumble(0, 20000, 40000, 1000);
        QCOMPARE(pad.lowFrequency, 10000);
        QCOMPARE(pad.highFrequency, 20000);
        input.setControllerProfiles({{key, QVariantMap{{QStringLiteral("vibrationIntensity"), 0}}}});
        QCOMPARE(pad.lowFrequency, 0);
        QVERIFY(!input.testRumble(pad.id));
        input.playRumble(0, 20000, 40000, 1000);
        QCOMPARE(pad.lowFrequency, 0);
        QVERIFY(!input.testRumble(0));
    }

    void independentSettingsApplyWithoutNewAxisEvents()
    {
        ControllerInput input;
        TuningPad pad;
        QVERIFY(pad.id);
        QTRY_COMPARE(input.controllerCount(), 1);
        QCOMPARE(input.leftStickDeadzone(), 5);
        QCOMPARE(input.rightStickDeadzone(), 5);
        input.setLeftStickDeadzone(24);
        input.setRightStickDeadzone(27);
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

    void exclusiveRumbleStopsOnlyTheOwnedController()
    {
        QVERIFY(SDL_InitSubSystem(SDL_INIT_GAMEPAD));
        const auto sdlCleanup = qScopeGuard([] { SDL_QuitSubSystem(SDL_INIT_GAMEPAD); });
        TuningPad first;
        TuningPad second;
        QVERIFY(first.id && second.id);
        {
            ControllerInput startup;
            startup.setBackgroundControllerEnabled(true);
            startup.stopRumble();
            QCOMPARE(first.rumbleCalls, 0);
            QCOMPARE(second.rumbleCalls, 0);
        }
        QCOMPARE(first.rumbleCalls, 0);
        QCOMPARE(second.rumbleCalls, 0);
        auto input = std::make_unique<ControllerInput>();
        QCOMPARE(input->controllerCount(), 2);
        auto *firstExternalOwner = SDL_OpenGamepad(first.id);
        auto *secondExternalOwner = SDL_OpenGamepad(second.id);
        QVERIFY(firstExternalOwner && secondExternalOwner);
        const auto ownersCleanup = qScopeGuard([&] {
            SDL_CloseGamepad(firstExternalOwner);
            SDL_CloseGamepad(secondExternalOwner);
        });
        input->setInputControllerId(first.id);
        input->setBackgroundControllerEnabled(true);
        input->setShellCaptureEnabled(false);
        QVERIFY(SDL_RumbleGamepad(secondExternalOwner, 12000, 18000, 60'000));
        auto unownedCalls = second.rumbleCalls;
        input->playRumble(0, 5000, 9000, 60'000);
        QCOMPARE(first.lowFrequency, 5000);
        input->setInputSuspended(true);
        QCOMPARE(first.lowFrequency, 0);
        QCOMPARE(second.lowFrequency, 12000);
        QCOMPARE(second.rumbleCalls, unownedCalls);
        input->setInputSuspended(false);
        input->playRumble(0, 5000, 9000, 60'000);
        input->setShellCaptureEnabled(true);
        QCOMPARE(first.lowFrequency, 0);
        QCOMPARE(second.rumbleCalls, unownedCalls);
        input->setShellCaptureEnabled(false);
        input->playRumble(0, 5000, 9000, 60'000);
        input->setInputControllerId(second.id);
        QCOMPARE(first.lowFrequency, 0);
        QCOMPARE(second.lowFrequency, 12000);
        QCOMPARE(second.rumbleCalls, unownedCalls);
        QVERIFY(SDL_RumbleGamepad(firstExternalOwner, 15000, 21000, 60'000));
        unownedCalls = first.rumbleCalls;
        input->playRumble(0, 6000, 10000, 60'000);
        input->setInputControllerId(0);
        QCOMPARE(second.lowFrequency, 0);
        QCOMPARE(first.lowFrequency, 15000);
        QCOMPARE(first.rumbleCalls, unownedCalls);
        input->stopRumble();
        QCOMPARE(first.rumbleCalls, unownedCalls);
        input->setInputControllerId(second.id);
        input->playRumble(0, 6000, 10000, 60'000);
        input.reset();
        QCOMPARE(second.lowFrequency, 0);
        QCOMPARE(second.highFrequency, 0);
        QCOMPARE(first.lowFrequency, 15000);
        QCOMPARE(first.highFrequency, 21000);
        QCOMPARE(first.rumbleCalls, unownedCalls);
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

    void disconnectedSelectedControllerReleasesPlayerOneForReplacement()
    {
        ControllerInput input;
        auto pad = std::make_unique<TuningPad>();
        QVERIFY(pad->id);
        QTRY_COMPARE(input.controllerCount(), 1);
        input.setInputControllerId(pad->id);
        input.setShellCaptureEnabled(false);
        input.playRumble(0, 5000, 9000, 1000);
        QCOMPARE(pad->lowFrequency, 5000);
        const auto selected = pad->id;
        pad.reset();
        QTRY_COMPARE(input.controllerCount(), 0);
        QCOMPARE(input.inputControllerId(), selected);
        TuningPad replacement;
        QVERIFY(replacement.id && replacement.id != selected);
        QTRY_COMPARE(input.availableControllers().size(), 1);
        QCOMPARE(input.controllerCount(), 0);
        input.playRumble(0, 5000, 9000, 1000);
        QCOMPARE(replacement.lowFrequency, 0);
        QCOMPARE(replacement.highFrequency, 0);
        input.setInputControllerId(replacement.id);
        QCOMPARE(input.controllerCount(), 1);
        QCOMPARE(input.controllers().first().toMap().value(QStringLiteral("slot")).toInt(), 1);
        QCOMPARE(replacement.lowFrequency, 0);
        input.playRumble(0, 5000, 9000, 1000);
        QCOMPARE(replacement.lowFrequency, 5000);
        QCOMPARE(replacement.highFrequency, 9000);
    }
};

QTEST_MAIN(ControllerTuningTest)
#include "tst_controllertuning.moc"
