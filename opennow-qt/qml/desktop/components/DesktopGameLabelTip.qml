import QtQuick
import QtQuick.Controls
import OpenNOW

Item {
    id: root
    objectName: "desktopGameLabelTipAnchor"
    property var game: null
    property bool requested: false
    readonly property bool hasContent: ConsoleStores.launchers(game).length > 0
        || DesktopTokens.gameFeatures(game).length > 0 || ShellStore.gameLockedByMembership(game)
    property bool shown: false
    // Popup content lives in the window overlay, outside its catalog tile's
    // visual tree. Retained hover must never reopen it above a live session.
    readonly property bool anchorUsable: visible && enabled
        && AppController.route !== "stream" && AppController.route !== "inserting"
    readonly property bool tipHovered: tipLoader.item ? tipLoader.item.hovered : false

    onRequestedChanged: requested ? openDelay.restart() : closeDelay.restart()
    onAnchorUsableChanged: if (!anchorUsable) { openDelay.stop(); shown = false }
    onTipHoveredChanged: if (!tipHovered) closeDelay.restart()

    Timer {
        id: openDelay
        interval: 300
        onTriggered: root.shown = root.requested && root.hasContent && root.anchorUsable && AppController.overlay === ""
    }
    Timer {
        id: closeDelay
        interval: 150
        onTriggered: if (!root.requested && !root.tipHovered) root.shown = false
    }
    Connections {
        target: AppController
        enabled: root.shown
        function onOverlayChanged() { root.shown = false }
    }

    Loader {
        id: tipLoader
        objectName: "desktopGameLabelTipLoader"
        anchors.fill: parent
        active: root.shown && root.anchorUsable
        sourceComponent: Item {
            readonly property bool hovered: tipHover.hovered

            Popup {
                id: tip
                objectName: "desktopGameLabelTip"
                readonly property var launchers: ConsoleStores.launchers(root.game)
                readonly property var features: DesktopTokens.gameFeatures(root.game)
                readonly property bool locked: ShellStore.gameLockedByMembership(root.game)
                readonly property bool fitsRight: {
                    const window = root.Window.window
                    return !window || root.mapToItem(null, root.width, 0).x + 8 + width <= window.width - 8
                }
                visible: true
                closePolicy: Popup.NoAutoClose
                margins: 8
                padding: 12
                width: 236
                x: fitsRight ? root.width + 8 : -width - 8
                y: 0
                background: Rectangle {
                    radius: 12
                    color: "#F20B0F1A"
                    border.width: 1
                    border.color: "#24FFFFFF"
                    HoverHandler { id: tipHover }
                }
                contentItem: Column {
                    spacing: 8

                    Text {
                        width: parent.width
                        text: String(root.game && root.game.title || "").toUpperCase()
                        color: "#73FFFFFF"
                        elide: Text.ElideRight
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: 9
                        font.weight: Font.Bold
                        font.letterSpacing: 0.8
                    }

                    Rectangle {
                        objectName: "desktopGameLabelTipLock"
                        visible: tip.locked
                        width: parent.width
                        height: lockText.implicitHeight + 16
                        radius: 8
                        color: "#1AFFD166"
                        border.width: 1
                        border.color: "#66FFD166"

                        Image {
                            x: 10
                            y: 10
                            width: 12
                            height: 12
                            source: "qrc:/qt/qml/OpenNOW/res/icons/desktop-lock.svg"
                            sourceSize: Qt.size(32, 32)
                        }
                        Column {
                            id: lockText
                            x: 30
                            y: 8
                            width: parent.width - 40
                            spacing: 2
                            Text {
                                width: parent.width
                                text: qsTr("Needs a paid membership")
                                color: DesktopTokens.amber
                                wrapMode: Text.Wrap
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: 12
                                font.weight: Font.Bold
                            }
                            Text {
                                width: parent.width
                                text: qsTr("Your Free plan can't start this game.")
                                color: "#B3FFFFFF"
                                wrapMode: Text.Wrap
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: 11
                            }
                        }
                    }

                    Repeater {
                        model: tip.features
                        Row {
                            required property var modelData
                            spacing: 8
                            Image {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 12
                                height: 12
                                source: DesktopTokens.featureIconUrl(parent.modelData)
                                sourceSize: Qt.size(32, 32)
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                width: tip.availableWidth - 20
                                text: DesktopTokens.featureTip(parent.modelData)
                                wrapMode: Text.Wrap
                                color: "#E6FFFFFF"
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: 12
                            }
                        }
                    }

                    Rectangle {
                        visible: tip.launchers.length > 0 && (tip.features.length > 0 || tip.locked)
                        width: parent.width
                        height: 1
                        color: "#1AFFFFFF"
                    }
                    Text {
                        visible: tip.launchers.length > 0
                        text: qsTr("LAUNCHERS")
                        color: "#73FFFFFF"
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: 9
                        font.weight: Font.Bold
                        font.letterSpacing: 0.8
                    }
                    Repeater {
                        model: tip.launchers
                        Row {
                            required property var modelData
                            spacing: 8
                            Item {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 12
                                height: 12
                                Image {
                                    anchors.fill: parent
                                    source: DesktopTokens.storeIconUrl(parent.parent.modelData.store)
                                    sourceSize: Qt.size(32, 32)
                                    fillMode: Image.PreserveAspectFit
                                }
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: parent.modelData.owned
                                    ? qsTr("%1 · owned").arg(DesktopTokens.storeLabel(parent.modelData.store))
                                    : DesktopTokens.storeLabel(parent.modelData.store)
                                color: parent.modelData.owned ? "#E6FFFFFF" : "#A6FFFFFF"
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: 12
                            }
                        }
                    }

                    Row {
                        objectName: "desktopGameLabelTipGuide"
                        spacing: 6
                        Image {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 12
                            height: 12
                            source: "qrc:/qt/qml/OpenNOW/res/icons/desktop-info.svg"
                            sourceSize: Qt.size(32, 32)
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("What the icons mean")
                            color: guideHover.hovered ? Qt.lighter(Theme.focus, 1.2) : Theme.focus
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 11
                            font.weight: Font.Bold
                        }
                        HoverHandler { id: guideHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: AppController.showOverlay("desktop-icon-guide") }
                    }
                }
            }
        }
    }
}
