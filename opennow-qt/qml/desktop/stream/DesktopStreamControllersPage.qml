pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    objectName: "streamControllersPage"
    width: 456
    height: 868
    property bool active: false
    property bool controllerHints: false
    signal backRequested()
    signal resumeRequested()

    readonly property var pads: ControllerInput.availableControllers || []
    readonly property var flightDevices: ControllerInput.flightDevices || []
    readonly property int activeFlightCount: flightDevices.filter(device => device.status === "active").length
    readonly property color flightColor: activeFlightCount === flightDevices.length
        ? (Theme.lightMode ? DesktopTokens.flightReady : DesktopTokens.flightMint)
        : (Theme.lightMode ? DesktopTokens.flightWarning : DesktopTokens.ledAmber)
    readonly property var profiles: ShellStore.settings.controllerProfiles || ({})
    property int selectedInstanceId: 0
    property string lostPadName: ""
    property string lostPadKey: ""
    property var previousPads: []
    property var sample: ({})
    readonly property var selectedPad: pads.find(pad => Number(pad.instanceId) === selectedInstanceId) || null
    readonly property var selectedProfile: selectedPad ? (profiles[selectedPad.profileKey] || ({})) : ({})
    readonly property bool custom: ["leftStickDeadzone", "rightStickDeadzone", "vibrationIntensity"]
        .some(field => root.selectedProfile[field] !== undefined)
    readonly property var sharedWith: selectedPad
        ? pads.filter(pad => pad.profileKey === selectedPad.profileKey && pad.instanceId !== selectedPad.instanceId) : []
    readonly property bool playerOneMissing: Number(ControllerInput.inputControllerId) !== 0
        && !pads.some(pad => Number(pad.instanceId) === Number(ControllerInput.inputControllerId))
    readonly property int globalLeft: Number(ShellStore.settings.controllerLeftStickDeadzone ?? 5)
    readonly property int globalRight: Number(ShellStore.settings.controllerRightStickDeadzone ?? 5)
    readonly property int globalVibration: Number(ShellStore.settings.controllerVibrationIntensity ?? 100)

    Binding {
        target: ControllerInput
        property: "leftStickNavigationEnabled"
        value: false
        when: root.active
    }

    function padLabel(pad) {
        return qsTr("P%1").arg(pad.slot)
    }
    function flightType(device) {
        switch (device.type) {
        case "throttle": return qsTr("throttle")
        case "pedals": return qsTr("pedals")
        case "wheel": return qsTr("wheel")
        default: return qsTr("flight stick")
        }
    }
    function flightStatus(device) {
        switch (device.status) {
        case "active": return qsTr("ACTIVE")
        case "declined": return qsTr("DECLINED")
        case "no-access": return qsTr("NO PERMISSION")
        case "no-hid-path": return qsTr("NO HID PATH")
        case "controller": return qsTr("CONTROLLER")
        case "ignored": return qsTr("IGNORED")
        default: return qsTr("READY")
        }
    }
    function flightTone(device) {
        if (device.status === "active") return DesktopTokens.flightReady
        if (device.status === "declined" || device.status === "no-access" || device.status === "no-hid-path") return DesktopTokens.flightDeclined
        if (device.status === "controller") return DesktopTokens.flightWarning
        return DesktopTokens.textMuted
    }
    function flightDetail(device) {
        let detail = flightType(device)
        switch (device.status) {
        case "declined": detail = qsTr("declined by the server, not sent"); break
        case "no-access": detail = qsTr("no permission to open, not sent"); break
        case "no-hid-path": detail = qsTr("no hidraw device, not sent"); break
        case "controller": detail = qsTr("sent as controller P%1").arg(device.player); break
        case "ignored": detail = qsTr("not forwarded"); break
        }
        return [device.vidPid, detail].filter(value => value).join(" · ")
    }
    function padStatus(pad) {
        if (pad.charging) return pad.batteryPercent >= 0 ? qsTr("Charging · %1%").arg(pad.batteryPercent) : qsTr("Charging")
        if (pad.batteryPercent >= 0) return pad.batteryPercent + "%"
        return pad.powerState === "noBattery" ? qsTr("Wired") : ""
    }
    function padGlyph(pad) {
        return InputPromptIcons.sourceFor(pad.family === "playstation" ? "playstation" : "controller", DesktopTokens.textHigh)
    }
    function selectPad(step) {
        if (pads.length < 2) return
        const index = Math.max(0, pads.findIndex(pad => Number(pad.instanceId) === selectedInstanceId))
        selectedInstanceId = Number(pads[(index + step + pads.length) % pads.length].instanceId)
        lostPadName = ""
    }
    function profileCopy() {
        return JSON.parse(JSON.stringify(profiles))
    }
    function writePad(pad, changes) {
        if (!pad) return
        const updated = profileCopy()
        const profile = Object.assign({}, updated[pad.profileKey] || {}, changes)
        profile.name = String(pad.name || "")
        profile.lastSeen = Date.now()
        updated[pad.profileKey] = profile
        ShellStore.setSetting("controllerProfiles", updated)
    }
    function resetPad() {
        if (!selectedPad || !custom) return
        const updated = profileCopy()
        delete updated[selectedPad.profileKey]
        ShellStore.setSetting("controllerProfiles", updated)
    }
    function applyToAll() {
        if (!selectedPad || pads.length < 2) return
        const values = {
            leftStickDeadzone: Number(selectedPad.leftStickDeadzone),
            rightStickDeadzone: Number(selectedPad.rightStickDeadzone),
            vibrationIntensity: Number(selectedPad.vibrationIntensity)
        }
        const updated = profileCopy()
        for (const pad of pads)
            updated[pad.profileKey] = Object.assign({}, updated[pad.profileKey] || {}, values,
                {name: String(pad.name || ""), lastSeen: Date.now()})
        ShellStore.setSetting("controllerProfiles", updated)
    }
    function setPlayerOne(enabled) {
        if (enabled && !selectedPad) return
        ControllerInput.inputControllerId = enabled ? Number(selectedPad.instanceId) : 0
    }

    readonly property var focusItems: selectedPad
        ? [backButton, padTabs, leftTune.control, rightTune.control, vibrationTune.control, testButton,
           playerOneRow, resetButton, applyButton]
        : [backButton, globalLeftTune.control, globalRightTune.control, globalVibrationTune.control]
    property var focusFollowed: []
    function followFocus(item) {
        if (!item || root.focusFollowed.indexOf(item) >= 0) return
        root.focusFollowed = root.focusFollowed.concat([item])
        item.activeFocusChanged.connect(() => {
            if (item.activeFocus) scroll.reveal(item)
        })
    }
    function focusFirst() {
        Qt.callLater(() => {
            const first = root.focusItems.find(item => item.visible && item.enabled && item !== backButton)
            ;(first || backButton).forceActiveFocus()
            scroll.contentY = 0
        })
    }
    function stepFocus(step) {
        const items = focusItems.filter(item => item.visible && item.enabled)
        if (items.length === 0) return
        const from = items.findIndex(item => item.activeFocus)
        const next = items[(Math.max(from, step > 0 ? -1 : 0) + step + items.length) % items.length]
        next.forceActiveFocus()
        scroll.reveal(next)
    }

    onPadsChanged: {
        const missing = selectedInstanceId !== 0 && !pads.some(pad => Number(pad.instanceId) === selectedInstanceId)
        if (missing) {
            const lost = previousPads.find(pad => Number(pad.instanceId) === selectedInstanceId)
            if (active && lost) {
                lostPadName = String(lost.name || qsTr("Controller"))
                lostPadKey = String(lost.profileKey || "")
            }
            selectedInstanceId = pads.length > 0 ? Number(pads[0].instanceId) : 0
        } else if (selectedInstanceId === 0 && pads.length > 0) {
            selectedInstanceId = Number(pads[0].instanceId)
        }
        if (lostPadKey !== "" && pads.some(pad => pad.profileKey === lostPadKey)) {
            lostPadName = ""
            lostPadKey = ""
        }
        previousPads = pads
        if (active && !root.activeFocusItemInside()) focusFirst()
    }
    function activeFocusItemInside() {
        return focusItems.some(item => item.activeFocus)
    }
    onFocusItemsChanged: focusItems.forEach(root.followFocus)
    onActiveChanged: {
        lostPadName = ""
        lostPadKey = ""
        if (active && !selectedPad && pads.length > 0) selectedInstanceId = Number(pads[0].instanceId)
    }
    Component.onCompleted: {
        previousPads = pads
        focusItems.forEach(root.followFocus)
    }

    Timer {
        objectName: "controllerStickSampler"
        interval: 33
        repeat: true
        running: root.active && root.selectedPad !== null
        onTriggered: root.sample = ControllerInput.stickSample(Number(root.selectedPad.instanceId)) || ({})
        onRunningChanged: if (!running) root.sample = ({})
    }

    component SectionLabel: Text {
        color: DesktopTokens.textFaint
        font.family: DesktopTokens.monoFont
        font.pixelSize: 9
        font.weight: Font.Bold
        font.letterSpacing: 1.2
    }

    component Notice: Rectangle {
        id: notice
        property color tone: DesktopTokens.amber
        property string title: ""
        property string body: ""
        width: parent ? parent.width : 0
        height: noticeColumn.height + 24
        radius: 12
        color: Qt.rgba(tone.r, tone.g, tone.b, 0.08)
        border.width: 1
        border.color: Qt.rgba(tone.r, tone.g, tone.b, 0.32)
        Rectangle { x: 14; y: 18; width: 7; height: 7; radius: 4; color: notice.tone }
        Column {
            id: noticeColumn
            x: 30; y: 12
            width: parent.width - 44
            spacing: 3
            Text {
                width: parent.width
                visible: notice.title !== ""
                text: notice.title
                color: notice.tone
                font.family: DesktopTokens.bodyFont; font.pixelSize: 13; font.weight: Font.Bold
                wrapMode: Text.WordWrap
            }
            Text {
                width: parent.width
                text: notice.body
                color: DesktopTokens.textBody
                font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold
                wrapMode: Text.WordWrap
                lineHeight: 1.2
            }
        }
    }

    component TuneSlider: Item {
        id: tune
        property string title: ""
        property string note: ""
        property real value: 0
        property int maximum: 50
        property int trailingWidth: 0
        readonly property alias control: slider.control
        signal committed(int value)
        width: parent ? parent.width : 0
        height: 58
        Rectangle {
            anchors.fill: parent
            anchors.margins: -4
            radius: 12
            color: slider.control.activeFocus ? "#0F7FD4FF" : "transparent"
            border.width: 2
            border.color: DesktopTokens.focus
            visible: slider.control.activeFocus && AppController.inputMode !== "pointer"
        }
        Text {
            x: 6; y: 4
            text: tune.title
            color: DesktopTokens.textHigh
            font.family: DesktopTokens.bodyFont; font.pixelSize: 13; font.weight: Font.Bold
        }
        Text {
            anchors.right: parent.right; anchors.rightMargin: tune.trailingWidth + 6; y: 6
            text: tune.note
            color: DesktopTokens.textFaint
            font.family: DesktopTokens.monoFont; font.pixelSize: 9; font.weight: Font.Bold; font.letterSpacing: 0.9
        }
        DesktopSettingsSlider {
            id: slider
            x: 6; y: 26
            from: 0; to: tune.maximum; stepSize: 1
            value: tune.value
            wheelEnabled: true
            accessibleName: tune.title
            trackWidth: tune.width - 12 - tune.trailingWidth - DesktopTokens.px(80) - spacing
            onCommitted: value => tune.committed(Math.round(value))
        }
    }

    component StickRing: Item {
        id: ring
        property string label: ""
        property int deadzone: 5
        property real rawX: 0
        property real rawY: 0
        property real outX: 0
        property real outY: 0
        property bool live: false
        readonly property real radius: Math.min(width, 132) / 2 - 2
        readonly property real magnitude: Math.min(1, Math.hypot(rawX, rawY))
        readonly property bool absorbed: magnitude * 100 <= deadzone
        width: 180
        height: 196
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: ring.label + "  " + ring.deadzone + "%"
            color: DesktopTokens.textMuted
            font.family: DesktopTokens.monoFont; font.pixelSize: 9; font.weight: Font.Bold; font.letterSpacing: 1.2
        }
        Item {
            id: face
            x: (parent.width - width) / 2
            y: 22
            width: ring.radius * 2
            height: width
            Rectangle { anchors.fill: parent; radius: width / 2; color: "#0AFFFFFF"; border.width: 1; border.color: "#2EFFFFFF" }
            Rectangle { x: parent.width / 2; y: 8; width: 1; height: parent.height - 16; color: "#14FFFFFF" }
            Rectangle { x: 8; y: parent.height / 2; width: parent.width - 16; height: 1; color: "#14FFFFFF" }
            Rectangle {
                objectName: "deadzoneCircle"
                anchors.centerIn: parent
                width: Math.max(4, parent.width * ring.deadzone / 100)
                height: width
                radius: width / 2
                color: "#14FFD166"
                border.width: 1
                border.color: "#80FFD166"
            }
            Rectangle {
                visible: ring.live && !ring.absorbed
                x: face.width / 2
                y: face.height / 2 - 1
                width: Math.hypot(ring.outX, ring.outY) * ring.radius
                height: 2
                color: DesktopTokens.focus
                transformOrigin: Item.Left
                rotation: Math.atan2(ring.outY, ring.outX) * 180 / Math.PI
            }
            Rectangle {
                visible: ring.live
                width: 10; height: 10; radius: 5
                x: face.width / 2 + ring.rawX * ring.radius - 5
                y: face.height / 2 + ring.rawY * ring.radius - 5
                color: ring.absorbed ? DesktopTokens.amber : "#FFFFFF"
            }
            Rectangle {
                visible: ring.live && !ring.absorbed
                width: 12; height: 12; radius: 6
                x: face.width / 2 + ring.outX * ring.radius - 6
                y: face.height / 2 + ring.outY * ring.radius - 6
                color: DesktopTokens.focus
            }
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            y: face.y + face.height + 8
            text: !ring.live ? qsTr("Waiting for the stick")
                : ring.magnitude < 0.01 ? qsTr("Centered")
                : ring.absorbed ? qsTr("Drift %1% · absorbed").arg(Math.round(ring.magnitude * 100))
                : qsTr("Out %1% · in-game").arg(Math.round(Math.hypot(ring.outX, ring.outY) * 100))
            color: ring.live && ring.absorbed && ring.magnitude >= 0.01 ? DesktopTokens.amber : DesktopTokens.textMuted
            font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold
        }
    }

    Item {
        id: header
        x: 22
        y: 20
        width: parent.width - 44
        height: 44
        DesktopStreamDrawerButton {
            id: backButton
            objectName: "streamControllersBack"
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            width: 40; height: 40; radius: 12
            text: qsTr("Back")
            description: qsTr("Back to the menu")
            onClicked: root.backRequested()
            Text {
                anchors.centerIn: parent
                anchors.horizontalCenterOffset: -1
                text: "‹"
                color: DesktopTokens.textHigh
                font.family: DesktopTokens.bodyFont; font.pixelSize: 24; font.weight: Font.Bold
            }
        }
        Text {
            anchors.left: backButton.right
            anchors.leftMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            text: qsTr("Controllers")
            color: DesktopTokens.text
            font.family: DesktopTokens.displayFont; font.pixelSize: 22; font.weight: Font.Black
        }
        KeyboardGlyph {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: !root.controllerHints
            shortcut: "Esc"; keySize: 20; ink: DesktopTokens.textMuted
        }
        ControllerGlyph {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            visible: root.controllerHints
            glyph: "B"; label: ""; glyphSize: 20; spacing: 0; glyphColor: DesktopTokens.danger
        }
    }

    Flickable {
        id: scroll
        x: 22
        y: header.y + header.height + 14
        width: parent.width - 44
        height: footer.y - y - 12
        contentWidth: width
        contentHeight: content.height + 8
        interactive: contentHeight > height
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        ScrollIndicator.vertical: ScrollIndicator {}
        function reveal(item) {
            const top = item.mapToItem(content, 0, 0).y - 10
            const bottom = top + item.height + 20
            if (top < contentY) contentY = Math.max(0, top)
            else if (bottom > contentY + height) contentY = Math.min(contentHeight - height, bottom - height)
        }

        Column {
            id: content
            x: 4
            y: 4
            width: scroll.width - 8
            spacing: 14

            Column {
                objectName: "streamFlightDevices"
                visible: root.flightDevices.length > 0
                width: parent.width
                spacing: 10
                Item {
                    width: parent.width
                    height: 16
                    SectionLabel {
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - flightCount.width - 12
                        text: ControllerInput.forwardFlightControls ? qsTr("FLIGHT CONTROLS · SENT AS THE REAL DEVICE")
                            : qsTr("FLIGHT CONTROLS · FORWARDING OFF")
                        elide: Text.ElideRight
                    }
                    SectionLabel {
                        id: flightCount
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("%1 OF %2").arg(root.activeFlightCount).arg(root.flightDevices.length)
                        color: root.flightColor
                    }
                }
                Rectangle {
                    width: parent.width
                    height: flightRows.height + 2
                    radius: 14
                    color: "#08FFFFFF"
                    border.width: 1
                    border.color: "#1FFFFFFF"
                    Column {
                        id: flightRows
                        x: 1; y: 1
                        width: parent.width - 2
                        Repeater {
                            model: root.flightDevices
                            delegate: Item {
                                id: flightDevice
                                required property var modelData
                                required property int index
                                width: flightRows.width
                                height: 56
                                readonly property color tone: root.flightTone(modelData)
                                Rectangle { visible: flightDevice.index > 0; width: parent.width; height: 1; color: "#0FFFFFFF" }
                                Rectangle {
                                    x: 12; anchors.verticalCenter: parent.verticalCenter
                                    width: 30; height: 30; radius: 8; color: "#12FFFFFF"
                                    DesktopSettingsIcon {
                                        anchors.centerIn: parent; width: 16; height: 16
                                        glyph: ["throttle", "pedals", "wheel"].includes(flightDevice.modelData.type)
                                            ? flightDevice.modelData.type : "stick"
                                        ink: DesktopTokens.textBody
                                    }
                                }
                                Column {
                                    x: 54; anchors.verticalCenter: parent.verticalCenter
                                    width: Math.max(0, stateBadge.x - x - 12)
                                    spacing: 2
                                    Text {
                                        width: parent.width
                                        text: flightDevice.modelData.name
                                        color: DesktopTokens.textHigh
                                        font.family: DesktopTokens.bodyFont; font.pixelSize: 13; font.weight: Font.Bold
                                        elide: Text.ElideRight
                                    }
                                    Text {
                                        width: parent.width
                                        text: root.flightDetail(flightDevice.modelData)
                                        color: DesktopTokens.textMuted
                                        font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold
                                        elide: Text.ElideRight
                                    }
                                }
                                Rectangle {
                                    id: stateBadge
                                    anchors.right: parent.right; anchors.rightMargin: 14
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: flightState.implicitWidth + 20; height: 20; radius: 10
                                    color: Qt.rgba(flightDevice.tone.r, flightDevice.tone.g, flightDevice.tone.b, 0.09)
                                    border.width: 1
                                    border.color: Qt.rgba(flightDevice.tone.r, flightDevice.tone.g, flightDevice.tone.b, 0.22)
                                    SectionLabel {
                                        id: flightState
                                        anchors.centerIn: parent
                                        text: root.flightStatus(flightDevice.modelData)
                                        color: flightDevice.tone
                                        font.pixelSize: 8; font.letterSpacing: 0.8
                                    }
                                }
                                Accessible.role: Accessible.ListItem
                                Accessible.name: modelData.name + " · " + root.flightDetail(modelData) + " · " + root.flightStatus(modelData)
                            }
                        }
                    }
                }
                Text {
                    width: parent.width
                    text: qsTr("The game reads these devices directly, so set dead zones and curves in the game. Turn forwarding off in Settings › Controls.")
                    color: DesktopTokens.textFaint
                    font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold
                    wrapMode: Text.WordWrap
                    lineHeight: 1.2
                }
            }

            Notice {
                objectName: "controllerDisconnectedNotice"
                visible: root.lostPadName !== ""
                tone: DesktopTokens.danger
                title: qsTr("%1 disconnected").arg(root.lostPadName)
                body: qsTr("The game got a neutral pad, so nothing stays pressed. Its values are saved and come back when it reconnects.")
            }

            Rectangle {
                objectName: "noControllerState"
                visible: root.pads.length === 0
                width: parent.width
                height: 176
                radius: 14
                color: "#08FFFFFF"
                border.width: 1
                border.color: "#1FFFFFFF"
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 22
                    width: 52; height: 52; radius: 14
                    color: "#12FFFFFF"
                    DesktopGlyph { anchors.centerIn: parent; width: 28; height: 28; icon: "desktop-gamepad.svg"; opacity: 0.8 }
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 88
                    text: qsTr("No controller connected")
                    color: DesktopTokens.textHigh
                    font.family: DesktopTokens.bodyFont; font.pixelSize: 16; font.weight: Font.Bold
                }
                Text {
                    x: 24; y: 116
                    width: parent.width - 48
                    horizontalAlignment: Text.AlignHCenter
                    text: qsTr("Plug in or pair a pad and it shows up here. Mouse and keyboard keep working as usual.")
                    color: DesktopTokens.textMuted
                    font.family: DesktopTokens.bodyFont; font.pixelSize: 12; font.weight: Font.DemiBold
                    wrapMode: Text.WordWrap
                    lineHeight: 1.2
                }
            }

            Column {
                visible: root.pads.length === 0
                width: parent.width
                spacing: 10
                SectionLabel { text: qsTr("DEFAULTS FOR EVERY PAD") }
                TuneSlider {
                    id: globalLeftTune
                    objectName: "controllerDefaultLeftDeadzone"
                    title: qsTr("Left stick dead zone")
                    value: root.globalLeft
                    onCommitted: value => ShellStore.setSetting("controllerLeftStickDeadzone", value)
                }
                TuneSlider {
                    id: globalRightTune
                    objectName: "controllerDefaultRightDeadzone"
                    title: qsTr("Right stick dead zone")
                    value: root.globalRight
                    onCommitted: value => ShellStore.setSetting("controllerRightStickDeadzone", value)
                }
                TuneSlider {
                    id: globalVibrationTune
                    objectName: "controllerDefaultVibration"
                    title: qsTr("Vibration")
                    maximum: 100
                    value: root.globalVibration
                    onCommitted: value => ShellStore.setSetting("controllerVibrationIntensity", value)
                }
                Text {
                    width: parent.width
                    text: qsTr("These are the same values as Settings › Controllers. A pad without its own values uses them. Changes apply right away.")
                    color: DesktopTokens.textFaint
                    font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold
                    wrapMode: Text.WordWrap
                }
            }

            FocusScope {
                id: padTabs
                objectName: "controllerPadTabs"
                visible: root.selectedPad !== null
                width: parent.width
                height: root.pads.length > 2 ? 44 : 54
                activeFocusOnTab: true
                readonly property bool focusVisible: activeFocus && AppController.inputMode !== "pointer"
                Accessible.role: Accessible.PageTabList
                Accessible.name: qsTr("Controllers")
                Keys.onLeftPressed: event => { root.selectPad(-1); event.accepted = true }
                Keys.onRightPressed: event => { root.selectPad(1); event.accepted = true }
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: -4
                    radius: 14
                    color: "transparent"
                    border.width: 2
                    border.color: DesktopTokens.focus
                    visible: padTabs.focusVisible
                }
                Row {
                    id: tabRow
                    width: parent.width
                    height: parent.height
                    spacing: 8
                    ControllerGlyph {
                        id: lbGlyph
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.controllerHints && root.pads.length > 1
                        glyph: "LB"; label: ""; glyphSize: 22; spacing: 0
                    }
                    Repeater {
                        model: root.pads
                        delegate: Rectangle {
                            id: tab
                            required property var modelData
                            readonly property bool selected: Number(modelData.instanceId) === root.selectedInstanceId
                            readonly property bool shared: root.pads.some(pad => pad.profileKey === tab.modelData.profileKey
                                && pad.instanceId !== tab.modelData.instanceId)
                            objectName: "controllerTab-" + modelData.slot
                            width: (tabRow.width - (lbGlyph.visible ? lbGlyph.width + rbGlyph.width + 16 : 0)
                                - 8 * (root.pads.length - 1)) / root.pads.length
                            height: parent.height
                            radius: 12
                            color: selected ? "#1F7FD4FF" : tabHover.hovered ? "#14FFFFFF" : "#0AFFFFFF"
                            border.width: 1
                            border.color: selected ? "#667FD4FF" : "#17FFFFFF"
                            Accessible.role: Accessible.PageTab
                            Accessible.name: tab.modelData.name
                            Accessible.selected: selected
                            Image {
                                id: tabIcon
                                x: 10; anchors.verticalCenter: parent.verticalCenter
                                width: 22; height: 22
                                source: root.padGlyph(tab.modelData)
                                sourceSize: Qt.size(44, 44)
                                fillMode: Image.PreserveAspectFit
                            }
                            Column {
                                x: tabIcon.x + tabIcon.width + 8
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - x - 8
                                spacing: 2
                                Text {
                                    width: parent.width
                                    text: root.pads.length > 2 ? root.padLabel(tab.modelData) : String(tab.modelData.name)
                                    color: DesktopTokens.textHigh
                                    font.family: DesktopTokens.bodyFont; font.pixelSize: 12; font.weight: Font.Bold
                                    elide: Text.ElideRight
                                }
                                Text {
                                    width: parent.width
                                    visible: root.pads.length <= 2
                                    text: [root.padLabel(tab.modelData), root.padStatus(tab.modelData)].filter(part => part !== "").join(" · ")
                                    color: tab.selected ? DesktopTokens.focus : DesktopTokens.textMuted
                                    font.family: DesktopTokens.monoFont; font.pixelSize: 9; font.weight: Font.Bold
                                    elide: Text.ElideRight
                                }
                            }
                            Rectangle {
                                objectName: "controllerSharedMarker"
                                visible: tab.shared
                                anchors.right: parent.right; anchors.rightMargin: 8; y: 8
                                width: 8; height: 8; radius: 4
                                color: DesktopTokens.amber
                            }
                            HoverHandler { id: tabHover }
                            TapHandler {
                                onTapped: {
                                    root.selectedInstanceId = Number(tab.modelData.instanceId)
                                    root.lostPadName = ""
                                    padTabs.forceActiveFocus()
                                }
                            }
                        }
                    }
                    ControllerGlyph {
                        id: rbGlyph
                        anchors.verticalCenter: parent.verticalCenter
                        visible: lbGlyph.visible
                        glyph: "RB"; label: ""; glyphSize: 22; spacing: 0
                    }
                }
            }

            Item {
                visible: root.selectedPad !== null
                width: parent.width
                height: 24
                Rectangle {
                    objectName: "controllerProfileBadge"
                    anchors.verticalCenter: parent.verticalCenter
                    width: badgeText.implicitWidth + 26
                    height: 22
                    radius: 11
                    color: root.custom ? "#1A7FD4FF" : "#0FFFFFFF"
                    border.width: 1
                    border.color: root.custom ? "#4D7FD4FF" : "#1FFFFFFF"
                    Rectangle { x: 9; anchors.verticalCenter: parent.verticalCenter; width: 6; height: 6; radius: 3; color: root.custom ? DesktopTokens.focus : DesktopTokens.textFaint }
                    Text {
                        id: badgeText
                        x: 20; anchors.verticalCenter: parent.verticalCenter
                        text: root.custom ? qsTr("CUSTOM FOR THIS PAD") : qsTr("USING DEFAULTS")
                        color: root.custom ? DesktopTokens.focus : DesktopTokens.textMuted
                        font.family: DesktopTokens.monoFont; font.pixelSize: 9; font.weight: Font.Bold; font.letterSpacing: 1
                    }
                }
                Text {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Saved · applies instantly")
                    color: DesktopTokens.textFaint
                    font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold
                }
            }

            Notice {
                objectName: "controllerSharedProfileNotice"
                visible: root.sharedWith.length > 0
                title: ""
                body: root.selectedPad && root.sharedWith.length > 0
                    ? qsTr("%1 and %2 can't be told apart (same model, no serial number), so they share one profile. A change here also changes %2.")
                        .arg(root.padLabel(root.selectedPad))
                        .arg(root.sharedWith.map(pad => root.padLabel(pad)).join(", "))
                    : ""
            }

            Row {
                objectName: "controllerStickRings"
                visible: root.selectedPad !== null
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 16
                StickRing {
                    objectName: "controllerLeftRing"
                    label: qsTr("LEFT STICK")
                    deadzone: root.selectedPad ? Number(root.selectedPad.leftStickDeadzone) : 5
                    live: root.sample.leftX !== undefined
                    rawX: Number(root.sample.leftX || 0); rawY: Number(root.sample.leftY || 0)
                    outX: Number(root.sample.leftOutX || 0); outY: Number(root.sample.leftOutY || 0)
                }
                StickRing {
                    objectName: "controllerRightRing"
                    label: qsTr("RIGHT STICK")
                    deadzone: root.selectedPad ? Number(root.selectedPad.rightStickDeadzone) : 5
                    live: root.sample.rightX !== undefined
                    rawX: Number(root.sample.rightX || 0); rawY: Number(root.sample.rightY || 0)
                    outX: Number(root.sample.rightOutX || 0); outY: Number(root.sample.rightOutY || 0)
                }
            }
            Row {
                visible: root.selectedPad !== null
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 18
                Repeater {
                    model: [
                        { swatch: "#FFFFFF", label: qsTr("Stick") },
                        { swatch: DesktopTokens.focus, label: qsTr("Sent to game") },
                        { swatch: DesktopTokens.amber, label: qsTr("Dead zone") }
                    ]
                    delegate: Row {
                        id: legend
                        required property var modelData
                        spacing: 6
                        Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 8; height: 8; radius: 4; color: legend.modelData.swatch }
                        Text {
                            text: legend.modelData.label
                            color: DesktopTokens.textMuted
                            font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold
                        }
                    }
                }
            }
            Text {
                visible: root.selectedPad !== null
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: qsTr("Move the sticks to test. On this page the left stick never moves the menu; use the D-pad, Tab or the mouse.")
                color: DesktopTokens.textFaint
                font.family: DesktopTokens.bodyFont; font.pixelSize: 10; font.weight: Font.DemiBold
                wrapMode: Text.WordWrap
            }

            TuneSlider {
                id: leftTune
                objectName: "controllerLeftDeadzone"
                visible: root.selectedPad !== null
                title: qsTr("Left stick dead zone")
                note: root.selectedProfile.leftStickDeadzone !== undefined ? qsTr("DEFAULT %1%").arg(root.globalLeft) : qsTr("USING DEFAULT")
                value: root.selectedPad ? Number(root.selectedPad.leftStickDeadzone) : root.globalLeft
                onCommitted: value => root.writePad(root.selectedPad, {leftStickDeadzone: value})
            }
            TuneSlider {
                id: rightTune
                objectName: "controllerRightDeadzone"
                visible: root.selectedPad !== null
                title: qsTr("Right stick dead zone")
                note: root.selectedProfile.rightStickDeadzone !== undefined ? qsTr("DEFAULT %1%").arg(root.globalRight) : qsTr("USING DEFAULT")
                value: root.selectedPad ? Number(root.selectedPad.rightStickDeadzone) : root.globalRight
                onCommitted: value => root.writePad(root.selectedPad, {rightStickDeadzone: value})
            }
            Item {
                visible: root.selectedPad !== null
                width: parent.width
                height: vibrationTune.height
                TuneSlider {
                    id: vibrationTune
                    objectName: "controllerVibration"
                    title: qsTr("Vibration")
                    maximum: 100
                    trailingWidth: testButton.width + 10
                    note: root.selectedProfile.vibrationIntensity !== undefined ? qsTr("DEFAULT %1%").arg(root.globalVibration) : qsTr("USING DEFAULT")
                    value: root.selectedPad ? Number(root.selectedPad.vibrationIntensity) : root.globalVibration
                    onCommitted: value => root.writePad(root.selectedPad, {vibrationIntensity: value})
                }
                DesktopStreamDrawerButton {
                    id: testButton
                    objectName: "controllerTestVibration"
                    anchors.right: parent.right
                    y: 22
                    width: 72; height: 34; radius: 10
                    enabled: root.selectedPad !== null && Number(root.selectedPad.vibrationIntensity) > 0
                    text: qsTr("Test")
                    description: qsTr("Rumbles this pad briefly at the chosen strength")
                    onClicked: ControllerInput.testRumble(Number(root.selectedPad.instanceId))
                    Text {
                        anchors.centerIn: parent
                        text: testButton.text
                        color: DesktopTokens.textHigh
                        font.family: DesktopTokens.bodyFont; font.pixelSize: 12; font.weight: Font.Bold
                    }
                }
            }

            DesktopStreamDrawerButton {
                id: playerOneRow
                objectName: "controllerPlayerOne"
                visible: root.selectedPad !== null
                readonly property bool playerOne: root.playerOneMissing || (root.selectedPad !== null
                    && Number(ControllerInput.inputControllerId) === Number(root.selectedPad.instanceId))
                width: parent.width
                height: 58
                text: root.playerOneMissing ? qsTr("Player 1 pad disconnected") : qsTr("Only this pad is Player 1")
                description: root.playerOneMissing ? qsTr("Other pads are ignored. Turn this off to use them.")
                    : qsTr("Ignores the other pads until the app restarts")
                Accessible.checkable: true
                Accessible.checked: playerOne
                onClicked: root.setPlayerOne(!playerOne)
                Column {
                    x: 14; anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 90
                    spacing: 2
                    Text {
                        text: playerOneRow.text
                        color: DesktopTokens.textHigh
                        font.family: DesktopTokens.bodyFont; font.pixelSize: 13; font.weight: Font.Bold
                    }
                    Text {
                        width: parent.width
                        text: playerOneRow.description
                        color: DesktopTokens.textMuted
                        font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold
                        elide: Text.ElideRight
                    }
                }
                DesktopSettingsToggle {
                    anchors.right: parent.right; anchors.rightMargin: 14
                    anchors.verticalCenter: parent.verticalCenter
                    focusPolicy: Qt.NoFocus
                    checked: playerOneRow.playerOne
                    onValueChangedByUser: value => root.setPlayerOne(value)
                }
            }

            Row {
                visible: root.selectedPad !== null
                width: parent.width
                spacing: 10
                DesktopStreamDrawerButton {
                    id: resetButton
                    objectName: "controllerResetPad"
                    width: (parent.width - parent.spacing) / 2
                    height: 44
                    enabled: root.custom
                    text: qsTr("Reset to defaults")
                    description: qsTr("Removes this pad's own values so it follows the defaults again")
                    onClicked: root.resetPad()
                    Row {
                        anchors.centerIn: parent
                        spacing: 8
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: resetButton.text
                            color: DesktopTokens.textHigh
                            font.family: DesktopTokens.bodyFont; font.pixelSize: 12; font.weight: Font.Bold
                        }
                        ControllerGlyph {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: root.controllerHints
                            glyph: "Y"; label: ""; glyphSize: 18; spacing: 0; glyphColor: DesktopTokens.amber
                        }
                    }
                }
                DesktopStreamDrawerButton {
                    id: applyButton
                    objectName: "controllerApplyAll"
                    width: (parent.width - parent.spacing) / 2
                    height: 44
                    enabled: root.pads.length > 1
                    text: qsTr("Apply to all pads")
                    description: qsTr("Copies these values to every connected pad")
                    onClicked: root.applyToAll()
                    Row {
                        anchors.centerIn: parent
                        spacing: 8
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: applyButton.text
                            color: DesktopTokens.textHigh
                            font.family: DesktopTokens.bodyFont; font.pixelSize: 12; font.weight: Font.Bold
                        }
                        ControllerGlyph {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: root.controllerHints
                            glyph: "X"; label: ""; glyphSize: 18; spacing: 0; glyphColor: DesktopTokens.focus
                        }
                    }
                }
            }
            Text {
                visible: root.pads.length >= 4
                width: parent.width
                text: qsTr("OpenNOW reads up to four pads. A fifth pad is ignored until one disconnects.")
                color: DesktopTokens.textFaint
                font.family: DesktopTokens.bodyFont; font.pixelSize: 11; font.weight: Font.DemiBold
                wrapMode: Text.WordWrap
            }
        }
    }

    Row {
        id: footer
        x: 22
        y: parent.height - height - 18
        spacing: 14
        DesktopKeyHint { visible: !root.controllerHints; keyText: "Tab"; label: qsTr("Next"); compact: true }
        DesktopKeyHint { visible: !root.controllerHints; keyText: "← →"; shortcut: "Left+Right"; label: qsTr("Adjust"); compact: true }
        DesktopKeyHint { visible: !root.controllerHints && root.pads.length > 1; keyText: "PgUp PgDn"; shortcut: "PgUp+PgDn"; label: qsTr("Pad"); compact: true }
        DesktopKeyHint { visible: !root.controllerHints; keyText: "Esc"; label: qsTr("Back"); compact: true }
        ControllerGlyph { visible: root.controllerHints && root.pads.length > 1; glyph: "LB"; label: qsTr("Pad"); glyphSize: 18; labelPixelSize: 11; labelWeight: Font.DemiBold }
        ControllerGlyph { visible: root.controllerHints; glyph: "A"; label: qsTr("Select"); glyphSize: 18; labelPixelSize: 11; labelWeight: Font.DemiBold; glyphColor: DesktopTokens.mint }
        ControllerGlyph { visible: root.controllerHints; glyph: "B"; label: qsTr("Back"); glyphSize: 18; labelPixelSize: 11; labelWeight: Font.DemiBold; glyphColor: DesktopTokens.danger }
    }

    Keys.onPressed: event => {
        const fromPad = AppController.inputMode === "controller"
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.backRequested()
        } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Down) {
            root.stepFocus(1)
        } else if (event.key === Qt.Key_Backtab || event.key === Qt.Key_Up) {
            root.stepFocus(-1)
        } else if (event.key === Qt.Key_PageUp) {
            root.selectPad(-1)
        } else if (event.key === Qt.Key_PageDown) {
            root.selectPad(1)
        } else if (fromPad && event.key === Qt.Key_Y) {
            root.resetPad()
        } else if (fromPad && event.key === Qt.Key_X) {
            root.applyToAll()
        } else {
            return
        }
        event.accepted = true
    }
}
