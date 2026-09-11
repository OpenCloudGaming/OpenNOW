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
    readonly property string displayName: store.authSession && store.authSession.user
        ? String(store.authSession.user.displayName || "") : ""
    readonly property string membership: store.subscription && store.subscription.membershipTier
        ? String(store.subscription.membershipTier).toUpperCase() : ""
    readonly property var stepNames: [qsTr("Welcome"),qsTr("Mode"),qsTr("Picture"),qsTr("Boost"),qsTr("Support"),qsTr("Ready")]
    readonly property var titles: [qsTr("Fresh out of\nthe oven."),qsTr("Desk or couch?"),qsTr("Pick your picture."),
        qsTr("A little extra boost."),qsTr("Keep the lights on."),qsTr("You're set.")]
    readonly property var introductions: [
        qsTr("Welcome to the native Qt build of OpenNOW. This is a beta, so bugs may occur. If something breaks, tell us what happened on GitHub. Let's make the client feel at home on your screen."),
        qsTr("Both shells share one session, one library and one set of settings. Pick the one you'll open most. You can switch any time."),
        qsTr("Choose your stream preferences. Start with the defaults, or make them your own. You can change everything later."),
        store.onboardingAwdlController.state !== MacAwdlController.Unsupported
            ? qsTr("Complete the required network setup, then choose optional on-device picture processing.")
            : qsTr("Optional, on-device picture processing. Keep it simple now, or experiment with how your stream is presented."),
        qsTr("OpenNOW is free and open source. If it helps you, consider supporting its development through GitHub Sponsors. You can also help by reporting bugs or contributing to the project. Sponsoring is entirely optional, and you can continue without donating."),
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
        function onOnboardingRequirementsMissing() { root.goToStep(3) }
    }

    function goToStep(index) {
        if (saving)
            return
        stepIndex = Math.max(0, Math.min(stepCount - 1, index))
    }

    function next() {
        if (saving)
            return
        if (stepIndex === 3 && !store.verifyOnboardingRequirements())
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
        const margin = DesktopTokens.px(16)
        ancestor = focused.parent
        while (ancestor) {
            if (ancestor instanceof Flickable) {
                const point = focused.mapToItem(ancestor.contentItem, 0, 0)
                if (point.y < ancestor.contentY + margin)
                    ancestor.contentY = Math.max(0, point.y - margin)
                else if (point.y + focused.height > ancestor.contentY + ancestor.height - margin)
                    ancestor.contentY = Math.min(Math.max(0, ancestor.contentHeight - ancestor.height),
                        point.y + focused.height - ancestor.height + margin)
            }
            if (ancestor === pageScroll)
                break
            ancestor = ancestor.parent
        }
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

    DesktopOnboardingBackdrop { anchors.fill: parent }

    component Copy: Text {
        width: parent.width
        color: Theme.textMuted
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.bodySize
        wrapMode: Text.Wrap
        font.weight: Font.Medium
        lineHeightMode: Text.FixedHeight
        lineHeight: DesktopTokens.px(23)
        height: lineCount * lineHeight
    }
    component Eyebrow: Text {
        color: root.mint
        font.family: Theme.monoFont
        font.pixelSize: DesktopTokens.px(10)
        font.weight: Font.Bold
        font.letterSpacing: DesktopTokens.px(1)
        lineHeightMode: Text.FixedHeight
        lineHeight: DesktopTokens.px(12)
        height: lineCount * lineHeight
        wrapMode: Text.Wrap
    }
    component Action: Button {
        id: action
        property bool primary: false
        property color accent: root.mint
        property string keyHint: ""
        property real cornerRadius: DesktopTokens.px(10)
        hoverEnabled: true
        padding: 0
        leftPadding: DesktopTokens.px(20)
        rightPadding: DesktopTokens.px(20)
        implicitHeight: DesktopTokens.px(48)
        background: Rectangle {
            radius: action.cornerRadius
            color: action.primary ? action.accent : action.hovered || action.down ? DesktopTokens.raisedStrong : DesktopTokens.seamSoft
            border.width: action.activeFocus ? 2 : 1
            border.color: action.activeFocus ? Theme.focus : action.primary ? action.accent : Theme.seam
            Behavior on color { ColorAnimation { duration: DesktopTokens.quickDuration } }
        }
        contentItem: Text {
            id: actionLabel
            rightPadding: action.keyHint !== "" ? keyBadge.width + DesktopTokens.px(12) : 0
            text: action.text
            color: action.primary ? Theme.contrastText(action.accent) : Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.px(14)
            font.weight: Font.ExtraBold
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        Rectangle {
            id: keyBadge
            visible: action.keyHint !== ""
            anchors.right: parent.right; anchors.rightMargin: action.rightPadding
            anchors.verticalCenter: parent.verticalCenter
            width: keyText.implicitWidth + DesktopTokens.px(16)
            height: DesktopTokens.px(action.primary ? 32 : 22)
            radius: DesktopTokens.px(6)
            color: action.primary ? "#0B0F1A" : DesktopTokens.raised
            Text {
                id: keyText
                anchors.centerIn: parent
                text: action.keyHint
                color: action.primary ? action.accent : Theme.textMuted
                font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(10)
                font.weight: Font.Bold
            }
        }
        implicitWidth: actionLabel.implicitWidth + leftPadding + rightPadding
        opacity: enabled ? 1 : 0.55
    }

    component Card: Rectangle {
        default property alias content: contents.data
        implicitHeight: contents.implicitHeight + 2
        color: Theme.lightMode ? Theme.glass : "#C70B0F1A"
        radius: DesktopTokens.px(16)
        border.color: Theme.seam
        Column {
            id: contents
            x: 1; y: 1; width: parent.width - 2
        }
    }

    component CardHeading: Rectangle {
        property string text: ""
        property string note: ""
        property color ink: Theme.textMuted
        width: parent.width
        height: DesktopTokens.px(41)
        color: DesktopTokens.seamSoft
        Eyebrow {
            x: DesktopTokens.px(18); anchors.verticalCenter: parent.verticalCenter
            text: parent.text; color: parent.ink
        }
        Eyebrow {
            anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(18)
            anchors.verticalCenter: parent.verticalCenter
            text: parent.note; color: Theme.textMuted; font.weight: Font.Medium
        }
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.seam }
    }

    Text {
        anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(16)
        y: root.height - DesktopTokens.px(360)
        text: "0" + (root.stepIndex + 1)
        color: Theme.label; opacity: 0.035
        font.family: Theme.displayFont; font.pixelSize: DesktopTokens.px(260)
        font.weight: Font.Black; font.letterSpacing: -DesktopTokens.px(13)
    }
    Rectangle {
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
        height: DesktopTokens.px(96)
        color: Theme.lightMode ? "#B8EDF3F8" : "#660B0F1A"
        Rectangle { width: parent.width; height: 1; color: DesktopTokens.seamSoft }
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
            DesktopOnboardingMark {
                Layout.preferredWidth: DesktopTokens.px(26)
                Layout.preferredHeight: DesktopTokens.px(16)
            }
            Text {
                text: "OpenNOW"
                color: Theme.label; font.family: Theme.displayFont
                font.pixelSize: DesktopTokens.px(18); font.weight: Font.Black
            }
            Rectangle {
                visible: !root.compact
                implicitWidth: versionLabel.implicitWidth + DesktopTokens.px(14)
                implicitHeight: DesktopTokens.px(24)
                radius: DesktopTokens.px(6); color: DesktopTokens.raised
                Text {
                    id: versionLabel
                    anchors.centerIn: parent; text: Qt.application.version
                    color: Theme.textMuted; font.family: Theme.monoFont
                    font.pixelSize: DesktopTokens.px(11); font.weight: Font.Bold
                }
            }
            Rectangle {
                implicitWidth: betaLabel.implicitWidth + DesktopTokens.px(16)
                implicitHeight: DesktopTokens.px(24)
                radius: DesktopTokens.px(6)
                color: Qt.rgba(root.amber.r,root.amber.g,root.amber.b,0.12)
                Text {
                    id: betaLabel
                    anchors.centerIn: parent
                    text: qsTr("BETA")
                    color: root.amber; font.family: Theme.monoFont
                    font.pixelSize: DesktopTokens.smallSize; font.weight: Font.Bold
                }
            }
            Item { Layout.fillWidth: true }
            Rectangle {
                visible: !root.compact && root.displayName !== ""
                implicitWidth: DesktopTokens.px(28); implicitHeight: implicitWidth
                radius: width / 2; color: DesktopTokens.raised
                Text {
                    anchors.centerIn: parent; text: root.displayName.charAt(0).toUpperCase()
                    color: Theme.label; font.family: Theme.bodyFont
                    font.pixelSize: DesktopTokens.px(12); font.weight: Font.Black
                }
            }
            Text {
                visible: !root.compact
                text: root.displayName !== "" ? qsTr("Signed in as %1").arg(root.displayName)
                    : root.store.signedIn ? qsTr("Signed in") : qsTr("Offline setup")
                color: Theme.textMuted; font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.px(12)
            }
            Eyebrow {
                visible: !root.compact && root.membership !== ""
                text: root.membership
            }
            Rectangle {
                visible: !root.compact
                Layout.leftMargin: DesktopTokens.px(6); Layout.rightMargin: DesktopTokens.px(6)
                implicitWidth: 1; implicitHeight: DesktopTokens.px(16); color: Theme.seam
            }
            Action {
                objectName: "onboardingSkip"
                text: root.width < DesktopTokens.px(500) ? qsTr("Skip") : qsTr("Skip setup")
                keyHint: root.compact ? "" : "Esc"
                cornerRadius: height / 2
                leftPadding: DesktopTokens.px(14); rightPadding: DesktopTokens.px(6)
                implicitHeight: DesktopTokens.px(32)
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
                            x: DesktopTokens.px(49); y: DesktopTokens.px(26)
                            width: DesktopTokens.px(2); height: DesktopTokens.px(46)
                            visible: railStep.index < root.stepCount - 1
                            color: railStep.index < root.stepIndex ? Qt.rgba(root.mint.r,root.mint.g,root.mint.b,0.35) : DesktopTokens.seamSoft
                        }
                        Rectangle {
                            x: DesktopTokens.px(40); y: 0
                            width: DesktopTokens.px(20); height: width; radius: width / 2
                            color: railStep.index < root.stepIndex ? root.mint : "transparent"
                            border.width: railStep.index === root.stepIndex ? 2 : 0
                            border.color: root.mint
                            Rectangle {
                                anchors.centerIn: parent
                                width: DesktopTokens.px(8); height: width; radius: width / 2
                                color: railStep.index === root.stepIndex ? root.mint : DesktopTokens.textFaint
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
                                color: railStep.index <= root.stepIndex ? Theme.label : DesktopTokens.textFaint
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
                Layout.leftMargin: DesktopTokens.px(root.compact ? 20 : 40)
                Layout.rightMargin: DesktopTokens.px(root.compact ? 20 : 40)
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
                    readonly property real pageMargin: DesktopTokens.px(root.compact ? 16 : 28)
                    contentHeight: Math.max(height, pageBody.y + pageBody.implicitHeight + pageMargin)
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                    Column {
                        id: pageBody
                        objectName: "onboardingPageBody"
                        width: pageScroll.width
                        y: Math.max(pageScroll.pageMargin, (pageScroll.height - implicitHeight) / 2)
                        spacing: DesktopTokens.px(28)
                        Column {
                            visible: root.stepIndex !== 0
                            width: parent.width
                            spacing: DesktopTokens.px(12)
                            Text {
                                objectName: "onboardingHeading"
                                width: parent.width
                                text: root.stepIndex === 5 && root.displayName !== ""
                                    ? qsTr("You're set, %1.").arg(root.displayName) : root.titles[root.stepIndex]
                                color: Theme.label; font.family: Theme.displayFont
                                font.pixelSize: DesktopTokens.px(root.compact ? 38 : root.stepIndex === 5 ? 56 : 48)
                                font.weight: Font.Black
                                font.letterSpacing: -DesktopTokens.px(root.compact ? 38 : root.stepIndex === 5 ? 56 : 48) * 0.03
                                lineHeightMode: Text.FixedHeight
                                lineHeight: DesktopTokens.px(root.compact ? 44 : root.stepIndex === 5 ? 60 : 52)
                                height: lineCount * lineHeight
                                topPadding: -DesktopTokens.px(7)
                                horizontalAlignment: root.stepIndex === 5 ? Text.AlignHCenter : Text.AlignLeft
                                wrapMode: Text.Wrap
                                Accessible.role: Accessible.Heading
                            }
                            Copy {
                                width: Math.min(parent.width, DesktopTokens.px(root.stepIndex === 5 ? 560 : 600))
                                x: root.stepIndex === 5 ? (parent.width - width) / 2 : 0
                                horizontalAlignment: root.stepIndex === 5 ? Text.AlignHCenter : Text.AlignLeft
                                text: root.introductions[root.stepIndex]
                            }
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
            Layout.leftMargin: DesktopTokens.px(root.compact ? 20 : 276)
            Layout.rightMargin: DesktopTokens.px(root.compact ? 20 : 40)
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
            Layout.rightMargin: DesktopTokens.px(root.compact ? 20 : 40)
            spacing: DesktopTokens.px(12)
            RowLayout {
                visible: !root.compact
                Layout.fillWidth: true
                spacing: DesktopTokens.px(16)
                Repeater {
                    model: [{key:"Enter",label:qsTr("Continue")},{key:"Tab",label:qsTr("Explore")},{key:"Esc",label:qsTr("Skip setup")}]
                    delegate: Row {
                        id: footerHint
                        required property var modelData
                        spacing: DesktopTokens.px(8)
                        Rectangle {
                            width: hint.implicitWidth + DesktopTokens.px(12); height: DesktopTokens.px(22)
                            radius: DesktopTokens.px(4); color: DesktopTokens.raised
                            Text {
                                id: hint
                                anchors.centerIn: parent; text: footerHint.modelData.key
                                color: Theme.textMuted; font.family: Theme.monoFont
                                font.pixelSize: DesktopTokens.px(10)
                            }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter; text: footerHint.modelData.label
                            color: Theme.textMuted; font.family: Theme.bodyFont
                            font.pixelSize: DesktopTokens.px(12)
                        }
                    }
                }
                Rectangle { implicitWidth: 1; implicitHeight: DesktopTokens.px(14); color: Theme.seam }
                Eyebrow {
                    Layout.fillWidth: true
                    text: qsTr("YOU CAN CHANGE ALL OF THIS LATER IN SETTINGS")
                    color: Theme.textMuted; font.pixelSize: DesktopTokens.px(9)
                }
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
                keyHint: root.compact ? "" : "ENTER"
                leftPadding: DesktopTokens.px(22); rightPadding: DesktopTokens.px(root.compact ? 22 : 8)
                Layout.minimumWidth: DesktopTokens.px(152)
                text: root.saving ? qsTr("Saving…") : root.stepIndex === 5 ? qsTr("Finish setup")
                    : root.stepIndex === 0 ? qsTr("Let's set things up")
                    : root.stepIndex === 3 && !root.store.onboardingAwdlReady ? qsTr("Disable AWDL to continue") : qsTr("Continue")
                Layout.maximumWidth: root.compact ? Math.max(DesktopTokens.px(144), root.width - DesktopTokens.px(160)) : Infinity
                enabled: !root.saving && (root.stepIndex !== 3 || root.store.onboardingAwdlReady)
                onClicked: root.next()
            }
        }
    }

    Component {
        id: welcomePage
        GridLayout {
            columns: width >= DesktopTokens.px(900) ? 2 : 1
            columnSpacing: DesktopTokens.px(56); rowSpacing: DesktopTokens.px(24)
            Column {
                Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: DesktopTokens.px(640)
                Layout.minimumWidth: 0
                spacing: DesktopTokens.px(16)
                topPadding: DesktopTokens.px(8)
                Row {
                    spacing: DesktopTokens.px(10)
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: DesktopTokens.px(10); height: width; radius: width / 2; color: root.amber
                    }
                    Eyebrow { text: qsTr("NATIVE QT · BETA"); color: root.amber; font.pixelSize: DesktopTokens.px(11); lineHeight: DesktopTokens.px(14) }
                }
                Column {
                    width: parent.width; spacing: DesktopTokens.px(12)
                    Text {
                        width: parent.width; text: root.titles[0]
                        color: Theme.label; font.family: Theme.displayFont
                        font.pixelSize: DesktopTokens.px(root.compact ? 38 : 52); font.weight: Font.Black
                        font.letterSpacing: -DesktopTokens.px(root.compact ? 38 : 52) * 0.03
                        lineHeightMode: Text.FixedHeight; lineHeight: DesktopTokens.px(root.compact ? 44 : 56)
                        height: lineCount * lineHeight
                        topPadding: -DesktopTokens.px(7)
                        wrapMode: Text.Wrap; Accessible.role: Accessible.Heading
                    }
                    Copy { width: Math.min(parent.width, DesktopTokens.px(540)); text: root.introductions[0] }
                }
                Card {
                    width: parent.width
                    border.color: Qt.rgba(root.amber.r,root.amber.g,root.amber.b,0.28)
                    CardHeading { text: qsTr("WHAT BETA MEANS HERE"); ink: root.amber; note: qsTr("BUGS MAY OCCUR") }
                    Repeater {
                        model: [
                            {glyph:"monitor",title:qsTr("Report what went wrong"),body:qsTr("If a stream, a setting or a screen misbehaves, tell us what happened and what you expected instead.")},
                            {glyph:"sliders",title:qsTr("Help us reproduce the problem"),body:qsTr("Include the steps you took, your operating system and the OpenNOW version. Screenshots can help too.")},
                            {glyph:"controller",title:qsTr("Make the client your own"),body:qsTr("Choose desktop or console mode, then adjust your picture. You can revisit all of these choices in Settings.")}
                        ]
                        delegate: Item {
                            id: betaRow
                            required property var modelData
                            required property int index
                            width: parent.width
                            height: Math.max(DesktopTokens.px(82), betaCopy.implicitHeight + DesktopTokens.px(24))
                            Rectangle {
                                x: DesktopTokens.px(18); y: DesktopTokens.px(12)
                                width: DesktopTokens.px(32); height: width; radius: DesktopTokens.px(9)
                                color: DesktopTokens.raised
                                DesktopSettingsIcon {
                                    anchors.centerIn: parent; width: DesktopTokens.px(16); height: width
                                    glyph: betaRow.modelData.glyph
                                }
                            }
                            Column {
                                id: betaCopy
                                x: DesktopTokens.px(64); y: DesktopTokens.px(12)
                                width: parent.width - x - DesktopTokens.px(18); spacing: DesktopTokens.px(3)
                                Copy {
                                    text: betaRow.modelData.title; color: Theme.label
                                    font.pixelSize: DesktopTokens.px(14); font.weight: Font.ExtraBold; lineHeight: DesktopTokens.px(18)
                                }
                                Copy { text: betaRow.modelData.body; font.pixelSize: DesktopTokens.px(13); lineHeight: DesktopTokens.px(18) }
                            }
                            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: DesktopTokens.seamSoft; visible: betaRow.index < 2 }
                        }
                    }
                }
                Flow {
                    width: parent.width; spacing: DesktopTokens.px(12)
                    Action {
                        width: Math.min(implicitWidth, parent.width)
                        text: qsTr("Report a bug on GitHub Issues")
                        implicitHeight: DesktopTokens.px(44)
                        onClicked: Qt.openUrlExternally("https://github.com/OpenCloudGaming/OpenNOW/issues")
                    }
                    Action {
                        width: Math.min(implicitWidth, parent.width)
                        text: qsTr("Browse reported issues ↗")
                        implicitHeight: DesktopTokens.px(44)
                        flat: true
                        background: Item {}
                        onClicked: Qt.openUrlExternally("https://github.com/OpenCloudGaming/OpenNOW/issues")
                    }
                }
                Card {
                    width: parent.width; border.color: DesktopTokens.seamSoft
                    Item {
                        width: parent.width; height: privacyNote.implicitHeight + DesktopTokens.px(28)
                        Column {
                            id: privacyNote
                            x: DesktopTokens.px(18); y: DesktopTokens.px(14)
                            width: parent.width - DesktopTokens.px(36); spacing: DesktopTokens.px(2)
                            Copy { text: qsTr("Review what you share in a bug report"); color: Theme.label; font.weight: Font.ExtraBold; font.pixelSize: DesktopTokens.px(13); lineHeight: DesktopTokens.px(17) }
                            Copy { text: qsTr("Remove personal information from screenshots and logs before posting them on GitHub."); font.pixelSize: DesktopTokens.px(12); lineHeight: DesktopTokens.px(16) }
                        }
                    }
                }
            }
            Column {
                Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: DesktopTokens.px(420)
                Layout.minimumWidth: 0
                topPadding: DesktopTokens.px(parent.columns === 2 ? 100 : 0)
                spacing: DesktopTokens.px(16)
                Item {
                    width: parent.width - DesktopTokens.px(8); height: ticket.height
                    Card {
                        id: ticket
                        width: parent.width
                        Item {
                            width: parent.width; height: ticketTitle.implicitHeight + DesktopTokens.px(38)
                            Column {
                                id: ticketTitle
                                x: DesktopTokens.px(22); y: DesktopTokens.px(22)
                                width: parent.width - DesktopTokens.px(44); spacing: DesktopTokens.px(6)
                                Eyebrow { text: qsTr("THIS BUILD"); color: Theme.textMuted }
                                Text {
                                    width: parent.width; text: Qt.application.version
                                    color: Theme.label; font.family: Theme.monoFont
                                    font.pixelSize: DesktopTokens.px(22); font.weight: Font.Bold; wrapMode: Text.Wrap
                                }
                                Copy { text: qsTr("Qt desktop · native streamer · beta"); font.pixelSize: DesktopTokens.px(12); lineHeight: DesktopTokens.px(16) }
                            }
                            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.seam }
                        }
                        Repeater {
                            model: [
                                {key:qsTr("BUGS"),value:qsTr("OpenCloudGaming/OpenNOW · Issues")},
                                {key:qsTr("UPDATES"),value:qsTr("Releases are published on GitHub")},
                                {key:qsTr("SETTINGS"),value:qsTr("Revisit your choices after setup")},
                                {key:qsTr("SOURCE"),value:qsTr("Free and open source")}
                            ]
                            delegate: RowLayout {
                                id: ticketRow
                                required property var modelData
                                width: parent.width; height: DesktopTokens.px(44)
                                spacing: DesktopTokens.px(14)
                                Eyebrow {
                                    Layout.leftMargin: DesktopTokens.px(22); Layout.preferredWidth: DesktopTokens.px(96)
                                    text: ticketRow.modelData.key; color: Theme.textMuted
                                }
                                Copy {
                                    Layout.fillWidth: true; Layout.rightMargin: DesktopTokens.px(22)
                                    text: ticketRow.modelData.value; color: Theme.label
                                    font.pixelSize: DesktopTokens.px(13); font.weight: Font.Bold; lineHeight: DesktopTokens.px(17)
                                }
                            }
                        }
                    }
                    Rectangle {
                        anchors.right: parent.right; anchors.rightMargin: -DesktopTokens.px(14)
                        y: -DesktopTokens.px(24); width: DesktopTokens.px(75); height: DesktopTokens.px(38)
                        radius: DesktopTokens.px(6); rotation: -7
                        color: Theme.lightMode ? Theme.glass : "#DB0B0F1A"; border.color: root.amber; border.width: 2
                        Rectangle { anchors.fill: parent; anchors.margins: DesktopTokens.px(4); radius: DesktopTokens.px(3); color: "transparent"; border.color: root.amber }
                        Eyebrow { anchors.centerIn: parent; text: qsTr("BETA"); color: root.amber; font.pixelSize: DesktopTokens.px(14) }
                    }
                }
                Row {
                    width: parent.width - DesktopTokens.px(8); spacing: DesktopTokens.px(12)
                    DesktopSettingsIcon { width: DesktopTokens.px(20); height: width; glyph: "check"; ink: root.mint }
                    Copy {
                        width: parent.width - DesktopTokens.px(32)
                        text: qsTr("Thanks for trying the beta this early. Every clear bug report and screenshot helps make the next build better for the people who come after you.")
                        font.pixelSize: DesktopTokens.px(13); lineHeight: DesktopTokens.px(19)
                    }
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
            GridLayout {
                width: parent.width
                columns: width >= DesktopTokens.px(800) ? 2 : 1
                columnSpacing: DesktopTokens.px(24); rowSpacing: DesktopTokens.px(16)
                DesktopSettingsPanel {
                    Layout.fillWidth: true; Layout.preferredWidth: DesktopTokens.px(613)
                    paperStyle: true
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
                Card {
                    Layout.fillWidth: true; Layout.preferredWidth: DesktopTokens.px(479)
                    Item {
                        width: parent.width; height: Math.max(DesktopTokens.px(65), modeNote.implicitHeight + DesktopTokens.px(28))
                        Column {
                            id: modeNote
                            x: DesktopTokens.px(18); y: DesktopTokens.px(14)
                            width: parent.width - DesktopTokens.px(36); spacing: DesktopTokens.px(2)
                            Copy { text: qsTr("Your choice takes effect after setup"); font.pixelSize: DesktopTokens.px(13); lineHeight: DesktopTokens.px(17); color: Theme.label; font.weight: Font.ExtraBold }
                            Copy { text: qsTr("Switch between desktop and console later in Settings."); font.pixelSize: DesktopTokens.px(12); lineHeight: DesktopTokens.px(16) }
                        }
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
            columnSpacing: DesktopTokens.px(24); rowSpacing: DesktopTokens.px(24)
            Column {
                Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: DesktopTokens.px(672)
                Layout.minimumWidth: 0
                spacing: DesktopTokens.px(28)
                Card {
                    width: parent.width
                    border.color: Qt.rgba(root.coral.r,root.coral.g,root.coral.b,0.35)
                    Item {
                        width: parent.width; height: DesktopTokens.px(81)
                        Rectangle {
                            x: DesktopTokens.px(22); y: DesktopTokens.px(20)
                            width: DesktopTokens.px(44); height: width; radius: DesktopTokens.px(12)
                            color: Qt.rgba(root.coral.r,root.coral.g,root.coral.b,0.14)
                            DesktopSettingsIcon { anchors.centerIn: parent; width: DesktopTokens.px(22); height: width; glyph: "heart"; ink: root.coral }
                        }
                        Column {
                            x: DesktopTokens.px(80); y: DesktopTokens.px(20)
                            width: parent.width - x - DesktopTokens.px(22); spacing: DesktopTokens.px(2)
                            Copy { text: qsTr("Sponsor on GitHub"); color: Theme.label; font.pixelSize: DesktopTokens.px(20); font.weight: Font.Black; lineHeight: DesktopTokens.px(24) }
                            Copy { text: "github.com/sponsors/zortos293"; font.pixelSize: DesktopTokens.px(13); lineHeight: DesktopTokens.px(18) }
                        }
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: DesktopTokens.seamSoft }
                    }
                    Item {
                        width: parent.width; height: supportOptions.implicitHeight + DesktopTokens.px(24)
                        GridLayout {
                            id: supportOptions
                            x: DesktopTokens.px(22); y: DesktopTokens.px(18)
                            width: parent.width - DesktopTokens.px(44)
                            columns: width >= DesktopTokens.px(460) ? 3 : 1
                            columnSpacing: DesktopTokens.px(10); rowSpacing: DesktopTokens.px(10)
                            Repeater {
                                model: [
                                    {title:qsTr("Sponsor"),detail:qsTr("See the available ways to contribute on GitHub."),url:"https://github.com/sponsors/zortos293"},
                                    {title:qsTr("Contribute"),detail:qsTr("Help improve the code, translations or documentation."),url:"https://github.com/OpenCloudGaming/OpenNOW"},
                                    {title:qsTr("Report"),detail:qsTr("A clear bug report helps make OpenNOW better."),url:"https://github.com/OpenCloudGaming/OpenNOW/issues"}
                                ]
                                delegate: AbstractButton {
                                    id: contributionOption
                                    required property var modelData
                                    Layout.fillWidth: true; Layout.preferredWidth: DesktopTokens.px(200)
                                    implicitHeight: Math.max(DesktopTokens.px(117), optionCopy.implicitHeight + DesktopTokens.px(28))
                                    Accessible.name: modelData.title; hoverEnabled: true
                                    onClicked: Qt.openUrlExternally(modelData.url)
                                    background: Rectangle {
                                        color: contributionOption.hovered ? DesktopTokens.raised : DesktopTokens.seamSoft
                                        radius: DesktopTokens.px(12); border.width: contributionOption.activeFocus ? 2 : 1
                                        border.color: contributionOption.activeFocus ? Theme.focus : Theme.seam
                                    }
                                    Column {
                                        id: optionCopy
                                        x: DesktopTokens.px(14); y: DesktopTokens.px(14)
                                        width: parent.width - DesktopTokens.px(28); spacing: DesktopTokens.px(8)
                                        Copy { text: contributionOption.modelData.title; color: Theme.label; font.family: Theme.monoFont; font.pixelSize: DesktopTokens.px(18); font.weight: Font.Bold; lineHeight: DesktopTokens.px(24) }
                                        Copy { text: contributionOption.modelData.detail; font.pixelSize: DesktopTokens.px(12); lineHeight: DesktopTokens.px(17) }
                                    }
                                }
                            }
                        }
                    }
                    Item {
                        width: parent.width; height: sponsorActions.implicitHeight + DesktopTokens.px(34)
                        Flow {
                            id: sponsorActions
                            x: DesktopTokens.px(22); y: DesktopTokens.px(18)
                            width: parent.width - DesktopTokens.px(44); spacing: DesktopTokens.px(12)
                            Action {
                                width: Math.min(implicitWidth, parent.width)
                                text: qsTr("Sponsor on GitHub ↗"); primary: true; accent: root.coral
                                onClicked: Qt.openUrlExternally("https://github.com/sponsors/zortos293")
                            }
                            Action { text: qsTr("Not now"); onClicked: root.next() }
                        }
                    }
                    Item {
                        width: parent.width; height: sponsorPromise.implicitHeight + DesktopTokens.px(24)
                        Rectangle { width: parent.width; height: 1; color: DesktopTokens.seamSoft }
                        Copy {
                            id: sponsorPromise
                            x: DesktopTokens.px(22); y: DesktopTokens.px(12); width: parent.width - DesktopTokens.px(44)
                            text: qsTr("Sponsoring is optional. No payment is collected in OpenNOW.")
                            font.pixelSize: DesktopTokens.px(12); lineHeight: DesktopTokens.px(16)
                        }
                    }
                }
                Row {
                    width: parent.width; spacing: DesktopTokens.px(10)
                    Rectangle {
                        width: DesktopTokens.px(28); height: width; radius: width / 2; color: DesktopTokens.raised
                        Text { anchors.centerIn: parent; text: "Z"; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(12); font.weight: Font.Black }
                    }
                    Copy { width: parent.width - DesktopTokens.px(38); text: qsTr("Thank you for helping an open-source client keep improving."); font.pixelSize: DesktopTokens.px(13); lineHeight: DesktopTokens.px(28) }
                }
            }
            Column {
                Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
                Layout.preferredWidth: DesktopTokens.px(420)
                Layout.minimumWidth: 0
                spacing: DesktopTokens.px(20)
                Card {
                    width: parent.width
                    CardHeading { text: qsTr("OTHER WAYS TO HELP") }
                    Repeater {
                        model: [{glyph:"info",text:qsTr("Report a reproducible bug")},{glyph:"globe",text:qsTr("Help with translations")},{glyph:"folder",text:qsTr("Improve the documentation")},{glyph:"sliders",text:qsTr("Contribute to the code")}]
                        delegate: RowLayout {
                            id: contributionRow
                            required property var modelData
                            width: parent.width; height: DesktopTokens.px(38); spacing: DesktopTokens.px(12)
                            DesktopSettingsIcon { Layout.leftMargin: DesktopTokens.px(20); Layout.preferredWidth: DesktopTokens.px(16); Layout.preferredHeight: DesktopTokens.px(16); glyph: contributionRow.modelData.glyph; ink: root.coral }
                            Copy { Layout.fillWidth: true; Layout.rightMargin: DesktopTokens.px(20); text: contributionRow.modelData.text; font.pixelSize: DesktopTokens.px(13); lineHeight: DesktopTokens.px(18) }
                        }
                    }
                }
                Card {
                    width: parent.width
                    Item {
                        width: parent.width; height: repoNote.implicitHeight + DesktopTokens.px(36)
                        Column {
                            id: repoNote
                            x: DesktopTokens.px(20); y: DesktopTokens.px(18)
                            width: parent.width - DesktopTokens.px(40); spacing: DesktopTokens.px(10)
                            Eyebrow { text: qsTr("OPEN SOURCE · OPEN TO EVERYONE"); color: Theme.textMuted }
                            Action {
                                width: Math.min(implicitWidth, parent.width)
                                text: qsTr("Visit the repository ↗"); implicitHeight: DesktopTokens.px(34)
                                onClicked: Qt.openUrlExternally("https://github.com/OpenCloudGaming/OpenNOW")
                            }
                        }
                    }
                }
            }
        }
    }

    Component {
        id: readyPage
        Item {
            implicitHeight: summaryCard.implicitHeight
            Card {
                id: summaryCard
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.min(parent.width, DesktopTokens.px(780))
                CardHeading { text: qsTr("YOUR SETUP"); note: qsTr("READY TO SAVE") }
                Repeater {
                    model: [
                        {glyph:"monitor",label:qsTr("Picture"),step:2,value:qsTr("%1 · %2 FPS · %3 · %4 Mbps").arg(String(root.settings.resolution || "1920x1080").replace("x"," × "))
                            .arg(Number(root.settings.fps ?? 60) === 0 ? qsTr("Auto") : root.settings.fps ?? 60)
                            .arg(String(root.settings.codec || "auto").toUpperCase()).arg(root.settings.maxBitrateMbps ?? 75)},
                        {glyph:"controller",label:qsTr("Mode"),step:1,value:(root.settings.launchInConsoleMode === true ? qsTr("Console") : qsTr("Desktop"))
                            + (root.settings.switchToConsoleOnPad === true ? qsTr(" · switch on gamepad input") : "")},
                        {glyph:"bolt",label:qsTr("Boost"),step:3,value:(root.settings.frameGeneration === "2x" ? qsTr("Frame generation 2× · Experimental") : qsTr("Frame generation off"))
                            + (Qt.platform.os === "osx" ? (root.settings.upscaling === "metalfx" ? qsTr(" · MetalFX · clarity %1 · noise reduction %2").arg(root.settings.upscalingSharpness ?? 10).arg(root.settings.upscalingDenoise ?? 0) : qsTr(" · upscaling off")) : "")},
                        {glyph:"info",label:qsTr("Beta"),step:0,value:qsTr("Beta · report bugs on GitHub")},
                        {glyph:"heart",label:qsTr("Support"),step:4,value:qsTr("Optional · GitHub Sponsors")}
                    ]
                    delegate: AbstractButton {
                        id: summaryRow
                        required property var modelData
                        width: parent.width
                        implicitHeight: Math.max(DesktopTokens.px(59), summaryValue.implicitHeight + DesktopTokens.px(26))
                        hoverEnabled: true
                        Accessible.name: qsTr("Edit %1").arg(modelData.label) + ": " + modelData.value
                        onClicked: root.goToStep(modelData.step)
                        background: Rectangle {
                            color: summaryRow.hovered || summaryRow.activeFocus ? DesktopTokens.raised : "transparent"
                            border.width: summaryRow.activeFocus ? 2 : 0; border.color: Theme.focus
                        }
                        RowLayout {
                            anchors.fill: parent; anchors.leftMargin: DesktopTokens.px(22); anchors.rightMargin: DesktopTokens.px(22)
                            spacing: DesktopTokens.px(16)
                            Rectangle {
                                visible: !root.compact
                                Layout.preferredWidth: DesktopTokens.px(32); Layout.preferredHeight: DesktopTokens.px(32)
                                radius: DesktopTokens.px(9); color: DesktopTokens.raised
                                DesktopSettingsIcon { anchors.centerIn: parent; width: DesktopTokens.px(16); height: width; glyph: summaryRow.modelData.glyph }
                            }
                            Eyebrow { Layout.preferredWidth: DesktopTokens.px(root.compact ? 60 : 76); text: summaryRow.modelData.label.toUpperCase(); color: Theme.textMuted }
                            Copy {
                                id: summaryValue
                                Layout.fillWidth: true; text: summaryRow.modelData.value
                                color: Theme.label; font.weight: Font.ExtraBold; lineHeight: DesktopTokens.px(20)
                            }
                            Eyebrow { text: "0" + (summaryRow.modelData.step + 1); color: Theme.textMuted; font.weight: Font.Medium }
                        }
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: DesktopTokens.seamSoft }
                    }
                }
                Rectangle {
                    width: parent.width
                    implicitHeight: readyNote.implicitHeight + DesktopTokens.px(32)
                    color: DesktopTokens.seamSoft
                    Copy {
                        id: readyNote
                        x: DesktopTokens.px(22); y: DesktopTokens.px(16)
                        width: parent.width - DesktopTokens.px(44)
                        text: qsTr("Your preferences will be saved when you finish setup. You can revisit them in Settings.")
                        font.pixelSize: DesktopTokens.px(12); lineHeight: DesktopTokens.px(16)
                    }
                }
            }
        }
    }
}
