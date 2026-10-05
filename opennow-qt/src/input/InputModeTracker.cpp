#include "input/InputModeTracker.h"

#include "app/AppController.h"
#include "input/ControllerInput.h"

#include <QEvent>
#include <QKeyEvent>
#include <QKeySequence>
#include <QMouseEvent>

InputModeTracker::InputModeTracker(AppController *controller, QObject *parent)
    : QObject(parent)
    , m_controller(controller)
{
    connect(controller, &AppController::consoleLaunchInputBlockedChanged,
            this, &InputModeTracker::updateLaunchDrain);
}

void InputModeTracker::updateLaunchDrain()
{
    m_controller->setConsoleLaunchInputDraining(
        (m_controller->consoleLaunchInputBlocked() || m_controller->consoleLaunchInputDraining())
        && (!m_pressedKeys.isEmpty() || m_pressedButtons != Qt::NoButton
            || m_touchActive || m_tabletActive));
}

bool InputModeTracker::eventFilter(QObject *watched, QEvent *event)
{
    if (event->type() == QEvent::ShortcutOverride || event->type() == QEvent::KeyPress) {
        const auto *key = static_cast<QKeyEvent *>(event);
        if ((key->key() == Qt::Key_F4 && key->modifiers() == Qt::AltModifier)
                || key->matches(QKeySequence::Quit))
            return QObject::eventFilter(watched, event);
    }
    const bool blocked = m_controller->consoleLaunchInputBlocked()
        || m_controller->consoleLaunchInputDraining();
    bool inputEvent = false;
    bool skip = false;
    bool cancelledInput = false;
    switch (event->type()) {
    case QEvent::ShortcutOverride:
        if (blocked) {
            event->accept();
            return true;
        }
        break;
    case QEvent::KeyPress:
    case QEvent::KeyRelease: {
        const auto *key = static_cast<QKeyEvent *>(event);
        const quint64 identity = (quint64(key->nativeScanCode()) << 32) | quint32(key->key());
        inputEvent = true;
        cancelledInput = m_cancelledKeys.contains(identity);
        if (event->type() == QEvent::KeyPress && !key->isAutoRepeat()) {
            m_cancelledKeys.remove(identity);
            cancelledInput = false;
        } else if (event->type() == QEvent::KeyRelease && !key->isAutoRepeat()) {
            m_cancelledKeys.remove(identity);
        }
        if (!key->isAutoRepeat()
                && key->nativeScanCode() != ControllerInput::syntheticControllerScanCode) {
            if (event->type() == QEvent::KeyPress) m_pressedKeys.insert(identity);
            else m_pressedKeys.remove(identity);
        }
        skip = event->type() == QEvent::KeyPress && !key->isAutoRepeat();
        break;
    }
    case QEvent::MouseButtonPress:
    case QEvent::MouseButtonDblClick:
    case QEvent::MouseButtonRelease: {
        const auto *mouse = static_cast<QMouseEvent *>(event);
        cancelledInput = event->type() == QEvent::MouseButtonRelease
            && m_cancelledButtons.testFlag(mouse->button());
        m_cancelledButtons &= ~mouse->button();
        m_pressedButtons = mouse->buttons();
        inputEvent = true;
        skip = event->type() != QEvent::MouseButtonRelease;
        break;
    }
    case QEvent::TouchBegin:
        m_touchActive = true;
        inputEvent = skip = true;
        break;
    case QEvent::TouchEnd:
    case QEvent::TouchCancel:
        m_touchActive = false;
        inputEvent = true;
        break;
    case QEvent::TabletPress:
        m_tabletActive = true;
        inputEvent = skip = true;
        break;
    case QEvent::TabletRelease:
        m_tabletActive = false;
        inputEvent = true;
        break;
    case QEvent::Wheel:
        inputEvent = skip = true;
        break;
    case QEvent::MouseMove:
    case QEvent::HoverMove:
    case QEvent::TouchUpdate:
    case QEvent::TabletMove:
        inputEvent = true;
        break;
    case QEvent::ApplicationDeactivate:
        if (blocked) {
            m_cancelledKeys.unite(m_pressedKeys);
            m_cancelledButtons |= m_pressedButtons;
        }
        m_pressedKeys.clear();
        m_pressedButtons = Qt::NoButton;
        m_touchActive = m_tabletActive = false;
        break;
    default:
        break;
    }
    updateLaunchDrain();
    if ((blocked && inputEvent) || cancelledInput) {
        if (blocked && skip) emit m_controller->consoleLaunchSkipRequested();
        event->accept();
        return true;
    }
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
