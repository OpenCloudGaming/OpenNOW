import QtQuick

QtObject {
    id: root
    required property var coreClient
    required property bool eligible
    required property bool launchValid
    required property var authSession
    required property var subscription
    readonly property bool freeTier: {
        const provider = authSession && authSession.provider
        if (!provider || String(provider.code).toUpperCase() !== "NVIDIA") return false
        const tier = subscription && subscription.membershipTier
            ? subscription.membershipTier : authSession.user && authSession.user.membershipTier
        return String(tier || "").trim().toUpperCase() === "FREE"
    }
    property bool opened: false
    property string gameTitle: ""
    property string requestId: ""
    property var locations: []
    property string recommendedZoneId: ""
    property string error: ""
    readonly property bool loading: requestId !== ""
    signal selected(var location)
    signal dismissed()

    onEligibleChanged: if (!eligible && opened) dismiss()
    onFreeTierChanged: if (!freeTier && opened) dismiss()
    onLaunchValidChanged: if (!launchValid && opened) dismiss()

    function begin(title) {
        if (!eligible || !freeTier || !launchValid) return false
        gameTitle = title
        opened = true
        refresh()
        return true
    }

    function cancelRequest() {
        const id = requestId
        requestId = ""
        if (id !== "") coreClient.cancel(id)
    }

    function refresh() {
        if (!opened || loading) return
        locations = []
        recommendedZoneId = ""
        error = ""
        requestId = coreClient.request("queue.servers.list", {}, 30000)
        if (requestId === "") error = qsTr("Queue information could not be loaded. You can still use your default region.")
    }

    function dismiss() {
        opened = false
        cancelRequest()
        locations = []
        dismissed()
    }

    // A location's own zone id, or one of the other zones listed in its zones.
    function zone(zoneId) {
        for (const location of locations) {
            if (location.zoneId === zoneId) return location
            const match = (location.zones || []).find(item => item.zoneId === zoneId)
            if (match) return match
        }
        return null
    }

    function choose(zoneId) {
        if (!opened || !eligible || !freeTier || !launchValid) return
        const location = zoneId === "" ? null : zone(zoneId)
        if (zoneId !== "" && (!location || loading)) return
        opened = false
        cancelRequest()
        selected(location)
    }

    property Connections responses: Connections {
        target: root.coreClient
        function onResponseReceived(id, result) {
            if (id === "" || id !== root.requestId) return
            root.requestId = ""
            if (!root.opened || !root.eligible || !root.launchValid) return
            root.locations = result.locations || []
            root.recommendedZoneId = String(result.recommendedZoneId || "")
            if (!root.locations.length)
                root.error = qsTr("No queue information is available. You can still use your default region.")
        }
        function onRequestFailed(id, code, message) {
            if (id === "" || id !== root.requestId) return
            root.requestId = ""
            root.error = qsTr("Queue information could not be loaded. You can still use your default region.")
        }
    }
}
