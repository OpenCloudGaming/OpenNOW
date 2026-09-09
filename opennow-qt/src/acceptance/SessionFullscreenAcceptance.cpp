#include "acceptance/AcceptanceSession.h"
#include "app/AppController.h"

#include <QGuiApplication>
#include <QKeyEvent>
#include <QQmlApplicationEngine>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>
#include <QVariantMap>

#include <cstdlib>
#include <memory>

using namespace Qt::StringLiterals;

int AcceptanceSession::startSessionFullscreenWorkload()
{
    auto *window = m_engine.rootObjects().isEmpty() ? nullptr
        : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
    auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
    if (!window || !store) return EXIT_FAILURE;
    m_controller.navigate(u"home"_s);
    store->setProperty("settings", QVariantMap{{u"autoFullScreen"_s, false}});
    store->setProperty("streamer", QVariantMap{{u"status"_s, u"streaming"_s}});
    store->setProperty("streamState", u"streaming"_s);
    const auto restoredVisibility = m_arguments.contains(u"--smoke-fullscreen-restore-maximized"_s)
        ? QWindow::Maximized
        : m_arguments.contains(u"--smoke-fullscreen-restore-fullscreen"_s)
            ? QWindow::FullScreen : QWindow::Windowed;
    if (restoredVisibility == QWindow::Maximized) window->showMaximized();
    if (restoredVisibility == QWindow::FullScreen
        && !QMetaObject::invokeMethod(window, "toggleFullscreen")) return EXIT_FAILURE;
    const auto sessionVisibility = restoredVisibility == QWindow::FullScreen
        ? QWindow::Windowed : QWindow::FullScreen;
    const auto nextSessionVisibility = restoredVisibility == QWindow::Windowed
        ? QWindow::Maximized : QWindow::Windowed;
    window->requestActivate();
    const auto step = std::make_shared<int>(0);
    auto *timer = new QTimer(this);
    timer->setInterval(150);
    connect(timer, &QTimer::timeout, this,
            [this, window, store, restoredVisibility, sessionVisibility, nextSessionVisibility, step, timer] {
        const auto require = [this, step, timer](bool ok, const char *message) {
            if (!ok) {
                qCritical("Session fullscreen step %d: %s", *step, message);
                timer->stop();
                m_application.exit(EXIT_FAILURE);
            }
            return ok;
        };
        const auto keyClick = [window](Qt::Key key) {
            QKeyEvent press(QEvent::KeyPress, key, Qt::NoModifier);
            QGuiApplication::sendEvent(window, &press);
            QKeyEvent release(QEvent::KeyRelease, key, Qt::NoModifier);
            QGuiApplication::sendEvent(window, &release);
        };
        if (!require(!m_qmlWarningOccurred, "QML warning")) return;
        switch ((*step)++) {
        case 0:
            if (!require(window->visibility() == restoredVisibility,
                         "initial window state not applied")) return;
            m_controller.navigate(u"inserting"_s);
            if (!require(QMetaObject::invokeMethod(window, "toggleFullscreen"),
                         "fullscreen toggle unavailable")) return;
            break;
        case 1:
            if (!require(window->visibility() == sessionVisibility,
                         "launch screen did not toggle fullscreen")) return;
            m_controller.navigate(u"stream"_s);
            break;
        case 2: {
            auto *shortcut = window->findChild<QObject *>(u"shellFullscreenShortcut"_s);
            if (!require(!shortcut || !shortcut->property("enabled").toBool(),
                         "shell shortcut competed with stream input")) return;
            if (!require(window->visibility() == sessionVisibility,
                         "stream entry changed the session window mode")) return;
            if (!require(QMetaObject::invokeMethod(store, "requestStreamExitConfirmation"),
                         "exit confirmation unavailable")) return;
            break;
        }
        case 3:
            if (!require(m_controller.overlay() == u"desktop-stream-exit-confirm"_s
                    && window->visibility() == sessionVisibility,
                    "exit confirmation changed fullscreen state")) return;
            keyClick(Qt::Key_Escape);
            break;
        case 4:
            if (!require(m_controller.overlay().isEmpty()
                    && m_controller.route() == u"stream"_s
                    && window->visibility() == sessionVisibility,
                    "cancelling exit changed the session window mode")) return;
            if (!require(QMetaObject::invokeMethod(store, "requestStreamExitConfirmation"),
                         "exit confirmation unavailable")) return;
            break;
        case 5:
            keyClick(Qt::Key_Return);
            break;
        case 6:
            if (!require(m_controller.route() != u"stream"_s
                    && store->property("streamState").toString() == u"idle"_s
                    && window->visibility() == restoredVisibility,
                    "session exit did not restore the pre-session window mode")) return;
            keyClick(Qt::Key_F11);
            break;
        case 7:
            if (!require(window->visibility() == sessionVisibility,
                         "F11 did not toggle fullscreen from the restored shell")) return;
            window->contentItem()->forceActiveFocus();
            keyClick(Qt::Key_F11);
            break;
        case 8:
            if (!require(window->visibility() == restoredVisibility,
                         "F11 depended on the shell page focus")) return;
            if (nextSessionVisibility == QWindow::Maximized) window->showMaximized();
            else window->showNormal();
            break;
        case 9:
            m_controller.navigate(u"inserting"_s);
            if (!require(QMetaObject::invokeMethod(window, "toggleFullscreen"),
                         "second session fullscreen toggle unavailable")) return;
            break;
        case 10:
            m_controller.navigate(u"joining"_s);
            break;
        case 11:
            m_controller.navigate(u"stream"_s);
            break;
        case 12:
            m_controller.navigate(u"inserting"_s);
            break;
        case 13:
            if (!require(window->visibility() == QWindow::FullScreen,
                         "session route transitions restored the window prematurely")) return;
            m_controller.navigate(u"home"_s);
            break;
        case 14:
            if (!require(window->visibility() == nextSessionVisibility,
                         "aborted session reused the previous session's window mode")) return;
            store->setProperty("catalogState", u"loading"_s);
            store->setProperty("settings", QVariantMap{{u"autoFullScreen"_s, true}});
            m_controller.directLaunchRequested(u"12345"_s, u"Fullscreen acceptance fixture"_s);
            break;
        case 15:
            if (!require(window->visibility() == nextSessionVisibility,
                         "direct launch entered fullscreen before the session was ready")) return;
            m_controller.navigate(u"inserting"_s);
            break;
        case 16:
            if (!require(window->visibility() == nextSessionVisibility,
                         "session preparation entered fullscreen before readiness")) return;
            m_controller.navigate(u"stream"_s);
            break;
        case 17:
            if (!require(window->visibility() == QWindow::FullScreen,
                         "ready session did not apply automatic fullscreen")) return;
            if (!require(QMetaObject::invokeMethod(window, "toggleFullscreen"),
                         "manual fullscreen override unavailable")) return;
            break;
        case 18:
            if (!require(window->visibility() == nextSessionVisibility,
                         "manual fullscreen exit did not restore the launch mode")) return;
            m_controller.showOverlay(u"desktop-stream-menu"_s);
            break;
        case 19:
            if (!require(window->visibility() == nextSessionVisibility,
                         "overlay reapplied automatic fullscreen")) return;
            m_controller.showOverlay(QString{});
            m_controller.navigate(u"joining"_s);
            break;
        case 20:
            m_controller.navigate(u"stream"_s);
            break;
        case 21:
            if (!require(window->visibility() == nextSessionVisibility,
                         "reconnect overrode the manual window mode")) return;
            m_controller.navigate(u"home"_s);
            break;
        case 22:
            if (!require(window->visibility() == nextSessionVisibility,
                         "direct launch captured its automatic fullscreen as the initial mode")) return;
            m_controller.navigate(u"joining"_s);
            break;
        case 23:
            m_controller.navigate(u"stream"_s);
            break;
        case 24:
            if (!require(window->visibility() == QWindow::FullScreen,
                         "new session did not rearm automatic fullscreen")) return;
            m_controller.navigate(u"home"_s);
            break;
        case 25:
            if (!require(window->visibility() == nextSessionVisibility,
                         "automatic fullscreen did not restore the pre-session mode")) return;
            timer->stop();
            m_application.exit(EXIT_SUCCESS);
            break;
        }
    });
    timer->start();
    return EXIT_SUCCESS;
}
