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
        copyWidth: 860
        detailWidth: 780

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
                id: runningCard
                objectName: "conflictRunningCard"
                x: parent.width - width - 40
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
                Item {
                    x: runningCard.border.width; y: runningCard.border.width
                    width: parent.width - 2 * x
                    height: 300
                    Image {
                        anchors.fill: parent
                        source: runningArt.resolvedUrl
                        fillMode: Image.PreserveAspectCrop
                        sourceSize: Qt.size(1040, 600)
                        asynchronous: true
                    }
                    Rectangle {
                        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
                        height: 150
                        gradient: Gradient {
                            GradientStop { position: 0; color: "transparent" }
                            GradientStop { position: 1; color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.95) }
                        }
                    }
                    Rectangle {
                        x: 24; y: 24
                        width: runningRow.implicitWidth + 28; height: 34; radius: 17
                        color: Qt.rgba(0, 0, 0, 0.55)
                        Row {
                            id: runningRow
                            anchors.centerIn: parent
                            spacing: 8
                            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 9; height: 9; radius: 4.5; color: Theme.mint }
                            Text {
                                text: qsTr("Still running").toUpperCase()
                                color: Theme.mint
                                font.family: Theme.monoFont; font.pixelSize: 13; font.weight: Font.Bold; font.letterSpacing: 1.3
                            }
                        }
                    }
                }
                Text {
                    x: 32
                    anchors.baseline: parent.top
                    anchors.baselineOffset: 339.5
                    width: parent.width - 64
                    text: root.runningTitle || qsTr("Your running game")
                    elide: Text.ElideRight
                    color: Theme.label
                    font.family: Theme.displayFont; font.pixelSize: 32; font.weight: Font.Black
                }
                Text {
                    x: 32
                    anchors.baseline: parent.top
                    anchors.baselineOffset: 371.6
                    width: parent.width - 64
                    elide: Text.ElideRight
                    text: qsTr("On your GeForce NOW account")
                    color: Theme.textMuted
                    font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.Bold
                }
            }
        ]
    }
}
