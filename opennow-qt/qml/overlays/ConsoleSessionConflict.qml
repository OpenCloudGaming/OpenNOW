import QtQuick
import OpenNOW

FocusScope {
    id: root
    objectName: "sessionConflictDialog"
    anchors.fill: parent
    focus: true
    Accessible.name: qsTr("Existing GeForce NOW session")
    Accessible.role: Accessible.Dialog

    readonly property var conflict: ShellStore.conflictSession
    readonly property string runningTitle: ShellStore.sessionGameTitle(conflict)
    readonly property var runningGame: {
        const appId = String(conflict && conflict.appId || "")
        if (appId === "")
            return null
        const games = ShellStore.catalogGames || []
        return games.find(game => String(game.launchAppId || "") === appId
            || (game.variants || []).some(variant => String(variant && variant.id || "") === appId)) || null
    }
    readonly property var launchGame: ShellStore.selectedGame
        || ({ title: ShellStore.pendingLaunchParams && ShellStore.pendingLaunchParams.title || qsTr("GeForce NOW") })
    readonly property string launchTitle: ShellStore.pendingLaunchParams
        ? String(ShellStore.pendingLaunchParams.title || launchGame.title || qsTr("your selected game")) : ""

    Component.onCompleted: resumeButton.forceActiveFocus()

    SessionGlyphs { id: glyphs }

    LaunchStage {
        anchors.fill: parent
        game: root.launchGame
        tone: Theme.yellow
        statusText: qsTr("Needs a decision")
        eyebrow: qsTr("Your game is still running")
        headline: root.runningTitle ? qsTr("Return to %1?").arg(root.runningTitle) : qsTr("Return to your game?")
        detail: qsTr("OpenNOW found another session on your NVIDIA account. Ending it closes the game, and unsaved progress may be lost.")

        actions: [
            LaunchStage.LaunchAction {
                id: resumeButton
                objectName: "conflictResumeButton"
                primary: true
                text: root.runningTitle ? qsTr("Return to %1").arg(root.runningTitle) : qsTr("Return to game")
                detail: qsTr("Resume the running session")
                glyph: glyphs.button("A")
                keyboardGlyph: glyphs.keyboard
                KeyNavigation.down: newButton
                onClicked: ShellStore.resolveSessionConflict("resume")
            },
            LaunchStage.LaunchAction {
                id: newButton
                objectName: "conflictEndAndStartButton"
                danger: true
                text: root.launchTitle === "" ? qsTr("End game")
                    : root.runningTitle ? qsTr("End %1 and start %2").arg(root.runningTitle).arg(root.launchTitle)
                    : qsTr("End game and start %1").arg(root.launchTitle)
                detail: qsTr("Closes the running game first")
                KeyNavigation.up: resumeButton
                KeyNavigation.down: cancelButton
                onClicked: ShellStore.resolveSessionConflict("new")
            },
            LaunchStage.LaunchAction {
                id: cancelButton
                objectName: "conflictCancelButton"
                text: qsTr("Cancel")
                glyph: glyphs.button("B")
                keyboardGlyph: glyphs.keyboard
                KeyNavigation.up: newButton
                onClicked: ShellStore.resolveSessionConflict("cancel")
            }
        ]

        aside: [
            Rectangle {
                x: Math.round((parent.width - width) / 2)
                width: 520
                height: 410
                radius: 40
                clip: true
                color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.86)
                border.width: 1
                border.color: Theme.seam
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Still running: %1").arg(root.runningTitle || qsTr("a game on your GeForce NOW account"))

                ArtworkSource {
                    id: runningArt
                    sourceUrl: DesktopTokens.artworkUrl(root.runningGame, true)
                    active: root.visible && sourceUrl !== ""
                }
                Image {
                    anchors.fill: parent
                    source: runningArt.resolvedUrl
                    fillMode: Image.PreserveAspectCrop
                    sourceSize: Qt.size(1040, 820)
                    asynchronous: true
                }
                Rectangle {
                    anchors.fill: parent
                    gradient: Gradient {
                        GradientStop { position: 0.35; color: "transparent" }
                        GradientStop { position: 1; color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.96) }
                    }
                }
                Rectangle {
                    x: 26; y: 26
                    width: runningRow.implicitWidth + 28; height: 36; radius: 18
                    color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.8)
                    Row {
                        id: runningRow
                        anchors.centerIn: parent
                        spacing: 8
                        Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 8; height: 8; radius: 4; color: Theme.mint }
                        Text {
                            text: qsTr("Still running").toUpperCase()
                            color: Theme.mint
                            font.family: Theme.monoFont; font.pixelSize: 14; font.weight: Font.Bold; font.letterSpacing: 2
                        }
                    }
                }
                Column {
                    x: 32
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 30
                    width: parent.width - 64
                    Text {
                        width: parent.width
                        text: root.runningTitle || qsTr("Your running game")
                        elide: Text.ElideRight
                        color: Theme.label
                        font.family: Theme.displayFont; font.pixelSize: 34; font.weight: Font.Black
                    }
                    Text {
                        text: qsTr("On your GeForce NOW account")
                        color: Theme.textMuted
                        font.family: Theme.bodyFont; font.pixelSize: 18; font.weight: Font.DemiBold
                    }
                }
            }
        ]
    }
}
