import QtQuick
import OpenNOW

Item {
    id: root
    property var game: null
    property real liftedBottom: 0
    readonly property var launchers: ConsoleStores.launchers(game).filter(item => DesktopTokens.storeIconUrl(item.store) !== "")
    readonly property var features: DesktopTokens.gameFeatures(game)
    readonly property bool locked: ShellStore.gameLockedByMembership(game)
    readonly property bool hasContent: launchers.length > 0 || features.length > 0 || locked
    readonly property int maxLaunchers: 3
    readonly property int pillPadding: 7
    readonly property int glyphSize: 12
    readonly property int glyphGap: 5
    readonly property int overflowWidth: 16
    readonly property int shownLaunchers: {
        const span = count => pillPadding * 2 + count * glyphSize + Math.max(0, count - 1) * 6
            + (count < launchers.length ? 6 + overflowWidth : 0)
        const reserve = features.length > 0 ? 6 + pillPadding * 2 + overflowWidth : 0
        let count = Math.min(maxLaunchers, launchers.length)
        while (count > 1 && span(count) + reserve > width)
            --count
        return count
    }
    readonly property real featureRoom: width - (launcherPill.visible ? launcherPill.width + 6 : 0) - pillPadding * 2
    readonly property int shownFeatures: {
        const span = count => count * glyphSize + Math.max(0, count - 1) * glyphGap
        if (span(features.length) <= featureRoom)
            return features.length
        for (let count = features.length - 1; count > 0; --count) {
            if (span(count) + glyphGap + overflowWidth <= featureRoom)
                return count
        }
        return 0
    }
    readonly property string summary: [locked ? qsTr("Needs a paid membership") : ""]
        .concat(launchers.map(item => item.owned ? qsTr("%1, owned").arg(DesktopTokens.storeLabel(item.store)) : DesktopTokens.storeLabel(item.store)))
        .concat(features.map(key => DesktopTokens.featureLabel(key)))
        .filter(part => part !== "").join(". ")

    objectName: "desktopGameLabels"
    visible: hasContent

    Rectangle {
        id: launcherPill
        objectName: "desktopGameLaunchers"
        visible: root.launchers.length > 0
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.liftedBottom
        width: launcherRow.implicitWidth + root.pillPadding * 2
        height: 22
        radius: 7
        color: "#D90B0F1A"
        border.width: 1
        border.color: "#24FFFFFF"

        Row {
            id: launcherRow
            x: root.pillPadding
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6

            Repeater {
                model: root.shownLaunchers
                Item {
                    id: launcherMark
                    required property int index
                    readonly property var launcher: root.launchers[index]
                    objectName: "desktopGameLauncher" + index
                    anchors.verticalCenter: launcherRow.verticalCenter
                    width: root.glyphSize
                    height: root.glyphSize
                    opacity: launcher.owned ? 1 : 0.5

                    Image {
                        anchors.fill: parent
                        source: DesktopTokens.storeIconUrl(launcherMark.launcher.store)
                        sourceSize: Qt.size(32, 32)
                        fillMode: Image.PreserveAspectFit
                    }
                    Rectangle {
                        objectName: "desktopGameOwnedDot"
                        visible: launcherMark.launcher.owned
                        x: parent.width - 3
                        y: parent.height - 3
                        width: 6
                        height: 6
                        radius: 3
                        color: DesktopTokens.green
                        border.width: 1
                        border.color: "#0B0F1A"
                    }
                }
            }
            Text {
                objectName: "desktopGameLauncherOverflow"
                anchors.verticalCenter: parent.verticalCenter
                visible: root.launchers.length > root.shownLaunchers
                text: "+" + (root.launchers.length - root.shownLaunchers)
                color: "#CCFFFFFF"
                font.family: DesktopTokens.monoFont
                font.pixelSize: 10
                font.weight: Font.Bold
            }
        }
    }

    Rectangle {
        objectName: "desktopGameFeatures"
        visible: root.features.length > 0
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.liftedBottom
        width: featureRow.implicitWidth + root.pillPadding * 2
        height: 22
        radius: 7
        color: "#D90B0F1A"
        border.width: 1
        border.color: "#24FFFFFF"

        Row {
            id: featureRow
            x: root.pillPadding
            anchors.verticalCenter: parent.verticalCenter
            spacing: root.glyphGap

            Repeater {
                model: root.shownFeatures
                Image {
                    required property int index
                    objectName: "desktopGameFeature-" + root.features[index]
                    anchors.verticalCenter: featureRow.verticalCenter
                    width: root.glyphSize
                    height: root.glyphSize
                    source: DesktopTokens.featureIconUrl(root.features[index])
                    sourceSize: Qt.size(32, 32)
                    fillMode: Image.PreserveAspectFit
                }
            }
            Text {
                objectName: "desktopGameFeatureOverflow"
                anchors.verticalCenter: parent.verticalCenter
                visible: root.shownFeatures < root.features.length
                text: "+" + (root.features.length - root.shownFeatures)
                color: "#CCFFFFFF"
                font.family: DesktopTokens.monoFont
                font.pixelSize: 10
                font.weight: Font.Bold
            }
        }
    }

    Rectangle {
        objectName: "desktopGameLock"
        visible: root.locked
        anchors.right: parent.right
        anchors.top: parent.top
        width: 22
        height: 22
        radius: 11
        color: "#D90B0F1A"
        border.width: 1
        border.color: "#24FFFFFF"

        Image {
            anchors.centerIn: parent
            width: 12
            height: 12
            source: "qrc:/qt/qml/OpenNOW/res/icons/desktop-lock.svg"
            sourceSize: Qt.size(32, 32)
            fillMode: Image.PreserveAspectFit
        }
    }
}
