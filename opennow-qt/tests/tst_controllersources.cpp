#include "input/ControllerInput.h"

#include <QCoreApplication>
#include <QKeyEvent>
#include <QSignalSpy>
#include <QTest>

class VirtualPad final
{
public:
    explicit VirtualPad(bool sony = false)
    {
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.axis_mask = 1u << SDL_GAMEPAD_AXIS_LEFTX;
        descriptor.button_mask = (1u << SDL_GAMEPAD_BUTTON_COUNT) - 1;
        descriptor.name = "OpenNOW duplicate-source test pad";
        if (sony) {
            descriptor.vendor_id = 0x054c;
            descriptor.product_id = 0x05c4;
        }
        id = SDL_AttachVirtualJoystick(&descriptor);
    }

    ~VirtualPad()
    {
        if (id) SDL_DetachVirtualJoystick(id);
    }

    bool axis(Sint16 value) const
    {
        return SDL_SetJoystickVirtualAxis(SDL_GetJoystickFromID(id), SDL_GAMEPAD_AXIS_LEFTX, value);
    }

    bool button(bool pressed, SDL_GamepadButton button = SDL_GAMEPAD_BUTTON_SOUTH) const
    {
        return SDL_SetJoystickVirtualButton(SDL_GetJoystickFromID(id), button, pressed);
    }

    SDL_JoystickID id = 0;
};

class SourceKeySink final : public QObject
{
public:
    SourceKeySink() { QCoreApplication::instance()->installEventFilter(this); }
    ~SourceKeySink() override { QCoreApplication::instance()->removeEventFilter(this); }

    QHash<int, int> presses;
    QHash<int, int> releases;

protected:
    bool eventFilter(QObject *, QEvent *event) override
    {
        if (event->type() == QEvent::KeyPress || event->type() == QEvent::KeyRelease) {
            const auto *key = static_cast<QKeyEvent *>(event);
            if (key->nativeScanCode() == ControllerInput::syntheticControllerScanCode) {
                auto &counts = event->type() == QEvent::KeyPress ? presses : releases;
                ++counts[key->key()];
            }
        }
        return false;
    }
};

class ControllerSourcesTest final : public QObject
{
    Q_OBJECT

private slots:
    void anotherDeviceCannotCancelHeldNavigation()
    {
        ControllerInput input;
        SourceKeySink sink;
        VirtualPad first;
        VirtualPad second;
        QVERIFY(first.id && second.id);
        QTRY_COMPARE(input.controllerCount(), 2);
        QVERIFY(first.axis(-24000));
        QTRY_COMPARE(sink.presses.value(Qt::Key_Left), 1);
        QVERIFY(second.axis(1000));
        QTest::qWait(100);
        QTRY_VERIFY_WITH_TIMEOUT(sink.presses.value(Qt::Key_Left) >= 2, 1000);
        const auto beforeDisconnect = sink.presses.value(Qt::Key_Left);
        QVERIFY(SDL_DetachVirtualJoystick(second.id));
        second.id = 0;
        QTRY_COMPARE(input.controllerCount(), 1);
        QTRY_VERIFY_WITH_TIMEOUT(sink.presses.value(Qt::Key_Left) > beforeDisconnect, 1000);
        QVERIFY(first.axis(0));
        QTest::qWait(100);
        const auto released = sink.presses.value(Qt::Key_Left);
        QTest::qWait(350);
        QCOMPARE(sink.presses.value(Qt::Key_Left), released);
    }

