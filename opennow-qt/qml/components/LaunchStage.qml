import QtQuick
import OpenNOW

Item {
    id: root
    objectName: "launchStage"
    property var game: ({})
    readonly property var variant: game && game.variants && Number(game.selectedVariantIndex) >= 0
        ? (game.variants[Number(game.selectedVariantIndex)] || null) : null
    property string subtitle: [variant && variant.store ? DesktopTokens.storeLabel(variant.store) : "",
        String((ShellStore.activeSession || {}).serverLocation || ShellStore.selectedRegion || "")]
        .filter(part => part !== "").join(" · ")
    property string statusText: ""
    property color tone: Theme.focus
    property string eyebrow: ""
    property string headline: ""
    property string detail: ""
    property color detailColor: Theme.textMuted
    property bool railVisible: false
    property int activeStep: 1
    property int failedStep: 0
    property string activeStepDetail: ""
    property bool activityRunning: true
    property double waitStartedMs: 0
    property string footerText: ""
    property string footerGlyph: "B"
    property bool footerVisible: footerText !== ""
    property alias aside: asideSlot.data
    property alias actions: actionColumn.data
    signal footerRequested()

    property double nowMs: Date.now()
    readonly property int waitedSeconds: waitStartedMs > 0 ? Math.max(0, Math.floor((nowMs - waitStartedMs) / 1000)) : 0
    readonly property string artwork: DesktopTokens.artworkUrl(game, true)
    readonly property string iconArtwork: DesktopTokens.decodeArtworkUrl(game && (game.boxArtUrl || game.imageUrl) || "")

    function clock(seconds) {
        const hours = Math.floor(seconds / 3600)
        const minutes = Math.floor((seconds % 3600) / 60)
        return (hours > 0 ? hours + ":" + String(minutes).padStart(2, "0") : String(minutes).padStart(2, "0"))
            + ":" + String(seconds % 60).padStart(2, "0")
    }

    function stepLabel(step) {
        if (step === 1) return qsTr("Session requested")
        if (step === 2) return root.activeStep > 2 && root.failedStep !== 2 ? qsTr("Got a rig") : qsTr("Waiting for a rig")
        if (step === 3) return root.activeStep > 3 && root.failedStep !== 3 ? qsTr("Rig is set up") : qsTr("Setting up the rig")
        return qsTr("Connecting the stream")
    }

    component LaunchAction: ConsoleActionButton {
        id: action
        property string detail: ""
        property bool keyboardGlyph: false
        width: 640
        implicitWidth: 640
        implicitHeight: detail !== "" ? 84 : 72
        Accessible.description: detail
        contentItem: Item {
            implicitWidth: actionRow.implicitWidth
            implicitHeight: actionRow.implicitHeight
            Row {
                id: actionRow
                anchors.verticalCenter: parent.verticalCenter
                spacing: 16
                opacity: action.enabled ? 1 : 0.5
                ControllerGlyph {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: action.glyph !== ""
                    glyph: action.glyph
                    keyboard: action.keyboardGlyph
                    label: ""
                    glyphSize: 34
                    glyphColor: action.primary ? Theme.faceText : action.danger ? Theme.coral : Theme.face
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Text {
                        width: Math.min(implicitWidth, action.availableWidth - (action.glyph !== "" ? 50 : 0))
                        text: action.text
                        elide: Text.ElideRight
                        color: action.inkColor
                        font.family: Theme.displayFont; font.pixelSize: 24; font.weight: Font.Black
                    }
                    Text {
                        visible: action.detail !== ""
                        width: Math.min(implicitWidth, action.availableWidth - (action.glyph !== "" ? 50 : 0))
                        text: action.detail
                        elide: Text.ElideRight
                        color: action.primary ? Qt.rgba(Theme.faceText.r, Theme.faceText.g, Theme.faceText.b, 0.64) : Theme.textMuted
                        font.family: Theme.bodyFont; font.pixelSize: 16; font.weight: Font.DemiBold
                    }
                }
            }
        }
    }

    component InfoCard: Rectangle {
        id: card
        property string title: ""
        property string footnote: ""
        default property alias content: cardBody.data
        width: 520
        height: cardColumn.implicitHeight + 72
        radius: 40
        color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.86)
        border.width: 1
        border.color: Theme.seam
        Column {
            id: cardColumn
            x: 36; y: 36
            width: card.width - 72
            spacing: 18
            Text {
                text: card.title.toUpperCase()
                color: Theme.textMuted
                font.family: Theme.monoFont; font.pixelSize: 14; font.weight: Font.Bold; font.letterSpacing: 3
            }
            Column { id: cardBody; width: parent.width; spacing: 14 }
            Rectangle { visible: card.footnote !== ""; width: parent.width; height: 1; color: Theme.seam }
            Text {
                visible: card.footnote !== ""
                width: parent.width
                text: card.footnote
                wrapMode: Text.WordWrap
                color: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: 16
            }
        }
    }

    component RigCard: InfoCard {
        id: rigCard
        property var rows: []
        title: qsTr("Your rig")
        footnote: qsTr("Only values the session reports are shown. Rows without data are hidden, never estimated.")
        visible: rows.length > 0
        Repeater {
            model: rigCard.rows
            Item {
                required property var modelData
                width: parent.width
                height: 34
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData.label
                    color: Theme.textMuted
                    font.family: Theme.bodyFont; font.pixelSize: 22
                }
                Text {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, parent.width * 0.62)
                    horizontalAlignment: Text.AlignRight
                    elide: Text.ElideLeft
                    text: modelData.value
                    color: Theme.label
                    font.family: Theme.bodyFont; font.pixelSize: 22; font.weight: Font.Black
                }
            }
        }
    }

    component StopCard: InfoCard {
        id: stopCard
        property int reachedStep: 1
        property string code: ""
        title: qsTr("Where it stopped")
        footnote: code !== "" ? qsTr("Code %1").arg(code) : ""
        Repeater {
            model: Math.min(4, stopCard.reachedStep + 1)
            Row {
                id: stopRow
                required property int index
                readonly property int step: index + 1
                readonly property bool failed: step === stopCard.reachedStep
                readonly property bool done: step < stopCard.reachedStep
                spacing: 16
                Rectangle {
                    width: 34; height: 34; radius: 17
                    color: stopRow.done ? Theme.mint : stopRow.failed ? Theme.coral : "transparent"
                    border.width: stopRow.done || stopRow.failed ? 0 : 2
                    border.color: Theme.seam
                    Text {
                        anchors.centerIn: parent
                        visible: stopRow.done || stopRow.failed
                        text: stopRow.done ? "✓" : "✕"
                        color: Theme.contrastText(parent.color)
                        font.family: Theme.displayFont; font.pixelSize: 18; font.weight: Font.Black
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: stopRow.step === 1 ? qsTr("Session requested") : stopRow.step === 2 ? qsTr("Waiting for a rig")
                        : stopRow.step === 3 ? qsTr("Setting up the rig") : qsTr("Connecting the stream")
                    color: stopRow.failed ? Theme.coral : stopRow.done ? Theme.label : Theme.textMuted
                    font.family: Theme.displayFont; font.pixelSize: 22; font.weight: Font.Bold
                }
            }
        }
    }

    SessionGlyphs { id: glyphs }

    Timer {
        interval: 1000
        repeat: true
        running: root.visible && root.waitStartedMs > 0
        triggeredOnStart: true
        onTriggered: root.nowMs = Date.now()
    }

    Rectangle { anchors.fill: parent; color: Theme.shell }

    ArtworkSource {
        id: artworkSource
        sourceUrl: root.artwork
        active: root.visible && root.artwork !== ""
    }

    Image {
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.right: parent.right
        width: parent.width * 0.7
        source: artworkSource.resolvedUrl
        fillMode: Image.PreserveAspectCrop
        sourceSize: Qt.size(Math.ceil(width), Math.ceil(height))
        asynchronous: true
        opacity: status === Image.Ready ? 0.62 : 0
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Theme.shell }
            GradientStop { position: 0.34; color: Theme.shell }
            GradientStop { position: 0.62; color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.66) }
            GradientStop { position: 1; color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.34) }
        }
    }

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: parent.height * 0.3
        gradient: Gradient {
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 1; color: Theme.shell }
        }
    }

    Row {
        x: 120; y: 72
        spacing: 14
        Rectangle {
            width: 44; height: 44; radius: 10
            color: Theme.glassStrong
            clip: true
            visible: root.iconArtwork !== ""
            ArtworkSource { id: iconSource; sourceUrl: root.iconArtwork; active: root.visible && root.iconArtwork !== "" }
            Image {
                anchors.fill: parent
                source: iconSource.resolvedUrl
                fillMode: Image.PreserveAspectCrop
                sourceSize: Qt.size(88, 88)
                asynchronous: true
            }
        }
        Column {
            anchors.verticalCenter: parent.verticalCenter
            Text {
                width: Math.min(implicitWidth, 900)
                text: String(root.game && root.game.title || qsTr("GeForce NOW"))
                elide: Text.ElideRight
                color: Theme.label
                font.family: Theme.displayFont; font.pixelSize: 22; font.weight: Font.Black
            }
            Text {
                visible: root.subtitle !== ""
                text: root.subtitle
                color: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: 16; font.weight: Font.DemiBold
            }
        }
    }

    Rectangle {
        visible: root.statusText !== ""
        anchors.right: parent.right; anchors.rightMargin: 120
        y: 72
        width: statusRow.implicitWidth + 40; height: 44; radius: 22
        color: Qt.rgba(root.tone.r, root.tone.g, root.tone.b, 0.12)
        border.width: 1.5
        border.color: Qt.rgba(root.tone.r, root.tone.g, root.tone.b, 0.5)
        Accessible.role: Accessible.StaticText
        Accessible.name: root.statusText
        Row {
            id: statusRow
            anchors.centerIn: parent
            spacing: 10
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 8; height: 8; radius: 4; color: root.tone }
            Text {
                text: root.statusText.toUpperCase()
                color: root.tone
                font.family: Theme.monoFont; font.pixelSize: 15; font.weight: Font.Bold; font.letterSpacing: 2
            }
        }
    }

    Column {
        id: copy
        x: 120; y: 212
        width: Math.min(980, root.width * 0.52)
        spacing: 0
        Text {
            visible: root.eyebrow !== ""
            text: root.eyebrow.toUpperCase()
            color: root.tone
            font.family: Theme.monoFont; font.pixelSize: 16; font.weight: Font.Bold; font.letterSpacing: 3.5
        }
        Item { width: 1; height: 14 }
        Text {
            width: parent.width
            text: root.headline
            color: Theme.label
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
            lineHeight: 0.96
            font.family: Theme.displayFont; font.pixelSize: 84; font.weight: Font.Black
            Accessible.role: Accessible.Heading
            Accessible.name: text
        }
        Item { width: 1; height: 22 }
        Text {
            width: Math.min(parent.width, 880)
            visible: root.detail !== ""
            text: root.detail
            color: root.detailColor
            wrapMode: Text.WordWrap
            maximumLineCount: 4
            elide: Text.ElideRight
            lineHeight: 1.2
            font.family: Theme.bodyFont; font.pixelSize: 22
        }
    }

    Column {
        id: actionColumn
        x: 120; y: 556
        width: 640
        spacing: 14
    }

    Column {
        visible: root.railVisible
        x: 120
        y: Math.max(copy.y + copy.height + 40, 644)
        spacing: 26
        Accessible.role: Accessible.List
        Accessible.name: qsTr("Session steps")
        Repeater {
            model: 4
            Row {
                id: stepRow
                required property int index
                readonly property int step: index + 1
                readonly property bool failed: root.failedStep === step
                readonly property bool done: !failed && step < root.activeStep
                readonly property bool current: !failed && step === root.activeStep
                spacing: 18
                Accessible.role: Accessible.ListItem
                Accessible.name: root.stepLabel(step) + (done ? ", " + qsTr("done") : current ? ", " + qsTr("in progress") : failed ? ", " + qsTr("stopped here") : "")

                Item {
                    width: 40; height: 40
                    anchors.verticalCenter: parent.verticalCenter
                    Rectangle {
                        anchors.fill: parent
                        radius: 20
                        color: stepRow.done ? Theme.mint : stepRow.failed ? Theme.coral : "transparent"
                        border.width: stepRow.done || stepRow.failed ? 0 : 2
                        border.color: stepRow.current ? Qt.rgba(root.tone.r, root.tone.g, root.tone.b, 0.3) : Theme.seam
                        Text {
                            anchors.centerIn: parent
                            visible: !stepRow.current
                            text: stepRow.done ? "✓" : stepRow.failed ? "✕" : String(stepRow.step)
                            color: stepRow.done || stepRow.failed ? Theme.contrastText(stepRow.done ? Theme.mint : Theme.coral) : Theme.textMuted
                            font.family: Theme.displayFont; font.pixelSize: stepRow.done || stepRow.failed ? 20 : 15; font.weight: Font.Black
                        }
                    }
                    Canvas {
                        id: arc
                        anchors.fill: parent
                        visible: stepRow.current
                        property color ink: root.tone
                        onInkChanged: requestPaint()
                        onPaint: {
                            const context = getContext("2d")
                            context.reset()
                            context.lineWidth = 3
                            context.lineCap = "round"
                            context.strokeStyle = ink
                            context.beginPath()
                            context.arc(width / 2, height / 2, width / 2 - 2, -Math.PI / 2, Math.PI * 0.15)
                            context.stroke()
                        }
                        RotationAnimator on rotation {
                            from: 0; to: 360; duration: 1200
                            loops: Animation.Infinite
                            running: stepRow.current && root.activityRunning && root.visible && !AppController.reducedMotion
                        }
                    }
                    Rectangle {
                        visible: stepRow.current
                        anchors.centerIn: parent
                        width: 12; height: 12; radius: 6
                        color: root.tone
                    }
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Text {
                        text: root.stepLabel(stepRow.step)
                        color: stepRow.failed ? Theme.coral : stepRow.done || stepRow.current ? Theme.label : Theme.textMuted
                        font.family: Theme.displayFont
                        font.pixelSize: stepRow.current ? 26 : 22
                        font.weight: stepRow.current ? Font.Black : Font.Bold
                    }
                    Text {
                        visible: stepRow.current && root.activeStepDetail !== ""
                        text: root.activeStepDetail
                        color: Theme.textMuted
                        font.family: Theme.bodyFont; font.pixelSize: 15; font.weight: Font.DemiBold
                    }
                }
            }
        }
    }

    Item {
        id: asideSlot
        x: Math.round(root.width * 0.52)
        y: 212
        width: root.width - x - 120
        height: Math.max(0, root.height - y - 200)
    }

    Text {
        visible: root.waitStartedMs > 0
        x: 120
        anchors.bottom: parent.bottom; anchors.bottomMargin: 86
        text: qsTr("Waited %1 · measured on this device").arg(root.clock(root.waitedSeconds)).toUpperCase()
        color: Theme.textMuted
        font.family: Theme.monoFont; font.pixelSize: 16; font.weight: Font.Medium; font.letterSpacing: 2
    }

    MouseArea {
        visible: root.footerVisible
        anchors.right: parent.right; anchors.rightMargin: 120
        anchors.bottom: parent.bottom; anchors.bottomMargin: 78
        width: footerRow.implicitWidth; height: 44
        cursorShape: Qt.PointingHandCursor
        Accessible.role: Accessible.Button
        Accessible.name: root.footerText
        Accessible.onPressAction: root.footerRequested()
        onClicked: root.footerRequested()
        ControllerGlyph {
            id: footerRow
            anchors.verticalCenter: parent.verticalCenter
            glyph: glyphs.button(root.footerGlyph)
            keyboard: glyphs.keyboard
            glyphColor: Theme.coral
            glyphSize: 30
            label: root.footerText
        }
    }
}
