import QtQuick
import OpenNOW

QtObject {
    id: client
    property int serial: 0

    function request(method, params, timeout) {
        return method === "settings.set" ? "pending-setting-" + (++serial) : ""
    }

    function cancel(id) {}

    Component.onCompleted: {
        const owner = ShellStore.settingsOwnerState
        owner.confirmedSettings = Object.assign({}, owner.settings)
        owner.coreClient = client
        owner.ready = true
    }
}
