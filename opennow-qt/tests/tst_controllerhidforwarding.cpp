#include "input/ControllerInput.h"

#include <QCoreApplication>
#include <QSignalSpy>
#include <QTest>

class ControllerHidForwardingTest final : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase()
    {
        // The env switch would force forwarding on and mask the setter path.
        qunsetenv("OPENNOW_GENERIC_HID");
    }

    void parseVidPidAcceptsHexPairsWithAnOptionalInterface()
    {
        quint16 vendor = 0;
        quint16 product = 0;
        QVERIFY(ControllerInput::parseVidPid(QStringLiteral("0738:2221"), vendor, product));
        QCOMPARE(vendor, quint16(0x0738));
        QCOMPARE(product, quint16(0x2221));
        QVERIFY(ControllerInput::parseVidPid(QStringLiteral(" 0738:A221:2 "), vendor, product));
        QCOMPARE(product, quint16(0xa221));
        QVERIFY(ControllerInput::parseVidPid(QStringLiteral("044f:0402:00"), vendor, product));
        QCOMPARE(vendor, quint16(0x044f));
        for (const auto &invalid : {QStringLiteral("0738"), QStringLiteral("0738:22211"),
                                    QStringLiteral("zzzz:2221"), QStringLiteral("0738:2221:x"),
                                    QStringLiteral("0738:2221:123"), QStringLiteral("0738:2221:0:0"),
                                    QStringLiteral(""), QStringLiteral("738:2221")}) {
            QVERIFY2(!ControllerInput::parseVidPid(invalid, vendor, product), qPrintable(invalid));
        }
    }

    void forwardedDevicesAreSkippedBySdlAndReturnedWhenForwardingStops()
    {
        ControllerInput input;
        QSignalSpy enabledChanges(&input, &ControllerInput::hidForwardingEnabledChanged);
        QSignalSpy listChanges(&input, &ControllerInput::forwardedHidDevicesChanged);
        input.setForwardedHidDevices({QStringLiteral("0738:2221"), QStringLiteral("bad"),
                                      QStringLiteral("0738:A221")});
        QCOMPARE(listChanges.count(), 1);
        QCOMPARE(input.forwardedHidDevices(),
                 QStringList({QStringLiteral("0738:2221"), QStringLiteral("0738:a221")}));
        QVERIFY(input.isForwardedHidDevice(0x0738, 0x2221));
        QVERIFY(input.isForwardedHidDevice(0x0738, 0xa221));
        QVERIFY(!input.isForwardedHidDevice(0x0738, 0x0001));
        QVERIFY(!input.isForwardedHidDevice(0, 0));
        input.setHidForwardingEnabled(true);
        QCOMPARE(enabledChanges.count(), 1);
        input.setHidForwardingEnabled(true);
        QCOMPARE(enabledChanges.count(), 1);

        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.button_mask = 1u << SDL_GAMEPAD_BUTTON_SOUTH;
        descriptor.vendor_id = 0x0738;
        descriptor.product_id = 0x2221;
        descriptor.name = "OpenNOW forwarded X-56 stand-in";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        QVERIFY2(SDL_IsGamepad(id), "the virtual device must be gamepad-shaped for this test");
        QCOMPARE(SDL_GetGamepadVendorForID(id), quint16(0x0738));
        QTRY_COMPARE_WITH_TIMEOUT(input.skippedForwardedCount(), 1, 2000);
        QCOMPARE(input.controllerCount(), 0);
        QVERIFY(input.deviceClaims().isEmpty());

        // Disabling forwarding hands the device back to SDL immediately.
        input.setHidForwardingEnabled(false);
        QCOMPARE(input.skippedForwardedCount(), 0);
        QCOMPARE(input.controllerCount(), 1);
        QCOMPARE(input.deviceClaims().size(), 1);
        QCOMPARE(input.deviceClaims().first().vendor, quint16(0x0738));

        // Re-enabling closes the open slot and publishes a neutral snapshot.
        input.setShellCaptureEnabled(false);
        QSignalSpy snapshots(&input, &ControllerInput::gamepadSnapshot);
        input.setHidForwardingEnabled(true);
        QCOMPARE(input.controllerCount(), 0);
        QCOMPARE(input.skippedForwardedCount(), 1);
        QVERIFY(!snapshots.isEmpty());
        const auto neutral = snapshots.last();
        QCOMPARE(neutral.at(2).value<quint16>(), quint16(0));
        QCOMPARE(neutral.at(1).value<quint16>(), quint16(0));

        // Shrinking the allowlist also returns the device.
        input.setForwardedHidDevices({QStringLiteral("0738:a221")});
        QCOMPARE(input.skippedForwardedCount(), 0);
        QCOMPARE(input.controllerCount(), 1);
        input.setForwardedHidDevices({QStringLiteral("0738:2221")});
        QCOMPARE(input.controllerCount(), 0);
        QCOMPARE(input.skippedForwardedCount(), 1);

        // Unplugging a skipped device forgets it.
        SDL_DetachVirtualJoystick(id);
        QTRY_COMPARE_WITH_TIMEOUT(input.skippedForwardedCount(), 0, 2000);
        QCOMPARE(input.controllerCount(), 0);
    }

    void unrelatedDevicesStayWithSdlWhileForwardingIsEnabled()
    {
        ControllerInput input;
        input.setForwardedHidDevices({QStringLiteral("0738:2221")});
        input.setHidForwardingEnabled(true);
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.vendor_id = 0x045e;
        descriptor.product_id = 0x02ea;
        descriptor.name = "OpenNOW ordinary pad";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 1, 2000);
        QCOMPARE(input.skippedForwardedCount(), 0);
        SDL_DetachVirtualJoystick(id);
        QTRY_COMPARE_WITH_TIMEOUT(input.controllerCount(), 0, 2000);
    }
};

QTEST_MAIN(ControllerHidForwardingTest)
#include "tst_controllerhidforwarding.moc"