    void selectingSourceFiltersShellGameplayAndGuide()
    {
        ControllerInput input;
        SourceKeySink sink;
        QSignalSpy snapshots(&input, &ControllerInput::gamepadSnapshot);
        QSignalSpy actions(&input, &ControllerInput::localActionRequested);
        QSignalSpy activity(&input, &ControllerInput::controllerActivity);
        VirtualPad physical;
        VirtualPad mapped;
        QVERIFY(physical.id && mapped.id);
        QTRY_COMPARE(input.controllerCount(), 2);
        input.setInputControllerId(mapped.id);
        QCOMPARE(input.controllerCount(), 1);
        QCOMPARE(input.availableControllers().size(), 2);
        QCOMPARE(input.controllers().size(), 1);
        QCOMPARE(input.controllers().first().toMap().value(QStringLiteral("slot")).toInt(), 1);
        QCOMPARE(input.controllers().first().toMap().value(QStringLiteral("instanceId")).toUInt(), mapped.id);
        QVERIFY(physical.button(true));
        QVERIFY(physical.axis(-24000));
        QTest::qWait(350);
        QCOMPARE(sink.presses.value(Qt::Key_Return), 0);
        QCOMPARE(sink.presses.value(Qt::Key_Left), 0);
        QCOMPARE(activity.size(), 0);
        QVERIFY(mapped.button(true));
        QTRY_COMPARE(sink.presses.value(Qt::Key_Return), 1);
        input.setShellCaptureEnabled(false);
        QTRY_COMPARE(sink.releases.value(Qt::Key_Return), 1);
        snapshots.clear();
        QVERIFY(physical.button(true, SDL_GAMEPAD_BUTTON_GUIDE));
        QVERIFY(mapped.axis(24000));
        QTest::qWait(150);
        QCOMPARE(actions.size(), 0);
        QVERIFY(!snapshots.isEmpty());
        for (const auto &snapshot : snapshots) {
            QCOMPARE(snapshot.at(0).toUInt(), 0u);
            QCOMPARE(snapshot.at(1).toUInt(), 0x0101u);
            QCOMPARE(snapshot.at(2).toUInt(), 0x1000u);
            QVERIFY(snapshot.at(5).toInt() >= 0);
        }
        QVERIFY(mapped.button(true, SDL_GAMEPAD_BUTTON_GUIDE));
        QTRY_COMPARE(actions.size(), 1);
        input.setInputSuspended(true);
        QCOMPARE(snapshots.last().at(2).toUInt(), 0u);
        QCOMPARE(snapshots.last().at(5).toInt(), 0);
        const auto suspended = snapshots.size();
        QTest::qWait(150);
        QCOMPARE(snapshots.size(), suspended);
        input.setInputSuspended(false);
        QCOMPARE(snapshots.last().at(2).toUInt(), 0x1000u);
        QVERIFY(snapshots.last().at(5).toInt() > 0);
    }

    void sourceSwitchNeutralizesOldPlayersAndRestoresMultiplayer()
    {
        ControllerInput input;
        QSignalSpy snapshots(&input, &ControllerInput::gamepadSnapshot);
        VirtualPad first;
        VirtualPad second;
        QVERIFY(first.id && second.id);
        QTRY_COMPARE(input.controllerCount(), 2);
        input.setShellCaptureEnabled(false);
        QVERIFY(first.button(true));
        QVERIFY(second.axis(24000));
        QTest::qWait(100);
        snapshots.clear();
        input.setInputControllerId(second.id);
        QCOMPARE(snapshots.size(), 3);
        for (int index = 0; index < 2; ++index) {
            QCOMPARE(snapshots.at(index).at(0).toInt(), index);
            QCOMPARE(snapshots.at(index).at(2).toUInt(), 0u);
            QCOMPARE(snapshots.at(index).at(5).toInt(), 0);
        }
        QCOMPARE(snapshots.last().at(0).toUInt(), 0u);
        QCOMPARE(snapshots.last().at(1).toUInt(), 0x0101u);
        QVERIFY(snapshots.last().at(5).toInt() > 0);
        snapshots.clear();
        input.setInputControllerId(0);
        QCOMPARE(input.controllerCount(), 2);
        QCOMPARE(snapshots.size(), 3);
        QCOMPARE(snapshots.at(0).at(5).toInt(), 0);
        QCOMPARE(snapshots.at(1).at(0).toUInt(), 0u);
        QCOMPARE(snapshots.at(1).at(1).toUInt(), 0x0303u);
        QCOMPARE(snapshots.at(1).at(2).toUInt(), 0x1000u);
        QCOMPARE(snapshots.at(2).at(0).toUInt(), 1u);
        QVERIFY(snapshots.at(2).at(5).toInt() > 0);
    }

