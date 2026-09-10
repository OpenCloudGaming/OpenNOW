pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    objectName: "desktopOnboardingScreen"
    property var store: ShellStore
    property int stepIndex: 0
    readonly property int stepCount: 6
    readonly property bool saving: store.onboardingSaving
    readonly property string error: store.onboardingError
    readonly property var settings: store.onboardingSettings
    readonly property bool compact: width < DesktopTokens.px(960)
    readonly property color mint: Theme.accentColor("green")
    readonly property color amber: Theme.accentColor("amber")
    readonly property color coral: Theme.accentColor("coral")
    readonly property var stepNames: [qsTr("Welcome"),qsTr("Mode"),qsTr("Picture"),qsTr("Boost"),qsTr("Support"),qsTr("Ready")]
    readonly property var titles: [qsTr("Fresh out of\nthe oven."),qsTr("Desk or couch?"),qsTr("Pick your picture."),
        qsTr("A little extra boost."),qsTr("Keep the lights on."),qsTr("You're set.")]
    readonly property var introductions: [
        qsTr("Welcome to OpenNOW 1.0.0. This is a beta: bugs may occur. Let's make the client feel at home on your screen."),
        qsTr("One library, two ways to play. Pick the shell you'll open most. Your choice takes effect when setup finishes."),
        qsTr("Choose your stream preferences. Start with the defaults, or make them your own. You can change everything later."),
        qsTr("Optional, on-device picture processing. Keep it simple now, or experiment with how your stream is presented."),
        qsTr("OpenNOW is free and open source. If you'd like to support its development, GitHub Sponsors is one way to help."),
        qsTr("Here's what you picked. Finish setup to save your preferences, or go back to adjust anything.")]

    focus: true

    function updateUiScale() {
        if (width <= 0 || height <= 0)
            return
        const fitted = DesktopTokens.scaleForWindow(width, height)
        const preference = Number(store.settings.desktopUiScale || 1)
        DesktopTokens.uiScale = Math.min(1.4, Math.max(0.9, fitted * preference))
    }

    onWidthChanged: updateUiScale()
    onHeightChanged: updateUiScale()
    Component.onCompleted: updateUiScale()
    Connections {
        target: root.store
        function onSettingsChanged() { root.updateUiScale() }
    }

    function goToStep(index) {
        if (saving)
            return
        stepIndex = Math.max(0, Math.min(stepCount - 1, index))
    }

    function next() {
        if (saving)
            return
        if (stepIndex < stepCount - 1) {
            goToStep(stepIndex + 1)
            return
        }
        store.finishOnboarding()
    }

    function skip() {
        if (saving)
            return
        store.finishOnboarding()
    }

    function revealFocusedControl() {
        const win = root.Window.window
        const focused = win ? win.activeFocusItem : null
        if (!focused)
            return
        let ancestor = focused
        while (ancestor && ancestor !== pageBody)
            ancestor = ancestor.parent
        if (!ancestor)
            return
        const point = focused.mapToItem(pageBody, 0, 0)
        const margin = DesktopTokens.px(16)
        if (point.y < pageScroll.contentY + margin)
            pageScroll.contentY = Math.max(0, point.y - margin)
        else if (point.y + focused.height > pageScroll.contentY + pageScroll.height - margin)
            pageScroll.contentY = Math.min(Math.max(0, pageScroll.contentHeight - pageScroll.height),
                point.y + focused.height - pageScroll.height + margin)
    }

    onStepIndexChanged: {
        pageScroll.contentY = 0
        nextButton.forceActiveFocus(Qt.TabFocusReason)
    }
    Keys.onReturnPressed: event => { root.next(); event.accepted = true }
    Keys.onEnterPressed: event => { root.next(); event.accepted = true }
    Keys.onEscapePressed: event => { root.skip(); event.accepted = true }
    Shortcut {
        sequence: "Alt+Left"
        enabled: root.visible && root.enabled && !root.saving && root.stepIndex > 0
        onActivated: root.goToStep(root.stepIndex - 1)
    }
    Shortcut {
        sequence: "Alt+Right"
        enabled: root.visible && root.enabled && !root.saving
        onActivated: root.next()
    }
    Connections {
        target: root.Window.window
        function onActiveFocusItemChanged() { Qt.callLater(root.revealFocusedControl) }
    }

    DesktopBackdrop { anchors.fill: parent; artwork: "qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg" }

    component Copy: Text {
        width: parent.width
        color: Theme.textMuted
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.bodySize
        wrapMode: Text.Wrap
        lineHeight: 1.4
    }
    component Eyebrow: Text {
        color: root.mint
        font.family: Theme.monoFont
        font.pixelSize: DesktopTokens.monoSize
        font.weight: Font.Bold
        font.letterSpacing: DesktopTokens.px(1)
        wrapMode: Text.Wrap
    }
    component Action: Button {
        id: action
        property bool primary: false
        property color accent: root.mint
        hoverEnabled: true
        padding: 0
        leftPadding: DesktopTokens.px(20)
        rightPadding: DesktopTokens.px(20)
        implicitHeight: DesktopTokens.px(46)
        background: Rectangle {
            radius: DesktopTokens.px(10)
            color: action.primary ? action.accent : action.hovered || action.down ? DesktopTokens.raisedStrong : DesktopTokens.raised
            border.width: action.activeFocus ? 2 : 1
            border.color: action.activeFocus ? Theme.focus : action.primary ? action.accent : Theme.seam
            Behavior on color { ColorAnimation { duration: DesktopTokens.quickDuration } }
        }
        contentItem: Text {
            id: actionLabel
            text: action.text
            color: action.primary ? Theme.contrastText(action.accent) : Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.bodySize
            font.weight: Font.ExtraBold
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        implicitWidth: actionLabel.implicitWidth + DesktopTokens.px(40)
        opacity: enabled ? 1 : 0.55
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: DesktopTokens.px(72)
            Layout.leftMargin: DesktopTokens.px(root.compact ? 20 : 40)
            Layout.rightMargin: DesktopTokens.px(root.compact ? 20 : 40)
            spacing: DesktopTokens.px(10)
            Image {
                Layout.preferredWidth: DesktopTokens.px(26)
                Layout.preferredHeight: DesktopTokens.px(18)
                source: "qrc:/qt/qml/OpenNOW/res/brand/opennow-mark.png"
                fillMode: Image.PreserveAspectFit
            }
            Text {
                text: "OpenNOW"
                color: Theme.label; font.family: Theme.displayFont
                font.pixelSize: DesktopTokens.headingSize; font.weight: Font.Black
            }
            Rectangle {
                implicitWidth: betaLabel.implicitWidth + DesktopTokens.px(16)
                implicitHeight: DesktopTokens.px(25)
                radius: DesktopTokens.px(6)
                color: Qt.rgba(root.amber.r,root.amber.g,root.amber.b,0.12)
                Text {
                    id: betaLabel
                    anchors.centerIn: parent
                    text: root.width < DesktopTokens.px(500) ? qsTr("BETA") : qsTr("1.0.0 · BETA")
                    color: root.amber; font.family: Theme.monoFont
                    font.pixelSize: DesktopTokens.smallSize; font.weight: Font.Bold
                }
            }
            Item { Layout.fillWidth: true }
            Text {
                visible: !root.compact
                text: root.store.signedIn ? qsTr("Signed in") : qsTr("Offline setup")
                color: Theme.textMuted; font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.captionSize
            }
            Action {
                objectName: "onboardingSkip"
                text: root.width < DesktopTokens.px(500) ? qsTr("Skip") : qsTr("Skip setup")
                implicitHeight: DesktopTokens.px(34)
                enabled: !root.saving
                onClicked: root.skip()
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            Column {
                visible: !root.compact
                Layout.preferredWidth: DesktopTokens.px(236)
                Layout.alignment: Qt.AlignTop
                Layout.topMargin: DesktopTokens.px(36)
                Repeater {
                    model: root.stepCount
                    delegate: AbstractButton {
                        id: railStep
                        required property int index
                        width: parent.width; height: DesktopTokens.px(72)
                        enabled: !root.saving
                        Accessible.name: qsTr("Step %1: %2").arg(index + 1).arg(root.stepNames[index])
                        Accessible.role: Accessible.PageTab
                        Accessible.selected: root.stepIndex === index
                        onClicked: root.goToStep(index)
                        background: Rectangle {
                            x: DesktopTokens.px(28); width: parent.width - DesktopTokens.px(40)
                            height: parent.height - DesktopTokens.px(10)
                            radius: DesktopTokens.px(10)
                            color: railStep.hovered || railStep.activeFocus ? DesktopTokens.raised : "transparent"
                            border.width: railStep.activeFocus ? 2 : 0; border.color: Theme.focus
                        }
                        Rectangle {
                            x: DesktopTokens.px(49); y: DesktopTokens.px(27)
                            width: DesktopTokens.px(2); height: DesktopTokens.px(40)
                            visible: railStep.index < root.stepCount - 1
                            color: railStep.index < root.stepIndex ? Qt.rgba(root.mint.r,root.mint.g,root.mint.b,0.35) : DesktopTokens.seamSoft
                        }
                        Rectangle {
                            x: DesktopTokens.px(40); y: DesktopTokens.px(4)
                            width: DesktopTokens.px(20); height: width; radius: width / 2
                            color: railStep.index < root.stepIndex ? root.mint : "transparent"
                            border.width: railStep.index === root.stepIndex ? 2 : 0
                            border.color: root.mint
                            Rectangle {
                                anchors.centerIn: parent
                                width: DesktopTokens.px(8); height: width; radius: width / 2
                                color: railStep.index === root.stepIndex ? root.mint : Theme.textMuted
                                visible: railStep.index >= root.stepIndex
                            }
                            Text {
                                anchors.centerIn: parent; text: "✓"
                                visible: railStep.index < root.stepIndex
                                color: Theme.contrastText(root.mint)
                                font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Bold
                            }
                        }
                        Column {
                            x: DesktopTokens.px(76)
                            width: parent.width - x - DesktopTokens.px(16)
                            spacing: DesktopTokens.px(3)
                            Eyebrow {
                                text: "0" + (railStep.index + 1)
                                color: railStep.index === root.stepIndex ? root.mint : Theme.textMuted
                            }
                            Text {
                                width: parent.width; text: root.stepNames[railStep.index]
                                color: railStep.index <= root.stepIndex ? Theme.label : Theme.textMuted
                                font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize
                                font.weight: railStep.index === root.stepIndex ? Font.ExtraBold : Font.Bold
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.leftMargin: DesktopTokens.px(root.compact ? 20 : 24)
                Layout.rightMargin: DesktopTokens.px(root.compact ? 20 : 64)
                spacing: DesktopTokens.px(12)

                RowLayout {
                    visible: root.compact
                    Layout.fillWidth: true
                    Layout.bottomMargin: DesktopTokens.px(4)
                    Repeater {
                        model: root.stepCount
                        delegate: AbstractButton {
                            id: compactStep
                            required property int index
                            Layout.fillWidth: true
                            implicitHeight: DesktopTokens.px(36)
                            enabled: !root.saving
                            Accessible.name: root.stepNames[index]
                            Accessible.role: Accessible.PageTab
                            Accessible.selected: index === root.stepIndex
                            onClicked: root.goToStep(index)
                            background: Rectangle {
                                radius: DesktopTokens.px(8)
                                color: compactStep.index === root.stepIndex ? Qt.rgba(root.mint.r,root.mint.g,root.mint.b,0.14) : DesktopTokens.raised
                                border.width: parent.activeFocus ? 2 : 0
                                border.color: Theme.focus
                            }
                            Text {
                                anchors.centerIn: parent; text: "0" + (compactStep.index + 1)
                                color: compactStep.index === root.stepIndex ? root.mint : Theme.textMuted
                                font.family: Theme.monoFont; font.pixelSize: DesktopTokens.monoSize
                                font.weight: Font.Bold
                            }
                        }
                    }
                }

                Flickable {
                    id: pageScroll
                    objectName: "onboardingScroll"
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: width
                    contentHeight: pageBody.implicitHeight + DesktopTokens.px(24)
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                    Text {
                        anchors.right: parent.right
                        y: Math.max(0, pageScroll.height - height)
                        text: "0" + (root.stepIndex + 1)
                        color: Theme.label; opacity: Theme.lightMode ? 0.04 : 0.035
                        font.family: Theme.displayFont; font.pixelSize: DesktopTokens.px(260)
                        font.weight: Font.Black
                    }
                    Column {
                        id: pageBody
                        width: pageScroll.width - DesktopTokens.px(12)
                        spacing: DesktopTokens.px(24)
                        topPadding: DesktopTokens.px(root.compact ? 16 : 36)
                        Column {
                            width: parent.width
                            spacing: DesktopTokens.px(12)
                            Eyebrow {
                                width: parent.width
                                text: root.stepIndex === 0 ? qsTr("1.0.0 · BETA")
                                    : qsTr("%1 / 06 · %2").arg("0" + (root.stepIndex + 1)).arg(root.stepNames[root.stepIndex].toUpperCase())
                                color: root.stepIndex === 0 ? root.amber : root.stepIndex === 4 ? root.coral : root.mint
                            }
                            Text {
                                objectName: "onboardingHeading"
                                width: parent.width
                                text: root.titles[root.stepIndex]
                                color: Theme.label; font.family: Theme.displayFont
                                font.pixelSize: DesktopTokens.px(root.compact ? 38 : 52)
                                font.weight: Font.Black
                                font.letterSpacing: -DesktopTokens.px(1)
                                lineHeight: 1.05; wrapMode: Text.Wrap
                                Accessible.role: Accessible.Heading
                            }
                            Copy { width: Math.min(parent.width, DesktopTokens.px(670)); text: root.introductions[root.stepIndex] }
                        }
                        Loader {
                            id: stepLoader
                            objectName: "onboardingStepLoader"
                            width: parent.width
                            enabled: !root.saving
                            sourceComponent: [welcomePage,modePage,picturePage,boostPage,supportPage,readyPage][root.stepIndex]
                        }
                    }
                }
            }
        }

        Rectangle {
            visible: root.error !== ""
            Layout.fillWidth: true
            Layout.leftMargin: DesktopTokens.px(root.compact ? 20 : 260)
            Layout.rightMargin: DesktopTokens.px(root.compact ? 20 : 64)
            implicitHeight: errorLabel.implicitHeight + DesktopTokens.px(24)
            radius: DesktopTokens.px(10)
            color: Qt.rgba(root.coral.r,root.coral.g,root.coral.b,0.12)
            border.color: root.coral
            Text {
                id: errorLabel
                x: DesktopTokens.px(12); y: DesktopTokens.px(12)
                width: parent.width - DesktopTokens.px(24)
                text: root.error
                color: root.coral; font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.bodySize; wrapMode: Text.Wrap
                Accessible.role: Accessible.AlertMessage
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.minimumHeight: DesktopTokens.px(96)
            Layout.leftMargin: DesktopTokens.px(root.compact ? 20 : 40)
            Layout.rightMargin: DesktopTokens.px(root.compact ? 20 : 64)
            spacing: DesktopTokens.px(12)
            Column {
                visible: !root.compact
                Layout.fillWidth: true
                spacing: DesktopTokens.px(8)
                Copy { text: qsTr("Tab to explore · Alt + ← / → to move between steps"); font.pixelSize: DesktopTokens.captionSize }
                Eyebrow { text: qsTr("YOU CAN CHANGE ALL OF THIS LATER IN SETTINGS"); color: Theme.textMuted; font.pixelSize: DesktopTokens.smallSize }
            }
            Action {
                objectName: "onboardingBack"
                visible: root.stepIndex > 0
                text: qsTr("Back")
                enabled: !root.saving
                onClicked: root.goToStep(root.stepIndex - 1)
            }
            Item { visible: root.compact; Layout.fillWidth: true }
            BusyIndicator {
                visible: root.saving
                running: root.saving && !AppController.reducedMotion
                Layout.preferredWidth: DesktopTokens.px(28)
                Layout.preferredHeight: DesktopTokens.px(28)
            }
            Action {
                id: nextButton
                objectName: "onboardingNext"
                primary: true
                Layout.minimumWidth: DesktopTokens.px(144)
                text: root.saving ? qsTr("Saving…") : root.stepIndex === 5 ? qsTr("Finish setup")
                    : root.stepIndex === 0 ? qsTr("Let's set things up") : root.stepIndex === 4 ? qsTr("Continue without sponsoring") : qsTr("Continue")
                Layout.maximumWidth: root.compact ? Math.max(DesktopTokens.px(144), root.width - DesktopTokens.px(160)) : Infinity
                enabled: !root.saving
                onClicked: root.next()
            }
        }
    }

    Component {
        id: welcomePage
        GridLayout {
            columns: width >= DesktopTokens.px(900) ? 2 : 1
            columnSpacing: DesktopTokens.px(32); rowSpacing: DesktopTokens.px(24)
            DesktopSettingsPanel {
                Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: DesktopTokens.px(620)
                Layout.minimumWidth: 0
                padding: DesktopTokens.px(24); radius: DesktopTokens.px(16)
                border.color: Qt.rgba(root.amber.r,root.amber.g,root.amber.b,0.4)
                Column {
                    width: parent.width; spacing: DesktopTokens.px(20)
                    Eyebrow { width: parent.width; text: qsTr("WHAT BETA MEANS HERE"); color: root.amber }
                    Copy { text: qsTr("Bugs may occur."); color: Theme.label; font.pixelSize: DesktopTokens.titleSize; font.weight: Font.ExtraBold }
                    Copy { text: qsTr("If something breaks, report it on GitHub. Describe what happened, what you expected and how to reproduce it.") }
                    Action {
                        width: Math.min(implicitWidth, parent.width)
                        text: qsTr("Report a bug on GitHub ↗")
                        onClicked: Qt.openUrlExternally("https://github.com/OpenCloudGaming/OpenNOW/issues")
                    }
                    Copy { text: qsTr("OpenCloudGaming / OpenNOW / issues"); font.family: Theme.monoFont; font.pixelSize: DesktopTokens.monoSize }
                }
            }
            DesktopSettingsPanel {
                Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: DesktopTokens.px(320)
                Layout.minimumWidth: 0
                padding: DesktopTokens.px(24); radius: DesktopTokens.px(16)
                Column {
                    width: parent.width; spacing: DesktopTokens.px(20)
                    Eyebrow { text: qsTr("MAKE IT YOURS") }
                    Copy { text: qsTr("Your setup,\nyour way."); color: Theme.label; font.pixelSize: DesktopTokens.px(32); font.weight: Font.Black }
                    Copy { text: qsTr("Choose a shell, set your picture and explore optional enhancements. Skip any time to keep your current selections.") }
                    Rectangle { width: parent.width; height: 1; color: Theme.seam }
                    Copy { text: qsTr("Thanks for trying the beta. Your feedback helps shape what comes next."); font.pixelSize: DesktopTokens.captionSize }
                }
            }
        }
    }

    Component {
        id: modePage
        Column {
            spacing: DesktopTokens.px(24)
            GridLayout {
                width: parent.width
                columns: width >= DesktopTokens.px(680) ? 2 : 1
                columnSpacing: DesktopTokens.px(24); rowSpacing: DesktopTokens.px(16)
                DesktopOnboardingModeCard {
                    objectName: "onboardingDesktopMode"
                    Layout.fillWidth: true; Layout.fillHeight: true
                    selected: root.settings.launchInConsoleMode !== true
                    onClicked: root.store.setOnboardingSetting("launchInConsoleMode", false)
                }
                DesktopOnboardingModeCard {
                    objectName: "onboardingConsoleMode"
                    Layout.fillWidth: true; Layout.fillHeight: true
                    consoleMode: true; selected: root.settings.launchInConsoleMode === true
                    onClicked: root.store.setOnboardingSetting("launchInConsoleMode", true)
                }
            }
            DesktopSettingsPanel {
                width: parent.width; paperStyle: true
                DesktopSettingsRow {
                    width: parent.width; paperStyle: true; glyph: "controller"
                    title: qsTr("Switch to console on gamepad input")
                    description: qsTr("Let controller input switch the shell after setup is complete.")
                    showDivider: false
                    DesktopSettingsToggle {
                        objectName: "onboardingSwitchOnPad"
                        checked: root.settings.switchToConsoleOnPad === true
                        Accessible.name: qsTr("Switch to console on gamepad input")
                        onValueChangedByUser: value => root.store.setOnboardingSetting("switchToConsoleOnPad", value)
                    }
                }
            }
        }
    }
    Component { id: picturePage; DesktopOnboardingPicture { store: root.store } }
    Component { id: boostPage; DesktopOnboardingBoost { store: root.store } }

    Component {
        id: supportPage
        GridLayout {
            columns: width >= DesktopTokens.px(900) ? 2 : 1
            columnSpacing: DesktopTokens.px(32); rowSpacing: DesktopTokens.px(24)
            DesktopSettingsPanel {
                Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: DesktopTokens.px(620)
                Layout.minimumWidth: 0
                padding: DesktopTokens.px(28); radius: DesktopTokens.px(16)
                border.color: Qt.rgba(root.coral.r,root.coral.g,root.coral.b,0.4)
                Column {
                    width: parent.width; spacing: DesktopTokens.px(24)
                    Eyebrow { width: parent.width; text: qsTr("GITHUB SPONSORS · OPTIONAL"); color: root.coral }
                    Copy { text: qsTr("Support\nOpenNOW."); color: Theme.label; font.pixelSize: DesktopTokens.px(32); font.weight: Font.Black }
                    Copy { text: qsTr("Visit the maintainer's GitHub Sponsors page to see the available ways to contribute. Sponsoring is entirely optional.") }
                    Action {
                        width: Math.min(implicitWidth, parent.width)
                        text: qsTr("Sponsor on GitHub ↗"); primary: true; accent: root.coral
                        onClicked: Qt.openUrlExternally("https://github.com/sponsors/zortos293")
                    }
                    Copy { text: qsTr("Opens GitHub in your browser. No payment is collected in OpenNOW."); font.pixelSize: DesktopTokens.captionSize }
                }
            }
            Column {
                Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: DesktopTokens.px(320)
                spacing: DesktopTokens.px(20)
                Eyebrow { width: parent.width; text: qsTr("OTHER WAYS TO HELP"); color: Theme.textMuted }
                Copy { text: qsTr("Good bug reports matter too."); color: Theme.label; font.pixelSize: DesktopTokens.titleSize; font.weight: Font.ExtraBold }
                Copy { text: qsTr("Share feedback, help reproduce a bug or contribute to the project. You don't need to sponsor to continue setup.") }
                Action {
                    width: Math.min(implicitWidth, parent.width)
                    text: qsTr("Visit the repository ↗")
                    onClicked: Qt.openUrlExternally("https://github.com/OpenCloudGaming/OpenNOW")
                }
            }
        }
    }

    Component {
        id: readyPage
        DesktopSettingsPanel {
            paperStyle: true
            radius: DesktopTokens.px(16); border.color: Theme.seam
            Column {
                width: parent.width
                DesktopSettingsSection { text: qsTr("YOUR SETUP") }
                Repeater {
                    model: [
                        {label:qsTr("Picture"),step:2,value:qsTr("%1 · %2 FPS · %3 · %4 Mbps").arg(String(root.settings.resolution || "1920x1080").replace("x"," × "))
                            .arg(Number(root.settings.fps ?? 60) === 0 ? qsTr("Auto") : root.settings.fps ?? 60)
                            .arg(String(root.settings.codec || "auto").toUpperCase()).arg(root.settings.maxBitrateMbps ?? 75)},
                        {label:qsTr("Mode"),step:1,value:(root.settings.launchInConsoleMode === true ? qsTr("Console") : qsTr("Desktop"))
                            + (root.settings.switchToConsoleOnPad === true ? qsTr(" · switch on gamepad input") : "")},
                        {label:qsTr("Boost"),step:3,value:(root.settings.frameGeneration === "2x" ? qsTr("Frame generation 2× · Experimental") : qsTr("Frame generation off"))
                            + (Qt.platform.os === "osx" ? (root.settings.upscaling === "metalfx" ? qsTr(" · MetalFX · clarity %1 · noise reduction %2").arg(root.settings.upscalingSharpness ?? 10).arg(root.settings.upscalingDenoise ?? 0) : qsTr(" · upscaling off")) : "")},
                        {label:qsTr("Beta"),step:0,value:qsTr("1.0.0 beta · report bugs on GitHub")},
                        {label:qsTr("Support"),step:4,value:qsTr("Optional · GitHub Sponsors")}
                    ]
                    delegate: DesktopSettingsRow {
                        id: summaryRow
                        required property var modelData
                        width: parent.width; paperStyle: true
                        title: modelData.label; description: modelData.value
                        Action {
                            text: qsTr("Edit")
                            implicitHeight: DesktopTokens.px(32)
                            Accessible.name: qsTr("Edit %1").arg(summaryRow.modelData.label)
                            onClicked: root.goToStep(summaryRow.modelData.step)
                        }
                    }
                }
                Item {
                    width: parent.width
                    implicitHeight: readyNote.implicitHeight + DesktopTokens.px(32)
                    Copy {
                        id: readyNote
                        x: DesktopTokens.px(20); y: DesktopTokens.px(16)
                        width: parent.width - DesktopTokens.px(40)
                        text: qsTr("Your preferences will be saved when you finish setup. You can revisit them in Settings.")
                        font.pixelSize: DesktopTokens.captionSize
                    }
                }
            }
        }
    }
}
