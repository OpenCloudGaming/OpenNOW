import QtQuick
import QtQuick.Controls
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "QueueSelector"
    width: 1100
    height: 850
    when: windowShown

    QtObject {
        id: core
        property int serial: 0
        property var calls: []
        property var cancellations: []
        signal responseReceived(string id, var result)
        signal requestFailed(string id, string code, string message)
        function request(method, params, timeout) {
            calls = calls.concat([{method:method, params:params, timeout:timeout}])
            return "request-" + (++serial)
        }
        function cancel(id) { cancellations = cancellations.concat([id]) }
    }
    QtObject {
        id: settings
        property var writes: []
        function setSetting(key, value) { writes = writes.concat([{key:key, value:value}]) }
    }
    Component {
        id: stateComponent
        QueueSelectorState {
            coreClient: core
            eligible: true
            launchValid: true
            authSession: ({provider:{code:"NVIDIA"}, user:{membershipTier:"FREE"}})
            subscription: null
        }
    }
    Component {
        id: dialogComponent
        DesktopQueueSelector { settingsStore: settings }
    }
    SignalSpy { id: selections; signalName: "selected" }
    SignalSpy { id: dismissals; signalName: "dismissed" }
    property var state
    property var dialog
    readonly property var sampleLocations: [
        {zoneId:"NP-LAX-03", title:"Southern California", region:"US Southwest", queuePosition:12,
            etaMs:192000, pingMs:24, lastUpdated:1789623725, streamingBaseUrl:"https://np-lax-03.cloudmatchbeta.nvidiagrid.net/", alternateCount:1,
            zones:[
                {zoneId:"NP-LAX-03", title:"Southern California", region:"US Southwest", queuePosition:12,
                    etaMs:192000, pingMs:24, lastUpdated:1789623725, streamingBaseUrl:"https://np-lax-03.cloudmatchbeta.nvidiagrid.net/", alternateCount:0},
                {zoneId:"NP-LAX-04", title:"Southern California", region:"US Southwest", queuePosition:40,
                    etaMs:360000, pingMs:25, lastUpdated:1789623725, streamingBaseUrl:"https://np-lax-04.cloudmatchbeta.nvidiagrid.net/", alternateCount:0}
            ]},
        {zoneId:"NP-DAL-04", title:"Dallas", region:"US Central", queuePosition:3,
            etaMs:138000, pingMs:62, lastUpdated:1789623725, streamingBaseUrl:"https://np-dal-04.cloudmatchbeta.nvidiagrid.net/", alternateCount:0},
        {zoneId:"NP-PAR-01", title:"Paris", region:"EU Southwest", queuePosition:0,
            etaMs:null, pingMs:null, lastUpdated:1789623725, streamingBaseUrl:"https://np-par-01.cloudmatchbeta.nvidiagrid.net/", alternateCount:0}
    ]

    function init() {
        failOnWarning(/.*/)
        testCase.width = 1100
        testCase.height = 850
        core.calls = []
        core.cancellations = []
        settings.writes = []
        DesktopTokens.uiScale = 1
        state = createTemporaryObject(stateComponent, testCase)
        verify(state)
        selections.target = state
        dismissals.target = state
        selections.clear()
        dismissals.clear()
        dialog = createTemporaryObject(dialogComponent, testCase, {selector:state})
        verify(dialog)
    }

    function cleanup() {
        if (state.opened) state.dismiss()
        DesktopTokens.uiScale = 1
    }

    function load() {
        verify(state.begin("Cyberpunk 2077"))
        core.responseReceived(state.requestId, {locations:sampleLocations, recommendedZoneId:"NP-LAX-03"})
        tryCompare(dialog, "opened", true)
        compare(dialog.selectedZoneId, "NP-LAX-03")
    }

    function test_membership_data() {
        return [
            {tag:"free", provider:"NVIDIA", auth:"FREE", sub:"FREE", shown:true},
            {tag:"performance", provider:"NVIDIA", auth:"FREE", sub:"PERFORMANCE", shown:false},
            {tag:"ultimate", provider:"NVIDIA", auth:"ULTIMATE", sub:"", shown:false},
            {tag:"unknown", provider:"NVIDIA", auth:"", sub:"", shown:false},
            {tag:"alliance", provider:"ABYA", auth:"FREE", sub:"FREE", shown:false},
            {tag:"auth-fallback", provider:"NVIDIA", auth:"FREE", sub:"", shown:true}
        ]
    }
    function test_membership(data) {
        state.authSession = {provider:{code:data.provider}, user:{membershipTier:data.auth}}
        state.subscription = {membershipTier:data.sub}
        compare(state.begin("Game"), data.shown)
        compare(core.calls.length, data.shown ? 1 : 0)
    }
    function test_hiddenOrInvalid() {
        state.eligible = false
        verify(!state.begin("Game"))
        state.eligible = true
        state.launchValid = false
        verify(!state.begin("Game"))
        compare(core.calls.length, 0)
    }
    function test_requestLifecycle() {
        verify(state.begin("Game"))
        const id = state.requestId
        state.refresh()
        compare(core.calls.length, 1)
        state.dismiss()
        compare(core.cancellations[0], id)
        core.responseReceived(id, {locations:sampleLocations, recommendedZoneId:"NP-LAX-03"})
        compare(state.locations.length, 0)
        compare(selections.count, 0)
    }
    function test_invalidatedLaunch() {
        load()
        state.launchValid = false
        tryCompare(dialog, "visible", false)
        compare(dismissals.count, 1)
        state.choose("NP-LAX-03")
        compare(selections.count, 0)
    }
    function test_refreshDoesNotAcceptOldResponse() {
        load()
        const oldId = "request-" + core.serial
        state.refresh()
        verify(state.loading)
        compare(dialog.selectedZoneId, "")
        core.responseReceived(oldId, {locations:sampleLocations, recommendedZoneId:"NP-DAL-04"})
        verify(state.loading)
        compare(state.locations.length, 0)
        core.responseReceived(state.requestId, {locations:sampleLocations, recommendedZoneId:"NP-LAX-03"})
        compare(dialog.selectedZoneId, "NP-LAX-03")
    }
    function test_paidUpgradeClosesDialog() {
        load()
        state.subscription = {membershipTier:"ULTIMATE"}
        tryCompare(dialog, "visible", false)
        compare(dismissals.count, 1)
    }
    function test_errorAndRetry() {
        state.begin("Game")
        core.requestFailed(state.requestId, "network", "offline")
        verify(state.error !== "")
        verify(!state.loading)
        tryCompare(dialog, "opened", true)
        mouseClick(findChild(dialog, "queueSelectorRefresh"))
        compare(core.calls.length, 2)
        verify(state.loading)
        mouseClick(findChild(dialog, "queueSelectorDefault"))
        compare(selections.count, 1)
        compare(selections.signalArguments[0][0], null)
        compare(core.cancellations.length, 1)
    }
    function test_emptyResponse() {
        state.begin("Game")
        core.responseReceived(state.requestId, {locations:[]})
        verify(state.error !== "")
        verify(!findChild(dialog, "queueSelectorPlay").enabled)
        state.choose("unknown")
        compare(selections.count, 0)
    }
    function test_selectAndOptOut() {
        load()
        mouseClick(findChild(dialog, "queueSelectorDontShow"))
        mouseClick(findChild(dialog, "queueSelectorPlay"))
        compare(selections.count, 1)
        compare(selections.signalArguments[0][0].zoneId, "NP-LAX-03")
        compare(settings.writes.length, 1)
        compare(settings.writes[0].key, "hideQueueSelector")
        compare(settings.writes[0].value, true)
        tryCompare(dialog, "visible", false)
    }
    function test_chooseZoneWithinLocation() {
        load()
        const list = findChild(dialog, "queueSelectorLocations")
        tryVerify(() => list.itemAtIndex(0) !== null && list.itemAtIndex(1) !== null)
        const row = list.itemAtIndex(0)
        compare(row.objectName, "queueLocation_NP-LAX-03")
        const zones = findChild(row, "queueLocationZones_NP-LAX-03")
        verify(zones.visible)
        verify(!findChild(list.itemAtIndex(1), "queueLocationZones_NP-DAL-04").visible)
        const alternate = findChild(row, "queueZone_NP-LAX-04")
        tryVerify(() => alternate.width > 0 && alternate.height > 0)
        mouseClick(alternate)
        compare(dialog.selectedZoneId, "NP-LAX-04")
        verify(row.highlighted)
        verify(zones.visible)
        mouseClick(findChild(dialog, "queueSelectorPlay"))
        compare(selections.count, 1)
        compare(selections.signalArguments[0][0].zoneId, "NP-LAX-04")
        compare(selections.signalArguments[0][0].streamingBaseUrl, "https://np-lax-04.cloudmatchbeta.nvidiagrid.net/")
    }
    function test_escapeCancels() {
        load()
        keyClick(Qt.Key_Escape)
        tryCompare(state, "opened", false)
        compare(selections.count, 0)
        compare(dismissals.count, 1)
        compare(settings.writes.length, 0)
        load()
        verify(dialog.visible)
    }
    function test_latencyWarning() {
        load()
        verify(!dialog.higherLatency)
        dialog.selectedZoneId = "NP-DAL-04"
        verify(dialog.higherLatency)
        dialog.selectedZoneId = "NP-PAR-01"
        verify(!dialog.higherLatency)
    }
    function test_attributionLink() {
        load()
        mouseClick(findChild(dialog, "queueSelectorCredit"))
        compare(String(UrlCapture.openedUrl), "https://printedwaste.com/gfn")
        verify(state.opened)
        compare(selections.count, 0)
    }
    function test_scaledLayout_data() {
        return [{tag:"normal", scale:1, width:1100, height:850},
            {tag:"large", scale:1.4, width:1100, height:850},
            {tag:"compact", scale:1, width:960, height:540},
            {tag:"compact-large", scale:1.4, width:960, height:540}]
    }
    function test_scaledLayout(data) {
        DesktopTokens.uiScale = data.scale
        testCase.width = data.width
        testCase.height = data.height
        load()
        dialog.selectedZoneId = "NP-DAL-04"
        waitForRendering(dialog.contentItem)
        verify(dialog.height <= testCase.height)
        verify(dialog.width <= testCase.width)
        const play = findChild(dialog, "queueSelectorPlay")
        verify(play.visible && play.enabled)
        const list = findChild(dialog, "queueSelectorLocations")
        verify(list.height >= 80 * data.scale)
        const edge = play.mapToItem(dialog.contentItem, play.width, play.height)
        verify(edge.x <= dialog.contentItem.width && edge.y <= dialog.contentItem.height,
            "Launch action exceeds the dialog")
        mouseClick(play)
        compare(selections.count, 1)
    }
}