    void disconnectedSelectionReturnsToAutomaticInput()
    {
        ControllerInput input;
        QSignalSpy snapshots(&input, &ControllerInput::gamepadSnapshot);
        VirtualPad physical;
        VirtualPad mapped;
        QVERIFY(physical.id && mapped.id);
        QTRY_COMPARE(input.controllerCount(), 2);
        input.setInputControllerId(mapped.id);
        input.setShellCaptureEnabled(false);
        QSignalSpy selectionChanges(&input, &ControllerInput::inputControllerIdChanged);
        const auto selected = mapped.id;
        snapshots.clear();
        QVERIFY(SDL_DetachVirtualJoystick(mapped.id));
        mapped.id = 0;
        QTRY_COMPARE(input.availableControllers().size(), 1);
        auto *pollTimer = input.findChild<QTimer *>(QStringLiteral("controllerPollTimer"));
        QVERIFY(pollTimer);
        QCOMPARE(pollTimer->interval(), 100);
        QCOMPARE(input.inputControllerId(), selected);
        QCOMPARE(selectionChanges.size(), 0);
        QCOMPARE(input.controllerCount(), 0);
        QCOMPARE(input.controllers().size(), 0);
        QVERIFY(!snapshots.isEmpty());
        QCOMPARE(snapshots.first().at(0).toUInt(), 0u);
        QCOMPARE(snapshots.first().at(1).toUInt(), 0u);
        for (int index = 2; index < 9; ++index)
            QCOMPARE(snapshots.first().at(index).toInt(), 0);
        QCOMPARE(snapshots.size(), 1);
        input.setInputControllerId(0);
        QCOMPARE(input.inputControllerId(), 0u);
        QCOMPARE(selectionChanges.size(), 1);
        QCOMPARE(input.controllerCount(), 1);
        QCOMPARE(pollTimer->interval(), 4);
        QCOMPARE(snapshots.last().at(1).toUInt(), 0x0101u);
        snapshots.clear();
        VirtualPad reconnected;
        QVERIFY(reconnected.id);
        QTRY_COMPARE(input.availableControllers().size(), 2);
        QVERIFY(physical.button(true));
        QVERIFY(reconnected.button(true));
        QTest::qWait(150);
        QCOMPARE(input.controllerCount(), 2);
        QVERIFY(!snapshots.isEmpty());
        for (const auto &snapshot : snapshots)
            QCOMPARE(snapshot.at(1).toUInt(), 0x0303u);
        input.setInputControllerId(reconnected.id);
        QCOMPARE(input.controllerCount(), 1);
        QCOMPARE(pollTimer->interval(), 4);
        QCOMPARE(snapshots.last().at(0).toUInt(), 0u);
        QCOMPARE(snapshots.last().at(2).toUInt(), 0x1000u);
    }

    void disconnectedExclusiveSourceBlocksOtherDevices_data()
    {
        QTest::addColumn<bool>("sony");
        QTest::addColumn<bool>("shell");
        QTest::newRow("generic-gameplay") << false << false;
        QTest::newRow("generic-shell") << false << true;
        QTest::newRow("sony-gameplay") << true << false;
        QTest::newRow("sony-shell") << true << true;
    }

