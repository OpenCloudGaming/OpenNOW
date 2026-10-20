import QtQuick
import OpenNOW

Text {
    id: root

    readonly property var coop: ShellStore.coop
    readonly property int guestCount: coop.guests.length
    readonly property string uploadMbps: mbps(coop.hostStats ? Number(coop.hostStats.uploadKbps || 0) : 0)
    readonly property string neededMbps: mbps(coop.uploadNeededKbps)

    function mbps(kbps) {
        return String(Math.round(Number(kbps) / 100) / 10)
    }

    visible: coop.hostStats !== null && coop.uploadCapacityKbps === 0
    text: guestCount === 0 ? qsTr("Upload %1 Mbps · needs %2 Mbps before guests join").arg(uploadMbps).arg(neededMbps)
        : guestCount === 1 ? qsTr("Upload %1 Mbps · needs %2 Mbps for 1 guest").arg(uploadMbps).arg(neededMbps)
        : qsTr("Upload %1 Mbps · needs %2 Mbps for %3 guests").arg(uploadMbps).arg(neededMbps).arg(guestCount)
    color: "#A3FFFFFF"
    font.family: DesktopTokens.bodyFont
    font.pixelSize: 12
    font.weight: Font.DemiBold
    Accessible.role: Accessible.StaticText
    Accessible.name: text
}
