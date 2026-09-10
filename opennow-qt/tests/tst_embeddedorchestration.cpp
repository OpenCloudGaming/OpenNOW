#include <QFile>
#include <QJSEngine>
#include <QRegularExpression>
#include <QTest>

namespace {
QString source(const QString &relativePath)
{
    QFile file(QStringLiteral(OPENNOW_QT_SOURCE_DIR) + u'/' + relativePath);
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) return {};
    return QString::fromUtf8(file.readAll());
}
}

class EmbeddedOrchestrationTest final : public QObject
{
    Q_OBJECT

private slots:
    void premiumGamesRequirePaidMembershipBeforeLaunch_data()
    {
        QTest::addColumn<QString>("requiredTier");
        QTest::addColumn<QString>("subscriptionJson");
        QTest::addColumn<bool>("allowed");
        QTest::addColumn<bool>("refreshMembership");
        QTest::newRow("free-premium") << QStringLiteral("Premium") << QStringLiteral(R"({"membershipTier":"FREE"})") << false << false;
        QTest::newRow("free-tier-premium") << QStringLiteral("Performance") << QStringLiteral(R"({"membershipTier":" Free Tier "})") << false << false;
        QTest::newRow("free-hyphen-premium") << QStringLiteral("Ultimate") << QStringLiteral(R"({"membershipTier":"free-tier"})") << false << false;
        QTest::newRow("paid-premium") << QStringLiteral("Premium") << QStringLiteral(R"({"membershipTier":"PERFORMANCE"})") << true << false;
        QTest::newRow("alliance-paid-premium") << QStringLiteral("Premium") << QStringLiteral(R"({"membershipTier":"Priority"})") << true << false;
        QTest::newRow("unrestricted") << QStringLiteral("") << QStringLiteral(R"({"membershipTier":"FREE"})") << true << false;
        QTest::newRow("blank-requirement") << QStringLiteral("  ") << QStringLiteral("null") << true << false;
        QTest::newRow("free-requirement") << QStringLiteral(" FREE ") << QStringLiteral(R"({"membershipTier":"FREE"})") << true << false;
        QTest::newRow("free-tier-requirement") << QStringLiteral("Free Tier") << QStringLiteral("null") << true << false;
        QTest::newRow("free-hyphen-requirement") << QStringLiteral("free-tier") << QStringLiteral("null") << true << false;
        QTest::newRow("subscription-loading") << QStringLiteral("Premium") << QStringLiteral("null") << false << true;
        QTest::newRow("subscription-missing-tier") << QStringLiteral("Premium") << QStringLiteral("{}") << false << true;
        QTest::newRow("subscription-blank-tier") << QStringLiteral("Premium") << QStringLiteral(R"({"membershipTier":" "})") << false << true;
    }

