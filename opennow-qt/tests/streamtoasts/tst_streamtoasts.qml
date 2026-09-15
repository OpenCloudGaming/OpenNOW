import QtQuick
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "StreamToasts"
    when: windowShown
    visible: true
    width: 500; height: 220

    Component { id: stackComponent; DesktopStreamToasts { controllers: ControllerInput.controllers } }
    property real sampleTime: 100000
    Component {
        id: healthComponent
        ConnectionHealthState {
            active: ShellStore.streamer.status === "streaming" && !ShellStore.streamerStopExpected
            sessionId: ShellStore.activeSession.sessionId
            clock: () => testCase.sampleTime
        }
    }

    function sample(loss, elapsed = 100) {
        sampleTime += elapsed
        ShellStore.connectionHealth.acceptSample(loss)
    }

    function sustainedLoss(loss) {
        sample(loss)
        sample(loss, 2000)
    }

    function controller(id, battery, power) {
        return {instanceId: id, slot: id, name: "Test controller " + id,
            family: "xbox", batteryPercent: battery, powerState: power || "onBattery"}
    }

    function init() {
        sampleTime = 100000
        ControllerInput.controllers = []
        ShellStore.streamer = {status: "streaming"}
        ShellStore.activeSession = {sessionId: "test-session"}
        ShellStore.streamColorNotice = null
        ShellStore.streamColorNoticeShown = false
        ShellStore.streamerStopExpected = false
        ShellStore.connectionHealth = createTemporaryObject(healthComponent, testCase)
    }

    function test_colorFormatLifetimeAndRecovery() {
        const stack = createTemporaryObject(stackComponent, testCase)
        verify(stack !== null)
        ShellStore.streamColorNotice = {sessionId: "test-session", source: "decoder",
            requestedColorQuality: "10bit_444", actualColorQuality: "10bit_420"}
        const toast = findChild(stack, "streamColorFormatToast")
        verify(toast.visible)
        verify(!toast.activeFocus && !stack.activeFocus)
        compare(toast.subtitle, "Video output is 10-bit 4:2:0 instead of 10-bit 4:4:4.")
        verify(ShellStore.streamColorNoticeShown)
        tryCompare(stack, "colorNotice", false, 5000)
        ShellStore.streamer = {status: "reconnecting"}
        ShellStore.streamer = {status: "streaming"}
        verify(!toast.visible)
        stack.observeColorFormat()
        verify(!toast.visible)
    }

    function test_colorFormatWaitsForStreamingAndRejectsStaleSession() {
        ShellStore.streamer = {status: "starting"}
        const stack = createTemporaryObject(stackComponent, testCase)
        ShellStore.streamColorNotice = {sessionId: "old-session", source: "decoder",
            requestedColorQuality: "10bit_444", actualColorQuality: "10bit_420"}
        ShellStore.streamer = {status: "streaming"}
        verify(!stack.colorNotice)
        ShellStore.streamer = {status: "starting"}
        ShellStore.streamColorNotice = {sessionId: "test-session", source: "server",
            requestedColorQuality: "10bit_444", actualColorQuality: "10bit_420"}
        verify(!stack.colorNotice)
        ShellStore.streamer = {status: "streaming"}
        verify(stack.colorNotice)
        const toast = findChild(stack, "streamColorFormatToast")
        compare(toast.subtitle, "The server negotiated 10-bit 4:2:0 instead of 10-bit 4:4:4.")
        ShellStore.streamerStopExpected = true
        verify(!toast.visible)
    }

    function test_colorFormatWaitsForOverlayWithoutReplaying() {
        const stack = createTemporaryObject(stackComponent, testCase)
        stack.visible = false
        ShellStore.streamColorNotice = {sessionId: "test-session", source: "decoder",
            requestedColorQuality: "10bit_444", actualColorQuality: "10bit_420"}
        verify(!ShellStore.streamColorNoticeShown)
        stack.visible = true
        verify(stack.colorNotice)
        stack.visible = false
        verify(!stack.colorNotice)
        stack.visible = true
        verify(!stack.colorNotice)
    }

    function test_consoleShowsOnlyColorNotice() {
        const stack = createTemporaryObject(stackComponent, testCase)
        stack.connectionNotificationsEnabled = false
        ControllerInput.controllers = [controller(1, 82)]
        sustainedLoss(2)
        ShellStore.streamColorNotice = {sessionId: "test-session", source: "decoder",
            requestedColorQuality: "10bit_444", actualColorQuality: "10bit_420"}
        verify(findChild(stack, "streamColorFormatToast").visible)
        verify(!findChild(stack, "streamControllerToast").visible)
        verify(!findChild(stack, "streamPacketLossToast").visible)
    }

    function test_controllerChanges() {
        ControllerInput.controllers = [controller(1, 82)]
        const stack = createTemporaryObject(stackComponent, testCase)
        verify(stack !== null)
        compare(stack.controllerNotice, null)
        ControllerInput.controllers = [controller(1, 81)]
        compare(stack.controllerNotice, null)
        ControllerInput.controllers = [controller(1, 81), controller(2, 65)]
        compare(stack.controllerNotice.instanceId, 2)
        const toast = findChild(stack, "streamControllerToast")
        verify(toast.visible)
        compare(toast.batteryPercent, 65)
        verify(toast.subtitle.indexOf("Player 2") >= 0)
        for (const sample of [[0, "onBattery", 0], [100, "charged", 100],
                [-1, "onBattery", -1], [null, "onBattery", -1], [101, "onBattery", -1],
                [0, "unknown", -1], [70, "noBattery", -1]]) {
            ControllerInput.controllers = [controller(2, sample[0], sample[1])]
            compare(toast.batteryPercent, sample[2])
        }
        ControllerInput.controllers = []
        compare(stack.controllerNotice, null)
        verify(!toast.visible)
    }

    function test_packetLossHistoryAndCooldown() {
        const stack = createTemporaryObject(stackComponent, testCase)
        verify(stack !== null)
        for (const missing of [null, undefined, "", -1, 101, NaN, Infinity]) {
            sample(missing)
            verify(!stack.lossNotice)
            compare(stack.lossHistory.length, 0)
        }
        sample(0)
        verify(!stack.lossNotice)
        sample(0.5)
        verify(!stack.lossNotice)
        sample(0.5, 2000)
        verify(stack.lossNotice)
        compare(stack.lossHistory.length, 3)
        const toast = findChild(stack, "streamPacketLossToast")
        compare(toast.subtitle, "Packet loss · 0.5%")
        for (let i = 1; i <= 30; ++i)
            sample(i)
        compare(stack.lossHistory.length, 12)
        ShellStore.streamer = {status: "streaming", packetLossPercent: 30, pingMs: 20}
        compare(stack.lossHistory.length, 12)
        compare(stack.lossHistory[11], 30)
        sample(0)
        verify(!stack.lossNotice)
        sustainedLoss(2)
        verify(!stack.lossNotice)
        sample(null)
        compare(stack.lossHistory.length, 0)
    }

    function test_lifetimeAndBoundedStack() {
        const stack = createTemporaryObject(stackComponent, testCase)
        verify(stack !== null)
        for (let i = 1; i <= 4; ++i)
            ControllerInput.controllers = [controller(i, 82)]
        sustainedLoss(2)
        compare(stack.controllerNotice.instanceId, 4)
        tryCompare(stack, "height", 148)
        verify(!stack.activeFocus)
        tryCompare(stack, "controllerNotice", null, 5000)
        tryCompare(stack, "lossNotice", false, 1000)
        sample(3)
        verify(!stack.lossNotice)
    }

    function test_sessionAndVisibilityReset() {
        const stack = createTemporaryObject(stackComponent, testCase)
        verify(stack !== null)
        ControllerInput.controllers = [controller(1, 82)]
        sustainedLoss(2)
        verify(stack.lossNotice)
        ShellStore.activeSession = {sessionId: "next-session"}
        compare(stack.controllerNotice, null)
        compare(stack.lossHistory.length, 0)
        verify(!stack.lossNotice)
        ShellStore.streamer = {status: "reconnecting"}
        ControllerInput.controllers = [controller(2, 50)]
        ShellStore.streamer = {status: "streaming"}
        compare(stack.controllerNotice, null)
        ControllerInput.controllers = [controller(3, 50)]
        verify(stack.controllerNotice !== null)
        stack.visible = false
        compare(stack.controllerNotice, null)
        compare(stack.lossHistory.length, 0)
    }

    function test_missingSamplesExpireWithoutUpdates() {
        const stack = createTemporaryObject(stackComponent, testCase)
        sustainedLoss(2)
        verify(stack.lossNotice)
        sampleTime += 5000
        tryCompare(ShellStore.connectionHealth, "status", "unknown", 1000)
        verify(!stack.lossNotice)
        compare(stack.lossHistory.length, 0)
    }

    function test_reconnectPreservesCooldown() {
        const stack = createTemporaryObject(stackComponent, testCase)
        sustainedLoss(2)
        verify(stack.lossNotice)
        ShellStore.streamer = {status: "reconnecting"}
        verify(!stack.lossNotice)
        ShellStore.streamer = {status: "streaming"}
        sustainedLoss(2)
        compare(ShellStore.connectionHealth.status, "unstable")
        verify(!stack.lossNotice)
        sampleTime += 30000
        sustainedLoss(2)
        verify(stack.lossNotice)
    }

    function test_recoveryAtCooldownDoesNotClaimWarning() {
        const stack = createTemporaryObject(stackComponent, testCase)
        sustainedLoss(2)
        const noticeAt = ShellStore.connectionHealth.lastNoticeAt
        sample(0)
        sustainedLoss(2)
        while (sampleTime < noticeAt + 29000) sample(2, 1000)
        sample(0.04, noticeAt + 30000 - sampleTime)
        verify(!stack.lossNotice)
        compare(ShellStore.connectionHealth.lastNoticeAt, noticeAt)
        sustainedLoss(2)
        verify(stack.lossNotice)
    }
}
