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

int AcceptanceSession::startConsoleSessionWorkload()
{
    auto *window = m_engine.rootObjects().isEmpty() ? nullptr
        : qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
    auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
    if (!window || !store || window->property("desktopSurfaceActive").toBool())
        return EXIT_FAILURE;
    if (m_arguments.contains(u"--smoke-exit-fullscreen"_s)) window->showFullScreen();
    window->requestActivate();
    store->setProperty("streamerStartRequestId", u"console-session-fixture"_s);
    store->setProperty("streamInputPauseRequestId", u"console-session-fixture"_s);
    store->setProperty("selectedGame", QVariantMap{{u"title"_s, u"Console acceptance game"_s}});
    store->setProperty("activeSession", QVariantMap{{u"sessionId"_s, u"console-session-fixture"_s},
        {u"queuePosition"_s, 12}, {u"seatSetupStep"_s, 1}});
    store->setProperty("streamState", u"queued"_s);
    m_controller.navigate(u"inserting"_s);

    struct State {
        int step = 0;
        QPointer<QQuickItem> surface;
    };
    const auto state = std::make_shared<State>();
    auto *timer = new QTimer(this);
    timer->setInterval(300);
    connect(timer, &QTimer::timeout, this, [this, window, store, state, timer] {
        const auto require = [this, state, timer](bool ok, const char *message) {
            if (!ok) {
                qCritical("Console session step %d: %s", state->step, message);
                timer->stop();
                m_application.exit(EXIT_FAILURE);
            }
            return ok;
        };
        const auto key = [window](Qt::Key value) {
            QKeyEvent press(QEvent::KeyPress, value, Qt::NoModifier);
            QGuiApplication::sendEvent(window, &press);
            QKeyEvent release(QEvent::KeyRelease, value, Qt::NoModifier);
            QGuiApplication::sendEvent(window, &release);
        };
        const auto setStreamer = [store](const QString &status, bool firstFrame) {
            QVariantMap snapshot{{u"sessionId"_s, u"console-session-fixture"_s}, {u"status"_s, status}};
            if (firstFrame) snapshot.insert(u"firstFrameLatencyMs"_s, 37);
            if (status == u"error"_s) snapshot.insert(u"message"_s, u"Connection interrupted"_s);
            store->setProperty("streamer", snapshot);
        };
        const auto confirm = [store] {
            return QMetaObject::invokeMethod(store, "requestStreamExitConfirmation");
        };
        const auto safeFocused = [window] {
            return window->activeFocusItem()
                && window->activeFocusItem()->objectName() == u"streamExitKeepPlaying"_s;
        };
        auto *loader = window->findChild<QObject *>(u"mainRouteLoader"_s);
        auto *page = loader ? loader->property("item").value<QObject *>() : nullptr;
        auto *surface = window->findChild<QQuickItem *>(u"streamSurfaceHost"_s);
        if (!require(!m_qmlWarningOccurred && page, "QML warning or missing console page")) return;
        if (state->step <= 14 && state->surface && !require(surface == state->surface && m_controller.route() == u"stream"_s,
                                      "an overlay recreated or navigated away from the stream")) return;
        switch (state->step++) {
        case 0:
            if (!require(page->property("queued").toBool(), "queue state was not displayed")) return;
            key(Qt::Key_Escape);
            break;
        case 1:
            if (!require(m_controller.overlay() == u"desktop-stream-exit-confirm"_s && safeFocused(),
                         "queue cancellation did not focus Keep waiting")) return;
            {
                const QVariantMap queued{{u"sessionId"_s, u"console-session-fixture"_s},
                    {u"phase"_s, u"queued"_s}, {u"queuePosition"_s, 12}, {u"seatSetupStep"_s, 1},
                    {u"adState"_s, QVariantMap{{u"sessionAdsRequired"_s, true},
                        {u"sessionAds"_s, QVariantList{QVariantMap{{u"adId"_s, u"fixture"_s}}}}}}};
                if (!require(QMetaObject::invokeMethod(store, "acceptStreamingSession", Q_ARG(QVariant, queued))
                        && m_controller.overlay() == u"desktop-stream-exit-confirm"_s,
                             "a queue poll replaced the cancellation confirmation with an ad")) return;
            }
            key(Qt::Key_Return);
            if (!require(m_controller.overlay().isEmpty() && store->property("streamState") == u"queued"_s,
                         "Enter on Keep waiting cancelled the session")) return;
            setStreamer(u"starting"_s, false);
            store->setProperty("streamState", u"starting"_s);
            m_controller.navigate(u"stream"_s);
            break;
        case 2:
            state->surface = surface;
            if (!require(surface && page->property("launchCoverVisible").toBool()
                    && !surface->property("inputEnabled").toBool(), "initial connection exposed gameplay input")) return;
            setStreamer(u"streaming"_s, false);
            store->setProperty("streamState", u"streaming"_s);
            break;
        case 3:
            if (!require(page->property("launchCoverVisible").toBool()
                    && !surface->property("inputEnabled").toBool()
                    && window->property("shellCaptureEnabledForSmokeTest").toBool(),
                         "streaming status bypassed first-frame input ownership")) return;
            setStreamer(u"streaming"_s, true);
            break;
        case 4:
            if (!require(!page->property("launchCoverVisible").toBool()
                    && surface->property("inputEnabled").toBool()
                    && !window->property("shellCaptureEnabledForSmokeTest").toBool(),
                         "first frame did not hand input to the stream")) return;
            m_controller.showOverlay(u"guide-session"_s);
            break;
        case 5:
            if (!require(!surface->property("inputEnabled").toBool()
                    && window->findChild<QQuickItem *>(u"consoleSessionGuide"_s),
                         "guide failed to keep media and capture local input")) return;
            m_controller.showOverlay(u"guide-controls"_s);
            break;
        case 6:
            key(Qt::Key_Escape);
            if (!require(m_controller.overlay() == u"guide-session"_s,
                         "Back from the controller panel did not return to the guide")) return;
            break;
        case 7:
            key(Qt::Key_Return);
            if (!require(m_controller.overlay().isEmpty(), "Resume did not close the guide")) return;
            break;
        case 8:
            if (!require(surface->property("inputEnabled").toBool() && confirm(),
                         "guide did not restore input or confirmation failed")) return;
            break;
        case 9:
            if (!require(safeFocused(), "session confirmation did not focus Keep playing")) return;
            key(Qt::Key_Return);
            if (!require(m_controller.overlay().isEmpty() && store->property("streamState") == u"streaming"_s,
                         "Enter on Keep playing ended the session")) return;
            setStreamer(u"starting"_s, false);
            store->setProperty("streamState", u"reconnecting"_s);
            break;
        case 10:
            if (!require(page->property("reconnectBannerVisible").toBool()
                    && !surface->property("inputEnabled").toBool() && confirm(),
                         "reconnect lost its banner or allowed gameplay input")) return;
            break;
        case 11:
            if (!require(safeFocused(), "reconnect cancellation did not focus the safe action")) return;
            setStreamer(u"streaming"_s, true);
            store->setProperty("streamState", u"streaming"_s);
            break;
        case 12:
            if (!require(m_controller.overlay() == u"desktop-stream-exit-confirm"_s
                    && !surface->property("inputEnabled").toBool(),
                         "recovery dismissed confirmation or leaked gameplay input")) return;
            key(Qt::Key_Escape);
            break;
        case 13:
            if (!require(surface->property("inputEnabled").toBool(), "cancel failed to restore recovered input")) return;
            setStreamer(u"error"_s, false);
            store->setProperty("streamState", u"error"_s);
            break;
        case 14:
            if (!require(page->property("launchCoverVisible").toBool() && page->property("failed").toBool()
                    && !surface->property("inputEnabled").toBool(), "error lost its recovery UI")) return;
            store->setProperty("activeSession", QVariant{});
            store->setProperty("streamer", QVariantMap{{u"status"_s, u"stopped"_s}});
            store->setProperty("streamState", u"idle"_s);
            store->setProperty("streamerStartRequestId", QString{});
            store->setProperty("streamInputPauseRequestId", QString{});
            m_controller.navigate(u"home"_s);
            store->setProperty("updaterFailureMessage", u"Fixture update could not be completed"_s);
            break;
        case 15:
            if (!require(window->activeFocusItem()
                    && window->activeFocusItem()->objectName() == u"consoleUpdateFailureDismiss"_s,
                         "update failure notice did not take safe focus")) return;
            key(Qt::Key_Return);
            break;
        case 16: {
            auto *focus = window->activeFocusItem();
            while (focus && focus != page) focus = focus->parentItem();
            if (!require(store->property("updaterFailureMessage").toString().isEmpty() && focus == page,
                         "dismissing an update notice stranded focus outside the console page")) return;
            m_controller.navigate(u"updates"_s);
            break;
        }
        case 17:
            key(Qt::Key_Escape);
            break;
        case 18:
            if (m_controller.route() == u"updates"_s) key(Qt::Key_Escape);
            if (!require(m_controller.route() != u"updates"_s, "Back is trapped between update controls")) return;
            m_controller.navigate(u"settings-video-dropdown"_s);
            break;
        case 19:
            if (!require(page->property("dropdownOpen").toBool(), "settings choice fixture did not open")) return;
            key(Qt::Key_Escape);
            key(Qt::Key_Escape);
            if (!require(m_controller.route().startsWith(u"settings"_s)
                    && !page->property("dropdownOpen").toBool(), "quick double Back navigated away during sheet dismissal")) return;
            timer->stop();
            m_application.exit(EXIT_SUCCESS);
            break;
        }
    });
    timer->start();
    return EXIT_SUCCESS;
}
