#include "input/ControllerInput.h"

#include <QCoreApplication>
#include <QEvent>
#include <QKeyEvent>
#include <QScopeGuard>
#include <QSignalSpy>
#include <QTest>

class ControllerKeySink final : public QObject
{
public:
    int trackedKey = Qt::Key_Left;
    int leftPresses = 0;
    int repeatedLeftPresses = 0;

protected:
    bool eventFilter(QObject *watched, QEvent *event) override
    {
        if (event->type() == QEvent::KeyPress) {
            const auto *key = static_cast<QKeyEvent *>(event);
            if (key->key() == trackedKey
                    && key->nativeScanCode() == ControllerInput::syntheticControllerScanCode) {
                ++leftPresses;
                if (key->isAutoRepeat()) ++repeatedLeftPresses;
            }
        }
        return QObject::eventFilter(watched, event);
    }
};

static ControllerInput::SonySnapshot latestSony(const QSignalSpy &spy)
{
    if (spy.isEmpty()) return ControllerInput::SonySnapshot{};
    return spy.last().at(0).value<ControllerInput::SonySnapshot>();
}

class ControllerInputTest final : public QObject
{
    Q_OBJECT

private slots:
    void digitalDpadRepeatsUntilReleased_data()
    {
        QTest::addColumn<int>("button");
        QTest::addColumn<int>("key");
        QTest::newRow("left") << int(SDL_GAMEPAD_BUTTON_DPAD_LEFT) << int(Qt::Key_Left);
        QTest::newRow("right") << int(SDL_GAMEPAD_BUTTON_DPAD_RIGHT) << int(Qt::Key_Right);
        QTest::newRow("up") << int(SDL_GAMEPAD_BUTTON_DPAD_UP) << int(Qt::Key_Up);
        QTest::newRow("down") << int(SDL_GAMEPAD_BUTTON_DPAD_DOWN) << int(Qt::Key_Down);
    }

