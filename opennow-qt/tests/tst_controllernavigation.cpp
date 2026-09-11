#include "input/ControllerInput.h"

#include <QGuiApplication>
#include <QQmlComponent>
#include <QQuickItem>
#include <QQuickView>
#include <QScopeGuard>
#include <QTest>

class ControllerNavigationTest final : public QObject
{
    Q_OBJECT

private slots:
    void routesKeysThroughFocusedChildrenAndOverlays_data()
    {
        QTest::addColumn<bool>("fullscreen");
        QTest::newRow("windowed") << false;
        QTest::newRow("fullscreen") << true;
    }

    void routesKeysThroughFocusedChildrenAndOverlays()
    {
        QFETCH(bool, fullscreen);
        QQuickView view;
        QQmlComponent component(view.engine());
        component.setData(R"qml(
            import QtQuick
            FocusScope {
                width: 400; height: 300
                property int navigationPresses: 0
                property int activations: 0
                property int releases: 0
                Keys.onPressed: event => {
                    if (event.key === Qt.Key_Left) navigationPresses++
                    if (event.key === Qt.Key_Return) activations++
                    event.accepted = true
                }
                Keys.onReleased: event => {
                    if (event.key === Qt.Key_Return) releases++
                    event.accepted = true
                }
                Item { objectName: "focusedChild"; focus: true }
                FocusScope {
                    objectName: "overlay"
                    visible: false
                    property int navigationPresses: 0
                    property int activations: 0
                    property int releases: 0
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Left) navigationPresses++
                        if (event.key === Qt.Key_Return) activations++
                        event.accepted = true
                    }
                    Keys.onReleased: event => {
                        if (event.key === Qt.Key_Return) releases++
                        event.accepted = true
                    }
                    Item { objectName: "overlayChild"; focus: true }
                }
            }
        )qml", QUrl());
        auto *root = qobject_cast<QQuickItem *>(component.create());
        QVERIFY2(root, qPrintable(component.errorString()));
        view.setContent(QUrl(), &component, root);
        if (fullscreen) view.showFullScreen();
        else view.show();
        view.requestActivate();
        QTRY_VERIFY(view.isActive());
        auto *child = root->findChild<QQuickItem *>(QStringLiteral("focusedChild"));
        auto *overlay = root->findChild<QQuickItem *>(QStringLiteral("overlay"));
        auto *overlayChild = root->findChild<QQuickItem *>(QStringLiteral("overlayChild"));
        QVERIFY(child && overlay && overlayChild);
        child->forceActiveFocus();
        QCOMPARE(QGuiApplication::focusObject(), child);

        ControllerInput input;
        SDL_VirtualJoystickDesc descriptor{};
        SDL_INIT_INTERFACE(&descriptor);
        descriptor.type = SDL_JOYSTICK_TYPE_GAMEPAD;
        descriptor.naxes = SDL_GAMEPAD_AXIS_COUNT;
        descriptor.nbuttons = SDL_GAMEPAD_BUTTON_COUNT;
        descriptor.axis_mask = 1u << SDL_GAMEPAD_AXIS_LEFTX;
        descriptor.name = "OpenNOW menu navigation controller";
        const auto id = SDL_AttachVirtualJoystick(&descriptor);
        QVERIFY2(id != 0, SDL_GetError());
        const auto cleanup = qScopeGuard([id] { SDL_DetachVirtualJoystick(id); });
        QTRY_COMPARE(input.controllerCount(), 1);
        auto *joystick = SDL_GetJoystickFromID(id);
        QVERIFY(joystick);

        QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTX, -19000));
        QTRY_COMPARE(root->property("navigationPresses").toInt(), 1);
        QVERIFY(SDL_SetJoystickVirtualAxis(joystick, SDL_GAMEPAD_AXIS_LEFTX, 0));
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QTRY_COMPARE(root->property("activations").toInt(), 1);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));
        QTRY_COMPARE(root->property("releases").toInt(), 1);

        overlay->setVisible(true);
        overlayChild->forceActiveFocus();
        QCOMPARE(QGuiApplication::focusObject(), overlayChild);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_DPAD_LEFT, true));
        QTRY_COMPARE(overlay->property("navigationPresses").toInt(), 1);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_DPAD_LEFT, false));
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, true));
        QTRY_COMPARE(overlay->property("activations").toInt(), 1);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_SOUTH, false));
        QTRY_COMPARE(overlay->property("releases").toInt(), 1);
        QCOMPARE(root->property("activations").toInt(), 1);
        QCOMPARE(root->property("navigationPresses").toInt(), 1);

        overlay->setVisible(false);
        child->forceActiveFocus();
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_DPAD_LEFT, true));
        QTRY_COMPARE(root->property("navigationPresses").toInt(), 2);
        QVERIFY(SDL_SetJoystickVirtualButton(joystick, SDL_GAMEPAD_BUTTON_DPAD_LEFT, false));
        view.close();
    }
};

QTEST_MAIN(ControllerNavigationTest)

#include "tst_controllernavigation.moc"
