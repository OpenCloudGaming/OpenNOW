import QtQuick
import QtTest
import OpenNOW.OnboardingTests

TestCase {
    id: testCase
    name: "OnboardingReplay"

    QtObject {
        id: core
        property var calls: []
        property bool refuse: false
        function request(method, params) {
            const id = "replay-" + (calls.length + 1)
            calls = calls.concat([{id: id, method: method, params: params}])
            return refuse ? "" : id
        }
    }

    OnboardingState {
        id: state
        coreClient: core
        persistedSettings: ({onboardingCompleted: true, resolution: "2560x1440"})
        ready: true
        signedIn: true
        checkRequirements: () => ""
        replayAllowed: true
    }

    SignalSpy { id: restart; target: state; signalName: "restartRequested" }
    SignalSpy { id: saved; target: state; signalName: "settingSaved" }

    function init() {
        state.failReplay("")
        state.ready = true
        state.replayAllowed = true
        core.calls = []
        core.refuse = false
        restart.clear()
        saved.clear()
    }

    function test_restartWaitsForPersistedFalse() {
        state.replay()
        verify(state.replaying)
        compare(core.calls.length, 1)
        compare(core.calls[0].method, "settings.set")
        compare(core.calls[0].params, {key: "onboardingCompleted", value: false})
        compare(restart.count, 0)
        state.replay()
        state.finish()
        compare(core.calls.length, 1)
        verify(!state.acceptResponse("unrelated", {value: false}))
        verify(state.acceptResponse(core.calls[0].id, {value: false}))
        compare(restart.count, 1)
        compare(saved.count, 0)
        verify(!state.needed)
        compare(state.persistedSettings.resolution, "2560x1440")
        state.replay()
        verify(!state.acceptResponse(core.calls[0].id, {value: false}))
        compare(core.calls.length, 1)
        compare(restart.count, 1)
    }

    function test_failedSaveStaysOpenAndCanRetry() {
        state.replay()
        verify(state.acceptFailure(core.calls[0].id, "Disk full"))
        verify(!state.replaying)
        verify(state.replayError.indexOf("Disk full") >= 0)
        compare(restart.count, 0)
        verify(!state.needed)
        state.replay()
        compare(state.replayError, "")
        verify(state.acceptResponse(core.calls[1].id, {value: false}))
        compare(restart.count, 1)
    }

    function test_disconnectRejectsLateAcknowledgement() {
        state.replay()
        const id = core.calls[0].id
        state.ready = false
        verify(!state.replaying)
        verify(state.replayError.length > 0)
        state.ready = true
        verify(!state.acceptResponse(id, {value: false}))
        compare(restart.count, 0)
    }

    function test_refusedRequestDoesNotRestart() {
        core.refuse = true
        state.replay()
        verify(!state.replaying)
        verify(state.replayError.length > 0)
        compare(restart.count, 0)
    }

    function test_invalidAcknowledgementDoesNotRestart_data() {
        return [{tag: "true", value: true}, {tag: "string", value: "false"}, {tag: "missing"}]
    }

    function test_invalidAcknowledgementDoesNotRestart(data) {
        state.replay()
        verify(state.acceptResponse(core.calls[0].id, {value: data.value}))
        verify(!state.replaying)
        verify(state.replayError.length > 0)
        compare(restart.count, 0)
    }

    function test_unavailableReplayDoesNotWrite() {
        state.replayAllowed = false
        state.replay()
        compare(core.calls.length, 0)
        state.replayAllowed = true
        state.ready = false
        state.replay()
        compare(core.calls.length, 0)
        compare(restart.count, 0)
    }
}