    void digitalDpadRepeatsUntilReleased()
    {
        QFETCH(int, button);
        QFETCH(int, key);
        ControllerInput input;
        ControllerKeySink sink;
        sink.trackedKey = key;
        QCoreApplication::instance()->installEventFilter(&sink);
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.name = "OpenNOW held D-pad regression";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        const auto cleanup = qScopeGuard([id] { SDL_DetachVirtualJoystick(id); });
        QTRY_COMPARE(input.controllerCount(), 1);
        auto *joystick = SDL_GetJoystickFromID(id);
        QVERIFY(joystick);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, button, true));
        QTRY_COMPARE(sink.leftPresses, 1);
        QTest::qWait(150);
        QCOMPARE(sink.repeatedLeftPresses, 0);
        QTRY_VERIFY_WITH_TIMEOUT(sink.repeatedLeftPresses >= 2, 900);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, button, false));
        QTest::qWait(50);
        auto count = sink.leftPresses;
        QTest::qWait(300);
        QCOMPARE(sink.leftPresses, count);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, button, true));
        QTRY_COMPARE(sink.leftPresses, count + 1);
        input.setInputSuspended(true);
        count = sink.leftPresses;
        QTest::qWait(400);
        QCOMPARE(sink.leftPresses, count);
        input.setInputSuspended(false);
        QTest::qWait(350);
        QCOMPARE(sink.leftPresses, count);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, button, false));
        QTest::qWait(30);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, button, true));
        QTRY_COMPARE(sink.leftPresses, count + 1);
        input.setShellCaptureEnabled(false);
        count = sink.leftPresses;
        QTest::qWait(400);
        QCOMPARE(sink.leftPresses, count);
    }

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

        QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTX, 0));
        sink.trackedKey = Qt::Key_Return;
        sink.leftPresses = 0;
        sink.repeatedLeftPresses = 0;
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QTRY_COMPARE_WITH_TIMEOUT(sink.leftPresses, 1, 1'000);
        QTest::qWait(450);
        QCOMPARE(sink.leftPresses, 1);
        QCOMPARE(sink.repeatedLeftPresses, 0);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));

        QVERIFY(SDL_DetachVirtualJoystick(id));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), initialCount, 2'000);
        QCoreApplication::instance()->removeEventFilter(&sink);
    }

    void sonyTouchpadContactsGuideLatchAndClaims()
    {
        ControllerInput input;
        QSignalSpy snapshots(&input, &ControllerInput::sonySnapshot);
        QSignalSpy claims(&input, &ControllerInput::deviceClaimsChanged);
        QSignalSpy localActions(&input, &ControllerInput::localActionRequested);
        SDL_VirtualJoystickTouchpadDesc touchpad{2, 0, 0, 0};
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.vendor_id = 0x054c;
        descriptor.product_id = 0x05c4;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.ntouchpads = 1;
        descriptor.touchpads = &touchpad;
        descriptor.button_mask = (1u << SDL_GAMEPAD_BUTTON_TOUCHPAD)
            | (1u << SDL_GAMEPAD_BUTTON_GUIDE);
        descriptor.name = "OpenNOW virtual Sony controller";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 1, 2'000);
        QVERIFY(claims.size() >= 1);
        const auto inventory = input.deviceClaims();
        QCOMPARE(inventory.size(), 1);
        QCOMPARE(inventory.at(0).slot, 0);
        QCOMPARE(inventory.at(0).vendor, 0x054c);
        QCOMPARE(inventory.at(0).product, 0x05c4);
        QVERIFY(inventory.at(0).incarnation != 0);

        input.setShellCaptureEnabled(false);
        auto *joystick = SDL_GetJoystickFromID(id);
        QVERIFY(joystick);

        QVERIFY(SDL_SetJoystickVirtualTouchpad(joystick, 0, 0, true, 0.5f, 0.5f, 1.0f));
        QVERIFY(SDL_SetJoystickVirtualTouchpad(joystick, 0, 1, true, 0.5f, 0.5f, 1.0f));
        SDL_UpdateJoysticks();
        auto published = snapshots.size();
        QTRY_VERIFY_WITH_TIMEOUT(snapshots.size() > published, 1'000);
        published = snapshots.size();
        QTest::qWait(50);
        QVERIFY(snapshots.size() >= published);
        const auto touchSnapshot = latestSony(snapshots);
        QCOMPARE(touchSnapshot.slot, 0u);
        QCOMPARE(touchSnapshot.incarnation, inventory.at(0).incarnation);
        QCOMPARE(touchSnapshot.touchpadClick, false);
        QCOMPARE(touchSnapshot.contacts[0].active, true);
        QCOMPARE(touchSnapshot.contacts[0].x, 0.5f);
        QCOMPARE(touchSnapshot.contacts[0].y, 0.5f);
        QCOMPARE(touchSnapshot.contacts[1].active, true);
        QCOMPARE(touchSnapshot.contacts[1].x, 0.5f);
        QCOMPARE(touchSnapshot.contacts[1].y, 0.5f);
        QVERIFY(touchSnapshot.observedAtUs > 0);

        QVERIFY(SDL_SetJoystickVirtualTouchpad(joystick, 0, 1, false, 0.25f, 0.75f, 0.0f));
        SDL_UpdateJoysticks();
        published = snapshots.size();
        QTRY_VERIFY_WITH_TIMEOUT(snapshots.size() > published, 1'000);
        const auto releasedFinger = latestSony(snapshots);
        QCOMPARE(releasedFinger.contacts[1].active, false);
        QCOMPARE(releasedFinger.contacts[1].x, 0.25f);
        QCOMPARE(releasedFinger.contacts[1].y, 0.75f);
        QCOMPARE(releasedFinger.contacts[0].active, true);

        const auto pushClick = [&](bool down) {
            SDL_Event click{};
            click.type = down ? SDL_EVENT_GAMEPAD_BUTTON_DOWN : SDL_EVENT_GAMEPAD_BUTTON_UP;
            click.gbutton.which = id;
            click.gbutton.button = SDL_GAMEPAD_BUTTON_TOUCHPAD;
            click.gbutton.down = down;
            QVERIFY(SDL_PushEvent(&click));
        };
        const auto sawClick = [&](bool expected) {
            for (const auto &record : snapshots) {
                if (record.at(0).value<ControllerInput::SonySnapshot>().touchpadClick == expected)
                    return true;
            }
            return false;
        };
        pushClick(true);
        QTRY_VERIFY_WITH_TIMEOUT(sawClick(true), 1'000);
        pushClick(false);
        QTRY_VERIFY_WITH_TIMEOUT(!latestSony(snapshots).touchpadClick, 1'000);

        const auto pushGuide = [&](bool down) {
            SDL_Event guide{};
            guide.type = down ? SDL_EVENT_GAMEPAD_BUTTON_DOWN : SDL_EVENT_GAMEPAD_BUTTON_UP;
            guide.gbutton.which = id;
            guide.gbutton.button = SDL_GAMEPAD_BUTTON_GUIDE;
            guide.gbutton.down = down;
            QVERIFY(SDL_PushEvent(&guide));
        };
        pushGuide(true);
        QTRY_COMPARE_WITH_TIMEOUT(localActions.size(), 1, 1'000);
        input.setInputSuspended(true);
        QTest::qWait(30);
        pushGuide(false);
        QTest::qWait(30);
        input.setInputSuspended(false);
        QTest::qWait(30);
        QCOMPARE(localActions.size(), 1);
        pushGuide(true);
        QTRY_COMPARE_WITH_TIMEOUT(localActions.size(), 2, 1'000);
        pushGuide(false);
        QTest::qWait(30);
        QCOMPARE(localActions.size(), 2);

        QVERIFY(SDL_DetachVirtualJoystick(id));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 0, 2'000);
        QTRY_VERIFY_WITH_TIMEOUT(input.deviceClaims().isEmpty(), 1'000);
    }

    void sonyContactsReleaseOnSuspensionAndResampleOnResume()
    {
        ControllerInput input;
        QSignalSpy snapshots(&input, &ControllerInput::sonySnapshot);
        SDL_VirtualJoystickTouchpadDesc touchpad{2, 0, 0, 0};
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.vendor_id = 0x054c;
        descriptor.product_id = 0x0ce6;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.ntouchpads = 1;
        descriptor.touchpads = &touchpad;
        descriptor.name = "OpenNOW virtual DualSense";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 1, 2'000);
        input.setShellCaptureEnabled(false);
        auto *joystick = SDL_GetJoystickFromID(id);
        QVERIFY(joystick);
        QVERIFY(SDL_SetJoystickVirtualTouchpad(joystick, 0, 0, true, 0.75f, 0.25f, 1.0f));
        QVERIFY(SDL_SetJoystickVirtualTouchpad(joystick, 0, 1, true, 0.25f, 0.75f, 1.0f));
        SDL_UpdateJoysticks();
        auto published = snapshots.size();
        QTRY_VERIFY_WITH_TIMEOUT(snapshots.size() > published, 1'000);
        QTest::qWait(50);
        const auto touched = latestSony(snapshots);
        QCOMPARE(touched.contacts[0].active, true);
        QCOMPARE(touched.contacts[0].x, 0.75f);
        QCOMPARE(touched.contacts[0].y, 0.25f);
        QCOMPARE(touched.contacts[1].active, true);
        QCOMPARE(touched.contacts[1].x, 0.25f);
        QCOMPARE(touched.contacts[1].y, 0.75f);

        published = snapshots.size();
        input.setInputSuspended(true);
        QTRY_VERIFY_WITH_TIMEOUT(snapshots.size() > published, 1'000);
        const auto suspended = latestSony(snapshots);
        QCOMPARE(suspended.contacts[0].active, false);
        QCOMPARE(suspended.contacts[0].x, 0.75f);
        QCOMPARE(suspended.contacts[1].active, false);
        QCOMPARE(suspended.contacts[1].y, 0.75f);

        published = snapshots.size();
        input.setInputSuspended(false);
        QTRY_VERIFY_WITH_TIMEOUT(snapshots.size() > published, 1'000);
        const auto resumed = latestSony(snapshots);
        QCOMPARE(resumed.contacts[0].active, true);
        QCOMPARE(resumed.contacts[0].x, 0.75f);
        QCOMPARE(resumed.contacts[1].active, true);
        QCOMPARE(resumed.contacts[1].y, 0.75f);

        QVERIFY(SDL_DetachVirtualJoystick(id));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 0, 2'000);
    }

    void selectionAnnouncesClaimsBeforeItsSnapshots()
    {
        ControllerInput input;
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.vendor_id = 0x054c;
        descriptor.product_id = 0x05c4;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.axis_mask = (1u << SDL_GAMEPAD_AXIS_COUNT) - 1;
        descriptor.button_mask = 1u << SDL_GAMEPAD_BUTTON_SOUTH;
        descriptor.name = "OpenNOW selection announcement";
        const auto first = SDL_AttachVirtualJoystick(&descriptor);
        const auto second = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY(first != 0 && second != 0);
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 2, 2'000);
        input.setShellCaptureEnabled(false);
        auto announced = input.deviceClaims();
        bool allMatched = true;
        QObject observer;
        QObject::connect(&input, &ControllerInput::deviceClaimsChanged, &observer,
                         [&] { announced = input.deviceClaims(); });
        QObject::connect(&input, &ControllerInput::sonySnapshot, &observer,
                         [&](const ControllerInput::SonySnapshot &snapshot) {
            bool matches = false;
            for (const auto &claim : announced)
                matches |= claim.slot == snapshot.slot && claim.incarnation == snapshot.incarnation;
            allMatched &= matches;
        });
        input.setInputControllerId(second);
        QVERIFY2(allMatched, "native inventory must know each selected source before its snapshot arrives");
        QVERIFY(SDL_DetachVirtualJoystick(first));
        QVERIFY(SDL_DetachVirtualJoystick(second));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 0, 2'000);
    }

    void sonyOrdinaryButtonPublishesCompleteSnapshotWithoutTouch()
    {
        ControllerInput input;
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.vendor_id = 0x054c;
        descriptor.product_id = 0x05c4;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.axis_mask = (1u << SDL_GAMEPAD_AXIS_COUNT) - 1;
        descriptor.button_mask = 1u << SDL_GAMEPAD_BUTTON_SOUTH;
        descriptor.name = "OpenNOW ordinary Sony button";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 1, 2'000);
        input.setShellCaptureEnabled(false);
        QSignalSpy snapshots(&input, &ControllerInput::sonySnapshot);
        auto *joystick = SDL_GetJoystickFromID(id);
        QVERIFY(joystick);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        SDL_UpdateJoysticks();
        auto *gamepad = SDL_GetGamepadFromID(id);
        QVERIFY(gamepad);
        QTRY_VERIFY_WITH_TIMEOUT(SDL_GetGamepadButton(gamepad, SDL_GAMEPAD_BUTTON_SOUTH), 1'000);
        QTRY_VERIFY_WITH_TIMEOUT(
            !snapshots.isEmpty()
                && (latestSony(snapshots).buttons & 0x1000) == 0x1000,
            1'000);
        QCOMPARE(latestSony(snapshots).touchpadClick, false);
        QCOMPARE(latestSony(snapshots).contacts[0].active, false);
        QVERIFY(SDL_DetachVirtualJoystick(id));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 0, 2'000);
    }

    void sonyStickYKeepsRawSdlOrientation()
    {
        ControllerInput input;
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.vendor_id = 0x054c;
        descriptor.product_id = 0x05c4;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.axis_mask = (1u << SDL_GAMEPAD_AXIS_COUNT) - 1;
        descriptor.name = "OpenNOW ordinary Sony stick";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 1, 2'000);
        input.setShellCaptureEnabled(false);
        QSignalSpy snapshots(&input, &ControllerInput::sonySnapshot);
        auto *joystick = SDL_GetJoystickFromID(id);
        QVERIFY(joystick);
        QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTY, 24'000));
        SDL_UpdateJoysticks();
        auto *gamepad = SDL_GetGamepadFromID(id);
        QVERIFY(gamepad);
        QTRY_VERIFY_WITH_TIMEOUT(SDL_GetGamepadAxis(gamepad, SDL_GAMEPAD_AXIS_LEFTY) > 0, 1'000);
        QTRY_VERIFY_WITH_TIMEOUT(
            !snapshots.isEmpty() && latestSony(snapshots).leftStickY > 0, 1'000);

        input.setInputSuspended(true);
        input.setInputSuspended(false);
        QVERIFY(!snapshots.isEmpty());
        QVERIFY2(latestSony(snapshots).leftStickY > 0,
                 "Sony Y must not use ordinary-gamepad inversion");

        QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTY, -24'000));
        SDL_UpdateJoysticks();
        QTRY_VERIFY_WITH_TIMEOUT(SDL_GetGamepadAxis(gamepad, SDL_GAMEPAD_AXIS_LEFTY) < 0, 1'000);
        QTRY_VERIFY_WITH_TIMEOUT(
            !snapshots.isEmpty() && latestSony(snapshots).leftStickY < 0, 1'000);

        QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTY, -32'767));
        SDL_UpdateJoysticks();
        QTRY_VERIFY_WITH_TIMEOUT(
            !snapshots.isEmpty() && latestSony(snapshots).leftStickY < 0, 1'000);
        QVERIFY(SDL_DetachVirtualJoystick(id));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 0, 2'000);
    }

    void selectedSonySnapshotMatchesItsClaim()
    {
        ControllerInput input;
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.vendor_id = 0x054c;
        descriptor.product_id = 0x05c4;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.axis_mask = (1u << SDL_GAMEPAD_AXIS_COUNT) - 1;
        descriptor.name = "OpenNOW selected Sony controller";
        const auto first = SDL_AttachVirtualJoystick(&descriptor);
        const auto second = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY(first != 0 && second != 0);
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 2, 2'000);
        input.setShellCaptureEnabled(false);
        QSignalSpy snapshots(&input, &ControllerInput::sonySnapshot);
        QSignalSpy claimsChanged(&input, &ControllerInput::deviceClaimsChanged);
        const auto claimsBefore = claimsChanged.size();
        input.setInputControllerId(second);
        QTRY_VERIFY_WITH_TIMEOUT(claimsChanged.size() > claimsBefore, 1'000);
        QTRY_VERIFY_WITH_TIMEOUT(!snapshots.isEmpty(), 1'000);
        const auto snapshot = latestSony(snapshots);
        const auto claims = input.deviceClaims();
        QCOMPARE(claims.size(), 1);
        QCOMPARE(claims.at(0).slot, snapshot.slot);
        QCOMPARE(claims.at(0).incarnation, snapshot.incarnation);
        QCOMPARE(snapshot.slot, 0u);
        QVERIFY(SDL_DetachVirtualJoystick(first));
        QVERIFY(SDL_DetachVirtualJoystick(second));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 0, 2'000);
    }

    void sonyKeepalivePublishesSnapshotsWithoutEvents()
    {
        ControllerInput input;
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.vendor_id = 0x054c;
        descriptor.product_id = 0x05c4;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.axis_mask = (1u << SDL_GAMEPAD_AXIS_COUNT) - 1;
        descriptor.name = "OpenNOW keepalive Sony controller";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 1, 2'000);
        input.setShellCaptureEnabled(false);
        QSignalSpy snapshots(&input, &ControllerInput::sonySnapshot);
        QTRY_VERIFY_WITH_TIMEOUT(!snapshots.isEmpty(), 1'000);
        const auto before = snapshots.size();
        QTRY_VERIFY_WITH_TIMEOUT(snapshots.size() > before, 1'000);
        QCOMPARE(latestSony(snapshots).slot, 0u);
        QVERIFY(SDL_DetachVirtualJoystick(id));
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 0, 2'000);
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