    void disconnectedExclusiveSourceBlocksOtherDevices()
    {
        QFETCH(bool, sony);
        QFETCH(bool, shell);
        ControllerInput input;
        SourceKeySink sink;
        QSignalSpy snapshots(&input, &ControllerInput::gamepadSnapshot);
        QSignalSpy sonySnapshots(&input, &ControllerInput::sonySnapshot);
        QSignalSpy actions(&input, &ControllerInput::localActionRequested);
        QSignalSpy activity(&input, &ControllerInput::controllerActivity);
        QSignalSpy selectionChanges(&input, &ControllerInput::inputControllerIdChanged);
        VirtualPad other(sony);
        VirtualPad selected(sony);
        QVERIFY(other.id && selected.id);
        QTRY_COMPARE(input.controllerCount(), 2);
        const auto selectedId = selected.id;
        input.setInputControllerId(selectedId);
        input.setShellCaptureEnabled(shell);
        QVERIFY(selected.button(true));
        QVERIFY(selected.axis(-24000));
        if (shell) {
            QTRY_COMPARE(sink.presses.value(Qt::Key_Return), 1);
            QTRY_VERIFY(sink.presses.value(Qt::Key_Left) > 0);
        } else if (sony) {
            QTRY_VERIFY(!sonySnapshots.isEmpty()
                && sonySnapshots.last().at(0).value<ControllerInput::SonySnapshot>().buttons == 0x1000
                && sonySnapshots.last().at(0).value<ControllerInput::SonySnapshot>().leftStickX < 0);
        } else {
            QTRY_VERIFY(!snapshots.isEmpty() && snapshots.last().at(2).toUInt() == 0x1000u
                && snapshots.last().at(5).toInt() < 0);
        }
        snapshots.clear();
        sonySnapshots.clear();
        selectionChanges.clear();
        QVERIFY(SDL_DetachVirtualJoystick(selected.id));
        selected.id = 0;
        QTRY_COMPARE(input.availableControllers().size(), 1);
        QCOMPARE(input.inputControllerId(), selectedId);
        QCOMPARE(selectionChanges.size(), 0);
        QCOMPARE(input.controllerCount(), 0);
        QVERIFY(input.controllers().isEmpty());
        QVERIFY(input.deviceClaims().isEmpty());
        if (shell) QTRY_COMPARE(sink.releases.value(Qt::Key_Return), 1);
        if (sony && !shell) {
            QVERIFY(!sonySnapshots.isEmpty());
            const auto neutral = sonySnapshots.last().at(0).value<ControllerInput::SonySnapshot>();
            QCOMPARE(neutral.slot, 0);
            QCOMPARE(neutral.buttons, 0);
            QCOMPARE(neutral.leftStickX, 0);
            QVERIFY(!neutral.touchpadClick);
            for (const auto &contact : neutral.contacts) QVERIFY(!contact.active);
        } else if (!sony) {
            QVERIFY(!snapshots.isEmpty());
            QCOMPARE(snapshots.last().at(0).toUInt(), 0u);
            for (int index = 1; index < 9; ++index)
                QCOMPARE(snapshots.last().at(index).toInt(), 0);
        }
        snapshots.clear();
        sonySnapshots.clear();
        activity.clear();
        const auto presses = sink.presses;
        VirtualPad reconnected(sony);
        QVERIFY(reconnected.id && reconnected.id != selectedId);
        QTRY_COMPARE(input.availableControllers().size(), 2);
        QVERIFY(other.button(true));
        QVERIFY(other.axis(-24000));
        QVERIFY(other.button(true, SDL_GAMEPAD_BUTTON_GUIDE));
        QVERIFY(reconnected.button(true));
        QVERIFY(reconnected.axis(-24000));
        QVERIFY(reconnected.button(true, SDL_GAMEPAD_BUTTON_GUIDE));
        QTest::qWait(350);
        QCOMPARE(input.inputControllerId(), selectedId);
        QCOMPARE(input.controllerCount(), 0);
        QVERIFY(input.deviceClaims().isEmpty());
        QCOMPARE(selectionChanges.size(), 0);
        QCOMPARE(snapshots.size(), 0);
        QCOMPARE(sonySnapshots.size(), 0);
        QCOMPARE(actions.size(), 0);
        QCOMPARE(activity.size(), 0);
        QCOMPARE(sink.presses, presses);

        input.setInputControllerId(reconnected.id);
        QCOMPARE(input.controllerCount(), 1);
        QCOMPARE(input.deviceClaims().size(), 1);
        QCOMPARE(input.deviceClaims().first().slot, 0);
        QCOMPARE(selectionChanges.size(), 1);
        if (shell) {
            QVERIFY(reconnected.button(false));
            QTest::qWait(100);
            QVERIFY(reconnected.button(true));
            QTRY_COMPARE(sink.presses.value(Qt::Key_Return), 2);
            input.setInputControllerId(other.id);
            QTRY_COMPARE(sink.releases.value(Qt::Key_Return), 2);
        } else if (sony) {
            QCOMPARE(sonySnapshots.last().at(0).value<ControllerInput::SonySnapshot>().buttons, 0x1000);
            sonySnapshots.clear();
            input.setInputControllerId(other.id);
            QCOMPARE(sonySnapshots.size(), 2);
            QCOMPARE(sonySnapshots.first().at(0).value<ControllerInput::SonySnapshot>().buttons, 0);
            QCOMPARE(sonySnapshots.first().at(0).value<ControllerInput::SonySnapshot>().leftStickX, 0);
            QCOMPARE(sonySnapshots.last().at(0).value<ControllerInput::SonySnapshot>().buttons, 0x1000);
        } else {
            QCOMPARE(snapshots.last().at(2).toUInt(), 0x1000u);
            snapshots.clear();
            input.setInputControllerId(other.id);
            QCOMPARE(snapshots.size(), 2);
            for (int index = 2; index < 9; ++index)
                QCOMPARE(snapshots.first().at(index).toInt(), 0);
            QCOMPARE(snapshots.last().at(2).toUInt(), 0x1000u);
        }
        QCOMPARE(input.controllers().first().toMap().value(QStringLiteral("instanceId")).toUInt(), other.id);
        input.setInputControllerId(0);
        QCOMPARE(input.controllerCount(), 2);
        QCOMPARE(input.deviceClaims().size(), 2);
    }

