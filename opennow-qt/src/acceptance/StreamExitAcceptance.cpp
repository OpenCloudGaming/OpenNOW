#include "acceptance/AcceptanceSession.h"
#include "app/AppController.h"

#include <QGuiApplication>
#include <QKeyEvent>
#include <QPointer>
#include <QQmlApplicationEngine>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>
#include <QVariantMap>

#include <cstdlib>
#include <memory>

using namespace Qt::StringLiterals;

int AcceptanceSession::startStreamExitWorkload()
{
    auto *window = m_engine.rootObjects().isEmpty() ? nullptr
        : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
    auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
    if (!window || !store) return EXIT_FAILURE;
    const bool windowClose = m_arguments.contains(u"--smoke-exit-window-close"_s);
    const auto originalRoute = m_controller.route();
    const auto originalState = windowClose && originalRoute != u"stream"_s
        ? (originalRoute == u"inserting"_s ? u"queued"_s : u"idle"_s) : u"streaming"_s;
    store->setProperty("streamer", QVariantMap{{u"status"_s,
        originalState == u"streaming"_s ? u"streaming"_s : u"stopped"_s}});
    store->setProperty("streamState", originalState);
    if (windowClose && originalRoute != u"home"_s)
        store->setProperty("activeSession", QVariantMap{{u"sessionId"_s, u"window-close-fixture"_s}, {u"status"_s, 3}});
    const bool fullscreen = m_arguments.contains(u"--smoke-exit-fullscreen"_s);
    if (fullscreen) window->showFullScreen();
    window->requestActivate();
    struct State {
        int step = 0;
        QPointer<QQuickItem> surface;
    };
    const auto state = std::make_shared<State>();
    auto *timer = new QTimer(this);
    timer->setInterval(150);
    connect(timer, &QTimer::timeout, this, [this, window, store, fullscreen, windowClose, originalRoute, originalState, state, timer] {
        const auto require = [this, state, timer](bool ok, const char *message) {
            if (!ok) {
                qCritical("Stream exit step %d: %s", state->step, message);
                timer->stop();
                m_application.exit(EXIT_FAILURE);
            }
            return ok;
        };
        const auto keyClick = [window](Qt::Key key, Qt::KeyboardModifiers modifiers = Qt::NoModifier,
                                      bool repeat = false) {
            QKeyEvent press(QEvent::KeyPress, key, modifiers, {}, repeat);
            QGuiApplication::sendEvent(window, &press);
            QKeyEvent release(QEvent::KeyRelease, key, modifiers, {}, repeat);
            QGuiApplication::sendEvent(window, &release);
        };
        const auto openConfirmation = [state] {
            return QMetaObject::invokeMethod(state->surface, "localShortcutRequested",
                Q_ARG(QString, u"stop-stream"_s));
        };
        if (!require(!m_qmlWarningOccurred, "QML warning")
            || !require(window->visibility() == (fullscreen && state->step < 6
                            ? QWindow::FullScreen : QWindow::Windowed),
                        "confirmation or completed exit has the wrong window mode")) return;
        if (state->step == 0)
            state->surface = window->findChild<QQuickItem *>(u"streamSurfaceHost"_s);
        if (state->step < 6 && (!windowClose || originalRoute == u"stream"_s)
                && !require(state->surface && state->surface->isVisible()
                && window->findChild<QQuickItem *>(u"streamSurfaceHost"_s) == state->surface
                && m_controller.route() == u"stream"_s,
                "confirmation recreated or left the stream surface")) return;
        if (windowClose) {
            if (!require(store->property("streamState").toString() == originalState
                    && m_controller.route() == originalRoute,
                    "window confirmation changed the session or route")) return;
            switch (state->step++) {
            case 0:
            case 2:
            case 4:
                if (!require(window->isVisible() && m_controller.overlay().isEmpty()
                        && (!state->surface || state->surface->property("inputEnabled").toBool()),
                        "window close or cancellation did not retain the shell and input")) return;
                if (!require(!window->close(), "native window close bypassed confirmation")) return;
                break;
            case 1:
            case 3:
            case 5:
                if (!require(window->isVisible() && m_controller.overlay() == u"application-quit-confirm"_s
                        && (!state->surface || !state->surface->property("inputEnabled").toBool())
                        && window->activeFocusItem()
                        && window->activeFocusItem()->objectName() == u"quitConfirmKeepOpen"_s,
                        "application confirmation did not retain the window and own input")) return;
                if (state->step == 2) keyClick(Qt::Key_Space);
                else if (state->step == 4) keyClick(Qt::Key_Escape);
                else {
                    auto *confirmation = window->findChild<QQuickItem *>(u"applicationQuitConfirmation"_s);
                    if (!require(confirmation && QMetaObject::invokeMethod(confirmation, "confirmRequested"),
                            "application confirmation action unavailable")) return;
                    if (!require(!window->isVisible() && window->property("applicationCloseConfirmed").toBool(),
                            "confirmed native close did not close the window")) return;
                    timer->stop();
                }
                break;
            }
            return;
        }
        switch (state->step++) {
        case 0:
            if (!require(state->surface->property("inputEnabled").toBool(),
                         "closed overlay blocked gameplay")) return;
            if (!require(openConfirmation(), "stop-stream shortcut unavailable")) return;
            break;
        case 1:
        case 3:
        case 5: {
            if (!require(m_controller.overlay() == u"desktop-stream-exit-confirm"_s
                    && !state->surface->property("inputEnabled").toBool()
                    && window->activeFocusItem()
                    && window->activeFocusItem()->objectName() == u"streamExitKeepPlaying"_s,
                    "confirmation did not own input with safe default focus")) return;
            if (state->step == 2) {
                keyClick(Qt::Key_Space);
            } else if (state->step == 4) {
                keyClick(Qt::Key_Escape);
            } else {
                keyClick(Qt::Key_Return, Qt::NoModifier, true);
                if (!require(m_controller.overlay() == u"desktop-stream-exit-confirm"_s,
                             "auto-repeat confirmed session exit")) return;
                if (m_arguments.contains(u"--smoke-exit-tab-space"_s)) {
                    keyClick(Qt::Key_Tab);
                    if (!require(window->activeFocusItem()
                            && window->activeFocusItem()->objectName() == u"streamExitEndSession"_s,
                            "Tab did not focus End session")) return;
                    keyClick(Qt::Key_Space);
                } else if (m_arguments.contains(u"--smoke-exit-enter"_s)) {
                    keyClick(Qt::Key_Enter, Qt::KeypadModifier);
                } else {
                    keyClick(Qt::Key_Return);
                }
            }
            break;
        }
        case 2:
        case 4:
            if (!require(m_controller.overlay().isEmpty()
                    && state->surface->property("inputEnabled").toBool()
                    && store->property("streamState").toString() == u"streaming"_s,
                    "cancel ended the session or failed to restore input")) return;
            if (state->step == 3) {
                m_controller.showOverlay(u"desktop-stream-menu"_s);
                keyClick(Qt::Key_Q, Qt::ControlModifier | Qt::ShiftModifier);
            } else {
                m_controller.showOverlay(u"desktop-stream-stats"_s);
                if (!require(openConfirmation(), "stop-stream shortcut unavailable over stats")) return;
            }
            break;
        case 6:
            if (!require(m_controller.overlay().isEmpty() && m_controller.route() != u"stream"_s
                    && store->property("streamState").toString() == u"idle"_s,
                    "confirmation key did not end the session")) return;
            timer->stop();
            m_application.exit(EXIT_SUCCESS);
            break;
        }
    });
    timer->start();
    return EXIT_SUCCESS;
}
