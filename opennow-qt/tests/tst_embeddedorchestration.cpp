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
    void pendingRecoveryDoesNotHideAStartedVideoSurface()
    {
        for (const auto &path : {"qml/screens/StreamScreen.qml", "qml/desktop/DesktopStreamScreen.qml"}) {
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
            var nativeRuntimeReady = true, prepares = 0, claims = 0;
            var streamerRestartTimer = {running: false, restarts: 0,
                restart: function() { this.running = true; this.restarts++; },
                stop: function() { this.running = false; }};
            var streamPollTimer = {stop: function() {}, restart: function() {}};
            var CoreClient = {request: function(type) {
                if (type === 'streamer.prepare') { prepares++; return 'prepare'; }
                if (type === 'session.claim') { claims++; return 'claim'; }
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
        for (const auto &name : {"acceptStreamerSnapshot", "recoverStreamingSession",
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
            streamerRestartAttempts === 1 && streamerRestartTimer.restarts === 1
                && streamer.message === 'No video UDP packets';
        )JS")));
        QVERIFY(check(QStringLiteral(R"JS(
            acceptStreamingSession(activeSession);
            prepares === 0 && streamerRestartAttempts === 1;
        )JS")));
        QVERIFY(check(QStringLiteral(R"JS(
            streamerRestartTimer.running = false;
            streamer = {status: 'starting', sessionId: 'seat'};
            acceptStreamerSnapshot({status: 'streaming', sessionId: 'seat'});
            acceptStreamerSnapshot({status: 'error', message: 'Still no video'});
            streamerRestartAttempts === 2;
        )JS")));
        QVERIFY(check(QStringLiteral(R"JS(
            streamerRestartTimer.running = false;
            streamer = {status: 'starting', sessionId: 'seat'};
            acceptStreamerSnapshot({status: 'error', message: 'OPTIONS failed: 400'});
            sessionClaimRequestId = '';
            acceptStreamingSession(activeSession);
            streamerPrepareRequestId = '';
            acceptStreamerSnapshot({status: 'error', message: 'OPTIONS failed: 400'});
            sessionClaimRequestId = '';
            acceptStreamingSession(activeSession);
            streamerPrepareRequestId = '';
            acceptStreamerSnapshot({status: 'error', message: 'HTTP 503'});
            var previousPrepares = prepares;
            acceptStreamingSession(activeSession);
            startNativeStreamer();
            streamerRecoveryExhausted && claims === 2 && prepares === previousPrepares
                && streamState === 'error' && streamMessage === 'HTTP 503';
        )JS")));
        QVERIFY(check(QStringLiteral(R"JS(
            retryNativeStreamer();
            !streamerRecoveryExhausted && streamerRestartAttempts === 0
                && sessionReconnectAttempts === 0 && prepares === previousPrepares + 1;
        )JS")));
        QVERIFY(check(QStringLiteral(R"JS(
            streamerRestartAttempts = 2; sessionReconnectAttempts = 1;
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
        const auto main = source(QStringLiteral("src/main.cpp"));
        QVERIFY(!main.isEmpty());
        QVERIFY(main.contains(QStringLiteral("setContextProperty(u\"NativeStreamRuntime\"_s")));
        QVERIFY(main.contains(QStringLiteral("StreamVideoItem::setNativeStreamRuntime")));
        QVERIFY(!main.contains(QStringLiteral("StreamSurfaceController")));
    }

    void liveScreensRenderThroughStreamVideoItem()
    {
        const QStringList screens{
            QStringLiteral("qml/screens/StreamScreen.qml"),
            QStringLiteral("qml/desktop/DesktopStreamScreen.qml"),
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
        const auto host = source(QStringLiteral("qml/desktop/DesktopStreamOverlayHost.qml"));
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
        const auto desktop = source(QStringLiteral("qml/desktop/DesktopApp.qml"));
        const QStringList screens{
            QStringLiteral("qml/screens/StreamScreen.qml"),
            QStringLiteral("qml/desktop/DesktopStreamScreen.qml"),
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
        const auto host = source(QStringLiteral("qml/desktop/DesktopStreamOverlayHost.qml"));
        QVERIFY(!host.contains(QStringLiteral("visible: root.statsVisible\n        color:")));
        const auto menu = source(QStringLiteral("qml/desktop/DesktopInStreamMenu.qml"));
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
            "if (window.activeRoute !== \"stream\"\n                && window.switchToConsoleOnPad")));
        QVERIFY(!main.contains(QStringLiteral("function syncPadHold()")));
    }

    void cursorModeTransitionsCloseThePreviousButtonOwner()
    {
        const auto streamVideoSource = source(QStringLiteral("src/StreamVideoItem.cpp"));
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
        const auto header = source(QStringLiteral("src/StreamVideoItem.h"));
        QVERIFY(header.contains(QStringLiteral(
            "void hoverEnterEvent(QHoverEvent *event) override;")));
        QVERIFY(header.contains(QStringLiteral(
            "void hoverMoveEvent(QHoverEvent *event) override;")));

        const auto streamVideoSource = source(QStringLiteral("src/StreamVideoItem.cpp"));
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
