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
    store->setProperty("streamer", QVariantMap{{u"status"_s, u"streaming"_s}});
    store->setProperty("streamState", u"streaming"_s);
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
    connect(timer, &QTimer::timeout, this, [this, window, store, fullscreen, state, timer] {
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
        if (state->step < 6 && !require(state->surface && state->surface->isVisible()
                && window->findChild<QQuickItem *>(u"streamSurfaceHost"_s) == state->surface
                && m_controller.route() == u"stream"_s,
                "confirmation recreated or left the stream surface")) return;
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
