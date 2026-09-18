#include "acceptance/AcceptanceSession.h"
#include "app/AppController.h"

#include <QGuiApplication>
#include <QHash>
#include <QJSValue>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>
#include <QVariantMap>
#include <QtMath>

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
        if (m_arguments.contains(u"--smoke-alliance-routing"_s)) {
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (!store) return EXIT_FAILURE;
            const QVariantMap nvidia{{u"idpId"_s, u"nvidia-fixture"_s}, {u"displayName"_s, u"NVIDIA"_s}, {u"code"_s, u"NVIDIA"_s}};
            const QVariantMap alliance{{u"idpId"_s, u"alliance-fixture"_s}, {u"displayName"_s, u"Alliance fixture"_s}, {u"code"_s, u"ALLIANCE"_s}};
            store->setProperty("providers", QVariantList{nvidia, alliance});
            store->setProperty("selectedProviderIdpId", u"alliance-fixture"_s);
            const auto selected = store->property("selectedProvider").value<QJSValue>();
            if (selected.property(u"idpId"_s).toString() != u"alliance-fixture"_s) return EXIT_FAILURE;
            store->setProperty("providers", QVariantList{alliance, nvidia});
            if (store->property("selectedProvider").value<QJSValue>().property(u"idpId"_s).toString() != u"alliance-fixture"_s) return EXIT_FAILURE;
            store->setProperty("providerDiscoveryDegraded", true);
            store->setProperty("authRestorePending", false);
            store->setProperty("authSession", QVariant());
            store->setProperty("authState", u"idle"_s);
            m_controller.navigate(u"sign-in"_s);
        }
        const auto persistenceIndex = m_arguments.indexOf(u"--smoke-auth-persistence"_s);
        if (persistenceIndex >= 0) {
            if (persistenceIndex + 1 >= m_arguments.size()) return EXIT_FAILURE;
            const auto persistence = m_arguments.at(persistenceIndex + 1);
            if (persistence != u"memory-only"_s && persistence != u"migration-pending"_s
                    && persistence != u"unavailable"_s) return EXIT_FAILURE;
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (!store) return EXIT_FAILURE;
            store->setProperty("authRestorePending", false);
            store->setProperty("authSession", QVariant());
            store->setProperty("sessionPersistence", persistence);
            m_controller.navigate(u"sign-in"_s);
            if (store->property("sessionPersistenceMessage").toString().isEmpty()) return EXIT_FAILURE;
        }
        const auto resumeIndex = m_arguments.indexOf(u"--smoke-session-resume"_s);
        if (resumeIndex >= 0) {
            if (resumeIndex + 1 >= m_arguments.size()) return EXIT_FAILURE;
            const auto mode = m_arguments.at(resumeIndex + 1);
            if (mode != u"conflict"_s && mode != u"unavailable"_s && mode != u"resuming"_s
                    && mode != u"finished"_s && mode != u"not-found"_s)
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
            } else if (mode == u"finished"_s || mode == u"not-found"_s) {
                store->setProperty("activeSession", QVariantMap{{u"sessionId"_s, u"terminal-fixture"_s},
                    {u"status"_s, 3}, {u"appId"_s, u"123"_s}});
                store->setProperty("streamer", QVariantMap{{u"status"_s, u"error"_s}});
                store->setProperty("sessionRecoveryPending", true);
                store->setProperty("recoverySessionId", u"terminal-fixture"_s);
                store->setProperty("streamerRecoveryExhausted", true);
                m_controller.navigate(u"stream"_s);
                const QVariant result = mode == u"finished"_s
                    ? QVariantMap{{u"session"_s, QVariantMap{{u"sessionId"_s, u"terminal-fixture"_s}, {u"status"_s, 7}}}}
                    : QVariantMap{{u"termination"_s, QVariantMap{{u"source"_s, u"cloudmatch-http"_s},
                        {u"httpStatus"_s, 404}, {u"sessionId"_s, u"terminal-fixture"_s}, {u"resumable"_s, false}}}};
                if (!QMetaObject::invokeMethod(store, "acceptRecoverySessions", Q_ARG(QVariant, result)))
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
                {u"protocolVersion"_s, 7}, {u"supportsMicrophone"_s, true}});
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
        const auto settingsPageIndex = m_arguments.indexOf(u"--smoke-settings-page"_s);
        if (settingsPageIndex >= 0) {
            if (settingsPageIndex + 1 >= m_arguments.size()) return EXIT_FAILURE;
            const QHash<QString, int> pages{{u"account"_s, 0}, {u"stream"_s, 3},
                {u"audio"_s, 4}, {u"controls"_s, 5}, {u"network"_s, 6},
                {u"appearance"_s, 8}, {u"console"_s, 9}, {u"shortcuts"_s, 10},
                {u"about"_s, 11}, {u"recording"_s, 12}};
            auto *settings = window ? window->findChild<QObject *>(u"desktopSettingsScreen"_s) : nullptr;
            const auto page = pages.constFind(m_arguments.at(settingsPageIndex + 1));
            if (!settings || page == pages.cend()) return EXIT_FAILURE;
            settings->setProperty("selectedSection", page.value());
            if (m_arguments.contains(u"--smoke-settings-advanced"_s))
                settings->setProperty("advancedOpen", true);
            if (page.value() == 0 && m_arguments.contains(u"--smoke-paper-design"_s)) {
                auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
                if (!store) return EXIT_FAILURE;
                store->setProperty("authSession", QVariantMap{{u"user"_s, QVariantMap{
                    {u"displayName"_s, u"Demo player"_s}, {u"email"_s, u"player@example.invalid"_s}}}});
                store->setProperty("subscription", QVariantMap{{u"membershipTier"_s, u"ULTIMATE"_s},
                    {u"remainingHours"_s, 42}, {u"entitledResolutions"_s, QVariantList{
                        QVariantMap{{u"width"_s, 3840}, {u"height"_s, 2160}, {u"fps"_s, 120}}}}});
            }
        }
        const auto settingsScaleIndex = m_arguments.indexOf(u"--smoke-settings-scale"_s);
        if (settingsScaleIndex >= 0) {
            if (settingsScaleIndex + 1 >= m_arguments.size()) return EXIT_FAILURE;
            bool ok = false;
            const auto scale = m_arguments.at(settingsScaleIndex + 1).toDouble(&ok);
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (!ok || scale < 0.85 || scale > 1.25 || !store) return EXIT_FAILURE;
            if (!QMetaObject::invokeMethod(store, "applySetting", Q_ARG(QVariant, QVariant(u"desktopUiScale"_s)),
                    Q_ARG(QVariant, QVariant(scale)))) return EXIT_FAILURE;
        }
        if (m_arguments.contains(u"--smoke-settings-details"_s)) {
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (!store) return EXIT_FAILURE;
            const QVariantMap values{{u"replayBufferEnabled"_s, true}, {u"sessionProxyEnabled"_s, true},
                {u"upscaling"_s, QGuiApplication::platformName() == u"cocoa"_s ? u"metalfx"_s : u"fsr1"_s},
                {u"desktopBackground"_s, u"custom"_s},
                {u"desktopBackgroundImage"_s, u"qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg"_s}};
            for (auto it = values.cbegin(); it != values.cend(); ++it)
                if (!QMetaObject::invokeMethod(store, "applySetting", Q_ARG(QVariant, QVariant(it.key())),
                        Q_ARG(QVariant, it.value()))) return EXIT_FAILURE;
            QTimer::singleShot(150, this, [window] {
                if (!window) return;
                if (auto *settings = window->findChild<QObject *>(u"desktopSettingsScreen"_s))
                    settings->setProperty("advancedOpen", true);
                if (auto *stream = window->findChild<QObject *>(u"desktopStreamSettings"_s))
                    stream->setProperty("statisticsOpen", true);
                if (auto *stats = window->findChild<QObject *>(u"desktopStatsSettings"_s))
                    stats->setProperty("metricsOpen", true);
                if (auto *about = window->findChild<QObject *>(u"desktopAboutSettings"_s))
                    about->setProperty("releaseNotesOpen", true);
            });
        }
        const auto settingsScrollIndex = m_arguments.indexOf(u"--smoke-settings-scroll"_s);
        if (settingsScrollIndex >= 0) {
            if (settingsScrollIndex + 1 >= m_arguments.size()) return EXIT_FAILURE;
            bool ok = false;
            const auto fraction = m_arguments.at(settingsScrollIndex + 1).toDouble(&ok);
            if (!ok || fraction < 0 || fraction > 1) return EXIT_FAILURE;
            QTimer::singleShot(700, this, [this, window, fraction] {
                auto *content = window ? window->findChild<QQuickItem *>(u"desktopSettingsContent"_s) : nullptr;
                if (!content) {
                    m_application.exit(EXIT_FAILURE);
                    return;
                }
                content->setProperty("contentY", qMax(0.0, content->property("contentHeight").toDouble()
                    - content->height()) * fraction);
            });
        }
        if (m_arguments.contains(u"--smoke-settings-full-page"_s)) {
            QTimer::singleShot(500, this, [this, window] {
                auto *content = window ? window->findChild<QQuickItem *>(u"desktopSettingsContent"_s) : nullptr;
                if (!window || !content) {
                    m_application.exit(EXIT_FAILURE);
                    return;
                }
                const auto height = qCeil(content->property("contentHeight").toDouble()
                    + window->height() - content->height());
                window->resize(window->width(), qBound(900, height, 3840));
            });
        }
        if (m_arguments.contains(u"--smoke-settings-layout"_s)) {
            QTimer::singleShot(400, this, [this, window] {
                int rows = 0;
                const auto verify = [&rows](auto &&self, QQuickItem *item) -> bool {
                    if (!item || !item->isVisible()) return true;
                    if (item->objectName() == u"desktopStreamSettings"_s) {
                        auto *codec = item->findChild<QQuickItem *>(u"codecSettingsRow"_s);
                        if (!codec || !codec->isVisible()) {
                            qCritical("Codec must be visible without opening Advanced");
                            return false;
                        }
                    }
                    if (item->objectName() == u"settingsButtonLabel"_s) {
                        auto *button = item->parentItem()->parentItem()->parentItem();
                        if (!button->property("menu").toBool() && button->width() >= button->implicitWidth()
                                && item->property("truncated").toBool()) {
                            qCritical("Settings button label truncated: %s", qPrintable(button->property("text").toString()));
                            return false;
                        }
                    }
                    if (item->objectName() == u"settingsRowLabels"_s) {
                        auto *row = item->parentItem();
                        auto *controls = row->findChild<QQuickItem *>(u"settingsRowControls"_s);
                        if (!controls || item->width() <= 0 || item->x() < 0
                                || item->x() + item->width() > row->width() + 1
                                || item->y() + item->height() > row->height() + 1
                                || controls->x() < 0 || controls->x() + controls->width() > row->width() + 1
                                || controls->y() + controls->height() > row->height() + 1
                                || (!row->property("stacked").toBool()
                                    && item->x() + item->width() > controls->x() + 1)) {
                            qCritical("Settings row layout overflow: %s", qPrintable(row->property("title").toString()));
                            return false;
                        }
                        ++rows;
                    }
                    for (auto *child : item->childItems())
                        if (!self(self, child)) return false;
                    return true;
                };
                if (!window || !verify(verify, window->contentItem()) || rows == 0)
                    m_application.exit(EXIT_FAILURE);
            });
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
                auto *settings = findControl(u"desktopSettingsScreen"_s);
                if (!settings || !settings->setProperty("advancedOpen", true)) return EXIT_FAILURE;
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
            if (auto *stats = findControl(u"desktopStatsSettings"_s)) {
                if (!stats->setProperty("metricsOpen", true)) return EXIT_FAILURE;
                auto *fps = findControl(u"renew-statsShowFps"_s);
                exercised = true;
                auto *region = findControl(u"renew-statsShowRegion"_s);
                if (!fps || !region || !QMetaObject::invokeMethod(fps,"valueChangedByUser",Q_ARG(bool,false))
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
