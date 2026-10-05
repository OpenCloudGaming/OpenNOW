import QtQuick
import QtQuick.Controls
import QtMultimedia
import OpenNOW

FocusScope {
    id: root
    anchors.fill: parent
    focus: true
    Rectangle { anchors.fill: parent; color: "#05070D" }
    VideoOutput {
        id: output
        anchors.fill: parent
        fillMode: VideoOutput.PreserveAspectCrop
    }
    QueueAdPlayback {
        id: player
        videoOutput: output
    }

    Rectangle {
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
        height: 190
        gradient: Gradient {
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 1; color: Qt.rgba(0,0,0,0.9) }
        }
        Column {
            anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
            anchors.margins: 42; spacing: 10
            Text { text: qsTr("YOUR RIG IS GETTING READY"); color: Theme.mint; font.family: Theme.monoFont; font.pixelSize: 12; font.weight: Font.Black; font.letterSpacing: 1.5 }
            Text { text: player.ad ? (player.ad.title || player.adState.message || qsTr("A short message while you wait")) : qsTr("Preparing your session"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 30; font.weight: Font.Black }
            ProgressBar { width: parent.width; from: 0; to: Math.max(1, player.duration); value: player.position }
        }
    }

    GlassButton {
        anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 34
        text: player.playbackState === MediaPlayer.PlayingState ? qsTr("Pause") : qsTr("Resume"); glyph: "A"
        onClicked: player.toggle()
        Component.onCompleted: forceActiveFocus()
    }

}
