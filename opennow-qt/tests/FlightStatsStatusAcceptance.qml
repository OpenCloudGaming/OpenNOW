import QtQuick
import OpenNOW

DesktopStreamControllersAcceptance {
    property Component statsComponent: Component { DesktopStreamStats { visible: false } }

    function run(root) {
        ShellStore.settings = Object.assign({}, ShellStore.settings, {
            statsShowFlight: true, statsShowFlightDevice: true, statsShowGraphs: false
        })
        input.forwardFlightControls = true
        const stats = statsComponent.createObject(root)
        check(stats !== null, "statistics overlay is created")
        stats.expanded = Qt.application.arguments.includes("--smoke-stats-expanded")
        const cases = [
            ["no-hid-path", qsTr("NO HID PATH"), "#FF8A80"],
            ["no-access", qsTr("NO PERMISSION"), "#FF8A80"],
            ["declined", qsTr("IDLE"), "#FF8A80"],
            ["ready", qsTr("ready").toUpperCase(), "#A3FFFFFF"],
            ["controller", qsTr("controller P%1").arg(2).toUpperCase(), "#F5A623"],
            ["ignored", qsTr("ignored").toUpperCase(), "#A3FFFFFF"]
        ]
        for (const entry of cases) {
            input.flightDevices = [{deviceId: "fixture-stick", name: "Synthetic flight stick",
                type: "stick", status: entry[0], player: 2}]
            const card = stats.cards.find(item => item.key === "FlightDevice")
            check(card && card.textValue === entry[1], entry[0] + " retains its actual status")
            check(card.ink === entry[2], entry[0] + " uses the correct warning tone")
            check(stats.compactMetrics.some(item => item.text.includes(entry[1])),
                entry[0] + " appears in compact statistics")
            check(stats.report().includes(entry[1]), entry[0] + " appears in the copied report")
            if (entry[0] === "no-hid-path" || entry[0] === "no-access")
                check(card.detail.includes(entry[1]) && !card.detail.includes(qsTr("ready")),
                    entry[0] + " never describes a blocked device as ready")
        }
        input.flightDevices = [{deviceId: "fixture-stick", name: "SYNTHETIC FIXTURE · NOT LIVE",
            type: "stick", status: "no-hid-path", player: 0}]
        if (Qt.application.arguments.includes("--screenshot")) {
            stats.anchors.fill = root
            stats.z = 10000
            stats.visible = true
        } else {
            stats.destroy()
        }
        return true
    }
}
