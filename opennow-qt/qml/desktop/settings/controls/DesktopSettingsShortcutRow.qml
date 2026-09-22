import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    objectName: "shortcutCommand-" + settingKey
    required property string settingKey
    required property string title
    required property QtObject binding
    property bool capturing: false
    signal captureRequested()
    signal captureFinished()
    signal navigationRequested(int delta)
    signal searchRequested()

    property string message: ""
    property string tone: ""
    property string conflictOwner: ""
    property string attemptedChord: ""
    property string heldModifiers: ""
    property bool saving: false
    property bool restored: false
    property string pendingAnnouncement: ""

    readonly property string current: binding.value(settingKey)
    readonly property string fallback: String(binding.defaults[settingKey] ?? "")
    readonly property bool unset: current === ""
    readonly property bool changed: !binding.isDefault(settingKey)
    readonly property bool stripOpen: capturing || message !== ""
    readonly property color toneColor: tone === "danger" ? DesktopTokens.danger
        : tone === "amber" ? DesktopTokens.amber : Theme.focus
    readonly property string stateText: capturing && tone === "danger" ? qsTr("Already assigned")
        : capturing && tone === "amber" ? qsTr("Not available")
        : capturing ? qsTr("Listening for a new shortcut")
        : restored ? qsTr("Default restored")
        : unset && fallback !== "" ? qsTr("Cleared · %1 now reaches the game").arg(fallback)
        : changed ? qsTr("Changed · default %1").arg(fallback === "" ? qsTr("Not set") : fallback)
        : ""
    readonly property var actionButtons: [bindingButton, resetButton, clearButton]

    width: parent ? parent.width : 0
    implicitHeight: column.implicitHeight
    activeFocusOnTab: false

    function focusBinding() {
        bindingButton.forceActiveFocus()
    }

    function consumeShortcut(event) {
        if (capturing)
            event.accepted = true
    }

    function announce(text) {
        if (text !== "")
            ShellStore.accessibilityMessage = text
    }

    function resetFeedback() {
        message = ""
        tone = ""
        conflictOwner = ""
        attemptedChord = ""
        heldModifiers = ""
    }

    function beginCapture() {
        if (saving || ShellStore.shortcutUpdateRequestId !== "")
            return
        resetFeedback()
        restored = false
        captureRequested()
        bindingButton.forceActiveFocus()
        announce(qsTr("Listening for a new shortcut"))
    }

    function cancel() {
        const wasCapturing = capturing
        resetFeedback()
        if (wasCapturing)
            captureFinished()
    }

    function save(bindings, announcement) {
        if (ShellStore.updateShortcuts(bindings) === "") {
            tone = "danger"
            message = ShellStore.shortcutUpdateError !== "" ? ShellStore.shortcutUpdateError
                : qsTr("The shortcut could not be saved.")
            announce(message)
            return false
        }
        saving = true
        pendingAnnouncement = announcement
        return true
    }

    function modifierText(modifiers) {
        const parts = []
        if (modifiers & Qt.ControlModifier) parts.push("Ctrl")
        if (modifiers & Qt.AltModifier) parts.push("Alt")
        if (modifiers & Qt.ShiftModifier) parts.push("Shift")
        if (modifiers & Qt.MetaModifier) parts.push("Meta")
        return parts.join("+")
    }

    function captureKey(key, modifiers, autoRepeat, fromController) {
        if (autoRepeat || saving)
            return
        if (key === Qt.Key_Escape && (fromController || modifiers === Qt.NoModifier)) {
            cancel()
            return
        }
        if (fromController) {
            tone = ""
            message = qsTr("Press a key on your keyboard. B cancels.")
            announce(message)
            return
        }
        if ([Qt.Key_Control, Qt.Key_Shift, Qt.Key_Alt, Qt.Key_Meta, Qt.Key_AltGr].indexOf(key) >= 0) {
            heldModifiers = modifierText(modifiers)
            return
        }
        heldModifiers = ""
        const result = binding.validate(settingKey, {key: key, modifiers: modifiers})
        if (result.chord) {
            if (AppController.normalizeShortcut(result.chord) === AppController.normalizeShortcut(current)) {
                cancel()
                return
            }
            attemptedChord = result.chord
            save({[settingKey]: result.chord}, qsTr("%1 set to %2").arg(title).arg(result.chord))
            return
        }
        conflictOwner = result.owner || ""
        attemptedChord = result.attempted || AppController.shortcutFromKey(key, modifiers)
        tone = result.reason === "conflict" ? "danger" : "amber"
        message = result.reason === "conflict"
            ? qsTr("%1 is already used by %2.").arg(attemptedChord).arg(binding.title(conflictOwner))
            : result.error
        announce(message)
    }

    function useHere() {
        if (conflictOwner === "" || attemptedChord === "")
            return
        save({[settingKey]: attemptedChord, [conflictOwner]: ""},
            qsTr("%1 moved to %2. %3 is now not set.").arg(attemptedChord).arg(title).arg(binding.title(conflictOwner)))
    }

    function tryAgain() {
        resetFeedback()
        if (!capturing)
            captureRequested()
        bindingButton.forceActiveFocus()
    }

    function clearBinding() {
        if (unset || ShellStore.shortcutUpdateRequestId !== "")
            return
        resetFeedback()
        save({[settingKey]: ""}, qsTr("%1 shortcut cleared").arg(title))
    }

    function resetToDefault() {
        if (!changed || ShellStore.shortcutUpdateRequestId !== "")
            return
        resetFeedback()
        const owner = binding.owner(fallback, settingKey)
        if (owner !== "") {
            conflictOwner = owner
            attemptedChord = fallback
            tone = "danger"
            message = qsTr("%1 is already used by %2.").arg(fallback).arg(binding.title(owner))
            announce(message)
            return
        }
        if (save({[settingKey]: fallback}, qsTr("%1 reset to %2").arg(title).arg(fallback)))
            restored = true
    }

    function focusLane(delta) {
        const lanes = actionButtons.filter(item => item.visible && item.enabled && item.opacity > 0)
        const index = lanes.indexOf(lanes.find(item => item.activeFocus))
        const next = lanes[index + delta]
        if (next)
            next.forceActiveFocus()
    }

    function handleNavigation(event) {
        const controller = AppController.inputMode === "controller"
        if (event.key === Qt.Key_Up || event.key === Qt.Key_Down) {
            navigationRequested(event.key === Qt.Key_Up ? -1 : 1)
        } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
            focusLane(event.key === Qt.Key_Left ? -1 : 1)
        } else if (event.key === Qt.Key_Delete || controller && event.key === Qt.Key_X) {
            clearBinding()
        } else if (controller && event.key === Qt.Key_Y) {
            resetToDefault()
        } else if (event.key === Qt.Key_Slash) {
            searchRequested()
        } else if (event.key === Qt.Key_Escape && message !== "") {
            cancel()
        } else {
            return
        }
        event.accepted = true
    }

    Keys.onShortcutOverride: event => root.consumeShortcut(event)
    onActiveFocusChanged: if (!activeFocus && !saving && stripOpen) cancel()
    onCapturingChanged: {
        if (capturing)
            bindingButton.forceActiveFocus()
        else if (!saving)
            resetFeedback()
    }

    HoverHandler { id: hover }

    Timer {
        id: restoredTimer
        interval: 2000
        onTriggered: root.restored = false
    }

    Connections {
        target: ShellStore
        function onShortcutUpdateRequestIdChanged() {
            if (!root.saving || ShellStore.shortcutUpdateRequestId !== "")
                return
            root.saving = false
            if (ShellStore.shortcutUpdateError !== "") {
                root.restored = false
                root.tone = "danger"
                root.message = ShellStore.shortcutUpdateError
                root.announce(root.message)
                return
            }
            root.announce(root.pendingAnnouncement)
            root.pendingAnnouncement = ""
            if (root.restored)
                restoredTimer.restart()
            root.cancel()
            if (root.activeFocus)
                bindingButton.forceActiveFocus()
        }
    }

    Column {
        id: column
        width: parent.width

        DesktopSettingsRow {
            id: row
            objectName: "shortcutRow-" + root.settingKey
            width: parent.width
            paperStyle: true
            glyph: "keyboard"
            title: root.title
            description: root.stateText
            rowHeight: DesktopTokens.px(56)
            showDivider: false
            leadingColor: root.capturing ? Qt.rgba(root.toneColor.r, root.toneColor.g, root.toneColor.b, 0.18) : DesktopTokens.raised

            Row {
                spacing: DesktopTokens.px(8)

                AbstractButton {
                    id: bindingButton
                    objectName: "shortcutBinding-" + root.settingKey
                    readonly property real restingWidth: Math.max(DesktopTokens.px(84),
                        (root.unset ? unsetLabel.implicitWidth : currentGlyph.implicitWidth) + DesktopTokens.px(28))
                    width: root.capturing ? Math.max(DesktopTokens.px(220), wellContent.implicitWidth + DesktopTokens.px(28)) : restingWidth
                    height: DesktopTokens.controlHeight
                    hoverEnabled: true
                    padding: 0
                    focusPolicy: Qt.StrongFocus
                    opacity: root.saving ? 0.6 : 1
                    Accessible.role: Accessible.Button
                    Accessible.name: root.title + ", " + (root.unset ? qsTr("Not set") : root.current)
                    Accessible.description: root.capturing ? qsTr("Listening for a new shortcut") : root.stateText
                    onClicked: root.capturing ? undefined : root.beginCapture()
                    Behavior on width {
                        enabled: !AppController.reducedMotion
                        NumberAnimation { duration: DesktopTokens.motionDuration; easing.type: Easing.OutCubic }
                    }
                    Keys.onShortcutOverride: event => root.consumeShortcut(event)
                    Keys.onPressed: event => {
                        if (root.capturing) {
                            event.accepted = true
                            const plainTab = (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab)
                                && (event.modifiers & ~Qt.ShiftModifier) === Qt.NoModifier
                            if (plainTab && root.stripOpen && !event.isAutoRepeat) {
                                stripDisclosure.focusFirstAction()
                                return
                            }
                            root.captureKey(event.key, event.modifiers, event.isAutoRepeat,
                                AppController.inputMode === "controller")
                            return
                        }
                        if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space)
                                && !event.isAutoRepeat) {
                            event.accepted = true
                            root.beginCapture()
                            return
                        }
                        root.handleNavigation(event)
                    }
                    Keys.onReleased: event => {
                        if (!root.capturing)
                            return
                        event.accepted = true
                        root.heldModifiers = root.modifierText(event.modifiers)
                    }

                    background: Rectangle {
                        radius: DesktopTokens.px(10)
                        color: root.capturing ? Qt.rgba(0, 0, 0, Theme.lightMode ? 0.06 : 0.3)
                            : bindingButton.down || bindingButton.hovered ? DesktopTokens.raisedStrong : DesktopTokens.raised
                        border.width: root.capturing || bindingButton.activeFocus ? 2 : 1
                        border.color: root.capturing ? root.toneColor
                            : bindingButton.activeFocus ? DesktopTokens.focus
                            : root.unset ? DesktopTokens.textFaint : Theme.seam
                        Behavior on color { enabled: !AppController.reducedMotion; ColorAnimation { duration: Theme.focusDuration } }
                        Behavior on border.color { enabled: !AppController.reducedMotion; ColorAnimation { duration: DesktopTokens.quickDuration } }
                    }

                    contentItem: Item {
                        KeyboardGlyph {
                            id: currentGlyph
                            anchors.centerIn: parent
                            visible: !root.capturing && !root.unset
                            shortcut: root.current
                            keySize: DesktopTokens.px(22)
                        }
                        Text {
                            id: unsetLabel
                            anchors.centerIn: parent
                            visible: !root.capturing && root.unset
                            text: qsTr("Not set")
                            color: DesktopTokens.textBody
                            font.family: Theme.bodyFont
                            font.pixelSize: DesktopTokens.px(13)
                            font.weight: Font.ExtraBold
                        }
                        Row {
                            id: wellContent
                            objectName: "shortcutCaptureWell"
                            anchors.verticalCenter: parent.verticalCenter
                            x: DesktopTokens.px(14)
                            visible: root.capturing
                            spacing: DesktopTokens.px(8)
                            Rectangle {
                                id: listeningDot
                                visible: root.attemptedChord === "" || root.saving
                                anchors.verticalCenter: parent.verticalCenter
                                width: DesktopTokens.px(8); height: width; radius: width / 2
                                color: root.toneColor
                                SequentialAnimation on opacity {
                                    running: root.capturing && root.tone === "" && !root.saving && !AppController.reducedMotion
                                    loops: Animation.Infinite
                                    alwaysRunToEnd: false
                                    NumberAnimation { to: 0.45; duration: 600; easing.type: Easing.InOutSine }
                                    NumberAnimation { to: 1; duration: 600; easing.type: Easing.InOutSine }
                                    onRunningChanged: if (!running) listeningDot.opacity = 1
                                }
                            }
                            KeyboardGlyph {
                                visible: root.attemptedChord !== "" || root.heldModifiers !== ""
                                anchors.verticalCenter: parent.verticalCenter
                                shortcut: root.attemptedChord !== "" ? root.attemptedChord : root.heldModifiers
                                keySize: DesktopTokens.px(22)
                            }
                            Text {
                                visible: root.attemptedChord === ""
                                anchors.verticalCenter: parent.verticalCenter
                                text: root.heldModifiers !== "" ? "+ …" : qsTr("Press a key combination…")
                                color: DesktopTokens.textBody
                                font.family: Theme.monoFont
                                font.pixelSize: DesktopTokens.monoSize
                                font.weight: Font.DemiBold
                            }
                        }
                    }
                }

                AbstractButton {
                    id: resetButton
                    objectName: "shortcutReset-" + root.settingKey
                    readonly property bool shown: root.changed && !root.capturing
                    width: DesktopTokens.px(32); height: width
                    anchors.verticalCenter: parent.verticalCenter
                    hoverEnabled: true
                    enabled: shown && !root.saving
                    opacity: shown ? 1 : 0
                    focusPolicy: Qt.StrongFocus
                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("Reset %1 to %2").arg(root.title).arg(root.fallback === "" ? qsTr("Not set") : root.fallback)
                    onClicked: root.resetToDefault()
                    Keys.onPressed: event => root.handleNavigation(event)
                    Behavior on opacity { enabled: !AppController.reducedMotion; NumberAnimation { duration: DesktopTokens.quickDuration; easing.type: Easing.OutCubic } }
                    background: Rectangle {
                        radius: DesktopTokens.px(10)
                        color: parent.hovered || parent.down ? DesktopTokens.raisedStrong : DesktopTokens.raised
                        border.width: parent.activeFocus ? 2 : 0
                        border.color: DesktopTokens.focus
                    }
                    contentItem: Item {
                        DesktopSettingsIcon {
                            anchors.centerIn: parent
                            width: DesktopTokens.px(16); height: width
                            glyph: "reset"
                            ink: DesktopTokens.textBody
                        }
                    }
                }

                AbstractButton {
                    id: clearButton
                    objectName: "shortcutClear-" + root.settingKey
                    readonly property bool shown: !root.unset && !root.capturing && (hover.hovered || root.activeFocus)
                    width: DesktopTokens.px(32); height: width
                    anchors.verticalCenter: parent.verticalCenter
                    hoverEnabled: true
                    enabled: shown && !root.saving
                    opacity: shown ? 1 : 0
                    focusPolicy: Qt.StrongFocus
                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("Clear %1 shortcut").arg(root.title)
                    onClicked: root.clearBinding()
                    Keys.onPressed: event => root.handleNavigation(event)
                    Behavior on opacity { enabled: !AppController.reducedMotion; NumberAnimation { duration: DesktopTokens.quickDuration; easing.type: Easing.OutCubic } }
                    background: Rectangle {
                        radius: DesktopTokens.px(10)
                        color: parent.hovered || parent.down ? DesktopTokens.raisedStrong : DesktopTokens.raised
                        border.width: parent.activeFocus ? 2 : 0
                        border.color: DesktopTokens.focus
                    }
                    contentItem: Item {
                        DesktopSettingsIcon {
                            anchors.centerIn: parent
                            width: DesktopTokens.px(16); height: width
                            glyph: "close"
                            ink: DesktopTokens.textBody
                        }
                    }
                }
            }
        }

        DesktopSettingsDisclosure {
            id: stripDisclosure
            objectName: "shortcutStrip-" + root.settingKey
            width: parent.width
            expanded: root.stripOpen

            function focusFirstAction() {
                if (item && item.firstAction)
                    item.firstAction.forceActiveFocus()
            }

            sourceComponent: Rectangle {
                readonly property Item firstAction: primaryAction
                width: stripDisclosure.width
                implicitHeight: stripRow.implicitHeight + DesktopTokens.px(20)
                height: implicitHeight
                color: DesktopTokens.seamSoft
                Row {
                    id: stripRow
                    x: DesktopTokens.settingsLabelInset
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - x - DesktopTokens.settingsInset
                    spacing: DesktopTokens.px(10)
                    Text {
                        objectName: "shortcutMessage-" + root.settingKey
                        width: parent.width - actions.width - parent.spacing
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.message !== "" ? root.message : qsTr("Press a new shortcut or clear this binding. Escape cancels.")
                        color: root.tone === "danger" ? (Theme.lightMode ? "#9F1239" : "#FFC2C2")
                            : root.tone === "amber" ? (Theme.lightMode ? "#8A5A00" : DesktopTokens.amber)
                            : DesktopTokens.textBody
                        font.family: Theme.bodyFont
                        font.pixelSize: DesktopTokens.captionSize
                        font.weight: Font.Bold
                        wrapMode: Text.WordWrap
                        Accessible.role: Accessible.AlertMessage
                        Accessible.name: text
                    }
                    Row {
                        id: actions
                        spacing: DesktopTokens.px(8)
                        anchors.verticalCenter: parent.verticalCenter
                        DesktopSettingsButton {
                            id: primaryAction
                            objectName: root.conflictOwner !== "" ? "shortcutUseHere-" + root.settingKey : "shortcutClearInStrip-" + root.settingKey
                            compact: true
                            danger: root.conflictOwner !== ""
                            visible: root.conflictOwner !== "" || root.capturing
                            text: root.conflictOwner !== "" ? qsTr("Use here") : qsTr("Clear shortcut")
                            onClicked: root.conflictOwner !== "" ? root.useHere() : root.clearBinding()
                            Keys.onEscapePressed: root.cancel()
                            Keys.onShortcutOverride: event => root.consumeShortcut(event)
                        }
                        DesktopSettingsButton {
                            objectName: "shortcutCancel-" + root.settingKey
                            compact: true
                            text: root.conflictOwner !== "" && root.capturing ? qsTr("Try again") : qsTr("Cancel")
                            onClicked: root.conflictOwner !== "" && root.capturing ? root.tryAgain() : root.cancel()
                            Keys.onEscapePressed: root.cancel()
                            Keys.onShortcutOverride: event => root.consumeShortcut(event)
                        }
                    }
                }
            }
        }

        Rectangle {
            x: DesktopTokens.settingsInset
            width: parent.width - x * 2
            height: 1
            color: DesktopTokens.seamSoft
        }
    }
}
