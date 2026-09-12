import QtQuick
import OpenNOW

// Isolated transport: the production ShellStore and settings controls run
// unchanged, but no request can reach an account or the network.
QtObject {
    id: test
    property RegionChoicesAcceptance regionChoices: RegionChoicesAcceptance {}
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property int sequence: 0
        property var requests: []
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {} // No filesystem writes from the isolated mock.
        function request(method, params, timeout) {
            const id = "region-test-" + (++sequence)
            requests.push({ id: id, method: method, params: params })
            return id
        }
        function cancel(id) { requestFailed(id, "cancelled", "Cancelled"); return true }
    }
    property var regions: [
        {name: "EU West", url: "https://west.example.invalid"},
        {name: "EU Central", url: "https://central.example.invalid"},
        {name: "US East", url: "https://east.example.invalid"}
    ]
    function check(condition, message) {
        if (!condition) throw new Error("Region ping acceptance: " + message)
    }
    function invokePing(button) { button.clicked() }
    function hasRegionSelector(item) {
        if (item.title === "Region" || item.title === "Server region") return true
        const children = item.children || []
        for (let i = 0; i < children.length; ++i)
            if (hasRegionSelector(children[i])) return true
        return false
    }
    function run(screen, row, button, picker) {
        regionChoices.run(screen, picker)
        ShellStore.resetRegionPing()
        ShellStore.regionsRequestId = ""
        ShellStore.regions = []
        ShellStore.authSession = null
        invokePing(button)
        check(row.description.indexOf("Sign in") >= 0 && !ShellStore.regionPingBusy, "signed-out feedback")

        ShellStore.authSession = {user: {id: "region-fixture", userId: "region-fixture", displayName: "Region Test"}}
        ShellStore.settings = {region: ""}
        invokePing(button)
        const discovery = ShellStore.regionsRequestId
        check(discovery !== "" && ShellStore.regionPingPending && !button.enabled, "queued discovery")
        check(row.description.indexOf("Loading regions") >= 0, "visible discovery status")
        const before = client.requests.length
        ShellStore.pingRegions()
        check(client.requests.length === before, "duplicate click dispatched twice")
        client.responseReceived(discovery, {regions: regions, vpcId: "fixture"})
        const measurement = ShellStore.regionPingRequestId
        check(measurement !== "" && !ShellStore.regionPingPending && !button.enabled, "measurement after discovery")
        check(row.description.indexOf("Measuring") >= 0, "visible measuring status")
        client.responseReceived(measurement, {results: [
            {url: regions[0].url, pingMs: 0},
            {url: regions[1].url, pingMs: 21},
            {url: regions[2].url, pingMs: null, error: "offline"}
        ]})
        check(button.enabled && !ShellStore.regionPingBusy, "completion released busy state")
        check(row.description.indexOf("EU West") >= 0 && row.value === "Best: 0 ms", "automatic best result including zero")
        check(picker.items.find(item => item.value === regions[2].url).detail === "No response", "failed region feedback")
        check(ShellStore.settings.region === "", "ping changed preferred region")
        ShellStore.settings = {region: regions[1].url}
        check(row.value === "21 ms", "explicit region latency")

        invokePing(button)
        client.requestFailed(ShellStore.regionPingRequestId, "deadline_exceeded", "Latency request timed out")
        check(button.enabled && row.description === "Latency request timed out", "timeout feedback and retry")
        invokePing(button)
        client.responseReceived(ShellStore.regionPingRequestId, {results: regions.map(region => ({url: region.url, pingMs: null}))})
        check(row.description === "No region responded" && button.enabled, "all offline feedback")

        ShellStore.regions = []
        invokePing(button)
        client.responseReceived(ShellStore.regionsRequestId, {regions: []})
        check(!ShellStore.regionPingBusy && row.description.indexOf("No streaming regions") >= 0, "empty discovery")
        invokePing(button)
        client.requestFailed(ShellStore.regionsRequestId, "network_error", "Discovery unavailable")
        check(!ShellStore.regionPingBusy && row.description.indexOf("Discovery unavailable") >= 0, "discovery failure")

        // Core shutdown must not leave the action silently disabled.
        client.state = "failed"
        invokePing(button)
        check(row.description.indexOf("core is not ready") >= 0 && button.enabled, "core unavailable")
        client.state = "ready"
        ShellStore.regionsRequestId = ""
        ShellStore.regions = regions
        invokePing(button)
        const cancelled = ShellStore.regionPingRequestId
        ShellStore.resetRegionPing()
        client.responseReceived(cancelled, {results: [{url: regions[0].url, pingMs: 9}]})
        check(!ShellStore.regionPingBusy && Object.keys(ShellStore.regionPingResults).length === 0, "cancelled result was reapplied")

        // Leave a complete visible result for screenshot/layout acceptance.
        ShellStore.settings = {region: "", themePack: "aurora"}
        invokePing(button)
        client.responseReceived(ShellStore.regionPingRequestId, {results: [
            {url: regions[0].url, pingMs: 9}, {url: regions[1].url, pingMs: 21},
            {url: regions[2].url, pingMs: null}
        ]})
        screen.selectedSection = 0
        check(!hasRegionSelector(screen), "duplicate selector remains in Account")
        screen.selectedSection = 6
        return true
    }
}
