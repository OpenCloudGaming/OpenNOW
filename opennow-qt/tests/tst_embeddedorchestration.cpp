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

bool prepareLaunchGuards(QJSEngine &engine)
{
    if (engine.evaluate(QStringLiteral(R"JS(
        var launchInspectRequestId='', launchInspectStage='', directLookupRequestId='', authGeneration=0;
        var catalogOwner={selectedIdentity:'selection',actionGeneration:0,requestContextKey:'',mutationBusy:false,authScope:{generation:0}};
        var signedIn=true;
        var desktopUiActive=false, queueLaunchWaitingForSubscription=false;
        var remoteSessionsRequestId='', streamerPrepareRequestId='';
        var queueSelector={opened:false,begin:function(title){return false}};
    )JS")).isError()) return false;
    const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
    for (const auto &name : {"launchIntentCurrent", "inspectLaunch", "invalidateLaunchInspection",
             "continueInspectedLaunch", "discoverPendingLaunch"}) {
        const auto match = QRegularExpression(QStringLiteral(
            "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
            QRegularExpression::DotMatchesEverythingOption).match(shell);
        if (!match.hasMatch() || engine.evaluate(match.captured()).isError()) return false;
    }
    return true;
}

bool loadShellFunction(QJSEngine &engine, const QString &name, int indentation = 4)
{
    const auto prefix = QString(indentation, u' ');
    const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
    const auto match = QRegularExpression(prefix + QStringLiteral("function %1\\([^\\n]*\\) \\{\\n.*?\\n").arg(name)
        + prefix + u'}', QRegularExpression::DotMatchesEverythingOption)
        .match(indentation == 8 ? shell.section(QStringLiteral("property Connections coreConnections:"), 1) : shell);
    if (!match.hasMatch()) return false;
    const auto result = engine.evaluate(match.captured());
    if (result.isError()) qWarning().noquote() << result.toString();
    return !result.isError();
}

bool prepareAuthentication(QJSEngine &engine)
{
    const auto setup = engine.evaluate(QStringLiteral(R"JS(
        var root = this, ready = true, providersRequestId = '', providerRetryAttempts = 0;
        var providers = [{idpId:'alliance',displayName:'Alliance'}], selectedProviderIdpId = 'alliance';
        var providerDiscoveryDegraded = false, authGeneration = 0, authSessionRequestId = '';
        var authSession = {user:{userId:'old',displayName:'Old'},provider:{idpId:'alliance'}};
        Object.defineProperty(root, 'signedIn', {get: function() { return authSession !== null; }});
        var addingAccount = false, authState = 'signed-in', authMessage = '', accountMessage = '';
        var accountSwitchRequestId = '', accountRemoveRequestId = '', logoutAllRequestId = '';
        var deviceStartRequestId = '', devicePollRequestId = '', deviceCompleteRequestId = '';
        var pendingStaySignedIn = true, authChallenge = null, sessionPersistence = 'secure-store';
        var sessionPersistenceMessage = '', pinMessage = '', lastError = '', requests = [], cancelled = [];
        var providerRetryTimer = {running:false,interval:31000,restarts:0,
            restart:function() {this.running=true;this.restarts++;},stop:function() {this.running=false;}};
        var devicePollTimer = {running:false,interval:1000,
            restart:function() {this.running=true;},stop:function() {this.running=false;}};
        var CoreClient = {request:function(method,params) {
            requests.push({method:method,params:params});return 'request-' + requests.length;
        },cancel:function(id) {cancelled.push(id);}};
        var AppController = {route:'accounts',navigate:function(route) {this.route=route;}};
        var settingsOwner = {acceptResponse:function() {return false;},acceptFailure:function() {return false;}};
        var onboardingOwner = {acceptResponse:function() {return false;},acceptFailure:function() {return false;}};
        function ownedSessionTermination() {return null;}
        function finishArtworkRequest() {return false;}
        function acceptAuthEnvelope() {return true;}
        function reloadCatalogForSession() {}
        function refreshAccountServices() {}
        function resolveDirectLaunch() {}
        function qsTr(text) {return text;}
        String.prototype.arg = function(value) {return this.replace(/%[12]/,String(value));};
    )JS"));
    if (setup.isError()) return false;
    for (const auto &name : {"refreshProviders", "scheduleProviderRetry", "beginAddAccount",
             "startDeviceLogin", "cancelDeviceLogin", "switchAccount"}) {
        if (!loadShellFunction(engine, QString::fromLatin1(name))) return false;
    }
    return loadShellFunction(engine, QStringLiteral("onResponseReceived"), 8)
        && loadShellFunction(engine, QStringLiteral("onRequestFailed"), 8);
}
}

class EmbeddedOrchestrationTest final : public QObject
{
    Q_OBJECT

private slots:
    void providerRpcFailureOffersBoundedAndManualRecovery()
    {
        QJSEngine engine;
        QVERIFY(prepareAuthentication(engine));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            providers = [];
            refreshProviders();
            onRequestFailed(providersRequestId,'deadline_exceeded','Provider lookup timed out');
        )JS")).isError());
        QVERIFY(engine.evaluate(QStringLiteral("providerDiscoveryDegraded && providerRetryTimer.running && providersRequestId === ''")).toBool());
        for (int attempt = 1; attempt <= 3; ++attempt) {
            QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
                providerRetryTimer.running = false;
                providerRetryAttempts++;
                refreshProviders();
                onRequestFailed(providersRequestId,'network_error','Offline');
            )JS")).isError());
            QCOMPARE(engine.evaluate(QStringLiteral("providerRetryTimer.running")).toBool(), attempt < 3);
        }
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 4);
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            refreshProviders(true);
            var manualRequest = providersRequestId;
            refreshProviders(true);
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 5);
        QCOMPARE(engine.evaluate(QStringLiteral("providerRetryAttempts")).toInt(), 0);
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            onResponseReceived(manualRequest,{providers:[{idpId:'alliance'}],discovery:{state:'ready'}});
        )JS")).isError());
        QVERIFY(engine.evaluate(QStringLiteral("!providerDiscoveryDegraded && !providerRetryTimer.running")).toBool());
        const auto desktop = source(QStringLiteral("qml/desktop/auth/DesktopSignInScreen.qml"));
        const auto console = source(QStringLiteral("qml/screens/SignInScreen.qml"));
        QVERIFY(desktop.contains(QStringLiteral("onClicked: ShellStore.refreshProviders(true)")));
        QVERIFY(console.contains(QStringLiteral("onClicked: ShellStore.refreshProviders(true)")));
    }

    void failedDeviceLoginClearsTheChallengeAndCanRestart_data()
    {
        QTest::addColumn<QString>("phase");
        for (const auto &phase : {"expired", "denied", "start-rpc", "poll-rpc", "complete-rpc"})
            QTest::newRow(phase) << QString::fromLatin1(phase);
    }

    void failedDeviceLoginClearsTheChallengeAndCanRestart()
    {
        QFETCH(QString, phase);
        QJSEngine engine;
        QVERIFY(prepareAuthentication(engine));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            authSession = null;
            authChallenge = {attemptId:'expired-attempt',verificationUriComplete:'https://example.invalid/expired'};
            authState = 'waiting';
            devicePollTimer.running = true;
        )JS")).isError());
        QString failure;
        if (phase.endsWith(QStringLiteral("-rpc"))) {
            const auto field = phase == QStringLiteral("start-rpc") ? QStringLiteral("deviceStartRequestId")
                : phase == QStringLiteral("poll-rpc") ? QStringLiteral("devicePollRequestId") : QStringLiteral("deviceCompleteRequestId");
            failure = QStringLiteral("%1='failed'; onRequestFailed('failed','network_error','Login unavailable');").arg(field);
        } else {
            failure = QStringLiteral("devicePollRequestId='failed'; onResponseReceived('failed',{status:'%1',error:'Login unavailable'});").arg(phase);
        }
        const auto result = engine.evaluate(failure);
        QVERIFY2(!result.isError(), qPrintable(result.toString()));
        QVERIFY(engine.evaluate(QStringLiteral("authChallenge === null && authState === 'error' && !devicePollTimer.running")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("startDeviceLogin('alliance',false)")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests[requests.length-1].method")).toString(), QStringLiteral("auth.device.start"));
        QCOMPARE(engine.evaluate(QStringLiteral("authState")).toString(), QStringLiteral("starting"));
        QVERIFY(!engine.evaluate(QStringLiteral("pendingStaySignedIn")).toBool());
    }

    void addingAnAccountFinishesWithoutSignedInChanging()
    {
        QJSEngine engine;
        QVERIFY(prepareAuthentication(engine));
        QVERIFY(!engine.evaluate(QStringLiteral("beginAddAccount()")).isError());
        QVERIFY(engine.evaluate(QStringLiteral("addingAccount && signedIn && authSession.user.userId === 'old' && AppController.route === 'sign-in'")).toBool());
        const auto console = source(QStringLiteral("qml/screens/SignInScreen.qml"));
        const auto connected = QRegularExpression(QStringLiteral("readonly property bool connected: ([^\\n]+)")).match(console);
        QVERIFY(connected.hasMatch());
        engine.globalObject().setProperty(QStringLiteral("ShellStore"), engine.globalObject());
        QVERIFY(!engine.evaluate(connected.captured(1)).toBool());
        const auto completion = engine.evaluate(QStringLiteral(R"JS(
            deviceCompleteRequestId='complete';
            onResponseReceived('complete',{session:{user:{userId:'new',displayName:'New'},provider:{idpId:'alliance'}},persistence:'secure-store'});
        )JS"));
        QVERIFY2(!completion.isError(), qPrintable(completion.toString()));
        QVERIFY(engine.evaluate(QStringLiteral("signedIn && !addingAccount && authSession.user.userId === 'new'")).toBool());
        QCOMPARE(engine.evaluate(QStringLiteral("AppController.route")).toString(), QStringLiteral("home"));
        QVERIFY(source(QStringLiteral("qml/screens/AccountsScreen.qml")).contains(QStringLiteral("onClicked: ShellStore.beginAddAccount()")));
    }

    void cancellingAnAddedAccountReturnsToIdleInsteadOfWaiting()
    {
        QJSEngine engine;
        QVERIFY(prepareAuthentication(engine));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            beginAddAccount();
            authChallenge = {attemptId:'add-attempt',verificationUriComplete:'https://example.invalid/add'};
            authState = 'waiting';
            authMessage = 'Scan the QR code';
            devicePollTimer.running = true;
            cancelDeviceLogin();
        )JS")).isError());
        QVERIFY(engine.evaluate(QStringLiteral("authChallenge === null && authState === 'idle' && authMessage === ''")).toBool());
        QVERIFY(engine.evaluate(QStringLiteral("addingAccount && signedIn && authSession.user.userId === 'old'")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("devicePollTimer.running")).toBool());
        const auto desktop = source(QStringLiteral("qml/desktop/auth/DesktopSignInScreen.qml"));
        const auto waiting = QRegularExpression(QStringLiteral("readonly property bool waiting: ([^\\n]+)")).match(desktop);
        QVERIFY(waiting.hasMatch());
        engine.globalObject().setProperty(QStringLiteral("ShellStore"), engine.globalObject());
        QVERIFY(!engine.evaluate(waiting.captured(1)).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("startDeviceLogin('alliance',false)")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests[requests.length-1].method")).toString(), QStringLiteral("auth.device.start"));
        QCOMPARE(engine.evaluate(QStringLiteral("authState")).toString(), QStringLiteral("starting"));
    }

    void accountSwitchFailureIsVisibleAndClearedOnRetry()
    {
        QJSEngine engine;
        QVERIFY(prepareAuthentication(engine));
        const auto failure = engine.evaluate(QStringLiteral(R"JS(
            switchAccount('new','');
            onRequestFailed(accountSwitchRequestId,'network_error','Cannot reach your provider');
        )JS"));
        QVERIFY2(!failure.isError(), qPrintable(failure.toString()));
        QCOMPARE(engine.evaluate(QStringLiteral("accountMessage")).toString(), QStringLiteral("Cannot reach your provider"));
        QCOMPARE(engine.evaluate(QStringLiteral("pinMessage")).toString(), QStringLiteral("Cannot reach your provider"));
        QCOMPARE(engine.evaluate(QStringLiteral("authSession.user.userId")).toString(), QStringLiteral("old"));
        QVERIFY(!engine.evaluate(QStringLiteral("switchAccount('new','')")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("accountMessage")).toString(), QString());
        const auto accounts = source(QStringLiteral("qml/screens/AccountsScreen.qml"));
        QVERIFY(accounts.contains(QStringLiteral("objectName: \"accountActionError\"")));
        QVERIFY(accounts.contains(QStringLiteral("text: ShellStore.accountMessage")));
    }

    void exhaustedTransportNegotiationDoesNotReclaimTheSeat_data()
    {
        QTest::addColumn<QString>("code");
        QTest::addColumn<bool>("terminal");
        QTest::addColumn<bool>("recoveryPending");
        QTest::newRow("missing-video-peer") << QStringLiteral("missing-video-peer") << true << false;
        QTest::newRow("legacy-unsupported") << QStringLiteral("nvst-legacy-transport-unsupported") << true << false;
        QTest::newRow("missing-video-peer-during-recovery") << QStringLiteral("missing-video-peer") << true << true;
        QTest::newRow("legacy-unsupported-during-recovery") << QStringLiteral("nvst-legacy-transport-unsupported") << true << true;
        QTest::newRow("transient-network") << QStringLiteral("network_error") << false << false;
        QTest::newRow("unknown-error") << QStringLiteral("new_native_error") << false << false;
    }

    void exhaustedTransportNegotiationDoesNotReclaimTheSeat()
    {
        QFETCH(QString, code);
        QFETCH(bool, terminal);
        QFETCH(bool, recoveryPending);
        QJSEngine engine;
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var root=this, activeSession={sessionId:'seat'},streamer={status:'starting'},runtimeStreamProfile={};
            var streamInputStateKnown=true,streamReplayEnabled=false,mediaClipTargetRequestId='',streamClipRequestId='';
            var streamRecordingActive=false,streamerStopExpected=false,sessionRecoveryPending=false;
            var streamState='starting',streamMessage='',sessionReconnectAttempts=0,maximumSessionReconnectAttempts=8;
            var streamStopRequestId='',streamerRecoveryExhausted=false,lastError='',streamerRestartAttempts=0;
            var recoveryDiscoveryRequestId='',sessionClaimRequestId='',streamerStopRequestId='',recoverySessionId='';
            var sessionClaimIsRecovery=false,resumePollAttempts=0,resumePollDeadlineMs=0,ready=true;
            var NativeStreamRuntime={running:false},requests=[];
            var CoreClient={cancel:function() {},request:function(method,params) {requests.push({method:method,params:params});return 'recovery';}};
            var streamerRestartTimer={running:false,restart:function() {this.running=true;},stop:function() {this.running=false;}};
            var streamPollTimer={stop:function() {}},streamerPrepareRequestId='',streamPollRequestId='';
            function inspectStreamerOverlayRequest() {} function inspectStreamerScreenshotRequest() {}
            function inspectStreamerRecordingRequest() {} function inspectStreamerShortcutAction() {}
            function qsTr(text) {return text;}
        )JS")).isError());
        for (const auto &name : {"acceptStreamerSnapshot", "isRemoteSessionTermination", "cancelSessionRecovery",
                 "scheduleSessionRecovery", "retryNativeStreamer", "recoverStreamingSession", "discoverRecoverySession"})
            QVERIFY(loadShellFunction(engine, QString::fromLatin1(name)));
        engine.globalObject().setProperty(QStringLiteral("failureCode"), code);
        if (recoveryPending) {
            QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
                sessionRecoveryPending=true; recoveryDiscoveryRequestId='old-discovery';
                sessionClaimRequestId='old-claim'; streamerRestartTimer.running=true;
            )JS")).isError());
        }
        const auto result = engine.evaluate(QStringLiteral(R"JS(
            acceptStreamerSnapshot({status:'error',errorCode:failureCode,message:'Original transport failure'});
            acceptStreamerSnapshot({status:'stopped',message:'Later stopped event'});
        )JS"));
        QVERIFY2(!result.isError(), qPrintable(result.toString()));
        QCOMPARE(engine.evaluate(QStringLiteral("streamerRecoveryExhausted")).toBool(), terminal);
        QCOMPARE(engine.evaluate(QStringLiteral("streamerRestartTimer.running")).toBool(), !terminal);
        QCOMPARE(engine.evaluate(QStringLiteral("streamState")).toString(), terminal ? QStringLiteral("error") : QStringLiteral("reconnecting"));
        QCOMPARE(engine.evaluate(QStringLiteral("streamMessage")).toString(), QStringLiteral("Original transport failure"));
        QCOMPARE(engine.evaluate(QStringLiteral("activeSession.sessionId")).toString(), QStringLiteral("seat"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 0);
        if (terminal) {
            QVERIFY(engine.evaluate(QStringLiteral("!sessionRecoveryPending && recoveryDiscoveryRequestId === '' && sessionClaimRequestId === ''")).toBool());
        }
        QVERIFY(!engine.evaluate(QStringLiteral("retryNativeStreamer()")).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("streamerRecoveryExhausted")).toBool());
        QCOMPARE(engine.evaluate(QStringLiteral("requests[0].method")).toString(), QStringLiteral("session.poll"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests[0].params.sessionId")).toString(), QStringLiteral("seat"));
    }

    void allianceAccountInvalidationRejectsOldRegionsAndKeepsProviderPreferences()
    {
        const auto account = source(QStringLiteral("qml/state/account/AccountServicesState.qml"));
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        for (const auto &name : {"invalidateAccount", "acceptRegions"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(account);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var root = this, cancelled = [], mediaOwner = {id:'same-native-runtime'};
            var accountLinkPollTimer = {stop: function() {}}, coreClient = {cancel: function(id) {cancelled.push(id)}};
            var syncPollTimer = {stop: function() {}}, syncStatusRequestId = 'old-sync-status', syncCancelRequestId = 'old-sync-cancel';
            var syncOperation = {operationId:'old-sync'}, storeSubscriptions = [{id:'old-subscription'}], catalogDefinitions = {};
            var subscriptionRequestId = 'old-subscription', regionsRequestId = 'old-regions', regionPingRequestId = 'old-ping';
            var gameAccountsRequestId = 'old-accounts', gameAccountActionRequestId = 'old-action', accountLinkStartRequestId = 'old-link';
            var accountLinkPollRequestId = 'old-link-poll', storageLocationsRequestId = 'old-storage', storageResetRequestId = 'old-reset';
            var subscription = {membershipTier:'old'}, regions = [{name:'old'}], regionsVpcId = 'old-vpc';
            var regionPingPending = true, regionPingResults = {}, regionPingMessage = '', gameAccounts = [], gameAccountsState = 'loading';
            var gameAccountMessage = '', accountLinkAttempt = {}, storageLocations = [], storageMessage = '';
            invalidateAccount();
            if (regionsRequestId === 'old-regions') acceptRegions({regions:[{name:'wrong-account'}],vpcId:'wrong-vpc'});
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("cancelled.length")).toInt(), 11);
        QVERIFY(engine.evaluate(QStringLiteral("syncOperation === null && syncStatusRequestId === '' && syncCancelRequestId === '' && storeSubscriptions.length === 0")).toBool());
        QCOMPARE(engine.evaluate(QStringLiteral("regions.length")).toInt(), 0);
        QCOMPARE(engine.evaluate(QStringLiteral("regionsVpcId")).toString(), QString());
        QCOMPARE(engine.evaluate(QStringLiteral("subscriptionRequestId")).toString(), QString());
        QCOMPARE(engine.evaluate(QStringLiteral("mediaOwner.id")).toString(), QStringLiteral("same-native-runtime"));
        QVERIFY(!engine.evaluate(QStringLiteral("regionsRequestId='new-regions'; if (regionsRequestId==='new-regions') acceptRegions({regions:[{name:'Alliance region'}],vpcId:'alliance-vpc'})")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("regions[0].name")).toString(), QStringLiteral("Alliance region"));
        QVERIFY(shell.contains(QStringLiteral("requestId === root.regionsRequestId")));
        const auto settings = source(QStringLiteral("qml/state/settings/SettingsState.qml"));
        const auto selected = QRegularExpression(QStringLiteral("readonly property string selectedRegion: (\\{.*?\\n    \\})"),
            QRegularExpression::DotMatchesEverythingOption).match(settings);
        QVERIFY(selected.hasMatch());
        QVERIFY(!engine.evaluate(QStringLiteral("function selectedRegion() ") + selected.captured(1)).isError());
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var settings = {region:'old-nvidia',regionProviderIdpId:'nvidia',providerRegions:{alliance:'saved-alliance'}};
            var providerIdpId='alliance', providerCode='ALLIANCE';
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("selectedRegion()")).toString(), QStringLiteral("saved-alliance"));
        QCOMPARE(engine.evaluate(QStringLiteral("providerIdpId='another'; selectedRegion()")).toString(), QString());
        QCOMPARE(engine.evaluate(QStringLiteral("providerIdpId='alliance'; regions=[]; selectedRegion()")).toString(), QStringLiteral("saved-alliance"));
        for (const auto &name : {"matchesAuthScope", "acceptsSessionScope"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(shell);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var authGeneration=2, authSession={user:{userId:'b'},provider:{idpId:'alliance'}};
            var oldScope={generation:1,userId:'a',providerIdpId:'nvidia'};
            var activeSession={sessionId:'owned-seat',ownerScope:oldScope};
        )JS")).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("matchesAuthScope(oldScope)")).toBool());
        QVERIFY(engine.evaluate(QStringLiteral("acceptsSessionScope(oldScope)")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("acceptsSessionScope({generation:1,userId:'different',providerIdpId:'nvidia'})")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("authSession={user:{userId:'a'},provider:{idpId:'nvidia'}}; acceptsSessionScope(oldScope)")).toBool());
    }

    void authenticationEnvelopeRejectsOlderAccountsAndCancelsEveryLoginPhase()
    {
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        for (const auto &name : {"acceptAuthEnvelope", "cancelDeviceLogin", "pollDeviceLogin", "invalidateStoreLaunch"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(shell);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var authGeneration = 4, authSession = null, sessionPersistence = 'none', authWarnings = [];
            var root = this, activeSession = null, remoteSessions = [], pendingLaunchParams = null, conflictSession = null;
            var remoteSessionsRequestId = '', remoteSessionDiscoveryRequestId = '', sessionClaimRequestId = '', streamCreateRequestId = '', streamerPrepareRequestId = '';
            var accountServicesOwner = {invalidateAccount: function() {}}, Qt = {callLater: function() {}};
            var storeLaunchRequestId = '', storeLaunchTarget = null, storeLaunchFailed = false,
                storeLaunchDecision = {status: 'metadata_unconfirmed', message: ''};
            function reloadCatalogForSession() {} function refreshAccountServices() {}
            var ready = true, signedIn = false, authState = 'waiting', authSessionRequestId = '';
            var deviceStartRequestId = 'start', devicePollRequestId = 'poll', deviceCompleteRequestId = 'complete';
            var authChallenge = {attemptId: 'attempt'}, cancelled = [], requests = [];
            var devicePollTimer = {stop: function() {}};
            var CoreClient = {cancel: function(id) { cancelled.push(id) }, request: function(method, params) {
                requests.push({method: method, params: params}); return 'reconcile';
            }};
        )JS")).isError());
        QVERIFY(engine.evaluate(QStringLiteral("acceptAuthEnvelope({generation:5,session:{user:{userId:'new'},provider:{code:'NVIDIA'}},persistence:'secure-store'})")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("acceptAuthEnvelope({generation:4,session:null})")).toBool());
        QCOMPARE(engine.evaluate(QStringLiteral("authSession.user.userId")).toString(), QStringLiteral("new"));
        QVERIFY(!engine.evaluate(QStringLiteral("cancelDeviceLogin()")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("cancelled.join(',')")).toString(), QStringLiteral("start,complete,poll"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests[0].method")).toString(), QStringLiteral("auth.session.get"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests[1].method")).toString(), QStringLiteral("auth.device.cancel"));
        QVERIFY(!engine.evaluate(QStringLiteral("authChallenge={attemptId:'new'}; pollDeviceLogin()")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("Object.keys(requests[2].params).join(',')")).toString(), QStringLiteral("attemptId"));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            activeSession={ownerScope:{generation:6,userId:'new',providerIdpId:'alliance'}};
            streamerPrepareRequestId='current-owner-prepare';
            acceptAuthEnvelope({generation:6,session:{user:{userId:'new'},provider:{idpId:'alliance'}}});
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("streamerPrepareRequestId")).toString(), QStringLiteral("current-owner-prepare"));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            storeLaunchRequestId='store-launch-in-flight';
            storeLaunchTarget={appId:'previous-account'};
            storeLaunchFailed=true;
            acceptAuthEnvelope({generation:7,session:{user:{userId:'other'},provider:{idpId:'alliance'}}});
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("storeLaunchRequestId")).toString(), QString());
        QVERIFY(engine.evaluate(QStringLiteral("storeLaunchTarget === null")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("storeLaunchFailed")).toBool());
        QVERIFY(engine.evaluate(QStringLiteral("cancelled.indexOf('store-launch-in-flight') >= 0")).toBool());
        QCOMPARE(engine.evaluate(QStringLiteral("streamerPrepareRequestId")).toString(), QString());
        QVERIFY(!shell.contains(QStringLiteral("deviceCode")));
        const auto timer = shell.section(QStringLiteral("property Timer devicePollTimer:"), 1).section(QStringLiteral("property Timer streamPollTimer:"), 0, 0);
        QVERIFY(timer.contains(QStringLiteral("repeat: false")));
    }

    void desktopStoreSelectionUsesTheSelectedLaunchVariant()
    {
        const auto desktop = source(QStringLiteral("qml/desktop/shell/DesktopApp.qml"));
        QVERIFY(desktop.contains(QStringLiteral("onVariantSelected: index => ShellStore.selectGameVariant(index)")));
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        const auto catalog = source(QStringLiteral("qml/state/catalog/CatalogState.qml"));
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        engine.installExtensions(QJSEngine::TranslationExtension);
        for (const auto &name : {"selectGameVariant", "selectedLaunchAppId", "launchSelectedGame"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(
                    QString::fromLatin1(name) == "selectGameVariant" ? catalog : shell);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var signedIn = true, ready = true, streamBusy = false, onboardingReplaying = false;
            var selectedGame = {id:'parent',launchAppId: '1001', title: 'Multi Store Game', selectedVariantIndex: 0,
                variants: [{id: '1001', store: 'Steam', inLibrary: false},
                           {id: '1003', store: 'Xbox', inLibrary: true}]};
            var settings = {}, selectedRegion = '', regions = [], requests = [], pendingLaunchParams = null;
            var storeLaunchTarget = null;
            var CoreClient = {request: function(method, params) {
                requests.push({method: method, params: params}); return 'request';
            }};
            var AppController = {navigate: function(route) {}};
            function accessibilityAnnounced(message) {}
            selectGameVariant(1);
            launchSelectedGame(false);
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests[0].method")).toString(), QStringLiteral("catalog.launch.inspect"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests[0].params.variantId")).toString(), QStringLiteral("1003"));
        QVERIFY(engine.evaluate(QStringLiteral("pendingLaunchParams.accountLinked")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("launchInspectRequestId=''; selectGameVariant(0); launchSelectedGame(false);")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests[1].method")).toString(), QStringLiteral("catalog.launch.inspect"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests[1].params.variantId")).toString(), QStringLiteral("1001"));
        QVERIFY(!engine.evaluate(QStringLiteral("pendingLaunchParams.accountLinked")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral(
            "launchInspectRequestId=''; storeLaunchTarget={appId:'parent',variantId:'1001'}; launchSelectedGame(false);")).isError());
        QVERIFY(engine.evaluate(QStringLiteral("requests[2].params.storeLaunch")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral(
            "launchInspectRequestId=''; pendingLaunchParams=null; storeLaunchTarget={appId:'parent',variantId:'1003'}; selectGameVariant(0); launchSelectedGame(false);")).isError());
        QVERIFY(engine.evaluate(QStringLiteral("requests[3].params.storeLaunch")).isUndefined());
    }

    void existingSessionLaunchFlow_data()
    {
        QTest::addColumn<QString>("sessions");
        QTest::addColumn<bool>("conflictDetected");
        QTest::addColumn<QString>("expectedState");
        QTest::addColumn<QString>("expectedMethod");
        QTest::newRow("new-game") << QStringLiteral("[]") << false
            << QStringLiteral("requesting") << QStringLiteral("catalog.launch.inspect");
        QTest::newRow("same-game") << QStringLiteral(R"([{sessionId:'same',appId:'123',streamingBaseUrl:'https://region'}])") << false
            << QStringLiteral("resuming") << QStringLiteral("session.claim");
        QTest::newRow("same-game-after-conflict") << QStringLiteral(R"([{sessionId:'same',appId:123,streamingBaseUrl:'https://region'}])") << true
            << QStringLiteral("resuming") << QStringLiteral("session.claim");
        QTest::newRow("same-game-in-second-region") << QStringLiteral(R"([{sessionId:'other',appId:456},{sessionId:'same',appId:123,streamingBaseUrl:'https://region'}])") << false
            << QStringLiteral("resuming") << QStringLiteral("session.claim");
        QTest::newRow("different-game") << QStringLiteral(R"([{sessionId:'other',appId:456}])") << false
            << QStringLiteral("conflict") << QString();
        QTest::newRow("unknown-game") << QStringLiteral(R"([{sessionId:'unknown'}])") << false
            << QStringLiteral("conflict") << QString();
        QTest::newRow("conflict-not-discoverable") << QStringLiteral("[]") << true
            << QStringLiteral("error") << QString();
    }

    void existingSessionLaunchFlow()
    {
        QFETCH(QString, sessions);
        QFETCH(bool, conflictDetected);
        QFETCH(QString, expectedState);
        QFETCH(QString, expectedMethod);
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        engine.installExtensions(QJSEngine::TranslationExtension);
        for (const auto &name : {"inspectRemoteSessions", "resolveSessionConflict", "createPendingSession",
                                 "checkLaunchSessions", "handleSessionCreateFailure", "retrySessionLaunch"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(shell);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var ready = true, streamBusy = false, onboardingReplaying = false;
            var nativeRuntimeReady = true, nativeRuntimeCapabilities = {};
            var streamCreateRequestId = '', remoteSessionsRequestId = '', sessionClaimRequestId = '';
            var pendingLaunchParams = {appId:'123',title:'Selected game',catalogAppId:'parent',variantId:'123',selectionIdentity:'selection',authGeneration:0,actionGeneration:0,requestContextKey:''};
            var conflictSession = null, activeSession = null, remoteSessions = [], streamState = 'checking', streamMessage = '';
            var conflictSessionNeedsRefresh = false;
            var requests = [], overlays = [], created = 0, pollStops = 0;
            var streamPollTimer = {stop: function() { ++pollStops; }};
            var CoreClient = {request: function(method, params) {
                requests.push({method:method,params:params}); return 'request-' + requests.length;
            }};
            var AppController = {showOverlay: function(name) {overlays.push(name);},
                navigateFromLastPrimary: function() {}};
        )JS")).isError());
        engine.globalObject().setProperty(QStringLiteral("launchConflictDetected"), conflictDetected);
        const auto result = engine.evaluate(QStringLiteral("inspectRemoteSessions({sessions:%1})").arg(sessions));
        QVERIFY2(!result.isError(), qPrintable(result.toString()));
        QCOMPARE(engine.evaluate(QStringLiteral("streamState")).toString(), expectedState);
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), expectedMethod.isEmpty() ? 0 : 1);
        if (!expectedMethod.isEmpty())
            QCOMPARE(engine.evaluate(QStringLiteral("requests[0].method")).toString(), expectedMethod);
        if (expectedMethod == QStringLiteral("session.claim")) {
            QCOMPARE(engine.evaluate(QStringLiteral("requests[0].params.sessionId")).toString(), QStringLiteral("same"));
            QCOMPARE(engine.evaluate(QStringLiteral("requests[0].params.streamingBaseUrl")).toString(), QStringLiteral("https://region"));
            QVERIFY(engine.evaluate(QStringLiteral("requests[0].params.recoveryMode")).toBool());
            QVERIFY(!engine.evaluate(QStringLiteral("streamBusy = true; resolveSessionConflict('resume')")).isError());
            QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 1);
        } else if (expectedState == QStringLiteral("conflict")) {
            QCOMPARE(engine.evaluate(QStringLiteral("overlays[0]")).toString(), QStringLiteral("session-conflict"));
            QVERIFY(!engine.evaluate(QStringLiteral("resolveSessionConflict('new')")).isError());
            QCOMPARE(engine.evaluate(QStringLiteral("requests[0].method")).toString(), QStringLiteral("catalog.launch.inspect"));
            QCOMPARE(engine.evaluate(QStringLiteral("launchInspectStage")).toString(), QStringLiteral("stop"));
        } else if (conflictDetected) {
            QVERIFY(!engine.evaluate(QStringLiteral("retrySessionLaunch()")).isError());
            QCOMPARE(engine.evaluate(QStringLiteral("requests[0].method")).toString(), QStringLiteral("catalog.launch.inspect"));
            QVERIFY(!engine.evaluate(QStringLiteral("inspectRemoteSessions({sessions:[]})")).isError());
            QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 1);
        }
    }

    void sessionCreationConflictTriggersDiscoveryInsteadOfAnotherCreate()
    {
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        engine.installExtensions(QJSEngine::TranslationExtension);
        for (const auto &name : {"handleSessionCreateFailure", "checkLaunchSessions"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(shell);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var ready = true, streamBusy = false, pendingLaunchParams = {appId:'123',catalogAppId:'parent',variantId:'123',selectionIdentity:'selection',authGeneration:0,actionGeneration:0,requestContextKey:''};
            var launchConflictDetected = false, requests = [], streamState = 'requesting', streamMessage = '';
            var streamPollTimer = {stop:function(){}};
            var CoreClient = {request:function(method,params){requests.push(method);return 'discovery';}};
            handleSessionCreateFailure('session_conflict','SESSION_LIMIT_PER_DEVICE_EXCEEDED_STATUS');
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.join(',')")).toString(), QStringLiteral("catalog.launch.inspect"));
        QCOMPARE(engine.evaluate(QStringLiteral("streamState")).toString(), QStringLiteral("checking"));
        QVERIFY(engine.evaluate(QStringLiteral("launchConflictDetected")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("handleSessionCreateFailure('unauthorized','Sign in again')")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 1);
        QCOMPARE(engine.evaluate(QStringLiteral("streamMessage")).toString(), QStringLiteral("Sign in again"));
    }

    void cancelledLaunchDiscoveryCannotCreateASession()
    {
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        engine.installExtensions(QJSEngine::TranslationExtension);
        for (const auto &name : {"stopStreamingSession", "inspectRemoteSessions"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(shell);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        const auto result = engine.evaluate(QStringLiteral(R"JS(
            var remoteSessionsRequestId = 'discovery', streamCreateRequestId = 'create';
            var activeSession = null, streamPollRequestId = '', pendingLaunchParams = {appId:'123'};
            var conflictSession = {}, forceNewAfterStop = true, launchConflictDetected = true;
            var cancelled = [], creates = 0;
            function cancelSessionRecovery() {}
            function stopNativeStreamer() {}
            function createPendingSession() { ++creates; }
            var streamPollTimer = {stop:function(){}};
            var AppController = {navigateFromLastPrimary:function(){}};
            var CoreClient = {cancel:function(id) {
                if (remoteSessionsRequestId !== '' || streamCreateRequestId !== '') throw Error('live request ID');
                cancelled.push(id);
            }};
            stopStreamingSession();
            inspectRemoteSessions({sessions:[]});
        )JS"));
        QVERIFY2(!result.isError(), qPrintable(result.toString()));
        QCOMPARE(engine.evaluate(QStringLiteral("cancelled.join(',')")).toString(), QStringLiteral("discovery,create"));
        QCOMPARE(engine.evaluate(QStringLiteral("creates")).toInt(), 0);
        QVERIFY(engine.evaluate(QStringLiteral("pendingLaunchParams === null && conflictSession === null && !forceNewAfterStop")).toBool());
    }

    void cancelledMediaPreparationCannotStartOrRecover()
    {
        QJSEngine engine;
        QVERIFY(prepareAuthentication(engine));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var remoteSessionsRequestId = '', streamCreateRequestId = '', streamPollRequestId = '';
            var streamerPrepareRequestId = 'prepare', streamerStopRequestId = '', streamStopRequestId = '';
            var activeSession = {sessionId:'seat'}, streamStartedAtMs = 0, streamState = 'starting';
            var streamer = {status:'starting'}, nativeCommands = [], snapshots = 0;
            var negotiatedStreamProfile = {};
            var streamPollTimer = {stop:function(){}}, streamerRestartTimer = {stop:function(){}};
            function invalidateLaunchInspection() {}
            function cancelSessionRecovery() {}
            function acceptStreamerSnapshot() {snapshots++;}
            function prepareMicrophoneStart() {return false;}
            function sendNativeCommand(type) {nativeCommands.push(type); return 'native-' + type;}
            CoreClient.cancel = function(id) {
                if (streamerPrepareRequestId !== '') throw Error('prepare still owned during cancellation');
                cancelled.push(id);
                onRequestFailed(id, 'cancelled', 'Cancelled');
            };
        )JS")).isError());
        QVERIFY(loadShellFunction(engine, QStringLiteral("stopStreamingSession")));
        QVERIFY(loadShellFunction(engine, QStringLiteral("stopNativeStreamer")));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            stopStreamingSession();
            onResponseReceived('prepare', {context:{settings:{}},session:{sessionId:'seat'}});
            onRequestFailed('prepare', 'network_error', 'Late failure');
            activeSession = null;
            onResponseReceived('prepare', {context:{settings:{}},session:{sessionId:'seat'}});
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("cancelled.join(',')")).toString(), QStringLiteral("prepare"));
        QCOMPARE(engine.evaluate(QStringLiteral("nativeCommands.join(',')")).toString(), QStringLiteral("stop"));
        QCOMPARE(engine.evaluate(QStringLiteral("snapshots")).toInt(), 0);
        QVERIFY(engine.evaluate(QStringLiteral("streamStopRequestId !== ''")).toBool());
    }

    void preparedMediaRequiresCurrentLiveSession_data()
    {
        QTest::addColumn<QString>("transition");
        QTest::addColumn<bool>("starts");
        QTest::newRow("current") << QString() << true;
        QTest::newRow("ended") << QStringLiteral("activeSession = null;") << false;
        QTest::newRow("replaced") << QStringLiteral("activeSession = {sessionId:'new-seat'};") << false;
        QTest::newRow("cloud-stop") << QStringLiteral("streamStopRequestId = 'cloud-stop';") << false;
        QTest::newRow("native-stop") << QStringLiteral("streamerStopRequestId = 'native-stop';") << false;
        QTest::newRow("recovery") << QStringLiteral("sessionRecoveryPending = true;") << false;
        QTest::newRow("core-offline") << QStringLiteral("ready = false;") << false;
    }

    void preparedMediaRequiresCurrentLiveSession()
    {
        QFETCH(QString, transition);
        QFETCH(bool, starts);
        QJSEngine engine;
        QVERIFY(prepareAuthentication(engine));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var streamerPrepareRequestId = 'prepare', streamerStopRequestId = '', streamStopRequestId = '';
            var activeSession = {sessionId:'seat'}, sessionRecoveryPending = false, snapshots = 0;
            var negotiatedStreamProfile = {}, nativeCommands = [];
            function prepareMicrophoneStart() {return false;}
            function acceptStreamerSnapshot() {snapshots++;}
            function sendNativeCommand(type) {nativeCommands.push(type); return type;}
        )JS")).isError());
        QVERIFY(!engine.evaluate(transition).isError());
        const auto result = engine.evaluate(QStringLiteral(
            "onResponseReceived('prepare', {context:{settings:{}},session:{sessionId:'seat'}});"));
        QVERIFY2(!result.isError(), qPrintable(result.toString()));
        QCOMPARE(engine.evaluate(QStringLiteral("nativeCommands.length")).toInt(), starts ? 1 : 0);
        QCOMPARE(engine.evaluate(QStringLiteral("snapshots")).toInt(), starts ? 1 : 0);
        QCOMPARE(engine.evaluate(QStringLiteral("streamerPrepareRequestId")).toString(), QString());
    }

    void stoppingCloudSessionCannotPrepareMediaAgain()
    {
        QJSEngine engine;
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var ready = true, activeSession = {sessionId:'seat'}, sessionRecoveryPending = false;
            var streamStopRequestId = 'cloud-stop', streamerStopRequestId = '', streamerStartRequestId = '';
            var streamerPrepareRequestId = '', sessionClaimRequestId = '', streamerRecoveryExhausted = false;
            var streamerRestartTimer = {running:false}, streamer = null, nativeRuntimeReady = true;
            var nativeRuntimeCapabilities = {}, prepares = 0;
            var CoreClient = {request:function(){prepares++; return 'prepare';}};
        )JS")).isError());
        QVERIFY(loadShellFunction(engine, QStringLiteral("startNativeStreamer")));
        const auto result = engine.evaluate(QStringLiteral("startNativeStreamer();"));
        QVERIFY2(!result.isError(), qPrintable(result.toString()));
        QCOMPARE(engine.evaluate(QStringLiteral("streamerPrepareRequestId")).toString(), QString());
        QCOMPARE(engine.evaluate(QStringLiteral("prepares")).toInt(), 0);
    }

    void invalidPreparedMediaStillReportsFailure()
    {
        for (const auto &response : {"{}", "{session:{sessionId:'seat'}}", "{context:{settings:{}}}"}) {
            QJSEngine engine;
            QVERIFY(prepareAuthentication(engine));
            QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
                var streamerPrepareRequestId = 'prepare', streamerStopRequestId = '', streamStopRequestId = '';
                var activeSession = {sessionId:'seat'}, sessionRecoveryPending = false, failure = '';
                function acceptStreamerSnapshot(snapshot) {failure = snapshot.errorCode;}
            )JS")).isError());
            const auto result = engine.evaluate(QStringLiteral("onResponseReceived('prepare', %1);")
                .arg(QString::fromLatin1(response)));
            QVERIFY2(!result.isError(), qPrintable(result.toString()));
            QCOMPARE(engine.evaluate(QStringLiteral("failure")).toString(), QStringLiteral("invalid_stream_context"));
        }
    }

    void storePresentationRestartsAfterContextReset_data()
    {
        QTest::addColumn<int>("completedSections");
        QTest::newRow("marquee-loaded") << 1;
        QTest::newRow("panels-loaded") << 2;
        QTest::newRow("all-loaded") << 3;
    }

    void storePresentationRestartsAfterContextReset()
    {
        QFETCH(int, completedSections);
        QJSEngine engine;
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var root = this, ready = true, storeSource = 'store-browse';
            var storePresentationIndex = 0, storePresentationRequestId = 'old-presentation';
            var storeRequestId = '', storeShelfCache = [], storeShelfEpoch = 0;
            var storePageTimer = {stop:function(){}}, requests = [], cancelled = [];
            var coreClient = {cancel:function(id) {
                if (storePresentationRequestId !== '') throw Error('old presentation still owned');
                cancelled.push(id);
            }, request:function(method, params) {requests.push(params.section); return params.section;}};
            function storeSessionReset() {}
        )JS")).isError());
        engine.globalObject().setProperty(QStringLiteral("storePresentationIndex"), completedSections);
        const auto catalog = source(QStringLiteral("qml/state/catalog/CatalogState.qml"));
        for (const auto &name : {"reloadStoreForSession", "resetStoreShelves", "cancelStoreRequests",
                 "requestStorePresentation", "acceptStorePresentation"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(catalog);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        const auto result = engine.evaluate(QStringLiteral(R"JS(
            reloadStoreForSession();
            requestStorePresentation();
            acceptStorePresentation({section:'marquee', items:[{title:'New account'}]});
            acceptStorePresentation({section:'panels', items:[{title:'New shelves'}]});
            acceptStorePresentation({section:'filters', items:[{title:'New filters'}]});
        )JS"));
        QVERIFY2(!result.isError(), qPrintable(result.toString()));
        QCOMPARE(engine.evaluate(QStringLiteral("requests.join(',')")).toString(), QStringLiteral("marquee,panels,filters"));
        QCOMPARE(engine.evaluate(QStringLiteral("cancelled.join(',')")).toString(), QStringLiteral("old-presentation"));
        QCOMPARE(engine.evaluate(QStringLiteral("storePresentationIndex")).toInt(), 3);
        QCOMPARE(engine.evaluate(QStringLiteral("storeMarquee[0].title")).toString(), QStringLiteral("New account"));
        QCOMPARE(engine.evaluate(QStringLiteral("storePanels[0].title")).toString(), QStringLiteral("New shelves"));
        QCOMPARE(engine.evaluate(QStringLiteral("storeFilterGroups[0].title")).toString(), QStringLiteral("New filters"));
    }

    void consoleDestructiveControlsRequireConfirmation()
    {
        QJSEngine engine;
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var root = this, stops = 0;
            var AppController = {route:'stream', overlay:'guide-session', showOverlay:function(value){this.overlay=value;}};
            function stopStreamingSession() {stops++;}
            function qsTr(value) {return value;}
            var ShellStore = root;
        )JS")).isError());
        QVERIFY(loadShellFunction(engine, QStringLiteral("requestStreamExitConfirmation")));
        const auto guide = source(QStringLiteral("qml/overlays/GuideOverlay.qml"));
        const auto activate = QRegularExpression(QStringLiteral("    function activate\\(action\\) \\{.*?\\n    \\}"),
            QRegularExpression::DotMatchesEverythingOption).match(guide);
        QVERIFY(activate.hasMatch());
        QVERIFY(!engine.evaluate(activate.captured()).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("activate('end');")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("stops")).toInt(), 0);
        QCOMPARE(engine.evaluate(QStringLiteral("AppController.overlay")).toString(), QStringLiteral("desktop-stream-exit-confirm"));

        const auto screen = source(QStringLiteral("qml/screens/StreamScreen.qml"));
        const auto clicked = QRegularExpression(QStringLiteral(
            "text: qsTr\\(\"Stop session\"\\);[^\\n]*?onClicked: ([^;]+);")).match(screen);
        QVERIFY(clicked.hasMatch());
        QVERIFY(!engine.evaluate(QStringLiteral("AppController.overlay = ''; ") + clicked.captured(1)).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("stops")).toInt(), 0);
        QCOMPARE(engine.evaluate(QStringLiteral("AppController.overlay")).toString(), QStringLiteral("desktop-stream-exit-confirm"));
    }

    void dismissingSessionConflictPreservesTheRunningGame()
    {
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        const auto overlay = source(QStringLiteral("qml/overlays/OverlayHost.qml"));
        const auto resolve = QRegularExpression(QStringLiteral(
            "    function resolveSessionConflict\\([^\\n]*\\) \\{.*?\\n    \\}"),
            QRegularExpression::DotMatchesEverythingOption).match(shell);
        const auto keyHandler = QRegularExpression(QStringLiteral(
            "    Keys.onPressed: event => \\{(.*?)\\n    \\}"),
            QRegularExpression::DotMatchesEverythingOption).match(overlay);
        QVERIFY(resolve.hasMatch());
        QVERIFY(keyHandler.hasMatch());
        for (const int key : {Qt::Key_Escape, Qt::Key_Back}) {
            QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
            engine.installExtensions(QJSEngine::TranslationExtension);
            QVERIFY(!engine.evaluate(resolve.captured()).isError());
            QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
                var streamBusy = false, pendingLaunchParams = {appId:'123'}, conflictSession = {sessionId:'running'};
                var remoteSessions = [conflictSession], launchConflictDetected = true, requests = 0, left = false;
                var AppController = {showOverlay:function(){},navigateFromLastPrimary:function(){left=true;}};
                var CoreClient = {request:function(){++requests;}};
                var ShellStore = {resolveSessionConflict:resolveSessionConflict};
                var root = {presentedOverlay:'session-conflict'};
            )JS")).isError());
            QVERIFY(!engine.evaluate(QStringLiteral("var Qt = {Key_Escape:%1,Key_Back:%2}; var event = {key:%3,accepted:false};")
                .arg(Qt::Key_Escape).arg(Qt::Key_Back).arg(key)).isError());
            const auto result = engine.evaluate(keyHandler.captured(1));
            QVERIFY2(!result.isError(), qPrintable(result.toString()));
            QVERIFY(engine.evaluate(QStringLiteral("event.accepted && left && pendingLaunchParams === null && conflictSession === null")).toBool());
            QCOMPARE(engine.evaluate(QStringLiteral("requests")).toInt(), 0);
            QCOMPARE(engine.evaluate(QStringLiteral("remoteSessions[0].sessionId")).toString(), QStringLiteral("running"));
        }
    }

    void accountSessionDiscoveryDoesNotOwnTheLaunchSlotOrFailTheLaunch()
    {
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        engine.installExtensions(QJSEngine::TranslationExtension);
        for (const auto &name : {"refreshAccountServices", "refreshRemoteSessions", "launchSelectedGame"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(shell);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        const auto busy = QRegularExpression(QStringLiteral(
            "    readonly property bool streamBusy: (.*?)\\n\\n"),
            QRegularExpression::DotMatchesEverythingOption).match(shell);
        const auto failed = QRegularExpression(QStringLiteral(
            "        function onRequestFailed\\([^\\n]*\\) \\{(.*?)\\n        \\}"),
            QRegularExpression::DotMatchesEverythingOption).match(shell.section(QStringLiteral("property Connections coreConnections:"), 1));
        QVERIFY(busy.hasMatch());
        QVERIFY(failed.hasMatch());
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var root = this, ready = true, signedIn = true, activeSession = null, pendingLaunchParams = null;
            var remoteSessionsRequestId = '', remoteSessionDiscoveryRequestId = '', remoteSessions = [];
            var streamCreateRequestId = '', streamStopRequestId = '', sessionClaimRequestId = '';
            var subscriptionRequestId = 'subscription', regionsRequestId = 'regions', accountsRequestId = 'accounts';
            var gameAccountsRequestId = 'connections', streamState = 'idle', streamMessage = '', lastError = '';
            var requests = [], selectedGame = {title:'Game'}, settings = {}, selectedRegion = '', regions = [], onboardingReplaying = false;
            var storeLaunchTarget = null;
            var CoreClient = {request:function(method,params){requests.push(method);return 'request-'+requests.length;}};
            var AppController = {navigate:function(){}};
            var onboardingOwner = {acceptFailure:function(){return false;}};
            var settingsOwner = {acceptFailure:function(){return false;}};
            function finishArtworkRequest(){return false;}
            function selectedLaunchAppId(){return '123';}
            function selectedGameMembershipError(){return '';}
        )JS")).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("Object.defineProperty(this,'streamBusy',{get:function(){return %1;}});")
            .arg(busy.captured(1))).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("function fail(requestId,code,message){%1}").arg(failed.captured(1))).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("refreshAccountServices()")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("remoteSessionDiscoveryRequestId")).toString(), QStringLiteral("request-1"));
        QVERIFY(!engine.evaluate(QStringLiteral("streamBusy")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("fail('request-1','session_discovery_failed','Background lookup failed')")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("streamState")).toString(), QStringLiteral("idle"));
        QCOMPARE(engine.evaluate(QStringLiteral("lastError")).toString(), QString());
        QVERIFY(!engine.evaluate(QStringLiteral("refreshAccountServices(); launchSelectedGame(false)")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("streamState")).toString(), QStringLiteral("checking"));
        QCOMPARE(engine.evaluate(QStringLiteral("launchInspectRequestId")).toString(), QStringLiteral("request-3"));
        QCOMPARE(engine.evaluate(QStringLiteral("remoteSessionsRequestId")).toString(), QString());
        QVERIFY(engine.evaluate(QStringLiteral("streamBusy")).toBool());
    }

    void failedResumeRefreshesTheChosenSessionBeforeClaimingAgain()
    {
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        engine.installExtensions(QJSEngine::TranslationExtension);
        for (const auto &name : {"resolveSessionConflict", "inspectRemoteSessions"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(shell);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        const auto result = engine.evaluate(QStringLiteral(R"JS(
            var streamBusy = false, conflictSessionNeedsRefresh = true, pendingLaunchParams = null;
            var conflictSession = {sessionId:'chosen',appId:'456',streamingBaseUrl:'https://old.nvidiagrid.net'};
            var requests = [], creates = 0;
            var AppController = {showOverlay:function(){}};
            var CoreClient = {request:function(method,params){requests.push({method:method,params:params});return 'request';}};
            function createPendingSession(){++creates;}
            resolveSessionConflict('resume');
            inspectRemoteSessions({sessions:[]});
        )JS"));
        QVERIFY2(!result.isError(), qPrintable(result.toString()));
        QCOMPARE(engine.evaluate(QStringLiteral("requests[0].method")).toString(), QStringLiteral("session.remote.list"));
        QCOMPARE(engine.evaluate(QStringLiteral("streamState")).toString(), QStringLiteral("error"));
        QCOMPARE(engine.evaluate(QStringLiteral("creates")).toInt(), 0);
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            inspectRemoteSessions({sessions:[{sessionId:'other',appId:'123'},
                {sessionId:'chosen',appId:'456',streamingBaseUrl:'https://fresh.nvidiagrid.net'}]});
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests[1].method")).toString(), QStringLiteral("session.claim"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests[1].params.sessionId")).toString(), QStringLiteral("chosen"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests[1].params.streamingBaseUrl")).toString(), QStringLiteral("https://fresh.nvidiagrid.net"));
        QVERIFY(!engine.evaluate(QStringLiteral("conflictSessionNeedsRefresh")).toBool());
    }

    void pausedSessionsRemainAvailableToResume()
    {
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        const auto binding = QRegularExpression(QStringLiteral(
            "readonly property var resumableSession: \\{(.*?)\\n    \\}"),
            QRegularExpression::DotMatchesEverythingOption).match(shell);
        QVERIFY(binding.hasMatch());
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        QVERIFY(!engine.evaluate(QStringLiteral("function selectedSession() {%1}")
                                     .arg(binding.captured(1))).isError());
        for (int status = 1; status <= 7; ++status) {
            for (const bool local : {false, true}) {
                QVERIFY(!engine.evaluate(QStringLiteral(
                    "var seat = {sessionId: 'seat', status: %1};"
                    "var root = {activeSession: %2, remoteSessions: %3};")
                    .arg(status)
                    .arg(local ? QStringLiteral("seat") : QStringLiteral("null"))
                    .arg(local ? QStringLiteral("[]") : QStringLiteral("[seat]"))).isError());
                const auto result = engine.evaluate(QStringLiteral("selectedSession()"));
                QVERIFY2(!result.isError(), qPrintable(result.toString()));
                QCOMPARE(!result.isNull(), status >= 2 && status <= 5);
                if (!result.isNull())
                    QCOMPARE(result.property(QStringLiteral("sessionId")).toString(), QStringLiteral("seat"));
            }
        }
    }

    void updaterPollsManagedPendingWithoutRequiringAnotherApplicationRestart()
    {
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        QVERIFY(initializeUpdaterEngine(engine));
        QVERIFY(!engine.evaluate(QStringLiteral("acceptUpdaterState({status:'managed-pending',canCheck:false})")).isError());
        QVERIFY(engine.evaluate(QStringLiteral("updaterNeedsReconciliation")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("updaterInstallConfirmed")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("acceptUpdaterState({status:'failed',canCheck:true})")).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("updaterNeedsReconciliation")).toBool());
        QVERIFY(engine.evaluate(QStringLiteral("updaterState.canCheck")).toBool());
        QCOMPARE(engine.evaluate(QStringLiteral("callbacks.length")).toInt(), 0);
    }

    void updaterFreshStartupPollsUntilHelperReportsItsOutcome()
    {
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        QVERIFY(initializeUpdaterEngine(engine));
        QVERIFY(!engine.evaluate(QStringLiteral("acceptUpdaterState({status:'restarting',canCheck:false})")).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("updaterInstallConfirmed")).toBool());
        QVERIFY(engine.evaluate(QStringLiteral("updaterNeedsReconciliation")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("acceptUpdaterState({status:'succeeded',canCheck:true})")).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("updaterNeedsReconciliation")).toBool());
        QCOMPARE(engine.evaluate(QStringLiteral("callbacks.length")).toInt(), 0);
    }

    void updaterReconciliationPreservesRejectedOperationFeedback()
    {
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        QVERIFY(initializeUpdaterEngine(engine));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            reconcileUpdaterFailure('End your active session first');
            acceptUpdaterState({status:'downloaded',canInstall:true,canCheck:true});
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("updaterError")).toString(), QStringLiteral("End your active session first"));
        QVERIFY(!engine.evaluate(QStringLiteral("installUpdate(true)")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("updaterError")).toString(), QString{});
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            reconcileUpdaterFailure('Preparation timed out');
            acceptUpdaterState({status:'awaiting-exit',exitRequired:true});
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("updaterError")).toString(), QString{});
    }

    void updaterRequiresConsentAndAuthoritativeExit()
    {
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        QVERIFY(initializeUpdaterEngine(engine));
        QVERIFY(!engine.evaluate(QStringLiteral("installUpdate(); installUpdate(false)")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 0);
        QVERIFY(!engine.evaluate(QStringLiteral("installUpdate(true)")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests[0].method")).toString(), QStringLiteral("updater.install"));
        QVERIFY(engine.evaluate(QStringLiteral("requests[0].params.confirmed")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("acceptUpdaterState({status:'preparing',exitRequired:true})")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("callbacks.length")).toInt(), 0);
        QVERIFY(!engine.evaluate(QStringLiteral("acceptUpdaterState({status:'awaiting-exit',exitRequired:false})")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("callbacks.length")).toInt(), 0);
        QVERIFY(!engine.evaluate(QStringLiteral("acceptUpdaterState({status:'awaiting-exit',exitRequired:true}); callbacks.shift()()")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("quits")).toInt(), 1);
        QVERIFY(initializeUpdaterEngine(engine));
        QVERIFY(!engine.evaluate(QStringLiteral("acceptUpdaterState({status:'awaiting-exit',exitRequired:true})")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("callbacks.length")).toInt(), 0);
        QVERIFY(!engine.evaluate(QStringLiteral("updaterInstallConfirmed = true; acceptUpdaterState({status:'awaiting-exit',exitRequired:true}); activeSession = {sessionId:'new'}; callbacks.shift()()")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("quits")).toInt(), 0);
    }

    void updaterDefersForSessions_data()
    {
        QTest::addColumn<QString>("sessionState");
        QTest::newRow("active") << QStringLiteral("activeSession = {sessionId:'live'}");
        QTest::newRow("starting") << QStringLiteral("streamerStartRequestId = 'start'");
        QTest::newRow("recovering") << QStringLiteral("sessionRecoveryPending = true");
        QTest::newRow("discovering") << QStringLiteral("activeSessionRequestId = 'discover'");
        QTest::newRow("queued-launch") << QStringLiteral("pendingLaunchParams = {appId:'1'}");
        QTest::newRow("streaming") << QStringLiteral("streamerStatus = 'streaming'");
        QTest::newRow("negotiating-streamer") << QStringLiteral("streamerStatus = 'negotiating'");
        QTest::newRow("recovering-streamer") << QStringLiteral("streamerStatus = 'recovering'");
        QTest::newRow("unknown-streamer") << QStringLiteral("streamerStatus = 'unknown'");
    }

    void updaterDefersForSessions()
    {
        QFETCH(QString, sessionState);
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        QVERIFY(initializeUpdaterEngine(engine));
        QVERIFY(!engine.evaluate(sessionState).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("installUpdate(true); runAutomaticUpdates()")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 0);
        QVERIFY(!engine.evaluate(QStringLiteral("updaterInstallConfirmed = true; acceptUpdaterState({status:'awaiting-exit',exitRequired:true})")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("callbacks.length")).toInt(), 0);
    }

    void updaterReconcilesTimeoutWithoutInventingCapabilities()
    {
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        QVERIFY(initializeUpdaterEngine(engine));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            installUpdate(true);
            acceptUpdaterState({status:'preparing',canInstall:false,canCheck:false});
            updaterInstallRequestId = '';
            reconcileUpdaterFailure('Timed out');
            installUpdate(true);
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 2);
        QCOMPARE(engine.evaluate(QStringLiteral("requests[1].method")).toString(), QStringLiteral("updater.state.get"));
        QVERIFY(engine.evaluate(QStringLiteral("updaterInstallConfirmed")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("updaterState.canInstall")).toBool());
        QVERIFY(!engine.evaluate(QStringLiteral("acceptUpdaterState({status:'awaiting-exit',exitRequired:true}); callbacks.shift()()")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("quits")).toInt(), 1);

        QVERIFY(initializeUpdaterEngine(engine));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            installUpdate(true); updaterInstallRequestId = '';
            reconcileUpdaterFailure('Failed');
            acceptUpdaterState({status:'failed',canCheck:true,canDownload:false,canInstall:true});
            installUpdate(true);
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 3);
        QCOMPARE(engine.evaluate(QStringLiteral("requests[2].method")).toString(), QStringLiteral("updater.install"));
        QCOMPARE(engine.evaluate(QStringLiteral("quits")).toInt(), 0);
    }

    void updaterBackgroundPreferencesAreIndependent()
    {
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        QVERIFY(initializeUpdaterEngine(engine));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            settings = {autoCheckForUpdates:false,autoDownloadUpdates:true};
            updaterState = {status:'available',canCheck:true,canDownload:true,availableVersion:'2'};
            runAutomaticUpdates();
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests[0].method")).toString(), QStringLiteral("updater.download"));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            updaterDownloadRequestId = ''; runAutomaticUpdates();
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 1);
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            settings = {autoCheckForUpdates:true,autoDownloadUpdates:false};
            runAutomaticUpdates(); updaterCheckRequestId = ''; runAutomaticUpdates();
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 2);
        QCOMPARE(engine.evaluate(QStringLiteral("requests[1].method")).toString(), QStringLiteral("updater.check"));
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        const auto highlights = shell.mid(shell.indexOf(QStringLiteral("else if (name === \"updater.highlights.show\")")));
        QVERIFY(!highlights.contains(QStringLiteral("AppController.navigate")));
        for (const auto &path : {"qml/screens/UpdateScreen.qml", "qml/desktop/updates/DesktopUpdateScreen.qml"}) {
            const auto screen = source(QString::fromLatin1(path));
            QVERIFY(screen.contains(QStringLiteral("onAccepted: ShellStore.installUpdate(true)")));
            QVERIFY(screen.contains(QStringLiteral("onClicked: installConfirmation.open()")));
        }
    }

    void updaterInstallationFailuresRemainVisibleAfterBackgroundChecks()
    {
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        QVERIFY(initializeUpdaterEngine(engine));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            acceptUpdaterState({status:'failed',message:'Authorization failed',canCheck:true});
            acceptUpdaterState({status:'checking',message:'Checking releases'});
            acceptUpdaterState({status:'not-available',message:'Up to date',canCheck:true});
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("updaterFailureMessage")).toString(), QStringLiteral("Authorization failed"));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            updaterFailureMessage = '';
            acceptUpdaterState({status:'rolled-back',message:'Previous version restored'});
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("updaterFailureMessage")).toString(), QStringLiteral("Previous version restored"));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            updaterFailureMessage = '';
            acceptUpdaterState({status:'rolled-back',message:'Previous version restored'});
            acceptUpdaterState({status:'error',message:'Network unavailable'});
        )JS")).isError());
        QVERIFY(engine.evaluate(QStringLiteral("updaterFailureMessage")).toString().isEmpty());
    }

    void updaterRetriesTransientDownloadsWithBoundedBackoff()
    {
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        QVERIFY(initializeUpdaterEngine(engine));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var now = 1000000;
            Date.now = function() { return now; };
            settings = {autoCheckForUpdates:false,autoDownloadUpdates:true};
            updaterState = {status:'available',canCheck:true,canDownload:true,availableVersion:'2'};
            runAutomaticUpdates();
            updaterDownloadRequestId = '';
            runAutomaticUpdates();
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 1);
        QVERIFY(!engine.evaluate(QStringLiteral("now += 60000; runAutomaticUpdates(); updaterDownloadRequestId = ''; runAutomaticUpdates();")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 2);
        QVERIFY(!engine.evaluate(QStringLiteral("now += 60000; runAutomaticUpdates();")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 2);
        QVERIFY(!engine.evaluate(QStringLiteral("now += 60000; runAutomaticUpdates(); updaterDownloadRequestId = ''; now += 21600000; runAutomaticUpdates();")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 3);
        QVERIFY(!engine.evaluate(QStringLiteral("checkForUpdates(); updaterCheckRequestId = ''; runAutomaticUpdates();")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests[3].method")).toString(), QStringLiteral("updater.check"));
        QCOMPARE(engine.evaluate(QStringLiteral("requests[4].method")).toString(), QStringLiteral("updater.download"));
    }

    void updaterRetriesFailedChecksBeforeTheNormalSixHourInterval()
    {
        QJSEngine engine;
        QVERIFY(prepareLaunchGuards(engine));
        QVERIFY(initializeUpdaterEngine(engine));
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var now = 1000000;
            Date.now = function() { return now; };
            settings = {autoCheckForUpdates:true,autoDownloadUpdates:false};
            updaterState = {status:'error',canCheck:true};
            runAutomaticUpdates(); updaterCheckRequestId = '';
            now += 60000; runAutomaticUpdates();
        )JS")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 1);
        QVERIFY(!engine.evaluate(QStringLiteral("now += 240000; runAutomaticUpdates();")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 2);
    }

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
        QVERIFY(prepareLaunchGuards(engine));
        engine.installExtensions(QJSEngine::TranslationExtension);
        for (const auto &name : {"selectedLaunchAppId", "launchSelectedGame"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(shell);
            QVERIFY(match.hasMatch());
            QVERIFY(!engine.evaluate(match.captured()).isError());
        }
        engine.globalObject().setProperty(QStringLiteral("requiredTier"), requiredTier);
        QVERIFY(!engine.evaluate(QStringLiteral(R"JS(
            var signedIn = true, ready = true, streamBusy = false, onboardingReplaying = false;
            var root=this, selectedGame = {id:'parent',launchAppId: "123", title: "Test", membershipTierLabel: requiredTier, variants:[{id:'123',inLibrary:true}]};
            var authSession = {user: {membershipTier: "FREE"}};
            var subscriptionRequestId = "", streamState = "idle", streamMessage = "", lastError = "";
            var settings = {}, selectedRegion = '', regions = [], pendingLaunchParams = null;
            var requests = [], routes = [];
            var storeLaunchTarget = null;
            var CoreClient = {request: function(method, params) {
                requests.push({method: method, params: params}); return "request-" + requests.length;
            }};
            var AppController = {navigate: function(route) { routes.push(route); }, navigateFromLastPrimary: function(route) {routes.push(route);}};
            function matchesAuthScope(scope) {return true;}
        )JS")).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("var subscription = ") + subscriptionJson).isError());
        QVERIFY(!engine.evaluate(QStringLiteral("launchSelectedGame(false)")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 1);
        QCOMPARE(engine.evaluate(QStringLiteral("requests[0].method")).toString(), QStringLiteral("catalog.launch.inspect"));
        QCOMPARE(engine.evaluate(QStringLiteral("routes.length")).toInt(), 0);
        const auto handler = QRegularExpression(QStringLiteral(
            "        function onResponseReceived\\([^\\n]*\\) \\{.*?\\n        \\}"),
            QRegularExpression::DotMatchesEverythingOption).match(shell.section(QStringLiteral("property Connections launchInspectionResponses:"), 1));
        QVERIFY(handler.hasMatch());
        QVERIFY(!engine.evaluate(handler.captured()).isError());
        engine.globalObject().setProperty(QStringLiteral("decisionStatus"), allowed ? QStringLiteral("ready") : refreshMembership ? QStringLiteral("metadata_unconfirmed") : QStringLiteral("subscription_required"));
        QVERIFY(!engine.evaluate(QStringLiteral("catalogOwner.adoptGame=function(game){}; onResponseReceived('request-1',{appId:'parent',variantId:'123',game:selectedGame,decision:{status:decisionStatus,message:'Membership checked by core'}})")).isError());
        QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), allowed ? 2 : 1);
        QCOMPARE(engine.evaluate(QStringLiteral("streamState")).toString(), allowed ? QStringLiteral("checking") : QStringLiteral("error"));
        QCOMPARE(engine.evaluate(QStringLiteral("pendingLaunchParams !== null")).toBool(), allowed);
        QCOMPARE(engine.evaluate(QStringLiteral("routes[0]")).toString(), allowed ? QStringLiteral("inserting") : QStringLiteral("game-detail"));
        if (!allowed) {
            QVERIFY(!engine.evaluate(QStringLiteral("streamMessage")).toString().isEmpty());
            QCOMPARE(engine.evaluate(QStringLiteral("lastError")).toString(),
                     engine.evaluate(QStringLiteral("streamMessage")).toString());
            QVERIFY(!engine.evaluate(QStringLiteral("launchSelectedGame(true)")).isError());
            QCOMPARE(engine.evaluate(QStringLiteral("requests.length")).toInt(), 2);
            QVERIFY(!engine.evaluate(QStringLiteral(
                "subscription = {membershipTier: 'ULTIMATE'}; launchSelectedGame(true)")).isError());
            QCOMPARE(engine.evaluate(QStringLiteral("streamState")).toString(), QStringLiteral("checking"));
            QCOMPARE(engine.evaluate(QStringLiteral("requests[requests.length - 1].method")).toString(),
                     QStringLiteral("catalog.launch.inspect"));
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
        QVERIFY(prepareLaunchGuards(engine));
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
        QVERIFY(prepareLaunchGuards(engine));
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
        QVERIFY(prepareLaunchGuards(engine));
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
        QVERIFY(prepareLaunchGuards(engine));
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
        QVERIFY(prepareLaunchGuards(engine));
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
        QVERIFY(prepareLaunchGuards(engine));
        auto evaluate = [&](const QString &script) {
            const auto result = engine.evaluate(script);
            if (result.isError())
                qWarning().noquote() << result.toString() << result.property("stack").toString();
            return result;
        };
        QVERIFY(!evaluate(QStringLiteral(R"JS(
            var ready = true, activeSession = {sessionId: 'seat', phase: 'ready'};
            var streamColorProfileObserved = false, streamRequestedColorQuality = '', streamColorFormat = null;
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
            var CoreClient = {cancel: function() {}, request: function(type) {
                if (type === 'streamer.prepare') { prepares++; return 'prepare'; }
                if (type === 'session.claim') { claims++; return 'claim'; }
                if (type === 'session.poll') { discoveries++; return 'discovery'; }
                throw new Error('Unexpected request: ' + type);
            }};
            var AppController = {route: 'stream', overlay: '', showOverlay: function(value) {this.overlay = value;},
                navigateFromLastPrimary: function(value) {this.route = value;}};
            function resetStreamReplay() {}
            function stopNativeStreamer() {streamerStopExpected = true;}
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
                                "isRemoteSessionTermination", "finishRemoteSession", "cancelSessionRecovery",
                                "startNativeStreamer", "retryNativeStreamer", "acceptNativeEvent",
                                "observeNegotiatedColorFormat", "acceptStreamColorFormat"}) {
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
                acceptRecoverySessions({session: activeSession});
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
        QVERIFY(check(QStringLiteral(R"JS(
            activeSession = {sessionId: 'seat', status: 3};
            sessionRecoveryPending = true; recoverySessionId = 'seat';
            var oldClaims = claims;
            acceptRecoverySessions({session: {sessionId:'seat', status:7, phase:'finished'}});
            activeSession === null && claims === oldClaims && streamState === 'idle'
                && !streamerRestartTimer.running && AppController.route === 'game-detail';
        )JS")));
        QVERIFY(check(QStringLiteral(R"JS(
            activeSession = {sessionId:'seat', status:3};
            sessionRecoveryPending = true; recoverySessionId = 'seat';
            acceptRecoverySessions({session:null, termination:{source:'cloudmatch-http', httpStatus:404,
                sessionId:'seat', resumable:false}});
            activeSession === null && claims === oldClaims && !streamerRestartTimer.running;
        )JS")));
        QVERIFY(check(QStringLiteral(R"JS(
            activeSession = {sessionId:'seat', status:3}; streamerStopExpected = false;
            streamer = {status:'error', message:'transport lost'};
            streamerRestartTimer.restart();
            acceptStreamerSnapshot({status:'stopped', termination:{source:'cloudmatch-session-status', status:7,
                sessionId:'seat', resumable:false}});
            activeSession === null && !streamerRestartTimer.running && streamState === 'idle';
        )JS")));
        QVERIFY(check(QStringLiteral(R"JS(
            activeSession = {sessionId:'seat', status:3}; streamer = {status:'streaming'};
            streamerStopExpected = false; sessionRecoveryPending = false;
            acceptStreamerSnapshot({status:'error', message:'EOF', termination:{source:'nvst-transport', resumable:null}});
            activeSession !== null && streamerRestartTimer.running;
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
            "\"stop-stream\": [String(settings.shortcutStopStream ?? \"Ctrl+Shift+Q\")]")));
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
        QVERIFY(main.contains(QStringLiteral("sequence: window.configuredStatsShortcut")));
        QVERIFY(!main.contains(QStringLiteral("sequence: \"F3\"")));
        QVERIFY(main.contains(QStringLiteral("window.configuredStatsShortcut !== \"\"")));
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
private:
    static bool initializeUpdaterEngine(QJSEngine &engine)
    {
        engine.installExtensions(QJSEngine::TranslationExtension);
        const auto shell = source(QStringLiteral("qml/state/ShellStore.qml"));
        const auto setup = engine.evaluate(QStringLiteral(R"JS(
            var ready = true, activeSession = null, streamBusy = false, sessionRecoveryPending = false;
            var activeSessionRequestId = '', sessionClaimRequestId = '', streamCreateRequestId = '';
            var streamerStartRequestId = '', streamerPrepareRequestId = '', streamerStopRequestId = '';
            var pendingLaunchParams = null, pendingDirectLaunch = null, streamState = 'idle', streamerStatus = 'stopped';
            var updaterCheckRequestId = '', updaterDownloadRequestId = '', updaterInstallRequestId = '', updaterStateRequestId = '';
            var updaterError = '', accessibilityMessage = '', updaterInstallConfirmed = false, updaterExitScheduled = false, updaterReconciling = false;
            var updaterFailureMessage = '';
            var lastAutoUpdateCheckMs = 0, autoDownloadAttempt = '';
            var autoDownloadAttemptCount = 0, autoDownloadAttemptMs = 0;
            var updaterState = {status:'downloaded',canInstall:true,canCheck:true};
            var settings = {autoCheckForUpdates:true,autoDownloadUpdates:true};
            var requests = [], callbacks = [], quits = 0;
            var CoreClient = {request:function(method,params) { requests.push({method:method,params:params}); return 'request-' + requests.length; }};
            var AppController = {quitApplication:function() { ++quits; }};
            var Qt = {callLater:function(callback) { callbacks.push(callback); }};
        )JS"));
        if (setup.isError()) return false;
        for (const auto *name : {"updaterSessionSafe", "updaterBusy", "updaterCanInstall", "updaterNeedsReconciliation"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    readonly property bool %1: (.*?)(?=\\n    (?:property|readonly|on[A-Z]))").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(shell);
            if (!match.hasMatch()) return false;
            if (engine.evaluate(QStringLiteral("Object.defineProperty(this,'%1',{configurable:true,get:function(){return %2;}})")
                    .arg(QString::fromLatin1(name), match.captured(1))).isError()) return false;
        }
        for (const auto *name : {"installUpdate", "acceptUpdaterState", "refreshUpdaterState", "reconcileUpdaterFailure", "runAutomaticUpdates", "checkForUpdates", "downloadUpdate"}) {
            const auto match = QRegularExpression(QStringLiteral(
                "    function %1\\([^\\n]*\\) \\{.*?\\n    \\}").arg(QString::fromLatin1(name)),
                QRegularExpression::DotMatchesEverythingOption).match(shell);
            if (!match.hasMatch() || engine.evaluate(match.captured()).isError()) return false;
        }
        return true;
    }
};

QTEST_MAIN(EmbeddedOrchestrationTest)
#include "tst_embeddedorchestration.moc"
