#include "acceptance/AcceptanceSession.h"
#include "app/AppController.h"

#include <QGuiApplication>
#include <QHash>
#include <QJSValue>
#include <QQmlApplicationEngine>
#include <QQmlComponent>
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
        if (m_arguments.contains(u"--smoke-theme-settings"_s)
                || m_arguments.contains(u"--smoke-renew-settings-actions"_s)
                || m_arguments.contains(u"--smoke-upscaling"_s)
                || m_arguments.contains(u"--smoke-background-stream"_s)
                || m_arguments.contains(u"--smoke-controller-metadata"_s)
                || m_arguments.contains(u"--smoke-console-design"_s)
                || m_arguments.contains(u"--smoke-custom-background"_s)
                || m_arguments.contains(u"--smoke-frame-generation"_s)) {
            QQmlComponent component(&m_engine, QUrl(u"qrc:/acceptance/PendingSettingsClient.qml"_s));
            auto *client = component.create();
            if (!client) {
                qCritical() << component.errors();
                return EXIT_FAILURE;
            }
            client->setParent(&m_engine);
        }
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
        if (m_arguments.contains(u"--smoke-console-design"_s)) {
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (!store) return EXIT_FAILURE;
            QVariantList games;
            QStringList favorites;
            const QStringList storefronts{u"Steam"_s, u"Xbox"_s, u"Epic Games Store"_s, u"GOG"_s};
            for (int index = 0; index < 18; ++index) {
                const auto id = QString::number(1000 + index);
                const auto platform = storefronts.at(index % storefronts.size());
                QVariantMap game{{u"id"_s, id}, {u"title"_s, u"Sample game %1"_s.arg(index + 1)},
                    {u"launchAppId"_s, id}, {u"selectedVariantIndex"_s, 0},
                    {u"isAvailable"_s, true}, {u"isInLibrary"_s, true},
                    {u"publisherName"_s, u"Acceptance fixture"_s},
                    {u"genres"_s, QStringList{index % 2 ? u"Adventure"_s : u"Action"_s}},
                    {u"imageUrl"_s, u"qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg"_s},
                    {u"heroImageUrl"_s, u"qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg"_s},
                    {u"availableStores"_s, QStringList{platform}},
                    {u"variants"_s, QVariantList{QVariantMap{{u"id"_s, id}, {u"store"_s, platform},
                        {u"inLibrary"_s, true}, {u"libraryStatus"_s, u"PLATFORM_SYNC"_s}}}}};
                games.append(game);
                if (index < 14) favorites.append(id);
            }
            store->setProperty("settings", QVariantMap{{u"appTheme"_s, u"dark"_s}, {u"themePack"_s, u"nocturne"_s},
                {u"favoriteGameIds"_s, favorites}, {u"homeTileSizes"_s, QVariantMap{{u"1000"_s, u"wide"_s}}},
                {u"resolution"_s, u"2560x1440"_s}, {u"fps"_s, 120}, {u"codec"_s, u"auto"_s},
                {u"colorQuality"_s, u"8bit_420"_s}, {u"maxBitrateMbps"_s, 75}});
            store->setProperty("catalogGames", games);
            store->setProperty("catalogState", u"ready"_s);
            store->setProperty("catalogComplete", true);
            store->setProperty("catalogTotalCount", games.size());
            store->setProperty("storeGames", games);
            store->setProperty("storeState", u"ready"_s);
            store->setProperty("storeTotalCount", games.size());
            store->setProperty("storeHasMore", false);
            store->setProperty("selectedGame", games.first());
            if (m_controller.route() == u"inserting"_s || m_controller.route() == u"stream"_s) {
                store->setProperty("streamerStartRequestId", u"console-design-fixture"_s);
                store->setProperty("streamInputPauseRequestId", u"console-design-fixture"_s);
                store->setProperty("activeSession", QVariantMap{{u"sessionId"_s, u"console-design-fixture"_s},
                    {u"phase"_s, u"queued"_s}, {u"queuePosition"_s, 37}, {u"seatSetupStep"_s, 1}});
                const bool stream = m_controller.route() == u"stream"_s;
                store->setProperty("streamState", stream ? u"streaming"_s : u"queued"_s);
                if (stream) store->setProperty("streamer", QVariantMap{
                    {u"status"_s, u"streaming"_s}, {u"firstFrameLatencyMs"_s, 37}});
            }
        }
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
            if (persistence != u"local-file"_s && persistence != u"memory-only"_s && persistence != u"migration-pending"_s
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
                {u"protocolVersion"_s, 8}, {u"supportsMicrophone"_s, true}});
            store->setProperty("settings", QVariantMap{
                {u"microphoneMode"_s, muted ? u"voice-activity"_s : u"disabled"_s}});
            if (muted) {
                store->setProperty("sessionMicrophoneMode", muted ? u"disabled"_s : u"voice-activity"_s);
                store->setProperty("selectedGame", QVariantMap{{u"title"_s, u"Microphone acceptance fixture"_s}});
                store->setProperty("activeSession", QVariantMap{
                    {u"sessionId"_s, u"microphone-visual-fixture"_s}, {u"phase"_s, u"ready"_s}});
                store->setProperty("streamer", QVariantMap{
                    {u"status"_s, u"streaming"_s}, {u"firstFrameLatencyMs"_s, 37}, {u"microphoneState"_s, u"muted"_s},
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
                    {u"enableHdr"_s, false}, {u"webrtcCompatibilityMode"_s, u"auto"_s},
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
                        {u"supportsLinking"_s, i >= 3},
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
        const auto accountsIndex = m_arguments.indexOf(u"--smoke-accounts"_s);
        if (accountsIndex >= 0) {
            // Placeholder "Who's playing?" accounts for visual acceptance only;
            // the core is not asked to save, switch or remove anything.
            bool countOk = false;
            const int count = accountsIndex + 1 < m_arguments.size()
                ? m_arguments.at(accountsIndex + 1).toInt(&countOk) : 0;
            if (!countOk || count < 1 || count > 12) return EXIT_FAILURE;
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (!store) return EXIT_FAILURE;
            struct Fixture { const char16_t *name; const char16_t *tier; const char16_t *provider; bool alliance; bool pin; };
            static constexpr Fixture fixtures[] = {
                {u"Zortos", u"ULTIMATE", u"NVIDIA", false, false},
                {u"Mila", u"PERFORMANCE", u"NVIDIA", false, false},
                {u"jun.kr", u"ULTIMATE", u"LG U+", true, false},
                {u"Noah", u"FREE", u"NVIDIA", false, true},
                {u"Theodora Van Der Berghe-Castellanos", u"PERFORMANCE", u"bro.game", true, false},
                {u"ming.tw", u"ULTIMATE", u"NVIDIA", false, false},
                {u"sam", u"PERFORMANCE", u"NVIDIA", false, false},
                {u"QA test", u"FREE", u"NVIDIA", false, false},
                {u"Robin", u"FREE", u"NVIDIA", false, false},
                {u"Kai", u"ULTIMATE", u"NVIDIA", false, false},
                {u"Ari", u"PERFORMANCE", u"NVIDIA", false, false},
                {u"Lou", u"FREE", u"NVIDIA", false, false}};
            const auto stateIndex = m_arguments.indexOf(u"--smoke-account-state"_s);
            const auto state = stateIndex >= 0 && stateIndex + 1 < m_arguments.size()
                ? m_arguments.at(stateIndex + 1) : QString();
            QVariantList accounts;
            for (int index = 0; index < count; ++index) {
                const auto &fixture = fixtures[index];
                const auto name = QString::fromUtf16(fixture.name);
                accounts.append(QVariantMap{
                    {u"userId"_s, u"fixture-user-%1"_s.arg(index + 1)}, {u"displayName"_s, name},
                    {u"email"_s, u"%1@example.invalid"_s.arg(name.section(u' ', 0, 0).toLower())},
                    {u"membershipTier"_s, QString::fromUtf16(fixture.tier)},
                    {u"providerName"_s, QString::fromUtf16(fixture.provider)},
                    {u"alliance"_s, fixture.alliance}, {u"hasPin"_s, fixture.pin},
                    {u"needsSignIn"_s, state == u"expired"_s && index == 1}});
            }
            const auto active = accounts.first().toMap();
            store->setProperty("savedAccounts", accounts);
            store->setProperty("accountsLoaded", true);
            store->setProperty("launchAccountPickerConsidered", true);
            store->setProperty("authRestorePending", false);
            store->setProperty("authState", u"signed-in"_s);
            store->setProperty("authSession", QVariantMap{
                {u"user"_s, QVariantMap{{u"userId"_s, active.value(u"userId"_s)},
                    {u"displayName"_s, active.value(u"displayName"_s)}, {u"email"_s, active.value(u"email"_s)},
                    {u"membershipTier"_s, active.value(u"membershipTier"_s)}}},
                {u"provider"_s, QVariantMap{{u"idpId"_s, u"nvidia-fixture"_s}, {u"displayName"_s, u"NVIDIA"_s}}}});
            store->setProperty("sessionPersistence", u"os-credential-store"_s);
            const auto target = accounts.size() > 1 ? accounts.at(1).toMap() : active;
            if (state == u"switching"_s) {
                store->setProperty("accountSwitchRequestId", u"smoke-account-switch"_s);
                store->setProperty("accountSwitchTargetUserId", target.value(u"userId"_s));
            }
            if (state == u"switcher"_s) {
                auto *switcher = window ? window->findChild<QObject *>(u"desktopAccountSwitcher"_s) : nullptr;
                if (!switcher || !switcher->parent()) return EXIT_FAILURE;
                switcher->parent()->setProperty("accountSwitcherOpen", true);
                switcher->setProperty("currentIndex", 1);
            }
            if (state == u"switch-confirm"_s) {
                store->setProperty("activeSession", QVariantMap{{u"sessionId"_s, u"smoke-account-session"_s},
                    {u"ownerScope"_s, QVariantMap{{u"userId"_s, active.value(u"userId"_s)}}}});
                store->setProperty("selectedGame", QVariantMap{{u"title"_s, u"Cyberpunk 2077"_s}});
                store->setProperty("accountSwitchConfirmTarget", target);
                m_controller.showOverlay(u"account-switch-confirm"_s);
            }
            if (auto *picker = window ? window->findChild<QObject *>(u"desktopAccountPicker"_s) : nullptr) {
                if (state == u"expired"_s || state == u"switching"_s || state == u"remove"_s)
                    picker->setProperty("currentIndex", 1);
                if (state == u"manage"_s || state == u"remove"_s || state == u"remove-error"_s)
                    picker->setProperty("manageMode", true);
                if (state == u"switch-error"_s || state == u"remove-error"_s)
                    store->setProperty("accountMessage", u"Could not switch to this saved sign-in."_s);
                if (state == u"remove"_s) {
                    picker->setProperty("confirmTarget", target);
                    picker->setProperty("confirmKind", u"remove"_s);
                }
                if (state == u"pin"_s && accounts.size() > 3) {
                    picker->setProperty("currentIndex", 3);
                    picker->setProperty("pinAccount", accounts.at(3));
                    picker->setProperty("pinEntry", u"12"_s);
                }
            } else if (m_controller.route() == u"accounts"_s) {
                return EXIT_FAILURE;
            }
        }
        if (m_arguments.contains(u"--smoke-resolution-open"_s)
                || m_arguments.contains(u"--smoke-resolution-fits-monitor"_s)) {
            auto *picker = window ? window->findChild<QObject *>(u"renewResolutionPicker"_s) : nullptr;
            if (!picker) return EXIT_FAILURE;
            if (m_arguments.contains(u"--smoke-resolution-open"_s)) picker->setProperty("expanded", true);
            if (m_arguments.contains(u"--smoke-resolution-fits-monitor"_s)) picker->setProperty("fitsMonitor", true);
        }
        const auto streamEndIndex = m_arguments.indexOf(u"--smoke-stream-end-notice"_s);
        if (streamEndIndex >= 0) {
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (!store || streamEndIndex + 1 >= m_arguments.size()) return EXIT_FAILURE;
            const auto codeIndex = m_arguments.indexOf(u"--smoke-stream-end-code"_s);
            const auto codeText = codeIndex >= 0 && codeIndex + 1 < m_arguments.size()
                ? m_arguments.at(codeIndex + 1) : QString();
            bool parsed = false;
            const auto code = codeText.toUInt(&parsed, 0);
            store->setProperty("selectedGame", m_engine.rootContext()->contextProperty(u"SmokeTestGame"_s));
            store->setProperty("streamEndNotice", QVariantMap{{u"kind"_s, m_arguments.at(streamEndIndex + 1)},
                {u"code"_s, parsed ? QVariant(code) : QVariant()}, {u"codeText"_s, codeText}});
        }
        if (m_arguments.contains(u"--smoke-light-theme"_s)) {
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (store) store->setProperty("settings", QVariantMap{{u"appTheme"_s, u"light"_s}});
        }
        const auto themePackIndex = m_arguments.indexOf(u"--smoke-theme-pack"_s);
        if (themePackIndex >= 0) {
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (!store || themePackIndex + 1 >= m_arguments.size()) return EXIT_FAILURE;
            store->setProperty("settings", QVariantMap{{u"themePack"_s, m_arguments.at(themePackIndex + 1)},
                {u"appTheme"_s, m_arguments.contains(u"--smoke-light-theme"_s) ? u"light"_s : u"dark"_s}});
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
            if ((page.value() == 0 || page.value() == 3) && m_arguments.contains(u"--smoke-paper-design"_s)) {
                auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
                if (!store) return EXIT_FAILURE;
                store->setProperty("authSession", QVariantMap{{u"user"_s, QVariantMap{
                    {u"displayName"_s, u"Demo player"_s}, {u"email"_s, u"player@example.invalid"_s}}}});
                store->setProperty("subscription", QVariantMap{{u"membershipTier"_s, u"ULTIMATE"_s},
                    {u"remainingHours"_s, 42}, {u"entitledResolutions"_s, QVariantList{
                        QVariantMap{{u"width"_s, 3840}, {u"height"_s, 2160}, {u"fps"_s, 120}}}},
                    {u"dlssOverrides"_s, QVariantMap{
                        {u"superResolution"_s, QVariantMap{{u"entitled"_s, true}, {u"upsell"_s, false}}},
                        {u"frameGeneration"_s, QVariantMap{{u"entitled"_s, true}, {u"upsell"_s, false}}},
                        {u"rayReconstruction"_s, QVariantMap{{u"entitled"_s, false}, {u"upsell"_s, true}}},
                        {u"frameGenerationMultipliers"_s, QVariantList{3, 4}}}}});
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
            if (auto *updates = findControl(u"aboutOpenUpdates"_s)) {
                exercised = true;
                auto *settings = findControl(u"desktopSettingsScreen"_s);
                if (!settings || !QMetaObject::invokeMethod(updates, "clicked")
                    || m_controller.route() != u"settings-updates"_s
                    || settings->property("selectedSection") != settings->property("updatesSection")) return EXIT_FAILURE;
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
        if (coreProgram(m_arguments).isEmpty() && window
                && !window->property("settingsLoadedForSmokeTest").toBool()) {
            auto *store = m_engine.singletonInstance<QObject *>(u"OpenNOW"_s, u"ShellStore"_s);
            if (!store) return EXIT_FAILURE;
            store->setProperty("settings", QVariantMap{{u"uiSoundsEnabled"_s, false}});
        }
    }
    return EXIT_SUCCESS;
}
