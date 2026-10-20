pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import OpenNOW

// Host co-op block of the Ctrl+G menu (artboards HAF-1 and 5Q0-0): code, players, one-click invites and End co-op.
// Before co-op it offers Create co-op session for the running stream, with the setup's player count inline.
FocusScope {
    id: root
    objectName: "streamCoopSection"
    readonly property var coop: ShellStore.coop
    readonly property var friends: ShellStore.friends
    readonly property var cards: coop.isHost ? coop.slotCards() : []
    // Friends and co-op appear only for a linked account with the stream-menu setting on; a running
    // co-op session keeps its controls either way.
    readonly property bool socialShown: friends.linked && coop.preferences.showInStreamMenu !== false
    readonly property bool canCreate: socialShown && coop.role === "none" && coop.coopSupported && coop.hostStreaming
    readonly property var candidates: (coop.isHost || canCreate) && socialShown ? coop.inviteCandidates() : []
    property int createGuests: Math.max(1, Math.min(coop.maxGuestsLimit, Number(coop.preferences.maxGuests || coop.maxGuestsLimit)))
    property bool focusLobbyWhenReady: false
    readonly property bool hosting: coop.isHost
    onHostingChanged: {
        if (hosting && focusLobbyWhenReady) {
            focusLobbyWhenReady = false
            Qt.callLater(focusFirst)
        }
    }
    readonly property bool inviteVisible: candidates.length > 0
        && cards.some(card => card.kind === "open" || coop.uploadHolds(card))
    readonly property var slotColors: ["#7FD4FF", "#6EE7B7", "#FFD166", "#A78BFA"]
    readonly property real hostPing: coop.hostStats ? Number(coop.hostStats.hostToGfnPingMs || 0) : 0
    readonly property real capacityMbps: coop.uploadCapacityKbps / 1000
    readonly property real streamMbps: coop.hostStats ? Number(coop.hostStats.streamKbps || 0) / 1000 : 0
    property Item hint: null
    signal revealRequested(Item item)
    visible: coop.isHost || canCreate
    height: coop.isHost ? column.implicitHeight : canCreate ? createColumn.implicitHeight : 0
    Accessible.role: Accessible.Grouping
    Accessible.name: qsTr("Co-op")

    function mbps(value) {
        return String(Math.round(Number(value) * 10) / 10)
    }
    function inside(item) {
        for (let current = item; current; current = current.parent) {
            if (current === root)
                return true
        }
        return false
    }
    function step(forward) {
        const current = Window.window ? Window.window.activeFocusItem : null
        if (!current || !inside(current))
            return false
        const next = current.nextItemInFocusChain(forward)
        if (!next || !inside(next) || next === current)
            return false
        next.forceActiveFocus()
        revealRequested(next)
        return true
    }
    function focusFirst() {
        const first = coop.isHost ? copyButton : createButton
        first.forceActiveFocus()
        revealRequested(first)
    }
    function focusLast() {
        const last = coop.isHost ? endRow : createInviteRepeater.count > 0 ? createInviteRepeater.itemAt(createInviteRepeater.count - 1) : createButton
        last.forceActiveFocus()
        revealRequested(last)
    }
    function create(userId) {
        focusLobbyWhenReady = true
        coop.hostRunningStream(createGuests, userId || "")
    }
    onActiveFocusChanged: if (activeFocus && !inside(Window.window ? Window.window.activeFocusItem : null)) focusFirst()
    onFocusChanged: if (focus && activeFocus) focusFirst()
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Up || event.key === Qt.Key_Down)
            event.accepted = step(event.key === Qt.Key_Down)
    }

    component SmallAction: AbstractButton {
        id: action
        property string glyph: ""
        property color tone: "#FFFFFF"
        width: 26
        height: 26
        hoverEnabled: true
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.Button
        Accessible.name: text
        Keys.onReturnPressed: event => { action.clicked(); event.accepted = true }
        Keys.onEnterPressed: event => { action.clicked(); event.accepted = true }
        background: Rectangle {
            radius: 7
            color: action.hovered ? "#33FFFFFF" : "#1FFFFFFF"
            border.width: action.activeFocus && AppController.inputMode !== "pointer" ? 2 : 0
            border.color: DesktopTokens.focus
        }
        contentItem: Item {
            DesktopCoopHostIcon {
                anchors.centerIn: parent
                width: action.glyph === "more" ? 14 : 12
                height: width
                glyph: action.glyph
                ink: action.tone
            }
        }
    }

    component TextAction: AbstractButton {
        id: link
        hoverEnabled: true
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.Button
        Accessible.name: text
        Keys.onReturnPressed: event => { link.clicked(); event.accepted = true }
        Keys.onEnterPressed: event => { link.clicked(); event.accepted = true }
        background: Rectangle {
            anchors.fill: parent
            anchors.margins: -4
            radius: 6
            color: "transparent"
            border.width: link.activeFocus && AppController.inputMode !== "pointer" ? 2 : 0
            border.color: DesktopTokens.focus
        }
        contentItem: Text {
            text: link.text
            color: link.hovered ? "#FFFFFF" : "#A3FFFFFF"
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 12
            font.weight: Font.ExtraBold
        }
    }

    DesktopCoopHostFriendChip { id: paletteChip; visible: false; enabled: false }

    Column {
        id: createColumn
        objectName: "streamCoopCreateSection"
        visible: !root.coop.isHost && root.canCreate
        width: parent.width
        spacing: 0

        Item { width: 1; height: 4 }
        Text {
            text: qsTr("CO-OP · PLAY THIS GAME TOGETHER")
            color: "#7FD4FF"
            font.family: DesktopTokens.monoFont
            font.pixelSize: 10
            font.weight: Font.DemiBold
            font.letterSpacing: 1.4
        }
        Item { width: 1; height: 8 }
        Text {
            width: parent.width
            wrapMode: Text.WordWrap
            text: qsTr("Friends join this stream with a code or an invite. The game keeps running.")
            color: "#A3FFFFFF"
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 12
            font.weight: Font.Medium
        }
        Item { width: 1; height: 12 }
        Item {
            width: parent.width
            height: 28
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Players")
                color: "#FFFFFF"
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 13
                font.weight: Font.ExtraBold
            }
            Row {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                Repeater {
                    model: root.coop.maxGuestsLimit
                    delegate: DesktopCoopHostButton {
                        id: countButton
                        required property int index
                        readonly property bool chosen: root.createGuests === index + 1
                        objectName: "streamCoopPlayers-" + (index + 2)
                        height: 28
                        fontSize: 12
                        sidePadding: 12
                        text: String(index + 2)
                        fill: chosen ? "#FFFFFF" : "#1FFFFFFF"
                        ink: chosen ? "#0A0D14" : "#FFFFFF"
                        Accessible.name: qsTr("%1 players").arg(index + 2)
                        Accessible.checked: chosen
                        onClicked: root.createGuests = index + 1
                    }
                }
            }
        }
        Item { width: 1; height: 6 }
        Text {
            width: parent.width
            wrapMode: Text.WordWrap
            visible: root.coop.currentBitrateMbps() > 0
            text: qsTr("Each guest gets your %1 Mbps stream, so %2 guests need %3 Mbps of upload.")
                .arg(root.mbps(root.coop.currentBitrateMbps())).arg(root.createGuests)
                .arg(root.mbps(root.coop.currentBitrateMbps() * root.createGuests))
            color: "#66FFFFFF"
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 11
            font.weight: Font.Medium
        }
        Item { width: 1; height: 12 }
        DesktopCoopHostButton {
            id: createButton
            objectName: "streamCoopCreate"
            width: parent.width
            enabled: !root.coop.creatingLobby
            text: root.coop.creatingLobby ? qsTr("Creating co-op session…") : qsTr("Create co-op session")
            glyph: "play"; glyphSize: 12; keyText: "Enter"; fontSize: 15
            fill: "#FFFFFF"; hoverFill: "#14000000"; edge: "#7FD4FF"; edgeWidth: 2; ink: "#0A0D14"
            onClicked: root.create("")
        }
        Item { visible: createInviteGrid.visible; width: 1; height: 14 }
        Row {
            visible: createInviteGrid.visible
            spacing: 8
            Text {
                text: qsTr("INVITE FRIENDS")
                color: "#7FD4FF"
                font.family: DesktopTokens.monoFont
                font.pixelSize: 10
                font.weight: Font.DemiBold
                font.letterSpacing: 1.4
            }
            Text {
                text: qsTr("ONE CLICK · STARTS CO-OP")
                color: "#66FFFFFF"
                font.family: DesktopTokens.monoFont
                font.pixelSize: 10
                font.weight: Font.DemiBold
                font.letterSpacing: 1
            }
        }
        Item { visible: createInviteGrid.visible; width: 1; height: 8 }
        Grid {
            id: createInviteGrid
            visible: root.candidates.length > 0
            width: parent.width
            columns: 2
            columnSpacing: 8
            rowSpacing: 8
            Repeater {
                id: createInviteRepeater
                model: root.canCreate ? root.candidates.slice(0, 4) : []
                delegate: DesktopCoopHostFriendChip {
                    required property var modelData
                    objectName: "streamCoopCreateInvite-" + modelData.user.id
                    width: (createInviteGrid.width - createInviteGrid.columnSpacing) / 2
                    height: 44
                    user: modelData.user
                    presence: modelData.presence
                    inviteState: root.coop.queuedInvites.indexOf(modelData.user.id) >= 0 ? "pending" : "idle"
                    onClicked: root.create(modelData.user.id)
                }
            }
        }
        Item { width: 1; height: 14 }
    }

    Column {
        id: column
        visible: root.coop.isHost
        width: parent.width
        spacing: 0

        Item { width: 1; height: 4 }
        Item {
            width: parent.width
            height: 28
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("CO-OP · %1 · %2 OF %3").arg(root.coop.lobby ? String(root.coop.lobby.code || "") : "")
                    .arg(root.coop.playerCount).arg(root.coop.maxPlayers)
                color: "#7FD4FF"
                font.family: DesktopTokens.monoFont
                font.pixelSize: 10
                font.weight: Font.DemiBold
                font.letterSpacing: 1.4
            }
            DesktopCoopHostButton {
                id: copyButton
                anchors.right: parent.right
                height: 28
                fontSize: 12
                glyph: "link"
                fill: "#1FFFFFFF"
                text: qsTr("Copy link")
                onClicked: root.coop.copyLink()
            }
        }
        Item { width: 1; height: 10 }
        Column {
            width: parent.width
            spacing: 6
            Repeater {
                model: root.cards
                delegate: Rectangle {
                    id: row
                    required property var modelData
                    readonly property int slotNumber: modelData.slot
                    readonly property string kind: modelData.kind
                    readonly property var member: modelData.member
                    readonly property var invite: modelData.invite
                    readonly property bool held: root.coop.uploadHolds(modelData)
                    readonly property bool closedLook: held || kind === "closed"
                    readonly property color tone: kind === "invited" ? "#7FD4FF" : closedLook ? "#F5A623" : root.slotColors[slotNumber]
                    readonly property var peer: member ? root.coop.peer(member.userId) : null
                    readonly property string path: peer && peer.path ? String(peer.path) : ""
                    readonly property bool relayed: path === "relay"
                    readonly property var link: member ? root.coop.peerLink(member.userId) : null
                    readonly property string personName: member ? String(member.displayName || "")
                        : invite && invite.to ? String(invite.to.displayName || invite.to.username || "") : ""
                    readonly property bool actionsShown: kind === "guest" && (rowHover.hovered || actions.activeFocus || expanded)
                    property bool expanded: false
                    property bool kickArmed: false
                    width: parent.width
                    height: 44
                    radius: 10
                    color: "#0AFFFFFF"
                    readonly property bool dashed: kind === "invited" || kind === "open" || closedLook
                    border.width: dashed ? 0 : 1
                    border.color: "#0FFFFFFF"
                    Accessible.role: Accessible.Row
                    Accessible.name: kind === "host" ? qsTr("P%1, you").arg(slotNumber + 1)
                        : kind === "guest" ? qsTr("P%1, %2").arg(slotNumber + 1).arg(personName)
                        : kind === "invited" ? qsTr("P%1, invited %2").arg(slotNumber + 1).arg(personName)
                        : closedLook ? qsTr("P%1, closed").arg(slotNumber + 1) : qsTr("P%1, open slot").arg(slotNumber + 1)
                    HoverHandler { id: rowHover }
                    DesktopDashedOutline {
                        visible: row.dashed
                        anchors.fill: parent
                        radius: row.radius
                        stroke: Qt.rgba(row.tone.r, row.tone.g, row.tone.b, 0.6)
                    }
                    Timer { id: kickTimer; interval: 4000; onTriggered: row.kickArmed = false }
                    onActionsShownChanged: {
                        if (actionsShown)
                            root.hint = actions
                        else if (root.hint === actions)
                            root.hint = null
                    }
                    Component.onDestruction: if (root.hint === actions) root.hint = null

                    Row {
                        x: 10
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 10
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.max(30, badgeText.implicitWidth + 14)
                            height: 22
                            radius: 7
                            readonly property bool filled: row.kind === "host" || row.kind === "guest"
                            color: filled ? row.tone : "transparent"
                            border.width: 1
                            border.color: row.tone
                            Text {
                                id: badgeText
                                anchors.centerIn: parent
                                text: qsTr("P%1").arg(row.slotNumber + 1)
                                color: parent.filled ? "#0B0F1A" : row.tone
                                font.family: DesktopTokens.monoFont
                                font.pixelSize: 11
                                font.weight: Font.Bold
                            }
                        }
                        Rectangle {
                            visible: row.kind === "invited"
                            anchors.verticalCenter: parent.verticalCenter
                            width: 22
                            height: 22
                            radius: 11
                            color: row.invite ? paletteChip.colorFor(row.personName,
                                root.friends.isOnline(root.friends.friendById(row.invite.to.id))) : "transparent"
                            Text {
                                anchors.centerIn: parent
                                text: row.personName.slice(0, 1).toUpperCase()
                                color: "#0B0F1A"
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: 9
                                font.weight: Font.Black
                            }
                            DesktopAvatarPhoto {
                                anchors.fill: parent
                                url: row.invite && row.invite.to ? String(row.invite.to.avatarUrl || "") : ""
                            }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: row.kind === "host" ? qsTr("You") : row.closedLook ? qsTr("Closed")
                                : row.kind === "open" ? qsTr("Open slot") : row.personName
                            color: row.closedLook || row.kind === "open" ? "#A3FFFFFF" : "#FFFFFF"
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 13
                            font.weight: Font.ExtraBold
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: text !== ""
                            objectName: row.kind === "guest" ? "coopPeerLink-" + row.member.userId : ""
                            text: row.kind === "guest" ? row.link.label
                                : row.kind === "invited" ? qsTr("INVITED · WAITING")
                                : row.held ? qsTr("NEEDS %1 / %2 MBPS").arg(root.mbps(root.streamMbps * row.slotNumber)).arg(root.mbps(root.capacityMbps))
                                : ""
                            color: row.kind === "guest" ? ({failed: DesktopTokens.danger, reconnecting: "#FFD166", relay: "#FFD166",
                                    direct: "#1DB954"}[row.link.kind] || "#A3FFFFFF")
                                : row.kind === "invited" ? "#7FD4FF" : "#F5A623"
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 9
                            font.weight: Font.DemiBold
                            font.letterSpacing: 0.72
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: row.kind === "guest" && root.coop.decoderTier(row.member.userId, root.coop.streamCodec) === "software"
                            objectName: row.kind === "guest" ? "coopPeerSoftware-" + row.member.userId : ""
                            text: qsTr("SOFTWARE DECODE")
                            color: "#A3FFFFFF"
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 9
                            font.weight: Font.DemiBold
                            font.letterSpacing: 0.72
                        }
                    }
                    Row {
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 10
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: row.kind === "host" || row.kind === "guest"
                            objectName: row.kind === "guest" ? "coopPeerPing-" + row.member.userId : ""
                            text: row.kind === "host" ? (root.hostPing > 0 ? qsTr("%1 ms").arg(root.coop.msText(root.hostPing)) : "—")
                                : row.peer ? qsTr("%1 ms").arg(root.coop.msText(root.hostPing + Number(row.peer.peerToHostPingMs || 0))) : "—"
                            color: row.relayed ? "#FFD166" : "#FFFFFF"
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 12
                            font.weight: Font.DemiBold
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: row.kind === "open" && !row.held
                            text: qsTr("WAITING")
                            color: "#52FFFFFF"
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 9
                            font.weight: Font.DemiBold
                            font.letterSpacing: 0.72
                        }
                        Row {
                            id: actions
                            visible: row.actionsShown
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 4
                            SmallAction {
                                glyph: "ban"
                                tone: row.peer && row.peer.inputMuted ? "#FFD166" : "#FFFFFF"
                                text: row.peer && row.peer.inputMuted ? qsTr("Unmute input") : qsTr("Mute input")
                                onClicked: root.coop.setInputMuted(row.member.userId, !(row.peer && row.peer.inputMuted))
                            }
                            SmallAction {
                                glyph: "swap"
                                enabled: root.coop.nextFreeSlot() > 0
                                opacity: enabled ? 1 : 0.4
                                text: qsTr("Move slot")
                                onClicked: root.coop.moveSlot(row.member.userId, root.coop.nextFreeSlot())
                            }
                            SmallAction {
                                glyph: "user-x"
                                tone: row.kickArmed ? "#FF8A80" : "#FFFFFF"
                                text: row.kickArmed ? qsTr("Click again to kick %1").arg(row.personName) : qsTr("Kick")
                                onClicked: {
                                    if (!row.kickArmed) {
                                        row.kickArmed = true
                                        kickTimer.restart()
                                        return
                                    }
                                    row.kickArmed = false
                                    root.coop.kick(row.member.userId)
                                }
                            }
                        }
                        Item {
                            visible: (row.kind === "host" || row.kind === "guest") && !row.actionsShown
                            width: 26
                            height: 26
                            anchors.verticalCenter: parent.verticalCenter
                            AbstractButton {
                                id: moreButton
                                anchors.fill: parent
                                hoverEnabled: true
                                focusPolicy: Qt.StrongFocus
                                text: row.kind === "host" ? qsTr("Open the co-op lobby") : qsTr("Player actions")
                                Accessible.role: Accessible.Button
                                Accessible.name: text
                                Keys.onReturnPressed: event => { moreButton.clicked(); event.accepted = true }
                                onClicked: {
                                    if (row.kind === "host") {
                                        AppController.showOverlay("coop-lobby")
                                        return
                                    }
                                    row.expanded = true
                                    actions.children[0].forceActiveFocus()
                                }
                                background: Rectangle {
                                    radius: 7
                                    color: moreButton.hovered ? "#1FFFFFFF" : "transparent"
                                    border.width: moreButton.activeFocus && AppController.inputMode !== "pointer" ? 2 : 0
                                    border.color: DesktopTokens.focus
                                }
                                contentItem: Item {
                                    DesktopCoopHostIcon { anchors.centerIn: parent; width: 14; height: 14; glyph: "more"; ink: "#A3FFFFFF" }
                                }
                            }
                        }
                        TextAction {
                            visible: row.kind === "invited" || row.closedLook
                            anchors.verticalCenter: parent.verticalCenter
                            text: row.kind === "invited" ? qsTr("Cancel") : qsTr("Open")
                            onClicked: {
                                if (row.kind === "invited")
                                    root.coop.cancelInvite(row.invite.id)
                                else if (row.held)
                                    root.coop.heldSlotOpened = true
                                else
                                    root.coop.setMaxGuests(row.slotNumber)
                            }
                        }
                    }
                    Connections {
                        target: actions
                        function onActiveFocusChanged() { if (!actions.activeFocus && !rowHover.hovered) row.expanded = false }
                    }
                }
            }
        }

        Item { visible: startButton.visible; width: 1; height: 10 }
        DesktopCoopHostButton {
            id: startButton
            objectName: "streamCoopStart"
            visible: root.coop.lobby !== null && root.coop.lobby.state !== "streaming"
            enabled: root.coop.guestConnected
            width: parent.width
            text: qsTr("Start playing together"); glyph: "play"; glyphSize: 12; keyText: "Enter"; fontSize: 15
            fill: "#FFFFFF"; hoverFill: "#14000000"; edge: "#7FD4FF"; edgeWidth: 2; ink: "#0A0D14"
            Accessible.description: enabled ? "" : startCaption.text
            onClicked: root.coop.startPlayingTogether()
        }
        Text {
            id: startCaption
            objectName: "streamCoopStartCaption"
            visible: startButton.visible && !startButton.enabled
            width: parent.width
            topPadding: 6
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("Waits for a guest to connect")
            color: "#A3FFFFFF"
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 12
            font.weight: Font.Medium
        }
        Item { visible: sectionUsage.visible; width: 1; height: 10 }
        DesktopCoopUploadUsage { id: sectionUsage }
        Item { visible: root.coop.uploadShort; width: 1; height: 10 }
        Row {
            visible: root.coop.uploadShort
            spacing: 10
            DesktopCoopHostIcon { anchors.verticalCenter: parent.verticalCenter; width: 14; height: 14; glyph: "alert"; ink: "#F5A623" }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("P%1 would need %2 / %3 Mbps · slot held closed").arg(root.coop.guests.length + 2)
                    .arg(root.mbps(root.coop.uploadNeededKbps / 1000)).arg(root.mbps(root.capacityMbps))
                color: "#FFD166"
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 12
                font.weight: Font.Bold
            }
        }

        Item { visible: root.inviteVisible; width: 1; height: 12 }
        Item {
            visible: root.inviteVisible
            width: parent.width
            height: 16
            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8
                Text {
                    text: qsTr("INVITE FRIENDS")
                    color: "#7FD4FF"
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: 10
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1.4
                }
                Text {
                    text: qsTr("ONE CLICK · GAME KEEPS RUNNING")
                    color: "#66FFFFFF"
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: 10
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1
                }
            }
            TextAction {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("All %1").arg(root.coop.invitableOnlineCount)
                contentItem: Text {
                    text: parent.text
                    color: "#7FD4FF"
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 12
                    font.weight: Font.ExtraBold
                }
                onClicked: AppController.showOverlay("coop-lobby")
            }
        }
        Item { visible: root.inviteVisible; width: 1; height: 8 }
        Grid {
            id: inviteGrid
            visible: root.inviteVisible
            width: parent.width
            columns: 2
            columnSpacing: 8
            rowSpacing: 8
            Repeater {
                model: root.candidates.slice(0, 4)
                delegate: DesktopCoopHostFriendChip {
                    required property var modelData
                    width: (inviteGrid.width - inviteGrid.columnSpacing) / 2
                    height: 44
                    user: modelData.user
                    presence: modelData.presence
                    onClicked: root.coop.invite(modelData.user.id)
                }
            }
        }

        Item { width: 1; height: 14 }
        DesktopStreamDrawerButton {
            id: endRow
            objectName: "streamCoopEndRow"
            width: parent.width
            height: 58
            radius: 12
            text: qsTr("End co-op")
            description: qsTr("Guests leave, your stream keeps running")
            fill: "#12FF8A80"
            hoverFill: "#29FF8A80"
            edge: "#52FF8A80"
            onClicked: root.coop.requestEndCoop()
            Rectangle {
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                width: 30
                height: 30
                radius: 8
                color: "#1FFF8A80"
                DesktopCoopHostIcon { anchors.centerIn: parent; width: 16; height: 16; glyph: "logout"; ink: "#FF8A80" }
            }
            Column {
                x: 54
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    text: endRow.text
                    color: "#FFB4AE"
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 14
                    font.weight: Font.Bold
                }
                Text {
                    text: endRow.description
                    color: "#A3FFFFFF"
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 12
                }
            }
        }
    }
}
