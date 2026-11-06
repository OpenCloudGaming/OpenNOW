#include "acceptance/AcceptanceSession.h"
#include "app/AppController.h"

#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QTimer>
#include <QVariantMap>

#include <cstdlib>
#include <memory>

using namespace Qt::StringLiterals;

int AcceptanceSession::startSteamLaunchWorkload()
{
    auto *window = m_engine.rootObjects().isEmpty() ? nullptr
        : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
    auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
    if (!window || !store) return EXIT_FAILURE;
    if (window->visibility() != QWindow::FullScreen) {
        qCritical("--fullscreen did not open the window fullscreen");
        return EXIT_FAILURE;
    }
    const auto quitExpected = std::make_shared<bool>(false);
    connect(&m_controller, &AppController::applicationExitCommitted, this, [quitExpected] {
        if (!*quitExpected) qFatal("--quit-after-session quit while a session was stopping or relaunching");
    });
    const auto session = [](const QString &id) {
        return QVariantMap{{u"sessionId"_s, id}};
    };
    store->setProperty("pendingLaunchParams", QVariantMap{
        {u"title"_s, u"Steam launch fixture"_s}, {u"variantId"_s, u"100362511"_s}});
    store->setProperty("activeSession", session(u"steam-launch-first"_s));
    const auto step = std::make_shared<int>(0);
    auto *timer = new QTimer(this);
    timer->setInterval(200);
    connect(timer, &QTimer::timeout, this, [this, window, store, quitExpected, session, step, timer] {
        const auto current = (*step)++;
        const auto fail = [this, current, timer](const char *message) {
            qCritical("Steam launch step %d: %s", current, message);
            timer->stop();
            m_application.exit(EXIT_FAILURE);
        };
        if (m_qmlWarningOccurred) return fail("QML warning");
        switch (current) {
        case 0:
            store->setProperty("forceNewAfterStop", true);
            store->setProperty("streamStopRequestId", u"steam-launch-stop"_s);
            return;
        case 1:
            store->setProperty("streamStopRequestId", QString{});
            store->setProperty("activeSession", QVariant{});
            store->setProperty("forceNewAfterStop", false);
            return;
        case 2:
            if (window->visibility() != QWindow::FullScreen) return fail("the window left fullscreen");
            store->setProperty("launchInspectRequestId", u"steam-launch-inspect"_s);
            return;
        case 3:
            store->setProperty("launchInspectRequestId", QString{});
            store->setProperty("streamCreateRequestId", u"steam-launch-create"_s);
            return;
        case 4:
            store->setProperty("streamCreateRequestId", QString{});
            store->setProperty("activeSession", session(u"steam-launch-replacement"_s));
            return;
        case 5:
            *quitExpected = true;
            store->setProperty("activeSession", QVariant{});
            return;
        default:
            return fail("OpenNOW stayed open after the server ended the session");
        }
    });
    timer->start();
    return EXIT_SUCCESS;
}
