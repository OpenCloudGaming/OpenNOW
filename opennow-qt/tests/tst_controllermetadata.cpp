#include "input/ControllerInput.h"

#include <QSignalSpy>
#include <QTest>
#include <QVariantMap>

class ControllerMetadataTest final : public QObject
{
    Q_OBJECT

private slots:
    void identity_data()
    {
        QTest::addColumn<int>("vendor");
        QTest::addColumn<int>("product");
        QTest::addColumn<QString>("family");
        QTest::newRow("ps3") << 0x054c << 0x0268 << QStringLiteral("playstation");
        QTest::newRow("ps4") << 0x054c << 0x05c4 << QStringLiteral("playstation");
        QTest::newRow("ps5") << 0x054c << 0x0ce6 << QStringLiteral("playstation");
        QTest::newRow("xbox360") << 0x045e << 0x028e << QStringLiteral("xbox");
        QTest::newRow("xboxone") << 0x045e << 0x02d1 << QStringLiteral("xbox");
        QTest::newRow("unknown") << 0x0000 << 0x0000 << QStringLiteral("generic");
    }

    void identity()
    {
        QFETCH(int, vendor);
        QFETCH(int, product);
        QFETCH(QString, family);
        ControllerInput input;
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.vendor_id = static_cast<Uint16>(vendor);
        descriptor.product_id = static_cast<Uint16>(product);
        descriptor.name = "OpenNOW metadata test controller";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        struct Detach {
            SDL_JoystickID id;
            ~Detach() { if (id) SDL_DetachVirtualJoystick(id); }
        } detach{id};
        QTRY_COMPARE(input.controllers().size(), 1);
        const auto metadata = input.controllers().first().toMap();
        QCOMPARE(metadata.value(QStringLiteral("family")).toString(), family);
        QCOMPARE(metadata.value(QStringLiteral("name")).toString(), QString::fromUtf8(descriptor.name));
        QCOMPARE(metadata.value(QStringLiteral("powerState")).toString(), QStringLiteral("unknown"));
        QCOMPARE(metadata.value(QStringLiteral("batteryPercent")).toInt(), -1);
        QCOMPARE(metadata.value(QStringLiteral("charging")).toBool(), false);
        QCOMPARE(input.availableControllers().first(), input.controllers().first());

        auto *gamepad = SDL_GetGamepadFromID(id);
        QVERIFY(gamepad);
        auto *mapping = SDL_GetGamepadMapping(gamepad);
        QVERIFY(mapping);
        auto fields = QByteArray(mapping).split(',');
        SDL_free(mapping);
        fields.removeIf([](const QByteArray &field) { return field.startsWith("type:"); });
        const auto overridden = fields.join(',') + ",type:switchpro,";
        QVERIFY2(SDL_AddGamepadMapping(overridden.constData()) >= 0, SDL_GetError());
        QCOMPARE(SDL_GetGamepadType(gamepad), SDL_GAMEPAD_TYPE_NINTENDO_SWITCH_PRO);
        input.setInputControllerId(id);
        QCOMPARE(input.controllers().first().toMap().value(QStringLiteral("family")).toString(), family);

        QSignalSpy changes(&input, &ControllerInput::controllersChanged);
        QTest::qWait(2100);
        QCOMPARE(changes.size(), 0);
        QVERIFY(SDL_DetachVirtualJoystick(id));
        detach.id = 0;
        QTRY_VERIFY(input.controllers().isEmpty());
        QVERIFY(input.availableControllers().isEmpty());
    }
};

QTEST_GUILESS_MAIN(ControllerMetadataTest)
#include "tst_controllermetadata.moc"
