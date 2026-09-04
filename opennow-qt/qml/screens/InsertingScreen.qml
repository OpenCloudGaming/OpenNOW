import QtQuick
import OpenNOW

FocusScope {
    id: root
    readonly property var game: ShellStore.selectedGame || ({ title: qsTr("GeForce NOW") })
    readonly property var session: ShellStore.activeSession || ({})
    ScreenBackground { artwork: root.game.heroImageUrl || root.game.imageUrl || ""; tint: "#24354B" }
    GlassPanel {
        anchors.centerIn: parent; width: 720; height: 360; panelRadius: 40; strong: true
        Column {
            anchors.centerIn: parent; spacing: 24
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter; width: 84; height: 84; radius: 42; color: Theme.glassStrong; border.color: Theme.focus; border.width: 4
                Rectangle { anchors.centerIn: parent; width: 22; height: 22; radius: 11; color: Theme.focus }
                RotationAnimation on rotation { from: 0; to: 360; duration: 1100; loops: Animation.Infinite; running: !AppController.reducedMotion }
            }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: ShellStore.streamState === "error" ? qsTr("Session could not start") : qsTr("Inserting you into the cloud"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: I18n.source(ShellStore.streamMessage, I18n.revision); color: ShellStore.streamState === "error" ? Theme.coral : Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 17 }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: root.game.title + (root.session.gpuType ? " · " + root.session.gpuType : ""); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 14; font.weight: Font.DemiBold }
            GlassButton { id: cancelButton; anchors.horizontalCenter: parent.horizontalCenter; width: 220; text: ShellStore.streamState === "error" ? qsTr("Back") : qsTr("Cancel session"); glyph: "B"; onClicked: ShellStore.streamState === "error" ? AppController.navigateFromLastPrimary("game-detail") : ShellStore.stopStreamingSession(); Component.onCompleted: forceActiveFocus() }
        }
    }
    Keys.onPressed: event => {
        if (event.isAutoRepeat)
            return
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            event.accepted = true
            if (ShellStore.streamState === "error")
                AppController.navigateFromLastPrimary("game-detail")
            else
                ShellStore.stopStreamingSession()
        }
    }
    AppChrome { anchors.fill: parent; title: qsTr("Preparing session"); currentRoute: "home"; bottomVisible: false }
}
