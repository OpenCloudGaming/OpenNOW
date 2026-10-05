pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Shapes
import QtMultimedia
import OpenNOW

Item {
    id: root
    objectName: "consoleLaunchAnimation"

    property bool reducedMotion: false
    property bool soundEnabled: false
    property bool destinationReady: false
    property bool presentationReady: true
    property string statusText: ""

    readonly property bool active: clock.variant !== ""
    readonly property string variant: clock.variant
    readonly property real destinationScale: active ? clock.destinationScale : 1
    readonly property real chromeProgress: active ? clock.chromeProgress : 1
    readonly property bool focusHeld: active

    signal coverReached()
    signal finished()

    function start(variant) {
        if (active || (variant !== "cold" && variant !== "quick"))
            return
        clock.reset()
        clock.path = reducedMotion ? "fade" : "vector"
        clock.gate = variant === "cold" ? (reducedMotion ? 680 : 760) : (reducedMotion ? 0 : 160)
        clock.end = clock.path === "fade" ? clock.gate + (variant === "cold" ? 240 : 200) : (variant === "cold" ? 1480 : 680)
        clock.showMark = !(variant === "quick" && reducedMotion)
        clock.variant = variant
        captionTimer.restart()
        slowTimer.restart()
        root.coverReached()
    }

    function skip() {
        if (!active || clock.skipped)
            return
        clock.skipped = true
        clock.silence()
        clock.retime()
    }

    function cancel() {
        if (!active)
            return
        clock.silence()
        clock.reset()
    }

    visible: active
    Accessible.role: Accessible.Animation
    Accessible.name: qsTr("Starting Game Mode")
    Accessible.ignored: !active

    onDestinationReadyChanged: if (destinationReady && clock.waiting) clock.passGate()
    onSoundEnabledChanged: if (!soundEnabled) clock.silence()
    onReducedMotionChanged: {
        if (!active || !reducedMotion || clock.path === "fade")
            return
        if (clock.passedGate) {
            clock.finish()
            return
        }
        clock.path = "fade"
        clock.end = clock.gate + (clock.variant === "cold" ? 240 : 200)
    }

    QtObject {
        id: clock
        property string variant: ""
        property string path: "vector"
        property real t: 0
        property real rate: 1
        property real gate: 760
        property real end: 1480
        property bool waiting: false
        property bool passedGate: false
        property bool skipped: false
        property bool airPlayed: false
        property bool openPlayed: false
        property bool captionDue: false
        property bool slowDue: false
        property bool showMark: true
        property bool curveRendered: false

        readonly property bool cold: variant === "cold"
        readonly property bool vector: path === "vector"
        readonly property bool windowOpen: vector && passedGate
        readonly property real apertureStart: cold ? 800 : 160
        readonly property real apertureEnd: cold ? 1280 : 520

        readonly property real markDx: cold && vector ? -72 + 80 * ease(0.20, 0.70, 0.20, 1.00, seg(80, 760)) : 8
        readonly property real markOpacity: !showMark ? 0 : !cold ? (vector ? ease(0.33, 1.00, 0.68, 1.00, seg(0, 160)) : 1) : vector ? ease(0.33, 1.00, 0.68, 1.00, seg(80, 260)) : seg(80, 280)
        readonly property real markScaleIn: !cold && vector ? 0.96 + 0.04 * ease(0.33, 1.00, 0.68, 1.00, seg(0, 160)) : 1
        readonly property real fillOpacity: windowOpen ? 1 - ease(0.50, 1.00, 0.89, 1.00, seg(gate, gate + (cold ? 120 : 80))) : 1
        readonly property real apertureE: windowOpen ? ease(0.11, 0.00, 0.50, 0.00, seg(apertureStart, apertureEnd)) : 0
        readonly property real destinationScale: vector ? 1.10 - 0.10 * ease(0.33, 0.00, 0.20, 1.00, seg(apertureStart, end)) : 1
        readonly property real chromeProgress: vector ? ease(0.33, 1.00, 0.68, 1.00, seg(cold ? 1240 : 480, end)) : 1
        readonly property real overlayOpacity: vector ? 1 : 1 - seg(gate, end)

        function seg(from, to) {
            return Math.max(0, Math.min(1, (t - from) / (to - from)))
        }

        function ease(x1, y1, x2, y2, x) {
            if (x <= 0)
                return 0
            if (x >= 1)
                return 1
            let lo = 0, hi = 1, u = x
            for (let i = 0; i < 22; ++i) {
                u = (lo + hi) / 2
                const bx = 3 * (1 - u) * (1 - u) * u * x1 + 3 * (1 - u) * u * u * x2 + u * u * u
                if (bx < x)
                    lo = u
                else
                    hi = u
            }
            u = (lo + hi) / 2
            return 3 * (1 - u) * (1 - u) * u * y1 + 3 * (1 - u) * u * u * y2 + u * u * u
        }

        function streak(index) {
            const delay = [0, 30, 60][index]
            const from = 80 + delay, to = 520 + delay
            if (!cold || !vector || t < from || t >= to)
                return 0
            return [520, 680, 380][index] * (1 - ease(0.30, 0.60, 0.20, 1.00, seg(from, to)))
        }

        function streakOpacity(index) {
            const from = 80 + [0, 30, 60][index], to = 520 + [0, 30, 60][index]
            if (t < from || t >= to)
                return 0
            return 0.9 * Math.min(1, (t - from) / 60) * (1 - Math.max(0, Math.min(1, (t - (to - 120)) / 120)))
        }

        function reset() {
            variant = ""
            curveRendered = false
            path = "vector"
            t = 0
            rate = 1
            waiting = false
            passedGate = false
            skipped = false
            airPlayed = false
            openPlayed = false
            captionDue = false
            slowDue = false
            captionTimer.stop()
            slowTimer.stop()
        }

        function retime() {
            if (!skipped)
                return
            if (!passedGate && !root.destinationReady)
                rate = Math.max(1, (gate - t) / 120)
            else
                rate = Math.max(1, (end - t) / (passedGate ? 160 : 400))
        }

        function advance(ms) {
            const next = t + Math.min(ms, 50) * rate
            if (!passedGate && next >= gate) {
                if (!root.destinationReady) {
                    t = gate
                    waiting = true
                    return
                }
                passGate()
            }
            t = next
            if (cold && vector && !airPlayed && t >= 80) {
                airPlayed = true
                cue("air")
            }
            if (t >= end)
                finish()
        }

        function passGate() {
            waiting = false
            passedGate = true
            if (vector && !curveRendered) {
                path = "fade"
                end = gate + (cold ? 240 : 200)
            }
            if (!openPlayed) {
                openPlayed = true
                cue("open")
            }
            retime()
        }

        function cue(name) {
            if (!root.soundEnabled || skipped)
                return
            const sound = name === "air" ? airSound : openSound
            soundFade.stop()
            sound.volume = 1
            sound.play()
        }

        function silence() {
            if (airSound.playing || openSound.playing)
                soundFade.restart()
        }

        function finish() {
            reset()
            root.finished()
        }
    }

    FrameAnimation {
        running: root.active && root.presentationReady && !clock.waiting
        onTriggered: clock.advance(frameTime * 1000)
    }

    Timer { id: captionTimer; interval: 2000; onTriggered: clock.captionDue = true }
    Timer { id: slowTimer; interval: 10000; onTriggered: clock.slowDue = true }

    Loader {
        id: stageLoader
        anchors.fill: parent
        active: root.active
        sourceComponent: Item {
            id: stage
            readonly property real unit: Math.max(0.0001, Math.min(width / 1920, height / 1080))
            readonly property real artScale: 1.3 * unit
            readonly property real endScale: 1.24 * Math.max(width / (153 * artScale), height / (86 * artScale))
            readonly property real zoom: Math.pow(endScale, clock.apertureE) * clock.markScaleIn
            readonly property real restX: width / 2 + (clock.markDx - 181 * 1.3) * unit
            readonly property real restY: height / 2 - (20 + 104 * 1.3) * unit
            readonly property real anchorX: restX + 240.5 * artScale + (width / 2 - restX - 240.5 * artScale) * clock.apertureE
            readonly property real anchorY: restY + 135 * artScale + (height / 2 - restY - 135 * artScale) * clock.apertureE
            readonly property string fillPath: "M271.9 187.6C270.7 187.5 267.1 187.5 263.9 187.5C260.7 187.5 252.3 187.5 245.2 187.5C230.6 187.4 226.7 187.4 211.6 187.5C205.7 187.5 198.1 187.5 194.9 187.5C191.6 187.5 184.2 187.5 178.5 187.5C171.1 187.5 167.3 187.5 165.4 187.4C163.2 187.3 161.5 187.3 157.0 187.4C144.4 187.7 131.3 187.6 126.3 187.1C117.8 186.2 109.0 183.0 103.6 178.7C102.9 178.2 100.6 176.5 98.5 174.9C92.9 170.8 91.3 169.0 88.0 163.6C85.7 159.7 86.1 159.9 76.8 160.3C72.4 160.4 65.1 160.3 64.4 160.0C62.5 159.2 63.0 157.1 65.2 156.7C66.2 156.5 84.9 156.6 87.1 156.8C89.3 157.0 90.8 157.0 102.2 156.6C105.2 156.5 107.4 156.5 111.8 156.6C123.8 157.0 125.0 156.7 128.5 153.3C135.7 146.0 132.4 135.1 122.3 132.5C121.3 132.3 109.3 132.3 100.9 132.4C98.0 132.5 96.1 132.5 94.5 132.4C93.3 132.3 91.4 132.2 90.3 132.2C86.0 132.2 70.7 132.3 66.7 132.4C63.5 132.5 61.3 132.5 57.8 132.3C54.1 132.2 52.6 132.2 50.8 132.3C49.5 132.3 47.3 132.4 46.0 132.4C43.1 132.3 36.9 132.3 31.1 132.4C22.7 132.5 21.2 132.2 20.9 130.2C20.6 128.1 22.1 127.5 26.4 127.8C28.8 128.0 29.8 128.0 33.0 127.9C35.0 127.9 38.2 127.8 40.1 127.8C46.5 127.8 69.3 127.9 74.3 127.8C77.1 127.8 81.2 127.9 83.5 127.9C86.3 128.0 88.4 128.0 89.9 127.9C91.5 127.8 93.3 127.8 96.1 127.9C98.8 127.9 101.0 127.9 103.0 127.8C105.1 127.8 108.5 127.8 114.8 127.8C120.3 127.9 126.3 127.9 130.8 127.9C134.9 127.8 139.7 127.8 142.1 127.9C156.4 128.3 159.5 127.1 162.3 120.1C164.7 114.0 161.2 106.7 155.1 105.1C153.9 104.8 143.1 104.6 133.6 104.7C130.5 104.8 126.8 104.7 124.3 104.6C121.1 104.5 119.9 104.5 118.7 104.6C117.7 104.7 115.2 104.8 111.4 104.8C100.8 104.7 95.1 104.7 91.4 104.8C88.8 104.9 86.9 104.9 85.1 104.7C81.7 104.5 62.1 104.5 60.4 104.7C57.8 105.0 51.5 104.8 50.7 104.4C49.3 103.7 49.3 101.8 50.6 101.1C51.0 100.9 51.2 100.9 52.7 101.0C53.6 101.0 56.0 101.0 58.0 101.0C60.0 101.0 63.5 101.0 65.7 101.0C73.3 101.0 76.7 101.0 81.8 101.0C84.8 101.0 87.7 101.0 89.1 101.1C90.7 101.2 92.2 101.2 94.5 101.1C100.0 100.9 112.2 100.8 116.0 101.0C122.8 101.3 125.9 100.5 128.5 98.0C133.0 93.6 130.3 85.4 123.7 83.6C122.2 83.2 109.6 83.1 103.2 83.3C97.3 83.6 92.1 83.4 91.5 82.8C91.0 82.3 91.5 80.7 93.1 78.1C94.0 76.5 94.3 76.2 96.3 73.9C97.5 72.7 98.4 71.5 100.3 69.3C102.2 67.0 110.1 61.1 113.4 59.5C118.8 57.0 123.4 55.4 128.1 54.6C129.6 54.3 131.0 54.1 131.1 54.1C131.2 54.0 132.4 53.9 133.8 53.8C137.4 53.5 139.9 53.6 145.6 54.4C149.6 54.9 149.7 54.9 151.4 52.5C154.6 47.6 161.2 40.6 165.3 37.5C166.2 36.8 167.6 35.7 168.5 35.0C170.5 33.2 171.1 32.8 172.6 31.9C173.3 31.5 174.8 30.7 175.9 30.1C178.4 28.6 180.7 27.4 182.7 26.6C183.6 26.3 185.2 25.7 186.2 25.3C193.1 22.7 194.5 22.2 198.2 21.7C199.8 21.4 201.8 21.1 202.5 21.0C205.8 20.4 209.5 20.2 212.8 20.4C214.1 20.4 216.4 20.6 217.9 20.6C220.9 20.8 221.7 20.9 226.4 21.7C229.4 22.2 230.0 22.3 231.6 22.9C232.7 23.3 234.1 23.8 234.8 24.0C238.9 25.1 241.6 26.2 244.6 27.9C247.0 29.4 250.8 31.4 252.1 32.0C254.7 33.2 259.9 37.3 263.2 40.8C264.4 42.1 266.4 44.2 267.7 45.5C270.1 48.0 271.2 49.3 272.5 51.1C272.9 51.7 273.6 52.8 274.2 53.5C275.3 55.0 276.5 57.0 277.2 58.6C277.8 60.0 279.2 62.7 279.9 63.9C282.0 67.3 283.6 71.2 285.1 76.8C287.0 84.0 286.7 83.6 291.8 83.9C294.9 84.0 296.9 84.2 297.8 84.4C298.1 84.4 299.2 84.6 300.1 84.8C301.9 85.0 304.6 85.7 306.4 86.3C308.6 87.0 315.7 90.7 318.1 92.3C318.7 92.7 320.0 93.7 321.0 94.4C325.4 97.4 326.0 98.0 330.8 104.3C332.1 106.0 333.6 108.3 335.1 110.8C335.5 111.6 336.2 112.8 336.6 113.4C338.4 116.5 339.5 120.8 340.6 128.4C340.8 130.2 341.1 132.1 341.2 132.6C341.5 134.8 341.3 138.3 340.7 142.7C339.7 149.1 339.4 150.1 337.0 155.1C336.5 156.2 335.7 157.8 335.4 158.7C334.3 161.5 332.8 164.1 331.2 165.9C324.2 173.9 320.3 177.5 316.4 179.5C315.5 179.9 312.6 181.5 309.4 183.2C308.3 183.8 307.1 184.3 306.8 184.5C300.9 186.6 295.6 187.2 282.7 187.5C279.0 187.6 275.6 187.7 275.1 187.7C274.5 187.7 273.1 187.7 271.9 187.6ZM35.2 160.1C32.8 159.8 31.9 157.9 33.8 156.9L34.3 156.6L38.4 156.7C40.6 156.7 42.6 156.8 42.9 156.9C44.8 157.4 44.7 159.6 42.7 160.0C41.9 160.1 36.2 160.3 35.2 160.1Z"
            readonly property string outlinePath: "M128.0 193.9C111.3 192.9 94.2 183.3 83.3 168.7C81.8 166.7 82.5 166.9 73.3 166.9C63.3 166.9 62.2 166.7 59.9 165.4C54.7 162.4 55.3 154.0 60.9 151.2C63.2 150.0 62.7 150.0 95.1 150.1C124.0 150.1 122.2 150.2 123.9 148.7C126.9 145.9 125.9 140.5 122.1 139.2L121.3 138.9L71.9 138.9C44.8 138.9 22.2 138.9 21.7 138.8C13.9 138.0 11.3 128.2 17.4 123.1C19.6 121.3 20.2 121.2 28.1 121.2C42.0 121.1 149.4 121.1 150.0 121.2C156.0 121.9 158.9 114.4 153.6 111.9C152.4 111.3 153.8 111.3 100.8 111.3C55.1 111.3 50.1 111.3 49.2 111.1C40.8 109.3 41.0 96.9 49.5 94.7L50.7 94.4L86.4 94.3L122.1 94.2L122.7 93.9C124.4 93.1 124.4 91.2 122.7 90.3L122.2 90.1L102.3 90.0C87.6 90.0 82.4 89.9 82.2 89.8C80.8 89.0 81.5 86.5 84.7 79.8C88.0 72.8 92.5 67.1 99.6 60.9C110.3 51.4 127.5 45.9 142.4 47.3C146.9 47.6 146.5 47.8 148.7 44.9C158.7 32.0 173.8 21.8 190.3 16.9C203.5 13.0 220.3 12.9 233.1 16.6C251.3 21.9 265.3 31.3 276.7 46.1C283.8 55.3 288.2 64.0 291.4 75.0C292.0 77.0 292.3 77.1 295.4 77.4C307.3 78.5 318.3 83.1 327.5 90.9C344.8 105.5 352.2 130.0 345.7 150.8C343.0 159.5 339.2 166.7 334.6 171.9C334.0 172.6 332.9 173.8 332.3 174.6C325.2 182.7 313.0 190.1 303.0 192.2C302.1 192.4 301.0 192.6 300.6 192.8C299.2 193.1 296.8 193.5 293.7 193.7C290.7 194.0 132.5 194.1 128.0 193.9ZM33.9 166.8C28.8 166.4 25.5 162.9 25.9 158.0C26.4 151.6 32.1 149.0 43.1 150.1C52.0 151.0 54.6 162.2 46.8 165.9C45.1 166.7 39.6 167.1 33.9 166.8Z"

            Binding {
                target: clock
                property: "curveRendered"
                value: matte.rendererType === Shape.CurveRenderer
            }

            Rectangle {
                anchors.fill: parent
                color: "#05070B"
                visible: !clock.vector
                opacity: clock.overlayOpacity
            }

            Item {
                id: mark
                x: stage.anchorX - 240.5 * stage.artScale * stage.zoom
                y: stage.anchorY - 135 * stage.artScale * stage.zoom
                width: 362
                height: 208
                scale: stage.artScale * stage.zoom
                transformOrigin: Item.TopLeft
                visible: clock.apertureE < 1

                Shape {
                    id: matte
                    visible: clock.vector
                    preferredRendererType: Shape.CurveRenderer
                    ShapePath {
                        strokeColor: "transparent"
                        fillColor: "#05070B"
                        fillRule: ShapePath.OddEvenFill
                        PathSvg { path: "M-6000 -6000H6362V6208H-6000Z" + stage.fillPath }
                    }
                }
                Rectangle {
                    width: 362
                    height: 208
                    color: "#05070B"
                    visible: clock.vector && !clock.windowOpen
                }
                Item {
                    opacity: clock.markOpacity * clock.overlayOpacity
                    Shape {
                        opacity: 1 - Math.max(0, Math.min(1, Math.log(stage.zoom) / Math.log(3)))
                        preferredRendererType: Shape.CurveRenderer
                        ShapePath {
                            strokeColor: "transparent"
                            fillColor: "#228B1A"
                            fillRule: ShapePath.OddEvenFill
                            PathSvg { path: stage.outlinePath + stage.fillPath }
                        }
                    }
                    Shape {
                        opacity: clock.fillOpacity
                        visible: opacity > 0
                        preferredRendererType: Shape.CurveRenderer
                        ShapePath {
                            strokeColor: "transparent"
                            fillRule: ShapePath.OddEvenFill
                            fillGradient: LinearGradient {
                                x1: 90; y1: 0; x2: 345; y2: 0
                                GradientStop { position: 0; color: "#5EEB2D" }
                                GradientStop { position: 1; color: "#8FF23F" }
                            }
                            PathSvg { path: stage.fillPath }
                        }
                    }
                }
                Repeater {
                    model: [{y: 103, tip: 50}, {y: 129, tip: 21}, {y: 158.4, tip: 33}]
                    Rectangle {
                        required property var modelData
                        required property int index
                        readonly property real length: clock.streak(index) / 1.3
                        visible: length > 0.4
                        x: modelData.tip - length
                        y: modelData.y - 2.5 / 1.3
                        width: length
                        height: 5 / 1.3
                        radius: height / 2
                        opacity: clock.streakOpacity(index)
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0; color: "#008FF23F" }
                            GradientStop { position: 0.7; color: "#8FF23F" }
                        }
                    }
                }
            }

            Column {
                id: caption
                x: 0
                y: stage.height / 2 + 172 * stage.unit
                width: stage.width
                spacing: 6 * stage.unit
                readonly property bool shown: clock.waiting && clock.captionDue && root.statusText !== ""
                opacity: shown ? 0.72 : 0
                visible: opacity > 0
                Behavior on opacity {
                    NumberAnimation { duration: caption.shown ? 240 : 120; easing.type: Easing.OutCubic }
                }
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: root.statusText
                    color: "#FFFFFF"
                    font.family: Theme.bodyFont
                    font.pixelSize: Math.round(24 * stage.unit)
                    font.weight: Font.Bold
                }
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    visible: clock.slowDue
                    text: qsTr("This is taking longer than usual.")
                    color: "#FFFFFF"
                    opacity: 0.8
                    font.family: Theme.bodyFont
                    font.pixelSize: Math.round(20 * stage.unit)
                    font.weight: Font.DemiBold
                }
            }
        }
    }

    SoundEffect {
        id: airSound
        objectName: "launchAirSound"
        source: root.soundEnabled ? "qrc:/qt/qml/OpenNOW/res/sounds/launch-air.wav" : ""
    }
    SoundEffect {
        id: openSound
        objectName: "launchOpenSound"
        source: root.soundEnabled ? "qrc:/qt/qml/OpenNOW/res/sounds/launch-open.wav" : ""
    }
    SequentialAnimation {
        id: soundFade
        ParallelAnimation {
            NumberAnimation { target: airSound; property: "volume"; to: 0; duration: 60 }
            NumberAnimation { target: openSound; property: "volume"; to: 0; duration: 60 }
        }
        ScriptAction { script: { airSound.stop(); openSound.stop() } }
    }
}
