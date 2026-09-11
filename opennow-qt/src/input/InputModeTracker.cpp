#include "input/InputModeTracker.h"

#include "app/AppController.h"
#include "input/ControllerInput.h"

#include <QEvent>
#include <QKeyEvent>

InputModeTracker::InputModeTracker(AppController *controller, QObject *parent)
    : QObject(parent)
    , m_controller(controller)
{
}

bool InputModeTracker::eventFilter(QObject *watched, QEvent *event)
{
    switch (event->type()) {
    case QEvent::KeyPress: {
        const auto *keyEvent = static_cast<QKeyEvent *>(event);
        if (keyEvent->isAutoRepeat()
                && keyEvent->nativeScanCode() == ControllerInput::syntheticControllerScanCode)
            break;
        m_controller->setInputMode(
            keyEvent->nativeScanCode() == ControllerInput::syntheticControllerScanCode
                ? QStringLiteral("controller")
                : QStringLiteral("keyboard"));
        break;
    }
    case QEvent::MouseButtonPress:
    case QEvent::MouseButtonDblClick:
    case QEvent::Wheel:
    case QEvent::TabletPress:
    case QEvent::TouchBegin:
        m_controller->setInputMode(QStringLiteral("pointer"));
        break;
    default:
        break;
    }
    return QObject::eventFilter(watched, event);
}
