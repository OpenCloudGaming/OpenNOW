pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Window
import OpenNOW

Row {
    id: root
    property string shortcut: ""
    property real keySize: 24
    property color ink: Theme.label
    readonly property real dpr: Math.max(1, Screen.devicePixelRatio)
    spacing: 3
    Accessible.role: Accessible.StaticText
    Accessible.name: shortcut

    Repeater {
        model: InputPromptIcons.keysFor(root.shortcut)
        delegate: Item {
            id: keycap
            required property string modelData
            width: icon.source.toString() !== "" ? icon.implicitWidth / root.dpr : fallbackText.implicitWidth + 12
            height: root.keySize

            Image {
                id: icon
                anchors.fill: parent
                source: InputPromptIcons.keyboardSourceFor(keycap.modelData, root.ink)
                sourceSize: Qt.size(-1, Math.max(1, root.keySize * root.dpr))
                fillMode: Image.PreserveAspectFit
                opacity: root.ink.a
            }
            Rectangle {
                anchors.fill: parent
                visible: icon.source.toString() === ""
                radius: 5
                color: "transparent"
                border.color: root.ink
                Text {
                    id: fallbackText
                    anchors.centerIn: parent
                    text: keycap.modelData
                    color: root.ink
                    font.family: Theme.monoFont
                    font.pixelSize: Math.max(9, root.keySize * 0.45)
                    font.weight: Font.DemiBold
                }
            }
        }
    }
}
