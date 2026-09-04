import QtQuick
import QtQuick.Controls
import OpenNOW

GlassPanel {
    id: root
    property bool actionsOpen: false
    property string actionMessage: ""
    readonly property var capabilities: ShellStore.socialCapabilities || ({})
    width: 520
    height: 609
    panelRadius: 34
    strong: true
    color: "#10131C"
    focus: visible
    Accessible.name: qsTr("Social and local co-op")
    Accessible.role: Accessible.Pane
    onVisibleChanged: if (visible) Qt.callLater(() => root.actionsOpen ? actionList.forceActiveFocus() : localJoin.forceActiveFocus())
    onActionsOpenChanged: if (visible) Qt.callLater(() => actionsOpen ? actionList.forceActiveFocus() : localJoin.forceActiveFocus())
    Component.onCompleted: if (visible) Qt.callLater(() => root.actionsOpen ? actionList.forceActiveFocus() : localJoin.forceActiveFocus())

    function explainUnavailable(label) {
        actionMessage = qsTr("%1 is unavailable because NVIDIA does not expose a supported friends API.").arg(label)
        Accessible.announce(actionMessage)
    }

    GlassPanel {
        visible: root.actionsOpen
        x: root.width + 24
        y: 94
        width: 400
        height: 483
        panelRadius: 30
        strong: true
        color: "#10131C"
        Column {
            anchors.fill: parent
            anchors.margins: 19
            spacing: 6
            Row {
                width: parent.width
                height: 75
                spacing: 14
                Rectangle {
                    width: 56; height: 56; radius: 14
                    gradient: Gradient {
                        GradientStop { position: 0; color: Theme.violet }
                        GradientStop { position: 1; color: Theme.shell }
                    }
                    Text { anchors.centerIn: parent; text: qsTr("GFN"); color: Theme.mediaForeground; font.family: Theme.displayFont; font.pixelSize: 12; font.weight: Font.Black }
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 70
                    Text { width: parent.width; text: qsTr("Provider social actions"); color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 16; font.weight: Font.Bold; elide: Text.ElideRight }
                    Text { width: parent.width; text: root.capabilities.reason || qsTr("Friends API unavailable"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 12; elide: Text.ElideRight }
                }
            }
            ListView {
                id: actionList
                width: parent.width
                height: 280
                spacing: 0
                clip: true
                keyNavigationWraps: false
                model: [
                    {t:"Invite to co-op", d:"Provider capability unavailable", glyph:"＋"},
                    {t:"Join session", d:"Provider capability unavailable", glyph:"↪"},
                    {t:"Send a message", d:"Provider capability unavailable", glyph:"✉"},
                    {t:"View profile", d:"Provider capability unavailable", glyph:"●"},
                    {t:"Remove friend", d:"Provider capability unavailable", glyph:"−"}
                ]
                delegate: SettingRow {
                    required property var modelData
                    required property int index
                    width: ListView.view.width
                    height: 56
                    title: modelData.glyph + "  " + modelData.t
                    description: modelData.d
                    value: index === 0 ? "A" : ""
                    currentItem: ListView.isCurrentItem
                    onClicked: root.explainUnavailable(modelData.t)
                }
                Keys.onReturnPressed: if (currentItem) currentItem.clicked()
                Keys.onEnterPressed: if (currentItem) currentItem.clicked()
            }
            Text {
                width: parent.width
                height: 54
                text: root.actionMessage || qsTr("Invite and friend actions require a provider-supported social API.")
                color: root.actionMessage ? Theme.coral : Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 12
                wrapMode: Text.WordWrap
                verticalAlignment: Text.AlignVCenter
            }
        }
    }

    Column {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 12

        Row {
            width: parent.width
            height: 48
            spacing: 10
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Social & co-op")
                color: Theme.label
                font.family: Theme.displayFont
                font.pixelSize: 24
                font.weight: Font.Black
            }
            GlassPanel {
                anchors.verticalCenter: parent.verticalCenter
                width: comingSoon.implicitWidth + 24; height: 30; panelRadius: 15
                strong: true
                Text {
                    id: comingSoon
                    anchors.centerIn: parent
                    text: qsTr("COMING SOON")
                    color: Theme.mint
                    font.family: Theme.bodyFont
                    font.pixelSize: 11
                    font.weight: Font.Black
                    font.letterSpacing: 0.8
                }
            }
        }

        GlassPanel {
            width: parent.width
            height: 185
            panelRadius: 26
            strong: true
            Column {
                anchors.fill: parent
                anchors.margins: 22
                spacing: 12
                Rectangle {
                    width: 48; height: 48; radius: 24
                    color: Theme.glassStrong
                    Text { anchors.centerIn: parent; text: qsTr("◎"); color: Theme.focus; font.pixelSize: 24; font.weight: Font.Black }
                }
                Text {
                    text: qsTr("GeForce NOW friends are unavailable")
                    color: Theme.label
                    font.family: Theme.bodyFont
                    font.pixelSize: 18
                    font.weight: Font.Bold
                }
                Text {
                    width: parent.width
                    text: root.capabilities.reason || qsTr("The provider does not expose a supported friends service.")
                    wrapMode: Text.WordWrap
                    color: Theme.textMuted
                    font.family: Theme.bodyFont
                    font.pixelSize: 14
                    lineHeight: 1.2
                }
            }
        }

        Text {
            text: qsTr("LOCAL CONTROLLERS")
            color: Theme.textMuted
            font.family: Theme.monoFont
            font.pixelSize: 12
            font.letterSpacing: 1.2
        }

        ListView {
            width: parent.width
            height: 95
            spacing: 6
            clip: true
            model: ControllerInput.controllers
            delegate: GlassPanel {
                required property var modelData
                width: ListView.view.width
                height: 52
                panelRadius: 20
                Row {
                    anchors.fill: parent
                    anchors.margins: 14
                    spacing: 14
                    Rectangle {
                        width: 34; height: 34; radius: 17; color: Theme.mint
                        Text { anchors.centerIn: parent; text: qsTr("P") + modelData.slot; color: Theme.contrastText(Theme.mint); font.weight: Font.Black }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 300
                        elide: Text.ElideRight
                        text: modelData.name
                        color: Theme.label
                        font.family: Theme.bodyFont
                        font.pixelSize: 15
                        font.weight: Font.Bold
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.batteryPercent >= 0 ? modelData.batteryPercent + "%" : qsTr("Ready")
                        color: Theme.textMuted
                        font.family: Theme.bodyFont
                        font.pixelSize: 13
                    }
                }
            }
        }

        GlassButton {
            id: localJoin
            width: parent.width
            text: AppController.controllerCount >= 2 ? qsTr("Set up local player two") : qsTr("Connect another controller")
            glyph: "A"
            primary: true
            onClicked: {
                AppController.showOverlay("")
                AppController.navigate("joining")
            }
        }
        GlassButton {
            width: parent.width
            text: qsTr("Controller settings")
            glyph: "›"
            onClicked: {
                AppController.showOverlay("")
                AppController.navigate("controllers")
            }
        }
    }
}
