import QtQuick
import QtQuick.Controls
import OpenNOW

Item {
    id: root
    objectName: "desktopSourceSwitcher"
    readonly property var store: ShellStore.sourceOwnerState
    readonly property var choices: store.playableSources
    readonly property var current: store.selectedSource
    implicitWidth: label.implicitWidth + DesktopTokens.px(44)
    implicitHeight: DesktopTokens.controlHeight
    visible: store.available && choices.length > 1

    function statusText(source) {
        if (source.id === store.gfnId)
            return ShellStore.signedIn ? qsTr("Signed in") : qsTr("Not signed in")
        const state = store.authState(source.id)
        if (!state)
            return ""
        if (state.state === "not-required")
            return qsTr("No account needed")
        if (state.state === "signed-in")
            return qsTr("Signed in as %1").arg(String(state.account && state.account.name || ""))
        return qsTr("Not signed in")
    }

    DesktopButton {
        id: button
        objectName: "desktopSourceSwitcherButton"
        width: root.width
        height: parent.height
        implicitWidth: label.implicitWidth + DesktopTokens.px(44)
        font.pixelSize: DesktopTokens.smallSize
        text: root.current ? String(root.current.name || root.current.id) : qsTr("GeForce NOW")
        Accessible.name: qsTr("Service: %1").arg(text)
        contentItem: Item {
            Text {
                id: label
                anchors.verticalCenter: parent.verticalCenter
                text: button.text
                textFormat: Text.PlainText
                color: DesktopTokens.text
                font: button.font
            }
            DesktopSettingsIcon {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: DesktopTokens.px(12); height: width
                glyph: "chevron"; rotation: 90; ink: Theme.textMuted
            }
        }
        onClicked: menu.opened ? menu.close() : menu.open()
    }

    Popup {
        id: menu
        objectName: "desktopSourceMenu"
        y: root.height + DesktopTokens.px(6)
        width: DesktopTokens.px(300)
        padding: DesktopTokens.px(6)
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { radius: DesktopTokens.px(12); color: Theme.shell; border.color: Theme.seam }
        contentItem: Column {
            spacing: DesktopTokens.px(2)
            Text {
                leftPadding: DesktopTokens.px(10); topPadding: DesktopTokens.px(6); bottomPadding: DesktopTokens.px(4)
                text: qsTr("BROWSE WITH")
                color: Theme.textMuted; font.family: Theme.monoFont
                font.pixelSize: DesktopTokens.px(10); font.weight: Font.Bold; font.letterSpacing: 1
            }
            Repeater {
                model: root.choices
                delegate: ItemDelegate {
                    required property var modelData
                    objectName: "desktopSourceChoice-" + modelData.id
                    width: menu.width - 2 * menu.padding
                    height: DesktopTokens.px(52)
                    padding: DesktopTokens.px(10)
                    Accessible.name: String(modelData.name || modelData.id)
                    background: Rectangle {
                        radius: DesktopTokens.px(8)
                        color: parent.hovered || parent.activeFocus ? DesktopTokens.raised : "transparent"
                        border.width: parent.activeFocus ? 2 : 0; border.color: Theme.focus
                    }
                    contentItem: Item {
                        Column {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - check.width - DesktopTokens.px(8)
                            spacing: DesktopTokens.px(2)
                            Text {
                                width: parent.width
                                text: String(modelData.name || modelData.id)
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                color: Theme.label; font.family: Theme.bodyFont
                                font.pixelSize: DesktopTokens.bodySize; font.weight: Font.Bold
                            }
                            Text {
                                width: parent.width
                                text: root.statusText(modelData)
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                                color: Theme.textMuted; font.family: Theme.bodyFont
                                font.pixelSize: DesktopTokens.captionSize
                            }
                        }
                        DesktopSettingsIcon {
                            id: check
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            width: DesktopTokens.px(14); height: width
                            visible: root.current && root.current.id === modelData.id
                            glyph: "check"; ink: Theme.mint
                        }
                    }
                    onClicked: {
                        menu.close()
                        root.store.select(modelData.id)
                    }
                }
            }
            DesktopButton {
                width: menu.width - 2 * menu.padding
                height: DesktopTokens.px(36)
                font.pixelSize: DesktopTokens.smallSize
                text: qsTr("Manage plugins")
                onClicked: {
                    menu.close()
                    AppController.navigate("settings-plugins")
                }
            }
        }
    }
}
