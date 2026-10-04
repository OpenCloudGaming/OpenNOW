import QtQuick
import QtQuick.Controls
import QtQuick.Shapes
import OpenNOW

Rectangle {
    id: root
    objectName: "desktopBugReportToast"
    property bool active: true
    readonly property var report: ShellStore.bugReports.latest
    readonly property string reportState: report ? String(report.state) : ""
    readonly property bool failed: reportState === "failed"
    readonly property bool sending: reportState === "sending"
    property bool shown: false
    signal settingsRequested()

    function summary(kind) {
        switch (kind) {
        case "session_error": return qsTr("Your session hit an error.")
        case "stream_error": return qsTr("Your stream stopped with an error.")
        case "frame_drops": return qsTr("Your stream kept dropping frames.")
        case "library_error": return qsTr("Your library couldn't load.")
        default: return qsTr("OpenNOW ran into a problem.")
        }
    }

    readonly property string title: sending ? qsTr("Sending bug report…")
        : failed ? qsTr("Couldn't send the bug report")
        : qsTr("Bug reported to the developer")
    readonly property string body: !report ? ""
        : failed && report.queued ? qsTr("It will be retried the next time OpenNOW starts.")
        : failed ? String(report.message || qsTr("Check your connection."))
        : summary(String(report.kind)) + " " + (sending
            ? qsTr("Collecting logs for the developer.")
            : report.issueStatus === "investigating"
            ? qsTr("The developer's assistant is looking into it.")
            : qsTr("We sent a report with logs so it can be fixed."))
    readonly property string meta: {
        if (!report) return ""
        const parts = []
        if (report.reportId) parts.push(qsTr("REF %1").arg(report.reportId))
        if (report.game) parts.push(String(report.game))
        return parts.join(" · ")
    }

    width: Math.min(DesktopTokens.px(400), parent ? parent.width - DesktopTokens.px(48) : DesktopTokens.px(400))
    height: column.implicitHeight + DesktopTokens.px(32)
    radius: DesktopTokens.px(18)
    color: Theme.shell
    border.width: 1
    border.color: failed ? "#4DF5A623" : DesktopTokens.seam
    visible: active && shown && report !== null
    Accessible.role: Accessible.AlertMessage
    Accessible.name: title + ". " + body

    Connections {
        target: ShellStore.bugReports
        function onGenerationChanged() {
            root.shown = true
            if (root.sending) hideTimer.stop()
            else hideTimer.restart()
        }
    }
    Timer { id: hideTimer; interval: 9000; onTriggered: root.shown = false }

    Rectangle {
        id: badge
        x: DesktopTokens.px(16)
        y: DesktopTokens.px(16)
        width: DesktopTokens.px(36); height: width; radius: DesktopTokens.px(11)
        color: root.failed ? "#26F5A623" : "#241DB954"
        border.color: root.failed ? "#4DF5A623" : "#471DB954"
        BusyIndicator {
            anchors.centerIn: parent
            width: DesktopTokens.px(22); height: width
            running: root.sending
            visible: root.sending
        }
        Shape {
            anchors.centerIn: parent
            width: 18; height: 18
            visible: !root.sending
            ShapePath {
                strokeColor: root.failed ? DesktopTokens.ledAmber : DesktopTokens.green
                strokeWidth: 2.2
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg { path: root.failed ? "M9 4v6M9 13.5v.5" : "M3.5 9.5l3.5 3.5 7.5-8" }
            }
        }
    }

    Column {
        id: column
        x: badge.x + badge.width + DesktopTokens.px(14)
        y: DesktopTokens.px(16)
        width: root.width - x - DesktopTokens.px(16)
        spacing: DesktopTokens.px(4)
        Text {
            width: parent.width
            text: root.title
            color: DesktopTokens.text
            font.family: DesktopTokens.bodyFont
            font.pixelSize: DesktopTokens.bodySize
            font.weight: Font.ExtraBold
            wrapMode: Text.WordWrap
        }
        Text {
            width: parent.width
            text: root.body
            color: DesktopTokens.textBody
            font.family: DesktopTokens.bodyFont
            font.pixelSize: DesktopTokens.captionSize
            wrapMode: Text.WordWrap
        }
        Text {
            width: parent.width
            visible: text !== ""
            text: root.meta
            color: DesktopTokens.textFaint
            font.family: DesktopTokens.monoFont
            font.pixelSize: DesktopTokens.microSize
            font.weight: Font.Bold
            elide: Text.ElideRight
        }
        Row {
            spacing: DesktopTokens.px(8)
            topPadding: DesktopTokens.px(6)
            visible: !root.sending
            DesktopButton {
                height: DesktopTokens.px(30)
                text: qsTr("Dismiss")
                onClicked: root.shown = false
            }
            DesktopButton {
                height: DesktopTokens.px(30)
                text: qsTr("Report settings")
                onClicked: { root.shown = false; root.settingsRequested() }
            }
        }
    }
}
