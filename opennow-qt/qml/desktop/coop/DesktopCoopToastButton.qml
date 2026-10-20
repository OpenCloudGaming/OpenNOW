import QtQuick
import QtQuick.Controls
import OpenNOW

AbstractButton {
    id: control
    property bool primary: false
    property bool quiet: false
    property bool compact: false
    property var keys: []
    readonly property color ink: primary ? "#0B0F1A" : quiet ? "#A3FFFFFF" : "#FFFFFF"

    implicitHeight: compact ? 32 : 36
    implicitWidth: contentRow.implicitWidth + leftPadding + rightPadding
    leftPadding: compact ? 12 : 14
    rightPadding: compact || keys.length === 0 ? leftPadding : 12
    hoverEnabled: true
    focusPolicy: Qt.NoFocus
    Accessible.role: Accessible.Button
    Accessible.name: keys.length > 0 ? qsTr("%1, %2").arg(text).arg(keys.join(" ")) : text

    background: Rectangle {
        radius: control.compact ? 8 : 10
        color: control.primary ? (control.down ? "#E6FFFFFF" : "#FFFFFF")
            : control.quiet ? (control.hovered ? "#0FFFFFFF" : "transparent")
            : control.down || control.hovered ? "#1FFFFFFF" : "#14FFFFFF"
        border.width: control.primary ? 0 : 1
        border.color: control.compact && !control.quiet ? "#14FFFFFF" : "#24FFFFFF"
    }

    contentItem: Item {
        implicitWidth: contentRow.implicitWidth
        implicitHeight: contentRow.implicitHeight
        Row {
            id: contentRow
            anchors.centerIn: parent
            spacing: 8
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: control.text
                color: control.ink
                font.family: DesktopTokens.bodyFont
                font.pixelSize: control.compact ? 13 : 14
                font.weight: Font.ExtraBold
            }
            Row {
                visible: control.keys.length > 0
                anchors.verticalCenter: parent.verticalCenter
                spacing: 3
                Repeater {
                    model: control.keys
                    delegate: Rectangle {
                        required property string modelData
                        width: Math.max(18, keyLabel.implicitWidth + 8)
                        height: 18
                        radius: 4
                        color: control.primary ? "#0B0F1A" : "#D9FFFFFF"
                        Text {
                            id: keyLabel
                            anchors.centerIn: parent
                            text: modelData
                            color: control.primary ? "#FFFFFF" : "#0B0F1A"
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 9
                            font.weight: Font.Bold
                        }
                    }
                }
            }
        }
    }
}
