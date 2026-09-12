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

    function controller(id, battery, power) {
        return {instanceId: id, slot: id, name: "Test controller " + id,
            family: "xbox", batteryPercent: battery, powerState: power || "onBattery"}
    }

    function init() {
        ControllerInput.controllers = []
        ShellStore.streamer = {status: "streaming"}
        ShellStore.activeSession = {sessionId: "test-session"}
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
            ShellStore.streamer = {status: "streaming", packetLossPercent: missing}
            verify(!stack.lossNotice)
            compare(stack.lossHistory.length, 0)
        }
        ShellStore.streamer = {status: "streaming", packetLossPercent: 0}
        verify(!stack.lossNotice)
        ShellStore.streamer = {status: "streaming", packetLossPercent: 0.5}
        verify(stack.lossNotice)
        compare(stack.lossHistory.length, 2)
        const toast = findChild(stack, "streamPacketLossToast")
        compare(toast.subtitle, "Packet loss · 0.5%")
        for (let i = 1; i <= 30; ++i)
            ShellStore.streamer = {status: "streaming", packetLossPercent: i}
        compare(stack.lossHistory.length, 12)
        ShellStore.streamer = {status: "streaming", packetLossPercent: 30, pingMs: 20}
        compare(stack.lossHistory.length, 12)
        compare(stack.lossHistory[11], 30)
        ShellStore.streamer = {status: "streaming", packetLossPercent: 0}
        verify(!stack.lossNotice)
        ShellStore.streamer = {status: "streaming", packetLossPercent: 2}
        verify(!stack.lossNotice)
        ShellStore.streamer = {status: "streaming", packetLossPercent: null}
        compare(stack.lossHistory.length, 0)
    }

    function test_lifetimeAndBoundedStack() {
        const stack = createTemporaryObject(stackComponent, testCase)
        verify(stack !== null)
        for (let i = 1; i <= 4; ++i)
            ControllerInput.controllers = [controller(i, 82)]
        ShellStore.streamer = {status: "streaming", packetLossPercent: 2}
        compare(stack.controllerNotice.instanceId, 4)
        tryCompare(stack, "height", 148)
        verify(!stack.activeFocus)
        tryCompare(stack, "controllerNotice", null, 5000)
        tryCompare(stack, "lossNotice", false, 1000)
        ShellStore.streamer = {status: "streaming", packetLossPercent: 3}
        verify(!stack.lossNotice)
    }

    function test_sessionAndVisibilityReset() {
        const stack = createTemporaryObject(stackComponent, testCase)
        verify(stack !== null)
        ControllerInput.controllers = [controller(1, 82)]
        ShellStore.streamer = {status: "streaming", packetLossPercent: 2}
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
}
