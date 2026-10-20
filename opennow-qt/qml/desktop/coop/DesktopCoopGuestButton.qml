import QtQuick
import QtQuick.Controls
import QtQuick.Window
import OpenNOW

Button {
    id: root
    property bool primary: false
    property bool danger: false
    property string glyph: ""
    property int glyphSize: 20
    readonly property int borderWidth: activeFocus ? 2 : 1
    readonly property color ink: primary ? "#0A0D14" : danger ? "#FFB4AE" : "#FFFFFF"
    height: 38
    leftPadding: 15 + borderWidth
    rightPadding: 15 + borderWidth
    topPadding: 0
    bottomPadding: 0
    implicitWidth: contentRow.implicitWidth + leftPadding + rightPadding
    focusPolicy: Qt.StrongFocus
    font.family: DesktopTokens.bodyFont
    font.pixelSize: 13
    font.weight: Font.ExtraBold
    Accessible.role: Accessible.Button
    Accessible.name: text
    Keys.onReturnPressed: event => { if (!event.isAutoRepeat) root.clicked() }
    Keys.onEnterPressed: event => { if (!event.isAutoRepeat) root.clicked() }

    background: Rectangle {
        radius: 10
        color: root.primary ? (root.down ? "#D9D9D9" : "#FFFFFF")
            : root.danger ? (root.hovered || root.activeFocus ? "#29FF8A80" : "#1AFF8A80")
            : root.hovered || root.activeFocus ? "#1FFFFFFF" : "transparent"
        border.width: root.borderWidth
        border.color: root.activeFocus ? "#7FD4FF" : root.primary ? "#FFFFFF" : root.danger ? "#59FF8A80" : "#24FFFFFF"
    }
    contentItem: Item {
        implicitWidth: contentRow.implicitWidth
        implicitHeight: 20
        Row {
            id: contentRow
            anchors.centerIn: parent
            spacing: 8
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.text
                color: root.ink
                font: root.font
            }
            Image {
                visible: root.glyph !== ""
                anchors.verticalCenter: parent.verticalCenter
                width: root.glyphSize
                height: root.glyphSize
                source: root.glyph === "" ? "" : InputPromptIcons.sourceFor(root.glyph, root.primary ? "#0A0D14" : "#FFFFFF")
                sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
                fillMode: Image.PreserveAspectFit
            }
        }
    }
}
