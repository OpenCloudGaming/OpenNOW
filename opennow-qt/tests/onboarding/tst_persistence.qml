import QtQuick
import QtTest
import OpenNOW.OnboardingTests

TestCase {
    id: testCase
    name: "OnboardingPersistence"
    property string requirementError: ""
    property int requirementChecks: 0
    property int rejectAtCheck: 0

    QtObject {
        id: core
        property var calls: []
        property bool refuse: false
        function request(method, params) {
            const id = "request-" + (calls.length + 1)
            calls = calls.concat([{id: id, method: method, params: params}])
            return refuse ? "" : id
        }
    }

    OnboardingState {
        id: state
        coreClient: core
        persistedSettings: ({})
        ready: true
        signedIn: true
        checkRequirements: function() {
            ++testCase.requirementChecks
            return testCase.rejectAtCheck > 0 && testCase.requirementChecks >= testCase.rejectAtCheck
                ? "AWDL was re-enabled" : testCase.requirementError
        }
        onSettingSaved: (key, value, changes) => {
            const updated = Object.assign({}, persistedSettings, changes)
            updated[key] = value
            persistedSettings = updated
        }
    }

    SignalSpy { id: completion; target: state; signalName: "completed" }
    SignalSpy { id: missing; target: state; signalName: "requirementsMissing" }

    function init() {
        state.ready = false
        state.draft = ({})
        state.error = ""
        state.persistedSettings = {onboardingCompleted: false, fps: 60, frameGeneration: "off"}
        state.ready = true
        state.signedIn = true
        core.calls = []
        core.refuse = false
        requirementError = ""
        requirementChecks = 0
        rejectAtCheck = 0
        completion.clear()
        missing.clear()
    }

    function acknowledge(changes) {
        const call = core.calls[core.calls.length - 1]
        verify(state.acceptResponse(call.id, {value: call.params.value, changes: changes || {}}))
    }

    function test_onlyExplicitFirstRunIsEligible() {
        verify(state.needed)
        state.persistedSettings = ({})
        verify(!state.needed)
        state.persistedSettings = {onboardingCompleted: true}
        verify(!state.needed)
    }

    function test_stageWithoutSavingOrChangingShell() {
        state.setSetting("launchInConsoleMode", true)
        state.setSetting("fps", 120)
        compare(state.settings.launchInConsoleMode, true)
        compare(state.settings.fps, 120)
        compare(state.persistedSettings.fps, 60)
        compare(core.calls.length, 0)
    }

    function test_skipSavesCompletionWithDefaultsUntouched() {
        state.finish()
        verify(state.saving)
        compare(core.calls.length, 1)
        compare(core.calls[0].params.key, "onboardingCompleted")
        compare(completion.count, 0)
        acknowledge()
        compare(completion.count, 1)
        verify(!state.saving)
        verify(!state.needed)
        compare(state.settings.frameGeneration, "off")
    }

    function test_completionWaitsForAllChoices() {
        state.setSetting("fps", 120)
        state.setSetting("launchInConsoleMode", true)
        state.finish()
        state.finish()
        state.setSetting("fps", 240)
        compare(state.settings.fps, 120)
        compare(core.calls.length, 1)
        compare(core.calls[0].params.key, "fps")
        acknowledge()
        compare(core.calls[1].params.key, "launchInConsoleMode")
        verify(state.needed)
        acknowledge()
        compare(core.calls[2].params.key, "switchToConsoleOnPad")
        acknowledge()
        compare(core.calls[3].params.key, "onboardingCompleted")
        compare(completion.count, 0)
        acknowledge()
        compare(completion.count, 1)
        compare(state.settings.launchInConsoleMode, true)
        verify(!state.saving)
    }

    function test_failedChoiceDoesNotCompleteAndRetryKeepsDraft() {
        state.setSetting("fps", 120)
        state.finish()
        verify(state.acceptFailure(state.requestId, "Disk full"))
        verify(!state.saving)
        verify(state.error.indexOf("Disk full") >= 0)
        compare(state.settings.fps, 120)
        verify(state.needed)
        compare(completion.count, 0)
        state.finish()
        acknowledge()
        acknowledge()
        compare(completion.count, 1)
        compare(state.error, "")
    }

    function test_modeSavePreservesAutomaticSwitchRegardlessOfEditOrder() {
        state.setSetting("switchToConsoleOnPad", true)
        state.setSetting("launchInConsoleMode", false)
        state.finish()
        compare(core.calls[0].params.key, "launchInConsoleMode")
        acknowledge({switchToConsoleOnPad: false})
        compare(core.calls[1].params.key, "switchToConsoleOnPad")
        compare(core.calls[1].params.value, true)
        acknowledge()
        acknowledge()
        compare(state.persistedSettings.switchToConsoleOnPad, true)
        compare(completion.count, 1)
    }

    function test_failedCompletionCanBeRetried() {
        state.finish()
        state.acceptFailure(state.requestId, "Write failed")
        verify(state.needed)
        compare(completion.count, 0)
        state.finish()
        acknowledge()
        compare(completion.count, 1)
    }

    function test_disconnectRejectsLateResponse() {
        state.setSetting("codec", "av1")
        state.finish()
        const id = state.requestId
        state.ready = false
        verify(!state.saving)
        verify(state.error.length > 0)
        verify(!state.acceptResponse(id, {value: "av1"}))
        state.finish()
        compare(core.calls.length, 1)
        state.ready = true
        state.finish()
        acknowledge()
        acknowledge()
        compare(completion.count, 1)
    }

    function test_unrelatedResponsesAreIgnored() {
        state.finish()
        verify(!state.acceptResponse("elsewhere", {value: true}))
        verify(!state.acceptFailure("elsewhere", "unrelated"))
        verify(state.saving)
        acknowledge()
    }

    function test_signOutClearsDraftAndCancelsSave() {
        state.setSetting("fps", 120)
        state.finish()
        state.signedIn = false
        verify(!state.saving)
        compare(Object.keys(state.draft).length, 0)
        state.finish()
        compare(core.calls.length, 1)
        verify(state.needed)
    }

    function test_refusedRequestKeepsDraft() {
        core.refuse = true
        state.setSetting("fps", 120)
        state.finish()
        verify(!state.saving)
        verify(state.error.length > 0)
        compare(state.settings.fps, 120)
        compare(completion.count, 0)
    }

    function test_completionFlagCannotBeSetThroughControls() {
        state.setSetting("onboardingCompleted", true)
        state.setSetting("errorReportingConsent", "granted")
        compare(Object.keys(state.draft).length, 0)
        verify(state.needed)
    }

    function test_requirementBlocksFinishAndSkip_data() {
        return [{tag: "skip-defaults", stage: false}, {tag: "finish-choices", stage: true}]
    }

    function test_requirementBlocksFinishAndSkip(data) {
        if (data.stage)
            state.setSetting("fps", 120)
        requirementError = "Disable AWDL before finishing"
        state.finish()
        compare(core.calls.length, 0)
        compare(completion.count, 0)
        compare(missing.count, 1)
        compare(state.error, requirementError)
        verify(!state.saving && state.needed)
        compare(state.settings.fps, data.stage ? 120 : 60)
    }

    function test_requirementRecheckedBeforeCompletionWrite() {
        state.setSetting("fps", 120)
        state.finish()
        compare(core.calls.length, 1)
        compare(core.calls[0].params.key, "fps")
        requirementError = "AWDL was re-enabled"
        acknowledge()
        compare(requirementChecks, 2)
        compare(core.calls.length, 1)
        compare(completion.count, 0)
        compare(missing.count, 1)
        verify(!state.saving && state.needed)
        compare(state.draft.fps, 120)
        requirementError = ""
        state.finish()
        acknowledge()
        compare(core.calls[2].params.key, "onboardingCompleted")
        acknowledge()
        compare(completion.count, 1)
        compare(state.error, "")
    }

    function test_skipRechecksImmediatelyBeforeWrite() {
        rejectAtCheck = 2
        state.finish()
        compare(requirementChecks, 2)
        compare(core.calls.length, 0)
        compare(completion.count, 0)
        verify(!state.saving && state.needed)
    }
}