    void repeatedReplacementUsesPlayerOne_data()
    {
        QTest::addColumn<bool>("selected");
        QTest::addColumn<bool>("shell");
        QTest::newRow("automatic-gameplay") << false << false;
        QTest::newRow("selected-gameplay") << true << false;
        QTest::newRow("automatic-shell") << false << true;
        QTest::newRow("selected-shell") << true << true;
    }

    void repeatedReplacementUsesPlayerOne()
    {
        QFETCH(bool, selected);
        QFETCH(bool, shell);
        ControllerInput input;
        SourceKeySink sink;
        QSignalSpy snapshots(&input, &ControllerInput::gamepadSnapshot);
        input.setShellCaptureEnabled(shell);
        SDL_JoystickID previous = 0;
        for (int cycle = 0; cycle < 3; ++cycle) {
            VirtualPad pad;
            QVERIFY(pad.id && pad.id != previous);
            const auto previousSelection = previous;
            previous = pad.id;
            QTRY_COMPARE(input.availableControllers().size(), 1);
            QCOMPARE(input.inputControllerId(), selected && cycle > 0 ? previousSelection : 0u);
            if (selected) input.setInputControllerId(pad.id);
            QCOMPARE(input.controllerCount(), 1);
            QCOMPARE(input.controllers().first().toMap().value(QStringLiteral("slot")).toInt(), 1);
            QVERIFY(pad.button(true));
            if (shell) {
                QTRY_COMPARE(sink.presses.value(Qt::Key_Return), cycle + 1);
            } else {
                QTRY_VERIFY(!snapshots.isEmpty() && snapshots.last().at(2).toUInt() == 0x1000u);
                QCOMPARE(snapshots.last().at(0).toUInt(), 0u);
                QCOMPARE(snapshots.last().at(1).toUInt(), 0x0101u);
            }
            snapshots.clear();
            QVERIFY(SDL_DetachVirtualJoystick(pad.id));
            pad.id = 0;
            QTRY_COMPARE(input.controllerCount(), 0);
            QCOMPARE(input.inputControllerId(), selected ? previous : 0u);
            QVERIFY(input.controllers().isEmpty());
            QVERIFY(input.availableControllers().isEmpty());
            if (shell) QTRY_COMPARE(sink.releases.value(Qt::Key_Return), cycle + 1);
            QVERIFY(!snapshots.isEmpty());
            QCOMPARE(snapshots.last().at(0).toUInt(), 0u);
            QCOMPARE(snapshots.last().at(1).toUInt(), 0u);
            for (int index = 2; index < 9; ++index)
                QCOMPARE(snapshots.last().at(index).toInt(), 0);
        }
    }

    void shellButtonsReleaseOnlyAfterLastSourceReleases()
    {
        ControllerInput input;
        SourceKeySink sink;
        VirtualPad first;
        VirtualPad second;
        QVERIFY(first.id && second.id);
        QTRY_COMPARE(input.controllerCount(), 2);
        QVERIFY(first.button(true));
        QVERIFY(second.button(true));
        QTest::qWait(100);
        QCOMPARE(sink.presses.value(Qt::Key_Return), 1);
        QVERIFY(first.button(false));
        QTest::qWait(100);
        QCOMPARE(sink.releases.value(Qt::Key_Return), 0);
        input.setInputSuspended(true);
        QTRY_COMPARE(sink.releases.value(Qt::Key_Return), 1);
        input.setInputSuspended(false);
        QVERIFY(second.button(false));
        QTest::qWait(100);
        QCOMPARE(sink.releases.value(Qt::Key_Return), 1);
        QVERIFY(second.button(true));
        QTRY_COMPARE(sink.presses.value(Qt::Key_Return), 2);
        input.setInputControllerId(first.id);
        QTRY_COMPARE(sink.releases.value(Qt::Key_Return), 2);
    }
};

QTEST_MAIN(ControllerSourcesTest)

#include "tst_controllersources.moc"
