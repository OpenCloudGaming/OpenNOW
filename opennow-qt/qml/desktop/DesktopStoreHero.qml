import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import OpenNOW

Item {
    id: root

    // Marquee slides from the CMS panels document:
    // {kind:"game"|"marketing", title, body, image, game?, actionLabel?}
    property var slides: []
    property int currentSlide: 0
    property int selectedAction: -1

    signal playRequested(var game)
    signal detailsRequested(var game)
    signal actionPointed(int index)

    width: 1160
    height: 260

    readonly property var slide: slides.length ? slides[Math.max(0, Math.min(currentSlide, slides.length - 1))] : null
    readonly property var slideGame: slide && slide.game ? slide.game : null

    function nextSlide() {
        if (slides.length > 1)
            currentSlide = (currentSlide + 1) % slides.length
    }

    onSlidesChanged: currentSlide = 0

    Timer {
        id: advanceTimer
        interval: 6000
        repeat: true
        running: root.visible && slides.length > 1 && !AppController.reducedMotion && !heroHover.hovered
        onTriggered: root.nextSlide()
    }

    Rectangle {
        id: heroMask
        anchors.fill: parent
        radius: 16
        color: "white"
        visible: false
        layer.enabled: true
    }

    Item {
        anchors.fill: parent
        layer.enabled: true
        layer.smooth: true
        layer.effect: MultiEffect {
            maskEnabled: true
            maskSource: heroMask
            maskThresholdMin: 0.25
            maskSpreadAtMin: 0.2
        }

        Rectangle {
            anchors.fill: parent
            color: Qt.rgba(0.043, 0.059, 0.102, 0.72)
        }

        Repeater {
            model: root.slides
            Item {
                required property var modelData
                required property int index
                anchors.fill: parent
                visible: opacity > 0
                opacity: index === root.currentSlide ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: AppController.reducedMotion ? 0 : 450; easing.type: Easing.OutCubic } }

                Image {
                    x: Math.round(parent.width * 0.28)
                    width: parent.width - x
                    height: parent.height
                    source: DesktopTokens.decodeArtworkUrl(String(modelData.image || ""))
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    cache: true
                    sourceSize: Qt.size(Math.ceil(width), Math.ceil(height))
                }
            }
        }

        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: Qt.rgba(0.043, 0.059, 0.102, 0.96) }
                GradientStop { position: 0.45; color: Qt.rgba(0.043, 0.059, 0.102, 0.72) }
                GradientStop { position: 0.75; color: Qt.rgba(0.043, 0.059, 0.102, 0.12) }
                GradientStop { position: 1; color: "transparent" }
            }
        }
    }

    Rectangle {
        anchors.fill: parent
        radius: 16
        color: "transparent"
        border.width: 1
        border.color: Qt.rgba(1, 1, 1, 0.12)
    }

    HoverHandler { id: heroHover }

    Column {
        x: 24
        y: 24
        width: Math.min(520, Math.max(300, root.width * 0.42))
        spacing: 10

        Text {
            text: root.slide && root.slide.kind === "marketing" ? qsTr("GEFORCE NOW") : qsTr("FEATURED")
            color: DesktopTokens.mint
            font.family: Theme.monoFont
            font.pixelSize: DesktopTokens.microSize
            font.weight: Font.Bold
            font.letterSpacing: 1.1
        }

        Text {
            width: parent.width
            text: root.slide ? String(root.slide.title || "") : ""
            color: "#FFFFFF"
            font.family: Theme.displayFont
            font.pixelSize: DesktopTokens.px(30)
            font.weight: Font.Black
            font.letterSpacing: -0.9
            elide: Text.ElideRight
            maximumLineCount: 2
            wrapMode: Text.WordWrap
        }

        Text {
            width: parent.width
            visible: text !== ""
            text: root.slide ? String(root.slide.body || "") : ""
            color: Qt.rgba(1, 1, 1, 0.72)
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.captionSize
            font.weight: Font.Medium
            lineHeightMode: Text.FixedHeight
            lineHeight: 19
            maximumLineCount: 2
            elide: Text.ElideRight
            wrapMode: Text.WordWrap
        }
    }

    Row {
        x: 24
        y: parent.height - 58
        height: 36
        spacing: 9
        visible: root.slideGame !== null

        Button {
            id: playButton
            width: 121
            height: 36
            padding: 0
            focusPolicy: Qt.NoFocus
            hoverEnabled: true
            Accessible.name: text
            text: qsTr("Play")
            onHoveredChanged: if (hovered) root.actionPointed(0)
            onClicked: if (root.slideGame) root.playRequested(root.slideGame)
            background: Rectangle {
                radius: 10
                color: playButton.down ? Qt.rgba(1, 1, 1, 0.82) : Qt.rgba(1, 1, 1, 0.95)
                border.width: root.selectedAction === 0 ? 2 : 0
                border.color: DesktopTokens.focus
            }
            contentItem: Text {
                text: playButton.text
                color: "#0B0F1A"
                font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.captionSize
                font.weight: Font.ExtraBold
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
            }
        }

        Button {
            id: detailsButton
            width: 153
            height: 36
            padding: 0
            focusPolicy: Qt.NoFocus
            hoverEnabled: true
            Accessible.name: text
            text: qsTr("View details")
            onHoveredChanged: if (hovered) root.actionPointed(1)
            onClicked: if (root.slideGame) root.detailsRequested(root.slideGame)
            background: Rectangle {
                radius: 10
                color: detailsButton.down ? Qt.rgba(1, 1, 1, 0.16) : Qt.rgba(1, 1, 1, 0.10)
                border.width: root.selectedAction === 1 ? 2 : 1
                border.color: root.selectedAction === 1 ? DesktopTokens.focus : Qt.rgba(1, 1, 1, 0.18)
            }
            contentItem: Text {
                text: detailsButton.text
                color: Qt.rgba(1, 1, 1, 0.88)
                font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.captionSize
                font.weight: Font.Bold
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
            }
        }
    }

    Row {
        x: 24
        y: parent.height - 20
        spacing: 6
        visible: root.slides.length > 1
        Repeater {
            model: root.slides.length
            Rectangle {
                required property int index
                width: index === root.currentSlide ? 18 : 6
                height: 6
                radius: 3
                color: index === root.currentSlide ? DesktopTokens.mint : Qt.rgba(1, 1, 1, 0.28)
                Behavior on width { NumberAnimation { duration: 180 } }
                HoverHandler { cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.currentSlide = index }
            }
        }
    }
}