    void premiumGamesRequirePaidMembershipBeforeLaunch()
    {
        QFETCH(QString, requiredTier);
        QFETCH(QString, subscriptionJson);
        QFETCH(bool, allowed);
        QFETCH(bool, refreshMembership);
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        QJSEngine engine;
        engine.installExtensions(QJSEngine::TranslationExtension);
        for (const auto &name : {"selectedLaunchAppId", "selectedGameMembershipError", "launchSelectedGame"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(shell);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        engine.globalObject().setProperty(QStringLiteral("requiredTier"), requiredTier);
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var signedIn = true, ready = true, streamBusy = false, onboardingReplaying = false;
            var selectedGame = {launchAppId: "123", title: "Test", membershipTierLabel: requiredTier};
            var authSession = {user: {membershipTier: "FREE"}};
            var subscriptionRequestId = "", streamState = "idle", streamMessage = "", lastError = "";
            var settings = {}, regions = [], pendingLaunchParams = null;
            var requests = [], routes = [];
            var CoreClient = {request: function(method, params) {
                requests.push({method: method, params: params}); return "request-" + requests.length;
            }};
            var AppController = {navigate: function(route) { routes.push(route); }};
        )JS")).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("var subscription = ") + subscriptionJson).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("launchSelectedGame(false)")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("streamState")).toString(),
                 allowed ? QStringLiteral("checking") : QStringLiteral("error"));
        QCOMPARE(engine.evaluate(QStringLiteral("pendingLaunchParams !== null")).toBool(), allowed);
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), allowed || refreshMembership ? 1 : 0);
        if (allowed || refreshMembership) {
            QCOMPARE(engine.evaluate(QStringLiteral("requests[0].method")).toString(),
                     allowed ? QStringLiteral("session.remote.list") : QStringLiteral("account.subscription.get"));
        }
        QCOMPARE(engine.evaluate(QStringLiteral("routes[0]")).toString(), QStringLiteral("inserting"));
        if (!allowed) {
            QVERIFY(!engine.evaluate(QStringLiteral("streamMessage")).toString().isEmpty());
            QCOMPARE(engine.evaluate(QStringLiteral("lastError")).toString(),
                     engine.evaluate(QStringLiteral("streamMessage")).toString());
            QVERIFY(!engine.evaluate(QStringLiteral("launchSelectedGame(true)")).isError());
            QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), refreshMembership ? 1 : 0);
            QVERIFY(!engine.evaluate(QStringLiteral(
                "subscription = {membershipTier: 'ULTIMATE'}; launchSelectedGame(true)")).isError());
            QCOMPARE(engine.evaluate(QStringLiteral("streamState")).toString(), QStringLiteral("checking"));
            QCOMPARE(engine.evaluate(QStringLiteral("requests[requests.length - 1].method")).toString(),
                     QStringLiteral("session.remote.list"));
        }
    }

    void clipboardPasteUsesTheSharedOptInOnBothSurfaces()
    {
        const auto controls = source(QStringLiteral(
            "qml/desktop/settings/pages/DesktopSettingsControlsPage.qml"));
        QVERIFY(controls.contains(QStringLiteral("boolSetting(\"clipboardPaste\", false)")));
        QVERIFY(controls.contains(QStringLiteral("setSetting(\"clipboardPaste\", value)")));
        const auto console = source(QStringLiteral("qml/screens/SettingsScreen.qml"));
        QVERIFY(console.contains(QStringLiteral("qsTr(\"Clipboard paste\")")));
        QVERIFY(console.contains(QStringLiteral("\"clipboardPaste\"")));
        for (const auto &path : {QStringLiteral("qml/screens/StreamScreen.qml"),
                                QStringLiteral("qml/desktop/stream/DesktopStreamScreen.qml")}) {
            const auto stream = source(path);
            QVERIFY(stream.contains(QStringLiteral(
                "clipboardPaste: ShellStore.settings.clipboardPaste === true")));
            QVERIFY(stream.contains(QStringLiteral(
                "onClipboardPasteFailed: clipboardPasteNotice.restart()")));
        }
    }

    void replayClippingRequiresAnEnabledSessionAndResetsPendingWork()
    {
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        QJSEngine engine;
        engine.installExtensions(QJSEngine::TranslationExtension);
        for (const auto &name : {"saveStreamClip", "disableStreamReplay", "resetStreamReplay"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(shell);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var streamClipBusy = false, streamReplayEnabled = false, replayBufferRequested = false;
            var activeSession = {sessionId: "test"}, streamer = {status: "streaming"};
            var selectedGame = {title: "Test game"};
            var mediaClipTargetRequestId = "", streamClipRequestId = "";
            var mediaMessage = "", accessibilityMessage = "", lastError = "send failed";
            var requests = [], commands = [];
            var NativeStreamRuntime = {running: true};
            var CoreClient = {request: function(method, params) {
                requests.push({method: method, params: params}); return "target-1";
            }};
            function sendNativeCommand(type) { commands.push(type); return "native-1"; }
            function streamCaptureAnnounced(message) {}
        )JS")).isError());
        engine.evaluate(QStringLiteral("saveStreamClip()"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 0);
        QVERIFY(engine.evaluate(QStringLiteral("accessibilityMessage.length > 0")).toBool());
        engine.evaluate(QStringLiteral("replayBufferRequested = true; saveStreamClip()"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 0);
        engine.evaluate(QStringLiteral("streamReplayEnabled = true; saveStreamClip()"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 1);
        QCOMPARE(engine.evaluate(QStringLiteral("requests[0].method")).toString(),
                 QStringLiteral("media.recording.target"));
        engine.evaluate(QStringLiteral("streamClipBusy = true; saveStreamClip()"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 1);
        engine.evaluate(QStringLiteral("disableStreamReplay()"));
        QCOMPARE(engine.evaluate(QStringLiteral("commands[0]")).toString(), QStringLiteral("replay-stop"));
        QVERIFY(!engine.evaluate(QStringLiteral("streamReplayEnabled")).toBool());
        QCOMPARE(engine.evaluate(QStringLiteral("mediaClipTargetRequestId")).toString(), QString());
        engine.evaluate(QStringLiteral("disableStreamReplay()"));
        QCOMPARE(engine.evaluate(QStringLiteral("commands.length")).toInt(), 1);
        engine.evaluate(QStringLiteral("streamClipRequestId = 'stale'; resetStreamReplay()"));
        QCOMPARE(engine.evaluate(QStringLiteral("streamClipRequestId")).toString(), QString());
    }

    void lastPlayedUsesElapsedUnits_data()
    {
        QTest::addColumn<QString>("raw");
        QTest::addColumn<qint64>("elapsedMs");
        QTest::addColumn<QString>("expected");
        const auto timestamp = QStringLiteral("2026-09-07T18:49:52.000Z");
        QTest::newRow("now") << timestamp << qint64(0) << QStringLiteral("Just now");
        QTest::newRow("future") << timestamp << qint64(-60000) << QStringLiteral("Just now");
        QTest::newRow("second") << timestamp << qint64(1000) << QStringLiteral("1 second ago");
        QTest::newRow("seconds") << timestamp << qint64(59999) << QStringLiteral("59 seconds ago");
        QTest::newRow("minute") << timestamp << qint64(60000) << QStringLiteral("1 minute ago");
        QTest::newRow("minutes") << timestamp << qint64(3599999) << QStringLiteral("59 minutes ago");
        QTest::newRow("hour") << timestamp << qint64(3600000) << QStringLiteral("1 hour ago");
        QTest::newRow("hours") << timestamp << qint64(86399999) << QStringLiteral("23 hours ago");
        QTest::newRow("day") << timestamp << qint64(86400000) << QStringLiteral("1 day ago");
        QTest::newRow("days") << timestamp << qint64(30LL * 86400000) << QStringLiteral("30 days ago");
        QTest::newRow("offset") << QStringLiteral("2026-09-07T20:49:52.000+02:00")
                                << qint64(7200000) << QStringLiteral("2 hours ago");
        QTest::newRow("missing") << QString() << qint64(0) << QString();
        QTest::newRow("invalid") << QStringLiteral("not a timestamp") << qint64(0) << QString();
    }

    void lastPlayedUsesElapsedUnits()
    {
        QFETCH(QString, raw);
        QFETCH(qint64, elapsedMs);
        QFETCH(QString, expected);
        const auto tokens = source(QStringLiteral("qml/desktop/components/DesktopTokens.qml"));
        const auto match = QRegularExpression(QStringLiteral(
            "    function relativeLastPlayed\\([^\\n]*\\) \\{.*?\\n    \\}"),
            QRegularExpression::DotMatchesEverythingOption).match(tokens);
        QVERIFY(match.hasMatch());
        QJSEngine engine;
        engine.installExtensions(QJSEngine::TranslationExtension);
        auto formatter = engine.evaluate(u'(' + match.captured() + u')');
        QVERIFY2(formatter.isCallable(), qPrintable(formatter.toString()));
        const auto now = engine.evaluate(QStringLiteral("Date.parse('2026-09-07T18:49:52.000Z')")).toNumber();
        const auto result = formatter.call({raw, now + elapsedMs});
        QVERIFY2(!result.isError(), qPrintable(result.toString()));
        QCOMPARE(result.toString(), expected);
    }

    void continuePlayingFormatsMetadata()
    {
        QJSEngine engine;
        engine.installExtensions(QJSEngine::TranslationExtension);
        for (const auto &entry : {
                 qMakePair(QStringLiteral("qml/desktop/components/DesktopTokens.qml"), QStringLiteral("relativeLastPlayed")),
                 qMakePair(QStringLiteral("qml/desktop/home/DesktopHomeScreen.qml"), QStringLiteral("heroMeta"))}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(entry.second),
                QRegularExpression::DotMatchesEverythingOption).match(source(entry.first));
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var DesktopTokens = {relativeLastPlayed: relativeLastPlayed};
            var root = {heroGame: {lastPlayed: '2026-09-07T18:49:52.000', hoursPlayed: 14},
                        lastPlayedNowMs: Date.parse('2026-09-07T20:49:52.000')};
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("heroMeta()")).toString(), QStringLiteral("2 hours ago · 14 h played"));
        QCOMPARE(engine.evaluate(QStringLiteral("root.heroGame.hoursPlayed = 0; heroMeta()")).toString(), QStringLiteral("2 hours ago"));
        QCOMPARE(engine.evaluate(QStringLiteral("root.lastPlayedNowMs += 3600000; heroMeta()")).toString(), QStringLiteral("3 hours ago"));
        QCOMPARE(engine.evaluate(QStringLiteral("root.heroGame.lastPlayed = 'invalid'; heroMeta()")).toString(), QStringLiteral("Ready to stream from your library"));
        QCOMPARE(engine.evaluate(QStringLiteral("root.heroGame.hoursPlayed = 14; heroMeta()")).toString(), QStringLiteral("14 h played"));
        QCOMPARE(engine.evaluate(QStringLiteral("root.heroGame = null; heroMeta()")).toString(), QStringLiteral("Sign in and sync your library to continue a game."));
    }

    void resolutionFitsMonitorUsesPhysicalBounds_data()
    {
        QTest::addColumn<int>("screenWidth");
        QTest::addColumn<int>("screenHeight");
        QTest::addColumn<double>("devicePixelRatio");
        QTest::addColumn<bool>("fitsMonitor");
        QTest::addColumn<QStringList>("included");
        QTest::addColumn<QStringList>("excluded");

        QTest::newRow("mac-retina") << 1512 << 982 << 2.0 << true
            << QStringList{QStringLiteral("2560x1600"), QStringLiteral("2560x1440"), QStringLiteral("1920x1200")}
            << QStringList{QStringLiteral("3200x1800"), QStringLiteral("3840x2160"), QStringLiteral("3840x2400")};
        QTest::newRow("normal-16-9") << 1920 << 1080 << 1.0 << true
            << QStringList{QStringLiteral("1280x720"), QStringLiteral("1600x900"), QStringLiteral("1920x1080")}
            << QStringList{QStringLiteral("1920x1200"), QStringLiteral("2560x1440"), QStringLiteral("2560x1080")};
        QTest::newRow("too-tall-and-wide") << 1440 << 900 << 1.0 << true
            << QStringList{QStringLiteral("1280x720"), QStringLiteral("1280x800"), QStringLiteral("1440x900")}
            << QStringList{QStringLiteral("1600x900"), QStringLiteral("1920x1080"), QStringLiteral("1920x1200")};
        QTest::newRow("all-mode") << 1 << 1 << 1.0 << false
            << QStringList{QStringLiteral("7680x4320"), QStringLiteral("3840x2400"), QStringLiteral("5120x1440")}
            << QStringList{};
    }

    void resolutionFitsMonitorUsesPhysicalBounds()
    {
        QFETCH(int, screenWidth);
        QFETCH(int, screenHeight);
        QFETCH(double, devicePixelRatio);
        QFETCH(bool, fitsMonitor);
        QFETCH(QStringList, included);
        QFETCH(QStringList, excluded);

        const auto settings = source(QStringLiteral("qml/state/settings/SettingsState.qml"));
        const auto picker = source(QStringLiteral("qml/desktop/settings/controls/DesktopSettingsResolution.qml"));
        const auto itemsMatch = QRegularExpression(QStringLiteral(
            "    function resolutionItems\\([^\\n]*\\) \\{.*?\\n    \\}"),
            QRegularExpression::DotMatchesEverythingOption).match(settings);
        const auto groupsMatch = QRegularExpression(QStringLiteral(
            "    readonly property var groups: \\{(?<body>.*?)\\n    \\}\\n    readonly property real revealProgress"),
            QRegularExpression::DotMatchesEverythingOption).match(picker);
        QVERIFY(itemsMatch.hasMatch());
        QVERIFY(groupsMatch.hasMatch());

        QJSEngine engine;
        engine.installExtensions(QJSEngine::TranslationExtension);
        auto evaluate = [&engine](const QString &script) {
            const auto result = engine.evaluate(script);
            if (result.isError())
                qWarning().noquote() << result.toString() << result.property("stack").toString();
            return result;
        };
        QVERIFY(!evaluate(QStringLiteral(R"JS(
            function qsTr(text) { return text; }
            var root = {subscription: null, entitledFpsForResolution: function() { return [60]; }};
        )JS")).isError());
        QVERIFY(!evaluate(itemsMatch.captured()).isError());
        QVERIFY(!evaluate(QStringLiteral(
            "var Screen = {width:%1, height:%2, devicePixelRatio:%3};"
            "var fitsMonitor = %4;"
            "var items = resolutionItems();")
                .arg(screenWidth)
                .arg(screenHeight)
                .arg(devicePixelRatio, 0, 'f', 3)
                .arg(fitsMonitor ? QStringLiteral("true") : QStringLiteral("false"))).isError());

        const auto result = evaluate(QStringLiteral(R"JS(
            var values = [];
            var groups = (function() {%1
            })();
            for (var i = 0; i < groups.length; i++)
                for (var j = 0; j < groups[i].items.length; j++)
                    values.push(groups[i].items[j].value);
            JSON.stringify(values);
        )JS").arg(groupsMatch.captured(QStringLiteral("body"))));
        QVERIFY2(!result.isError(), qPrintable(result.toString()));
        const auto values = result.toString();
        for (const auto &value : included)
            QVERIFY2(values.contains(QStringLiteral("\"%1\"").arg(value)), qPrintable(values));
        for (const auto &value : excluded)
            QVERIFY2(!values.contains(QStringLiteral("\"%1\"").arg(value)), qPrintable(values));
    }

    void pendingRecoveryDoesNotHideAStartedVideoSurface()
    {
        for (const auto &path : {"qml/screens/StreamScreen.qml", "qml/desktop/stream/DesktopStreamScreen.qml"}) {
            const QRegularExpression status(QStringLiteral(
                "readonly property string status: \\{(.*?)\\n    \\}"),
                QRegularExpression::DotMatchesEverythingOption);
            const auto match = status.match(source(QString::fromLatin1(path)));
            QVERIFY(match.hasMatch());
            QJSEngine engine;
            engine.evaluate(QStringLiteral(
                "var streamer = {status:'streaming'}; var root = {streamer:streamer};"
                "var ShellStore = {streamState:'streaming', streamerRestartAttempts:2};"));
            const auto result = engine.evaluate(
                QStringLiteral("(function() {%1})()").arg(match.captured(1)));
            QVERIFY2(!result.isError(), qPrintable(result.toString()));
            QCOMPARE(result.toString(), QStringLiteral("streaming"));
        }
    }

    void mediaRecoveryIsBoundedUntilVideoActuallyStarts()
    {
        // Execute the actual ShellStore functions with side effects stubbed,
        // rather than just checking source text for a retry-limit constant.
        QJSEngine engine;
        auto evaluate = [&](const QString &script) {
            const auto result = engine.evaluate(script);
            if (result.isError())
                qWarning().noquote() << result.toString() << result.property("stack").toString();
            return result;
        };
        QVERIFY(!evaluate(QStringLiteral(R"JS(
            var ready = true, activeSession = {sessionId: 'seat', phase: 'ready'};
            var streamer = {status: 'starting', sessionId: 'seat'};
            var runtimeStreamProfile = {}, streamMessage = '', streamState = '', lastError = '';
            var streamerRestartAttempts = 0, sessionReconnectAttempts = 0;
            var streamerRecoveryExhausted = false, streamerRestartRecoveryCount = 0, sessionRecoveryCount = 0;
            var streamerStopExpected = false, streamInputStateKnown = false, streamRecordingActive = false;
            var streamStartedAtMs = 1, desiredStreamInputPaused = false, sessionClaimRequestId = '';
            var sessionClaimIsRecovery = false, streamerStartRequestId = '', streamerPrepareRequestId = '';
            var sessionRecoveryPending = false, recoveryDiscoveryRequestId = '', recoverySessionId = '';
            var streamerStopRequestId = '', streamStopRequestId = '', streamPollRequestId = '';
            var NativeStreamRuntime = {running: false}, nativeRuntimeCapabilities = {};
            var nativeRuntimeReady = true, prepares = 0, claims = 0, discoveries = 0;
            var streamerRestartTimer = {running: false, restarts: 0,
                restart: function() { this.running = true; this.restarts++; },
                stop: function() { this.running = false; }};
            var streamPollTimer = {stop: function() {}, restart: function() {}};
            var CoreClient = {request: function(type) {
                if (type === 'streamer.prepare') { prepares++; return 'prepare'; }
                if (type === 'session.claim') { claims++; return 'claim'; }
                if (type === 'session.remote.list') { discoveries++; return 'discovery'; }
                throw new Error('Unexpected request: ' + type);
            }};
            var AppController = {route: 'stream', overlay: ''};
            function qsTr(text) { return text; }
            function inspectStreamerOverlayRequest() {}
            function inspectStreamerScreenshotRequest() {}
            function inspectStreamerRecordingRequest() {}
            function inspectStreamerShortcutAction() {}
            function setStreamInputPaused() {}
            function syncDiscordPresence() {}
            function sendNativeCommand() {}
            function updateStreamerFields(fields) { acceptStreamerSnapshot(Object.assign({}, streamer, fields)); }
        )JS")).isError());
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        const auto limit = QRegularExpression(QStringLiteral(
            "readonly property int maximumSessionReconnectAttempts: (\\d+)")).match(shell);
        QVERIFY(limit.hasMatch());
        QVERIFY(!evaluate(QStringLiteral("var maximumSessionReconnectAttempts = %1;")
                              .arg(limit.captured(1))).isError());
        for (const auto &name : {"acceptStreamerSnapshot", "recoverStreamingSession",
                                "scheduleSessionRecovery", "discoverRecoverySession", "acceptRecoverySessions",
                                "normalizedStreamingSession", "acceptStreamingSession",
                                "startNativeStreamer", "retryNativeStreamer", "acceptNativeEvent"}) {
            const QRegularExpression function(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption);
            const auto match = function.match(shell);
            QVERIFY2(match.hasMatch(), name);
            QVERIFY(!evaluate(match.captured()).isError());
        }
        auto check = [&](const QString &expression) {
            const auto result = evaluate(expression);
            return !result.isError() && result.toBool();
        };
        QVERIFY(check(QStringLiteral(R"JS(
            acceptStreamerSnapshot({status: 'error', message: 'No video UDP packets'});
            acceptStreamerSnapshot({status: 'stopped', message: 'Stopped'});
            acceptStreamerSnapshot({status: 'error', message: 'Late response'});
            sessionReconnectAttempts === 0 && streamerRestartTimer.restarts === 1
                && streamer.message === 'No video UDP packets';
        )JS")));
        QVERIFY(check(QStringLiteral(R"JS(
            acceptStreamingSession(activeSession);
            prepares === 0 && sessionReconnectAttempts === 0;
        )JS")));
        QVERIFY(check(QStringLiteral(R"JS(
            for (var attempt = 1; attempt <= maximumSessionReconnectAttempts; attempt++) {
                streamerRestartTimer.running = false;
                recoverStreamingSession('Connection lost');
                if (discoveries !== attempt || !sessionRecoveryPending || prepares !== attempt - 1)
                    throw new Error('Recovery must discover the active session before preparing media');
                recoveryDiscoveryRequestId = '';
                acceptRecoverySessions({sessions: [activeSession]});
                if (claims !== attempt || !sessionClaimIsRecovery)
                    throw new Error('Recovery must claim the discovered seat');
                // Model a successful claim followed by a ready poll. The polling
                // contract is separately exercised by the session resume tests.
                sessionClaimRequestId = ''; sessionClaimIsRecovery = false;
                sessionRecoveryPending = false;
                acceptStreamingSession(activeSession);
                if (prepares !== attempt || sessionReconnectAttempts !== attempt)
                    throw new Error('A ready seat must not reset the video recovery budget');
                streamerPrepareRequestId = '';
                acceptStreamerSnapshot({status: 'streaming', sessionId: 'seat'});
                if (sessionReconnectAttempts !== attempt)
                    throw new Error('A transport handshake is not video progress');
                acceptStreamerSnapshot({status: 'error', message: 'HTTP 503', sessionId: 'seat'});
            }
            var previousPrepares = prepares;
            acceptStreamingSession(activeSession);
            startNativeStreamer();
            streamerRecoveryExhausted && claims === maximumSessionReconnectAttempts && prepares === previousPrepares
                && streamState === 'error' && streamMessage === 'HTTP 503';
        )JS")));
        QVERIFY(check(QStringLiteral(R"JS(
            retryNativeStreamer();
            !streamerRecoveryExhausted && streamerRestartAttempts === 0
                && sessionReconnectAttempts === 1 && prepares === previousPrepares
                && discoveries === maximumSessionReconnectAttempts + 1 && sessionRecoveryPending;
        )JS")));
        QVERIFY(check(QStringLiteral(R"JS(
            streamerRestartAttempts = 2; sessionReconnectAttempts = 1;
            sessionRecoveryPending = false; recoveryDiscoveryRequestId = '';
            acceptStreamerSnapshot({status: 'streaming', sessionId: 'seat'});
            var unchanged = streamerRestartAttempts === 2 && sessionReconnectAttempts === 1;
            acceptNativeEvent({type: 'status', event: 'first-frame', status: 'streaming', backend: 'D3D11'});
            unchanged && streamerRestartAttempts === 0 && sessionReconnectAttempts === 0
                && streamerRestartRecoveryCount === 2 && sessionRecoveryCount === 1;
        )JS")));
    }

    void shellUsesCoreOnlyToPrepareEmbeddedContext()
    {
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        QVERIFY(!shell.isEmpty());
        QVERIFY(shell.contains(QStringLiteral("CoreClient.request(\"streamer.prepare\"")));

        const QStringList forbiddenRoutes{
            QStringLiteral("CoreClient.request(\"streamer.start\""),
            QStringLiteral("CoreClient.request(\"streamer.status.get\""),
            QStringLiteral("CoreClient.request(\"streamer.stop\""),
            QStringLiteral("CoreClient.request(\"streamer.input.pause\""),
            QStringLiteral("CoreClient.request(\"streamer.control\""),
            QStringLiteral("CoreClient.request(\"streamer.recording.start\""),
            QStringLiteral("CoreClient.request(\"streamer.recording.stop\""),
            QStringLiteral("CoreClient.request(\"streamer.surface.update\""),
            QStringLiteral("CoreClient.request(\"streamer.detect\""),
        };
        for (const auto &route : forbiddenRoutes)
            QVERIFY2(!shell.contains(route), qPrintable(route));

        const QStringList nativeCommands{
            QStringLiteral("sendNativeCommand(\"hello\""),
            QStringLiteral("sendNativeCommand(\"start\""),
            QStringLiteral("sendNativeCommand(\"stop\""),
            QStringLiteral("sendNativeCommand(\"input-paused\""),
            QStringLiteral("sendNativeCommand(\"recording-start\""),
            QStringLiteral("sendNativeCommand(\"recording-stop\""),
        };
        for (const auto &command : nativeCommands)
            QVERIFY2(shell.contains(command), qPrintable(command));
        QVERIFY(shell.contains(QStringLiteral("\"toggle-fullscreen\": \"fullscreen-toggle\"")));
        QVERIFY(shell.contains(QStringLiteral("target: NativeStreamRuntime")));
    }

    void applicationExposesRuntimeWithoutLegacySurfaceController()
    {
        const auto main = source(QStringLiteral("src/app/ApplicationStartup.cpp"));
        QVERIFY(!main.isEmpty());
        QVERIFY(main.contains(QStringLiteral("setContextProperty(u\"NativeStreamRuntime\"_s")));
        QVERIFY(main.contains(QStringLiteral("StreamVideoItem::setNativeStreamRuntime")));
        QVERIFY(!main.contains(QStringLiteral("StreamSurfaceController")));
    }

    void linuxVulkanOwnerOutlivesRuntimeAndHiddenRootAdoption()
    {
        const auto main = source(QStringLiteral("src/app/ApplicationStartup.cpp"));
        const auto owner = main.indexOf(QStringLiteral("LinuxVulkanGraphics::Device vulkanDevice"));
        const auto diagnostics = main.indexOf(QStringLiteral("NativeStreamRuntime::initializeDiagnostics()"));
        const auto runtime = main.indexOf(QStringLiteral("NativeStreamRuntime nativeStreamRuntime"));
        const auto engine = main.indexOf(QStringLiteral("QQmlApplicationEngine engine"));
        const auto hidden = main.indexOf(QStringLiteral("engine.setInitialProperties"));
        const auto load = main.indexOf(QStringLiteral("engine.loadFromModule"));
        const auto adopt = main.indexOf(QStringLiteral("vulkanDevice.adopt(rootWindow)"));
        const auto prepare = main.indexOf(QStringLiteral("acceptance.prepareWindow()"));
        const auto show = main.indexOf(QStringLiteral("rootWindow->show()"));
        QVERIFY(owner >= 0);
        QVERIFY(diagnostics >= 0);
        QVERIFY(diagnostics < owner);
        QVERIFY(runtime > owner);
        QVERIFY(engine > runtime);
        QVERIFY(hidden > engine);
        QVERIFY(load > hidden);
        QVERIFY(adopt > load);
        QVERIFY(prepare > adopt);
        QVERIFY(show > prepare);
        const auto graphics = source(QStringLiteral("src/streaming/rendering/LinuxVulkanGraphics.cpp"));
        QVERIFY(graphics.contains(QStringLiteral("m_instance.setVkInstance")));
        QVERIFY(graphics.contains(QStringLiteral("QQuickGraphicsDevice::fromDeviceObjects")));
        QVERIFY(graphics.indexOf(QStringLiteral("m_instance.destroy()"))
                < graphics.indexOf(QStringLiteral("m_api.destroy(m_device)")));
        QVERIFY(!graphics.contains(QStringLiteral("new QQuickWindow")));
    }

    void liveScreensRenderThroughStreamVideoItem()
    {
        const QStringList screens{
            QStringLiteral("qml/screens/StreamScreen.qml"),
            QStringLiteral("qml/desktop/stream/DesktopStreamScreen.qml"),
        };
        const QRegularExpression liveItem(
            QStringLiteral("StreamVideoItem\\s*\\{[^}]*objectName:\\s*\"streamSurfaceHost\""),
            QRegularExpression::DotMatchesEverythingOption);
        for (const auto &path : screens) {
            const auto qml = source(path);
            QVERIFY2(!qml.isEmpty(), qPrintable(path));
            QVERIFY2(liveItem.match(qml).hasMatch(), qPrintable(path));
            QVERIFY(qml.contains(QStringLiteral("visible: root.visible && root.streaming")));
            QVERIFY(qml.contains(QStringLiteral(
                "!ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)")));
            QVERIFY(qml.contains(QStringLiteral(
                "shortcutBindings: ShellStore.streamShortcutBindings()")));
            QVERIFY(qml.contains(QStringLiteral(
                "onLocalShortcutRequested: action => ShellStore.applyStreamShortcutAction(action)")));
            QVERIFY(!qml.contains(QStringLiteral(
                "visible: root.visible && root.streaming && AppController.overlay === \"\"")));
        }
    }

    void passiveStatsKeepGameplayInputWhileModalOverlaysTakeOwnership()
    {
        const auto main = source(QStringLiteral("qml/Main.qml"));
        const auto host = source(QStringLiteral("qml/desktop/stream/DesktopStreamOverlayHost.qml"));
        QVERIFY(!main.isEmpty());
        QVERIFY(main.contains(QStringLiteral(
            "ControllerInput.shellCaptureEnabled = shellOwnsInput")));
        QVERIFY(main.contains(QStringLiteral(
            "inputBlocking: ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)")));
        QVERIFY(host.contains(QStringLiteral("focus: visible && inputBlocking")));
        QVERIFY(host.contains(QStringLiteral("focus: false")));
        QVERIFY(!main.contains(QStringLiteral(
            "ShellStore.setStreamInputPaused(shellOwnsInput)")));
    }

    void gameplayEscapeIsForwardedWhileDedicatedStopStillConfirms()
    {
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        const auto desktop = source(QStringLiteral("qml/desktop/shell/DesktopApp.qml"));
        const QStringList screens{
            QStringLiteral("qml/screens/StreamScreen.qml"),
            QStringLiteral("qml/desktop/stream/DesktopStreamScreen.qml"),
        };
        QVERIFY(!shell.contains(QStringLiteral("\"request-exit\": [\"Escape\"]")));
        QVERIFY(shell.contains(QStringLiteral(
            "\"stop-stream\": [String(settings.shortcutStopStream || \"Ctrl+Shift+Q\")]")));
        QVERIFY(shell.contains(QStringLiteral("requestStreamExitConfirmation()")));
        QVERIFY(shell.contains(QStringLiteral("desktop-stream-exit-confirm")));
        QVERIFY(desktop.contains(QStringLiteral(
            "onStopRequested: ShellStore.requestStreamExitConfirmation()")));
        for (const auto &path : screens) {
            const auto qml = source(path);
            QVERIFY2(qml.contains(QStringLiteral("if (!root.streaming")), qPrintable(path));
        }
    }

    void statsOverlayNeverPaintsOverTheStream()
    {
        const auto host = source(QStringLiteral("qml/desktop/stream/DesktopStreamOverlayHost.qml"));
        QVERIFY(!host.contains(QStringLiteral("visible: root.statsVisible\n        color:")));
        const auto menu = source(QStringLiteral("qml/desktop/stream/DesktopInStreamMenu.qml"));
        QVERIFY(!menu.contains(QStringLiteral("Stream quality")));
    }

    void fullscreenStatsShortcutHasAWindowIndependentOwner()
    {
        const auto main = source(QStringLiteral("qml/Main.qml"));
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        QVERIFY(main.contains(QStringLiteral("sequence: \"F3\"")));
        QVERIFY(main.contains(QStringLiteral("context: Qt.ApplicationShortcut")));
        QVERIFY(main.contains(QStringLiteral(
            "onActivated: ShellStore.applyStreamShortcutAction(\"toggle-stats\")")));
        QVERIFY(main.contains(QStringLiteral("sequence: \"Shift+F3\"")));
        QVERIFY(main.contains(QStringLiteral(
            "onActivated: desktopStreamOverlay.copyStatsToClipboard()")));
        QVERIFY(!shell.contains(QStringLiteral("\"toggle-stats\": [\"F3\"")));
        QVERIFY(shell.contains(QStringLiteral(
            "if (AppController.overlay === compact)\n                AppController.showOverlay(expanded)")));
    }

    void inStreamOverlaysDoNotReactivateOrNormalizeTheWindow()
    {
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        const QRegularExpression shortcutAction(
            QStringLiteral("function applyStreamShortcutAction\\(action\\) \\{(?<body>.*?)\\n    \\}"),
            QRegularExpression::DotMatchesEverythingOption);
        const auto match = shortcutAction.match(shell);
        QVERIFY(match.hasMatch());
        QVERIFY(!match.captured(QStringLiteral("body")).contains(
            QStringLiteral("AppController.activateWindow()")));

        const QRegularExpression overlayRequest(
            QStringLiteral("function inspectStreamerOverlayRequest\\(value\\) \\{(?<body>.*?)\\n    \\}"),
            QRegularExpression::DotMatchesEverythingOption);
        const auto overlayMatch = overlayRequest.match(shell);
        QVERIFY(overlayMatch.hasMatch());
        QVERIFY(!overlayMatch.captured(QStringLiteral("body")).contains(
            QStringLiteral("AppController.activateWindow()")));

        const QRegularExpression exitRequest(
            QStringLiteral("function requestStreamExitConfirmation\\(\\) \\{(?<body>.*?)\\n    \\}"),
            QRegularExpression::DotMatchesEverythingOption);
        const auto exitMatch = exitRequest.match(shell);
        QVERIFY(exitMatch.hasMatch());
        QVERIFY(!exitMatch.captured(QStringLiteral("body")).contains(
            QStringLiteral("AppController.activateWindow()")));
    }

    void streamSurfaceModeIsStableUntilExplicitlyChanged()
    {
        const auto main = source(QStringLiteral("qml/Main.qml"));
        QVERIFY(main.contains(QStringLiteral("property bool streamSurfaceLocked: false")));
        QVERIFY(main.contains(QStringLiteral(
            "readonly property bool targetDesktopSurface: streamSurfaceLocked")));
        QVERIFY(main.contains(QStringLiteral(
            "window.lockedStreamDesktopSurface = !enabled")));
        QVERIFY(main.contains(QStringLiteral(
            "const allowed = window.activeRoute !== \"stream\"\n                && window.switchToConsoleOnPad")));
        QVERIFY(!main.contains(QStringLiteral("function syncPadHold()")));
    }

    void cursorModeTransitionsCloseThePreviousButtonOwner()
    {
        const auto streamVideoSource = source(QStringLiteral("src/streaming/StreamVideoItemInput.cpp"));
        const auto transition = streamVideoSource.indexOf(
            QStringLiteral("if (relative && !m_rawInputActive) releaseQtMouseButtons();"));
        const auto switchMode = streamVideoSource.indexOf(
            QStringLiteral("m_relativeMouse = relative;"), transition);
        QVERIFY(transition >= 0);
        QVERIFY(switchMode > transition);

        const auto embedded = source(QStringLiteral(
            "../native/opennow-streamer/crates/opennow-streamer-platform/src/embedded_input.rs"));
        QVERIFY(embedded.contains(QStringLiteral(
            "raw.set_capture(raw_enabled, relative_mouse);")));
    }

    void unlockedPointerMovementUsesQtHoverDelivery()
    {
        const auto header = source(QStringLiteral("src/streaming/StreamVideoItem.h"));
        QVERIFY(header.contains(QStringLiteral(
            "void hoverEnterEvent(QHoverEvent *event) override;")));
        QVERIFY(header.contains(QStringLiteral(
            "void hoverMoveEvent(QHoverEvent *event) override;")));

        const auto streamVideoSource = source(QStringLiteral("src/streaming/StreamVideoItemInput.cpp"));
        const auto hoverMove = streamVideoSource.indexOf(
            QStringLiteral("void StreamVideoItem::hoverMoveEvent(QHoverEvent *event)"));
        const auto wheel = streamVideoSource.indexOf(
            QStringLiteral("void StreamVideoItem::wheelEvent(QWheelEvent *event)"), hoverMove);
        QVERIFY(hoverMove >= 0);
        QVERIFY(wheel > hoverMove);
        const auto implementation = streamVideoSource.mid(hoverMove, wheel - hoverMove);
        QVERIFY(implementation.contains(QStringLiteral(
            "if (!m_captureActive || m_relativeMouse)")));
        QVERIFY(implementation.contains(QStringLiteral(
            "submitAbsoluteMouse(event->position());")));
    }
};

QTEST_MAIN(EmbeddedOrchestrationTest)
#include "tst_embeddedorchestration.moc"
