#include "acceptance/AcceptanceSession.h"
#include "app/AppController.h"

#include <QGuiApplication>
#include <QJSValue>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>
#include <QVariantMap>

#include <cstdio>
#include <cstdlib>
#include <memory>

using namespace Qt::StringLiterals;

int AcceptanceSession::prepareWindow()
{
    // Isolated visual acceptance options never start or alter a real account.
    if (m_smokeTest && !m_engine.rootObjects().isEmpty()) {
        auto *window = qobject_cast<QQuickWindow *>(m_engine.rootObjects().first());
        const auto dimension = [this](const QString &option, int fallback) {
            const auto index = m_arguments.indexOf(option);
            if (index < 0 || index + 1 >= m_arguments.size()) return fallback;
            bool ok = false;
            const int value = m_arguments.at(index + 1).toInt(&ok);
            return ok ? qBound(540, value, 3840) : fallback;
        };
        if (window) window->resize(dimension(u"--smoke-width"_s, 1600),
                                   dimension(u"--smoke-height"_s, 900));
        const auto resumeIndex = m_arguments.indexOf(u"--smoke-session-resume"_s);
        if (resumeIndex >= 0) {
            if (resumeIndex + 1 >= m_arguments.size()) return EXIT_FAILURE;
            const auto mode = m_arguments.at(resumeIndex + 1);
            if (mode != u"conflict"_s && mode != u"unavailable"_s && mode != u"resuming"_s)
                return EXIT_FAILURE;
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (!store) return EXIT_FAILURE;
            const QVariantMap selected{{u"launchAppId"_s, u"123"_s},
                {u"title"_s, u"Zenless Zone Zero"_s},
                {u"heroImageUrl"_s, u"qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg"_s}};
            store->setProperty("selectedGame", selected);
            store->setProperty("pendingLaunchParams", QVariantMap{
                {u"appId"_s, u"123"_s}, {u"title"_s, u"Zenless Zone Zero"_s}});
            store->setProperty("catalogGames", QVariantList{selected, QVariantMap{
                {u"launchAppId"_s, u"456"_s}, {u"title"_s, u"Genshin Impact"_s}}});
            m_controller.navigate(u"inserting"_s);
            if (mode == u"conflict"_s) {
                const QVariant sessions = QVariantMap{
                    {u"sessions"_s, QVariantList{QVariantMap{
                        {u"sessionId"_s, u"resume-visual-fixture"_s},
                        {u"appId"_s, u"456"_s}, {u"status"_s, 4}}}}};
                if (!QMetaObject::invokeMethod(store, "inspectRemoteSessions", Q_ARG(QVariant, sessions)))
                    return EXIT_FAILURE;
            } else if (mode == u"unavailable"_s) {
                store->setProperty("launchConflictDetected", true);
                const QVariant sessions = QVariantMap{{u"sessions"_s, QVariantList{}}};
                if (!QMetaObject::invokeMethod(store, "inspectRemoteSessions", Q_ARG(QVariant, sessions)))
                    return EXIT_FAILURE;
            } else {
                store->setProperty("streamState", u"resuming"_s);
                store->setProperty("streamMessage", tr("Reconnecting to your running game. You don't need to start again."));
            }
        }
        if (m_arguments.contains(u"--smoke-microphone-supported"_s)
                || m_arguments.contains(u"--smoke-microphone-muted"_s)) {
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (!store) return EXIT_FAILURE;
            const bool muted = m_arguments.contains(u"--smoke-microphone-muted"_s);
            store->setProperty("streamerStartRequestId", u"microphone-visual-fixture"_s);
            store->setProperty("streamInputPauseRequestId", u"microphone-visual-fixture"_s);
            store->setProperty("nativeRuntimeReady", true);
            store->setProperty("nativeRuntimeCapabilities", QVariantMap{
                {u"protocolVersion"_s, 6}, {u"supportsMicrophone"_s, true}});
            store->setProperty("settings", QVariantMap{
                {u"microphoneMode"_s, muted ? u"voice-activity"_s : u"disabled"_s}});
            if (muted) {
                store->setProperty("sessionMicrophoneMode", u"voice-activity"_s);
                store->setProperty("selectedGame", QVariantMap{{u"title"_s, u"Microphone acceptance fixture"_s}});
                store->setProperty("activeSession", QVariantMap{
                    {u"sessionId"_s, u"microphone-visual-fixture"_s}, {u"phase"_s, u"ready"_s}});
                store->setProperty("streamer", QVariantMap{
                    {u"status"_s, u"streaming"_s}, {u"microphoneState"_s, u"muted"_s},
                    {u"microphoneEnabled"_s, false},
                    {u"capabilities"_s, QVariantMap{{u"supportsMicrophone"_s, true}}}});
                store->setProperty("streamState", u"streaming"_s);
            }
        }
        if (m_arguments.contains(u"--smoke-paper-design"_s)) {
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (store) {
                store->setProperty("settings", QVariantMap{
                    {u"themePack"_s, u"aurora"_s}, {u"appTheme"_s, u"dark"_s},
                    {u"desktopSidebarHover"_s, false}, {u"desktopRailCollapsed"_s, true},
                    {u"resolution"_s, u"2560x1440"_s}, {u"fps"_s, 120},
                    {u"codec"_s, u"av1"_s}, {u"colorQuality"_s, u"10bit_420"_s},
                    {u"enableHdr"_s, false},
                    {u"maxBitrateMbps"_s, 75}, {u"enableCloudGsync"_s, true}});
                // Visual fixtures stay in the smoke-only QML store. The core is
                // not started, so no account state or preferences are persisted.
                QVariantList accounts;
                const QStringList providers{u"Steam"_s, u"Epic Games"_s, u"Xbox"_s,
                    u"Ubisoft"_s, u"Battle.net"_s, u"GOG"_s, u"Gaijin"_s};
                for (qsizetype i = 0; i < providers.size(); ++i) {
                    accounts.append(QVariantMap{
                        {u"provider"_s, providers.at(i)}, {u"label"_s, providers.at(i)},
                        {u"isConnected"_s, i < 3}, {u"supportsSync"_s, i < 3},
                        {u"syncedGames"_s, 42},
                        {u"status"_s, i < 3 ? u"connected"_s : i == 3 ? u"expired"_s : u"disconnected"_s}});
                }
                store->setProperty("gameAccounts", accounts);
                store->setProperty("gameAccountsState", u"ready"_s);
                store->setProperty("regions", QVariantList{
                    QVariantMap{{u"name"_s,u"EU West"_s},{u"url"_s,u"https://west.example.invalid"_s}},
                    QVariantMap{{u"name"_s,u"EU Central"_s},{u"url"_s,u"https://central.example.invalid"_s}},
                    QVariantMap{{u"name"_s,u"US East"_s},{u"url"_s,u"https://east.example.invalid"_s}}});
                store->setProperty("regionPingResults", QVariantMap{
                    {u"https://west.example.invalid"_s,9},{u"https://central.example.invalid"_s,21},
                    {u"https://east.example.invalid"_s,94}});
            }
        }
        if (m_arguments.contains(u"--smoke-resolution-open"_s)
                || m_arguments.contains(u"--smoke-resolution-fits-monitor"_s)) {
            auto *picker = window ? window->findChild<QObject *>(u"renewResolutionPicker"_s) : nullptr;
            if (!picker) return EXIT_FAILURE;
            if (m_arguments.contains(u"--smoke-resolution-open"_s)) picker->setProperty("expanded", true);
            if (m_arguments.contains(u"--smoke-resolution-fits-monitor"_s)) picker->setProperty("fitsMonitor", true);
        }
        if (m_arguments.contains(u"--smoke-light-theme"_s)) {
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (store) store->setProperty("settings", QVariantMap{{u"appTheme"_s, u"light"_s}});
        }
        if (m_arguments.contains(u"--smoke-settings-advanced"_s)) {
            auto *settings = window ? window->findChild<QObject *>(u"desktopSettingsScreen"_s) : nullptr;
            if (settings) settings->setProperty("advancedOpen", true);
        }
        const auto panelIndex = m_arguments.indexOf(u"--smoke-settings-panel"_s);
        if (panelIndex >= 0 && panelIndex + 1 < m_arguments.size()) {
            const auto panel = m_arguments.at(panelIndex + 1);
            const QStringList panels{u"stats"_s,u"audio"_s,u"interface"_s,u"console"_s,
                u"shortcuts"_s,u"controllers"_s,u"subscription"_s,u"recording"_s};
            auto *settings = window ? window->findChild<QObject *>(u"desktopSettingsScreen"_s) : nullptr;
            if (!settings || !panels.contains(panel)) return EXIT_FAILURE;
            settings->setProperty("acceptancePanel", panel);
        }
        if (m_arguments.contains(u"--smoke-choice-open"_s)) {
            auto *picker = window ? window->findChild<QObject *>(u"renewNetworkRegion"_s) : nullptr;
            if (!picker && window) picker = window->findChild<QObject *>(u"renewLanguageChoice"_s);
            if (!picker && window) picker = window->findChild<QObject *>(u"streamBackendChoice"_s);
            if (!picker) return EXIT_FAILURE;
            picker->setProperty("expanded", true);
        }
        if (m_arguments.contains(u"--smoke-renew-settings-actions"_s)) {
            // Repeater delegates may be incubated after the settings Loader.
            // Exercise controls once the first layout has had time to complete.
            QTimer::singleShot(150, this, [this, window] {
            const auto checkActions = [this, window]() -> int {
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (!window || !store) return EXIT_FAILURE;
            const auto findVisual = [](auto &&self, QQuickItem *item, const QString &name) -> QObject * {
                if (!item) return nullptr;
                if (item->objectName() == name) return item;
                for (auto *child : item->childItems())
                    if (auto *match = self(self, child, name)) return match;
                return nullptr;
            };
            const auto findControl = [window, &findVisual](const QString &name) -> QObject * {
                if (auto *object = window->findChild<QObject *>(name)) return object;
                return findVisual(findVisual, window->contentItem(), name);
            };
            const auto setting = [store](const QString &key) {
                const auto settings = store->property("settings");
                return settings.canConvert<QJSValue>()
                    ? settings.value<QJSValue>().property(key).toVariant()
                    : settings.toMap().value(key);
            };
            bool exercised = false;
            if (auto *theme = findControl(u"renewThemeChoice"_s)) {
                exercised = true;
                const auto items = theme->property("items").value<QJSValue>();
                if (items.property(u"length"_s).toInt() != 8) return EXIT_FAILURE;
                theme->setProperty("expanded", true);
                if (!QMetaObject::invokeMethod(theme,"selected",Q_ARG(QVariant,QVariant(u"bone"_s)))
                    || setting(u"themePack"_s).toString() != u"bone"_s) return EXIT_FAILURE;
            }
            if (auto *shortcuts = findControl(u"renewShortcutsDisclosure"_s)) {
                exercised = true;
                auto *settings = findControl(u"desktopSettingsScreen"_s);
                if (!QMetaObject::invokeMethod(shortcuts,"expansionRequested")) return EXIT_FAILURE;
                auto *inlinePanel = findControl(u"renewInlineShortcuts"_s);
                if (!settings || settings->property("advancedOpen").toBool()
                    || !inlinePanel || !inlinePanel->property("expanded").toBool()) return EXIT_FAILURE;
            }
            if (auto *region = window->findChild<QObject *>(u"renewNetworkRegion"_s)) {
                exercised = true;
                if (!QMetaObject::invokeMethod(region, "selected", Q_ARG(QVariant, QVariant(u"https://central.example.invalid"_s)))
                    || setting(u"region"_s).toString() != u"https://central.example.invalid"_s) return EXIT_FAILURE;
                auto *field = window->findChild<QObject *>(u"renewProxyAddress"_s);
                auto *toggle = window->findChild<QObject *>(u"renewProxyEnabled"_s);
                if (!field || !toggle) return EXIT_FAILURE;
                field->setProperty("text", u"http://proxy.example.invalid:8080"_s);
                if (!QMetaObject::invokeMethod(field,"editingFinished")
                    || !QMetaObject::invokeMethod(toggle,"valueChangedByUser",Q_ARG(bool,true))
                    || setting(u"sessionProxyUrl"_s).toString() != u"http://proxy.example.invalid:8080"_s
                    || !setting(u"sessionProxyEnabled"_s).toBool()) return EXIT_FAILURE;
            }
            if (auto *channel = window->findChild<QObject *>(u"renewUpdateChannel"_s)) {
                exercised = true;
                if (!QMetaObject::invokeMethod(channel,"selected",Q_ARG(int,1),
                        Q_ARG(QVariant,QVariant(QVariantMap{{u"label"_s,u"Nightly"_s},{u"value"_s,u"nightly"_s}})))
                    || setting(u"updateChannel"_s).toString() != u"nightly"_s) return EXIT_FAILURE;
            }
            if (auto *fps = findControl(u"renew-statsShowFps"_s)) {
                exercised = true;
                auto *region = findControl(u"renew-statsShowRegion"_s);
                if (!region || !QMetaObject::invokeMethod(fps,"valueChangedByUser",Q_ARG(bool,false))
                    || !QMetaObject::invokeMethod(region,"valueChangedByUser",Q_ARG(bool,false))
                    || !setting(u"statsShowFps"_s).isValid() || !setting(u"statsShowRegion"_s).isValid()
                    || setting(u"statsShowFps"_s).toBool() || setting(u"statsShowRegion"_s).toBool()) return EXIT_FAILURE;
            }
            if (!exercised) return EXIT_FAILURE;
            return EXIT_SUCCESS;
            };
            if (checkActions() != EXIT_SUCCESS) {
                std::fprintf(stderr, "Desktop Renew settings action acceptance failed\n");
                m_application.exit(EXIT_FAILURE);
            }
            });
        }
    }
    return EXIT_SUCCESS;
}
