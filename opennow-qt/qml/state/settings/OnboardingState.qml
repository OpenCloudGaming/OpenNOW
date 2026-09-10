import QtQuick

QtObject {
    id: root
    required property var coreClient
    required property var persistedSettings
    required property bool ready
    required property bool signedIn
    required property var checkRequirements
    property var draft: ({})
    readonly property var settings: Object.assign({}, persistedSettings, draft)
    readonly property bool needed: persistedSettings.onboardingCompleted === false
    property bool saving: false
    property string error: ""
    property string requestId: ""
    property string requestKey: ""
    property var remainingKeys: []
    signal completed()
    signal requirementsMissing()
    signal settingSaved(string key, var value, var changes)

    onReadyChanged: {
        if (!ready && saving)
            fail(qsTr("The connection was interrupted. Reconnect and try saving again."))
    }
    onSignedInChanged: {
        if (!signedIn) {
            if (saving)
                fail(qsTr("Sign in again to finish setup."))
            draft = ({})
        }
    }

    function setSetting(key, value) {
        if (saving || ["launchInConsoleMode", "switchToConsoleOnPad",
                "resolution", "fps", "enableHdr", "codec", "maxBitrateMbps",
                "frameGeneration", "upscaling", "upscalingSharpness", "upscalingDenoise"].indexOf(key) < 0)
            return
        const updated = Object.assign({}, draft)
        if (key === "launchInConsoleMode" && updated.switchToConsoleOnPad === undefined)
            updated.switchToConsoleOnPad = settings.switchToConsoleOnPad === true
        updated[key] = value
        draft = updated
        error = ""
    }

    function finish() {
        if (saving)
            return
        if (!ready || !signedIn) {
            error = qsTr("Connect and sign in before finishing setup.")
            return
        }
        if (!verifyRequirements())
            return
        error = ""
        const keys = Object.keys(draft).filter(key => key !== "switchToConsoleOnPad")
        if (draft.switchToConsoleOnPad !== undefined)
            keys.push("switchToConsoleOnPad")
        remainingKeys = keys.concat(["onboardingCompleted"])
        saving = true
        saveNext()
    }

    function verifyRequirements() {
        const requirementError = checkRequirements()
        if (requirementError !== "") {
            fail(requirementError)
            requirementsMissing()
            return false
        }
        error = ""
        return true
    }

    function saveNext() {
        if (!saving)
            return
        if (!remainingKeys.length) {
            draft = ({})
            saving = false
            completed()
            return
        }
        if (remainingKeys[0] === "onboardingCompleted" && !verifyRequirements())
            return
        requestKey = remainingKeys[0]
        requestId = coreClient.request("settings.set", {
            key: requestKey,
            value: requestKey === "onboardingCompleted" ? true : draft[requestKey]
        })
        if (requestId === "")
            fail(qsTr("Setup could not be saved. Your choices are still here. Try again."))
    }

    function acceptResponse(id, result) {
        if (!saving || id !== requestId || requestId === "")
            return false
        const key = requestKey
        requestId = ""
        requestKey = ""
        settingSaved(key, result.value, result.changes || ({}))
        remainingKeys = remainingKeys.slice(1)
        saveNext()
        return true
    }

    function acceptFailure(id, message) {
        if (!saving || id !== requestId || requestId === "")
            return false
        fail(qsTr("Setup could not be saved. Your choices are still here. %1").arg(message))
        return true
    }

    function fail(message) {
        requestId = ""
        requestKey = ""
        remainingKeys = []
        saving = false
        error = message
    }
}
