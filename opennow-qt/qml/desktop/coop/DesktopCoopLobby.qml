pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import OpenNOW

// Host co-op lobby over the stream or the shell (overlay coop-lobby). Without friends to invite it is the
// code-first lobby (artboard 5C7-0); with Friends linked it leads with one-click invites (C2 E6X-1, EGQ-1).
FocusScope {
    id: root
    objectName: "coopLobbyHost"
    property bool opened: false
    property bool showAllFriends: false
    property bool lookupOpen: false
    property real nowMs: Date.now()
    readonly property var coop: ShellStore.coop
    readonly property var friends: ShellStore.friends
    readonly property var lobby: coop.isHost ? coop.lobby : null
    readonly property string gameTitle: coop.game && coop.game.title ? String(coop.game.title) : qsTr("Your game")
    readonly property var cards: lobby ? coop.slotCards() : []
    readonly property var stripInvites: coop.outgoingInvites.filter(invite => invite.to
        && ["pending", "declined", "expired"].indexOf(invite.state) >= 0
        && !coop.members.some(member => member.userId === invite.to.id)).slice().reverse()
    readonly property var candidates: lobby && friends.linked ? coop.invitableFriends : []
    readonly property bool showStrip: friends.linked && (stripInvites.length > 0 || candidates.length > 0)
    readonly property var stripEntries: stripInvites.map(invite => ({invite: invite, user: invite.to,
            friend: friends.friendById(invite.to.id), inviteState: coop.inviteChipState(invite)}))
        .concat(candidates.map(friend => ({invite: null, user: friend.user, friend: friend, inviteState: "idle"})))
        .slice(0, showAllFriends ? 15 : 5)
    readonly property bool notesVisible: stripEntries.some(entry => ["declined", "expired", "dm", "waits"].indexOf(entry.inviteState) >= 0)
    readonly property bool lookupVisible: showStrip && (lookupOpen || friends.lookup !== null)
    readonly property var heldCard: cards.find(card => coop.uploadHolds(card)) || null
    readonly property bool uploadBarVisible: coop.uploadShort
    readonly property real capacityMbps: coop.uploadCapacityKbps / 1000
    readonly property real streamMbps: coop.hostStats ? Number(coop.hostStats.streamKbps || 0) / 1000 : 0
    readonly property var slotColors: ["#7FD4FF", "#6EE7B7", "#FFD166", "#A78BFA"]
    readonly property var pads: ControllerInput.availableControllers || []
    readonly property bool present: reveal.present
    visible: present
    enabled: opened
    focus: opened
    MotionProgress { id: reveal; shown: root.opened && root.lobby !== null }
    Accessible.role: Accessible.Dialog
    Accessible.name: qsTr("Co-op lobby")

    function clock(totalSeconds) {
        const seconds = Math.max(0, Math.floor(totalSeconds))
        return Math.floor(seconds / 60) + ":" + String(seconds % 60).padStart(2, "0")
    }
    function since(value) {
        const at = Date.parse(String(value || ""))
        return isNaN(at) ? 0 : (nowMs - at) / 1000
    }
    function until(value) {
        const at = Date.parse(String(value || ""))
        return isNaN(at) ? 0 : (at - nowMs) / 1000
    }
    function mbps(value) {
        return String(Math.round(Number(value) * 10) / 10)
    }
    function displayUrl() {
        return coop.joinUrl().replace(/^https?:\/\//, "")
    }
    function lookupName() {
        const raw = friends.lookup ? String(friends.lookup.username || "") : ""
        const letters = raw.replace(/[^A-Za-z]+$/, "")
        return letters.length > 0 ? letters.charAt(0).toUpperCase() + letters.slice(1) : raw
    }
    function activateChip(entry) {
        if (entry.inviteState === "idle")
            coop.invite(entry.user.id)
        else if (entry.inviteState === "pending" || entry.inviteState === "dm" || entry.inviteState === "waits")
            coop.cancelInvite(entry.invite.id)
        else
            coop.resendInvite(entry.user.id)
    }
    function chipNote(entry) {
        const name = String(entry.user.displayName || entry.user.username || "")
        if (entry.inviteState === "pending")
            return qsTr("Expires in %1 if they don't answer").arg(clock(until(entry.invite.expiresAt)))
        if (entry.inviteState === "declined")
            return qsTr("%1 said not now. Invite again in 5 min").arg(name)
        if (entry.inviteState === "expired")
            return qsTr("No answer in 10 min. One click resends")
        if (entry.inviteState === "dm")
            return qsTr("%1 is offline, so the bot DMed them").arg(name)
        if (entry.inviteState === "waits")
            return qsTr("%1 doesn't take DMs from the bot. The invite waits until they're online").arg(name)
        if (entry.friend && !friends.isOnline(entry.friend))
            return qsTr("Offline. The invite arrives as a Discord DM")
        return qsTr("Ready to invite")
    }
    function close() {
        AppController.showOverlay("")
    }
    function openLookup() {
        lookupOpen = true
        Qt.callLater(() => lookupInput.forceActiveFocus())
    }
    function closeLookup() {
        lookupOpen = false
        friends.lookupUser("")
        startButton.visible ? startButton.forceActiveFocus() : backButton.forceActiveFocus()
    }

    DesktopCoopHostFriendChip { id: paletteChip; visible: false; enabled: false }
    Timer { interval: 1000; repeat: true; running: root.visible; triggeredOnStart: true; onTriggered: root.nowMs = Date.now() }

    Rectangle {
        anchors.fill: parent
        color: "#C7000000"
        opacity: reveal.progress
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }
    }

    component SectionLabel: Row {
        id: label
        property string title: ""
        property string detail: ""
        property color titleColor: "#FFFFFF"
        spacing: 12
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: label.title
            color: label.titleColor
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 12
            font.weight: Font.ExtraBold
            font.letterSpacing: 1.2
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: label.detail
            color: "#66FFFFFF"
            font.family: DesktopTokens.monoFont
            font.pixelSize: 10
            font.weight: Font.DemiBold
            font.letterSpacing: 1
        }
    }

    component Pill: Rectangle {
        id: pill
        property string label: ""
        property color tone: "#7FD4FF"
        width: pillRow.implicitWidth + 20
        height: 22
        radius: 11
        color: Qt.rgba(tone.r, tone.g, tone.b, 0.10)
        border.width: 1
        border.color: Qt.rgba(tone.r, tone.g, tone.b, 0.45)
        Row {
            id: pillRow
            anchors.centerIn: parent
            spacing: 6
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 6; height: 6; radius: 3; color: pill.tone }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: pill.label
                color: pill.tone
                font.family: DesktopTokens.monoFont
                font.pixelSize: 9
                font.weight: Font.Bold
                font.letterSpacing: 0.72
            }
        }
    }

    component SlotBadge: Rectangle {
        id: badge
        property int slotNumber: 0
        property color tone: "#7FD4FF"
        property bool filled: true
        width: Math.max(34, badgeText.implicitWidth + 14)
        height: 26
        radius: 9
        color: filled ? tone : "transparent"
        border.width: 1
        border.color: tone
        Text {
            id: badgeText
            anchors.centerIn: parent
            text: qsTr("P%1").arg(badge.slotNumber + 1)
            color: badge.filled ? "#0B0F1A" : badge.tone
            font.family: DesktopTokens.monoFont
            font.pixelSize: 11
            font.weight: Font.Bold
        }
    }

    component ActionButton: AbstractButton {
        id: action
        property string glyph: ""
        property color tone: "#FFFFFF"
        property real boxSize: 30
        width: boxSize
        height: boxSize
        hoverEnabled: true
        focusPolicy: Qt.StrongFocus
        Accessible.role: Accessible.Button
        Accessible.name: text
        ToolTip.visible: hovered || (activeFocus && AppController.inputMode !== "pointer")
        ToolTip.text: text
        ToolTip.delay: 300
        Keys.onReturnPressed: event => { action.clicked(); event.accepted = true }
        Keys.onEnterPressed: event => { action.clicked(); event.accepted = true }
        background: Rectangle {
            radius: action.boxSize > 28 ? 9 : 7
            color: action.hovered ? "#2EFFFFFF" : "#14FFFFFF"
            border.width: action.activeFocus && AppController.inputMode !== "pointer" ? 2 : 0
            border.color: DesktopTokens.focus
        }
        contentItem: Item {
            DesktopCoopHostIcon {
                anchors.centerIn: parent
                width: action.boxSize > 28 ? 14 : 12
                height: width
                glyph: action.glyph
                ink: action.tone
            }
        }
    }

    component TextAction: AbstractButton {
        id: link
        property color tone: "#A3FFFFFF"
        property real fontSize: 12
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
            color: link.hovered ? "#FFFFFF" : link.tone
            font.family: DesktopTokens.bodyFont
            font.pixelSize: link.fontSize
            font.weight: Font.ExtraBold
            font.underline: link.hovered
        }
    }

    component SlotCard: FocusScope {
        id: card
        required property var modelData
        readonly property int slotNumber: modelData.slot
        readonly property string kind: modelData.kind
        readonly property var member: modelData.member
        readonly property var invite: modelData.invite
        readonly property bool held: root.coop.uploadHolds(modelData)
        readonly property bool closedLook: held || kind === "closed"
        readonly property bool dashed: kind === "invited" || kind === "open" || kind === "closed"
        readonly property color tone: kind === "invited" ? "#7FD4FF" : closedLook ? "#F5A623" : root.slotColors[slotNumber]
        readonly property var peer: member ? root.coop.peer(member.userId) : null
        readonly property real hostPing: root.coop.hostStats ? Number(root.coop.hostStats.hostToGfnPingMs || 0) : 0
        readonly property real peerPing: peer ? Number(peer.peerToHostPingMs || 0) : 0
        readonly property string path: peer && peer.path ? String(peer.path) : ""
        readonly property bool relayed: path === "relay"
        readonly property var link: member ? root.coop.peerLink(member.userId) : null
        readonly property bool showActions: kind === "guest" && (hover.hovered || card.activeFocus)
        readonly property string personName: member ? String(member.displayName || "")
            : invite && invite.to ? String(invite.to.displayName || invite.to.username || "") : ""
        readonly property var acceptedInvite: member ? root.coop.outgoingInvites.find(item => item.to
            && item.to.id === member.userId && item.state === "accepted") || null : null
        property bool kickArmed: false
        width: (cardsRow.width - 3 * cardsRow.spacing) / 4
        height: 200
        Accessible.role: Accessible.Grouping
        Accessible.name: kind === "host" ? qsTr("P%1, you, host").arg(slotNumber + 1)
            : kind === "guest" ? qsTr("P%1, %2").arg(slotNumber + 1).arg(personName)
            : kind === "invited" ? qsTr("P%1, invited %2").arg(slotNumber + 1).arg(personName)
            : closedLook ? qsTr("P%1, closed").arg(slotNumber + 1) : qsTr("P%1, open slot").arg(slotNumber + 1)
        HoverHandler { id: hover }
        Timer { id: kickTimer; interval: 4000; onTriggered: card.kickArmed = false }

        Rectangle {
            anchors.fill: parent
            radius: 20
            color: card.dashed ? "#990E1018" : "#E00E1018"
            border.width: card.dashed ? 0 : 1
            border.color: Qt.rgba(card.tone.r, card.tone.g, card.tone.b, 0.33)
        }
        DesktopDashedOutline {
            visible: card.dashed
            anchors.fill: parent
            radius: 20
            stroke: card.kind === "invited" ? "#997FD4FF" : card.closedLook ? "#99F5A623" : "#80A78BFA"
        }

        Row {
            x: 18
            y: 18
            spacing: 8
            SlotBadge { slotNumber: card.slotNumber; tone: card.tone; filled: card.kind === "host" || card.kind === "guest" }
            Pill {
                visible: (card.kind !== "open" || card.held) && card.width >= 230
                anchors.verticalCenter: parent.verticalCenter
                objectName: card.kind === "guest" ? "coopLobbyPeerLink-" + card.member.userId : ""
                tone: card.kind === "host" ? "#7FD4FF" : card.kind === "guest" ? ({failed: DesktopTokens.danger, reconnecting: "#F5A623",
                        relay: "#F5A623", direct: "#1DB954"}[card.link.kind] || "#A3FFFFFF")
                    : card.kind === "invited" ? "#7FD4FF" : "#F5A623"
                label: card.kind === "host" ? qsTr("HOST")
                    : card.kind === "guest" ? card.link.label
                    : card.kind === "invited" ? qsTr("INVITED") : qsTr("CLOSED")
            }
        }
        Text {
            visible: (card.kind === "guest" && !card.showActions) || card.kind === "invited"
            anchors.right: parent.right
            anchors.rightMargin: 18
            y: 22
            text: card.kind === "invited" ? root.clock(root.since(card.invite.createdAt))
                : card.peer ? qsTr("%1 ms").arg(root.coop.msText(card.hostPing + card.peerPing)) : "—"
            color: card.kind === "invited" ? "#A3FFFFFF" : card.relayed ? "#FFD166" : "#FFFFFF"
            font.family: DesktopTokens.monoFont
            font.pixelSize: 14
            font.weight: Font.Bold
        }
        Row {
            visible: card.showActions
            anchors.right: parent.right
            anchors.rightMargin: 18
            y: 16
            spacing: 6
            ActionButton {
                boxSize: 30
                glyph: "ban"
                tone: card.peer && card.peer.inputMuted ? "#FFD166" : "#FFFFFF"
                text: card.peer && card.peer.inputMuted ? qsTr("Unmute input") : qsTr("Mute input")
                onClicked: root.coop.setInputMuted(card.member.userId, !(card.peer && card.peer.inputMuted))
            }
            ActionButton {
                boxSize: 30
                glyph: "swap"
                enabled: root.coop.nextFreeSlot() > 0
                opacity: enabled ? 1 : 0.4
                text: qsTr("Move slot")
                onClicked: root.coop.moveSlot(card.member.userId, root.coop.nextFreeSlot())
            }
            ActionButton {
                boxSize: 30
                glyph: "user-x"
                tone: card.kickArmed ? "#FF8A80" : "#FFFFFF"
                text: card.kickArmed ? qsTr("Click again to kick %1").arg(card.personName) : qsTr("Kick")
                onClicked: {
                    if (!card.kickArmed) {
                        card.kickArmed = true
                        kickTimer.restart()
                        return
                    }
                    card.kickArmed = false
                    root.coop.kick(card.member.userId)
                }
            }
        }

        Item {
            id: footer
            x: 18
            width: parent.width - 36
            height: 16
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 18
            Text {
                visible: card.kind === "host" || card.kind === "guest"
                anchors.bottom: parent.bottom
                text: card.kind === "host"
                    ? (card.hostPing > 0 ? qsTr("%1 MS TO GEFORCE NOW").arg(root.coop.msText(card.hostPing)) : qsTr("MEASURING PING"))
                    : card.acceptedInvite ? qsTr("JOINED FROM AN INVITE") : qsTr("JOINED WITH THE CODE")
                color: card.kind === "host" ? "#A3FFFFFF" : "#66FFFFFF"
                font.family: DesktopTokens.monoFont
                font.pixelSize: 10
                font.weight: Font.DemiBold
                font.letterSpacing: 0.8
            }
            Text {
                visible: card.kind === "invited"
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Expires in %1").arg(card.invite ? root.clock(root.until(card.invite.expiresAt)) : "")
                color: "#66FFFFFF"
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 12
            }
            TextAction {
                visible: card.kind === "invited"
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Cancel")
                onClicked: root.coop.cancelInvite(card.invite.id)
            }
            Text {
                visible: card.closedLook && !(card.held && root.uploadBarVisible)
                anchors.verticalCenter: parent.verticalCenter
                text: card.held ? qsTr("Open anyway") : qsTr("Open P%1").arg(card.slotNumber + 1)
                color: "#A3FFFFFF"
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 12
                font.weight: Font.ExtraBold
            }
            DesktopSettingsToggle {
                visible: card.closedLook && !(card.held && root.uploadBarVisible)
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                scale: 0.77
                transformOrigin: Item.Right
                checked: false
                Accessible.name: card.held ? qsTr("Open P%1 anyway").arg(card.slotNumber + 1) : qsTr("Open P%1").arg(card.slotNumber + 1)
                onValueChangedByUser: {
                    if (card.held)
                        root.coop.heldSlotOpened = true
                    else
                        root.coop.setMaxGuests(card.slotNumber)
                }
            }
        }

        Row {
            visible: card.kind !== "open" || card.held
            x: 18
            anchors.bottom: footer.top
            anchors.bottomMargin: 10
            spacing: 12
            Rectangle {
                width: 44
                height: 44
                radius: 22
                readonly property color face: card.kind === "invited"
                    ? paletteChip.colorFor(card.personName, root.friends.isOnline(root.friends.friendById(card.invite.to.id))) : card.tone
                color: card.closedLook ? "transparent" : Qt.rgba(face.r, face.g, face.b, 0.13)
                border.width: 1
                border.color: Qt.rgba(face.r, face.g, face.b, card.closedLook ? 0.6 : 0.4)
                Text {
                    visible: !card.closedLook
                    anchors.centerIn: parent
                    text: (card.kind === "host" ? String(root.coop.hostName || "") : card.personName).slice(0, 1).toUpperCase()
                    color: parent.face
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 18
                    font.weight: Font.Black
                }
                DesktopAvatarPhoto {
                    objectName: "coopLobbyAvatar-" + card.slotNumber
                    anchors.fill: parent
                    url: card.closedLook ? "" : card.kind === "invited" ? String(card.invite.to.avatarUrl || "")
                        : card.member ? String(card.member.avatarUrl || "")
                        : card.kind === "host" && root.coop.hostMember ? String(root.coop.hostMember.avatarUrl || "") : ""
                }
                DesktopCoopHostIcon {
                    visible: card.closedLook
                    anchors.centerIn: parent
                    width: 18
                    height: 18
                    glyph: "lock"
                    ink: "#F5A623"
                }
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                width: card.width - 36 - 56
                spacing: 3
                Text {
                    width: parent.width
                    text: card.kind === "host" ? qsTr("You") : card.closedLook ? qsTr("P%1 is closed").arg(card.slotNumber + 1) : card.personName
                    color: card.closedLook ? "#A3FFFFFF" : "#FFFFFF"
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 20
                    font.weight: Font.Black
                    elide: Text.ElideRight
                }
                Text {
                    objectName: card.kind === "guest" ? "coopLobbyPeerStatus-" + card.member.userId : ""
                    width: parent.width
                    text: card.kind === "host" ? (root.pads.length > 0 ? String(root.pads[0].name || qsTr("Controller")) : qsTr("Keyboard and mouse"))
                        : card.kind === "guest" ? (card.link.kind === "failed" ? qsTr("Can't connect · kick to free the slot")
                            : card.link.kind === "reconnecting" ? qsTr("Reconnecting…")
                            : card.peer && card.peer.inputMuted ? qsTr("Input muted")
                            : card.relayed ? (root.coop.relayName(card.peer ? card.peer.relayId : "") !== ""
                                ? qsTr("Via the %1 relay").arg(root.coop.relayName(card.peer ? card.peer.relayId : "")) : qsTr("Via relay"))
                            : card.path === "direct" ? qsTr("Direct connection") : qsTr("Connecting…"))
                        : card.kind === "invited" ? qsTr("Invited · waiting")
                        : card.held ? qsTr("Needs %1 of your %2 Mbps").arg(root.mbps(root.streamMbps * (card.slotNumber))).arg(root.mbps(root.capacityMbps))
                        : qsTr("You closed this slot")
                    color: card.kind === "invited" ? "#7FD4FF" : card.closedLook ? "#F5A623"
                        : card.kind === "guest" && card.link.kind === "failed" ? DesktopTokens.danger : "#A3FFFFFF"
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 12
                    font.weight: card.kind === "invited" || card.closedLook ? Font.Bold : Font.Medium
                    elide: Text.ElideRight
                }
            }
        }

        Column {
            visible: card.kind === "open" && !card.held
            x: 18
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 18
            spacing: 6
            DesktopCoopHostIcon { width: 26; height: 26; glyph: "user-plus"; ink: "#A78BFA" }
            Text {
                text: qsTr("Open slot")
                color: "#A3FFFFFF"
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 20
                font.weight: Font.Black
            }
            Text {
                text: qsTr("Waiting for someone to join")
                color: "#52FFFFFF"
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 13
                font.weight: Font.Medium
            }
        }
    }

    Item {
        id: stage
        anchors.fill: parent
        opacity: reveal.progress
        scale: reveal.zoom

        Column {
            id: content
            objectName: "coopLobby"
            readonly property int designTop: !root.showStrip ? 70 : root.lookupVisible ? 56 : 120
            width: Math.min(1056, root.width - 48)
            x: Math.round((root.width - width) / 2)
            y: Math.max(24, Math.min(designTop, Math.round((root.height - height) / 2)))

            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                lineHeight: 14
                lineHeightMode: Text.FixedHeight
                text: root.coop.hostStreaming ? qsTr("CO-OP LOBBY · %1 IS RUNNING").arg(root.gameTitle.toUpperCase())
                    : qsTr("CO-OP LOBBY · %1 IS STARTING").arg(root.gameTitle.toUpperCase())
                color: "#7FD4FF"
                font.family: DesktopTokens.monoFont
                font.pixelSize: 11
                font.weight: Font.DemiBold
                font.letterSpacing: 11 * 0.18
            }
            Item { width: 1; height: 10 }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: root.showStrip ? qsTr("Invite friends to play")
                    : qsTr("Invite up to %1 friends").arg(root.lobby ? Number(root.lobby.maxGuests || 0) : 0)
                color: "#FFFFFF"
                font.family: DesktopTokens.displayFont
                font.pixelSize: 30
                font.weight: Font.Black
                lineHeight: 36
                lineHeightMode: Text.FixedHeight
            }

            Item { visible: !root.showStrip; width: 1; height: 8 }
            Item {
                visible: !root.showStrip
                width: parent.width
                height: 96
                Text {
                    anchors.centerIn: parent
                    text: root.lobby ? String(root.lobby.code || "") : ""
                    color: "#FFFFFF"
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: 84
                    font.weight: Font.Bold
                    font.letterSpacing: 84 * 0.05
                    Accessible.role: Accessible.StaticText
                    Accessible.name: qsTr("Co-op code %1").arg(text)
                }
            }
            Item { visible: !root.showStrip; width: 1; height: 6 }
            Row {
                visible: !root.showStrip
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 8
                DesktopCoopHostButton {
                    text: qsTr("Copy link"); glyph: "link"; fill: "#FFFFFF"; hoverFill: "#14000000"; edge: "#FFFFFF"; ink: "#0A0D14"
                    onClicked: root.coop.copyLink()
                }
                DesktopCoopHostButton {
                    text: qsTr("Copy code"); glyph: "copy"; fill: "#1FFFFFFF"
                    onClicked: root.coop.copyCode()
                }
                DesktopCoopHostButton {
                    text: qsTr("New code"); glyph: "refresh"; fill: "#14FFFFFF"; edge: "transparent"; ink: "#A3FFFFFF"
                    onClicked: root.coop.regenerateCode()
                }
            }

            Item { visible: root.showStrip; width: 1; height: root.lookupVisible ? 30 : 34 }
            Item {
                id: stripHeader
                visible: root.showStrip
                width: parent.width
                height: 18
                SectionLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    title: qsTr("INVITE FRIENDS")
                    detail: qsTr("ONE CLICK EACH · OFFLINE FRIENDS GET A DM")
                }
                AbstractButton {
                    id: allFriends
                    objectName: "coopLobbyAllFriends"
                    visible: root.stripInvites.length + root.candidates.length > 5
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: allRow.implicitWidth
                    height: 20
                    hoverEnabled: true
                    focusPolicy: Qt.StrongFocus
                    text: root.showAllFriends ? qsTr("Show fewer") : qsTr("All %1 friends").arg(root.coop.invitableCount)
                    Accessible.role: Accessible.Button
                    Accessible.name: text
                    onClicked: root.showAllFriends = !root.showAllFriends
                    Keys.onReturnPressed: event => { allFriends.clicked(); event.accepted = true }
                    contentItem: Row {
                        id: allRow
                        spacing: 6
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: allFriends.text
                            color: allFriends.hovered ? "#B5E6FF" : "#7FD4FF"
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 14
                            font.weight: Font.ExtraBold
                        }
                        DesktopCoopHostIcon {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 14; height: 14
                            glyph: "chevron"
                            rotation: root.showAllFriends ? -90 : 0
                            ink: "#7FD4FF"
                        }
                    }
                    background: Rectangle {
                        anchors.fill: parent
                        anchors.margins: -4
                        radius: 6
                        color: "transparent"
                        border.width: allFriends.activeFocus && AppController.inputMode !== "pointer" ? 2 : 0
                        border.color: DesktopTokens.focus
                    }
                }
            }
            Item { visible: root.showStrip; width: 1; height: 12 }
            Flow {
                id: strip
                objectName: "coopLobbyInviteStrip"
                visible: root.showStrip
                width: parent.width
                spacing: 9
                Repeater {
                    model: root.stripEntries
                    delegate: Column {
                        id: stripCell
                        required property var modelData
                        width: (strip.width - 4 * strip.spacing) / 5
                        spacing: 8
                        DesktopCoopHostFriendChip {
                            objectName: "coopLobbyInvite-" + stripCell.modelData.user.id
                            width: parent.width
                            user: stripCell.modelData.user
                            presence: stripCell.modelData.friend ? stripCell.modelData.friend.presence : null
                            inviteState: stripCell.modelData.inviteState
                            nowMs: root.nowMs
                            detail: stripCell.modelData.inviteState === "pending" && root.notesVisible
                                ? qsTr("Pending · %1").arg(root.clock(root.since(stripCell.modelData.invite.createdAt))) : ""
                            onClicked: root.activateChip(stripCell.modelData)
                        }
                        Text {
                            visible: root.notesVisible
                            width: parent.width
                            leftPadding: 4
                            rightPadding: 4
                            text: root.chipNote(stripCell.modelData)
                            color: stripCell.modelData.inviteState === "expired" ? "#F5A623" : "#66FFFFFF"
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 12
                            font.weight: Font.DemiBold
                            wrapMode: Text.WordWrap
                            lineHeight: 16
                            lineHeightMode: Text.FixedHeight
                        }
                    }
                }
            }

            Item { visible: root.lookupVisible; width: 1; height: 22 }
            Rectangle {
                id: lookupPanel
                objectName: "coopInviteLookup"
                visible: root.lookupVisible
                width: parent.width
                height: 133
                radius: 18
                color: "#E00E1018"
                border.width: 1
                border.color: "#24FFFFFF"
                readonly property var lookup: root.friends.lookup
                readonly property bool notFound: lookup !== null && lookup.state === "done" && lookup.relation === "not_found"
                Column {
                    id: lookupLeft
                    x: 20
                    y: 18
                    width: Math.min(380, Math.round(lookupPanel.width * 0.4))
                    spacing: 10
                    Text {
                        text: qsTr("SOMEONE NOT ON OPENNOW")
                        color: "#A3FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 12
                        font.weight: Font.ExtraBold
                        font.letterSpacing: 1.2
                    }
                    Rectangle {
                        width: parent.width
                        height: 44
                        radius: 12
                        color: "#B30B0F1A"
                        border.width: lookupInput.activeFocus ? 2 : 1
                        border.color: lookupInput.activeFocus ? DesktopTokens.focus : "#24FFFFFF"
                        Text {
                            id: atSign
                            x: 14
                            anchors.verticalCenter: parent.verticalCenter
                            text: "@"
                            color: "#66FFFFFF"
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 16
                            font.weight: Font.Bold
                        }
                        TextInput {
                            id: lookupInput
                            x: atSign.x + atSign.width + 10
                            width: parent.width - x - 40
                            anchors.verticalCenter: parent.verticalCenter
                            text: lookupPanel.lookup ? String(lookupPanel.lookup.username || "") : ""
                            color: "#FFFFFF"
                            selectionColor: "#667FD4FF"
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 15
                            font.weight: Font.Bold
                            maximumLength: 33
                            clip: true
                            Accessible.role: Accessible.EditableText
                            Accessible.name: qsTr("Username")
                            onAccepted: root.friends.lookupUser(text)
                            Keys.onEscapePressed: event => { root.closeLookup(); event.accepted = true }
                        }
                        AbstractButton {
                            id: clearLookup
                            anchors.right: parent.right
                            anchors.rightMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            width: 22
                            height: 22
                            text: qsTr("Clear")
                            Accessible.role: Accessible.Button
                            Accessible.name: text
                            onClicked: root.closeLookup()
                            contentItem: Item {
                                DesktopCoopHostIcon { anchors.centerIn: parent; width: 14; height: 14; glyph: "close"; ink: "#66FFFFFF" }
                            }
                            background: Item {}
                        }
                    }
                    Row {
                        spacing: 6
                        height: 17
                        visible: lookupPanel.lookup !== null
                        DesktopCoopHostIcon {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 14; height: 14
                            glyph: "info"
                            ink: lookupStatus.color
                        }
                        Text {
                            id: lookupStatus
                            anchors.verticalCenter: parent.verticalCenter
                            readonly property var lookup: lookupPanel.lookup
                            text: !lookup ? ""
                                : lookup.state === "searching" ? qsTr("Looking for @%1…").arg(lookup.username)
                                : lookup.state === "error" ? root.friends.errorText(lookup.code)
                                : lookupPanel.notFound ? qsTr("No one on OpenNOW is called @%1 yet").arg(lookup.username)
                                : lookup.relation === "self" ? qsTr("That's you.")
                                : qsTr("@%1 is on OpenNOW").arg(lookup.username)
                            color: lookup && (lookup.state === "error" || lookupPanel.notFound) ? "#F5A623"
                                : lookup && lookup.state === "done" ? "#6EE7B7" : "#A3FFFFFF"
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 13
                            font.weight: Font.Bold
                        }
                        TextAction {
                            readonly property var lookup: lookupPanel.lookup
                            readonly property var result: lookup ? root.friends.requestResults[lookup.username] : null
                            visible: lookup !== null && lookup.state === "done" && lookup.user !== null
                                && (lookup.relation === "none" || lookup.relation === "friends")
                            anchors.verticalCenter: parent.verticalCenter
                            tone: "#7FD4FF"
                            fontSize: 13
                            text: lookup && lookup.relation === "friends" ? qsTr("Invite")
                                : result && result.state === "sent" ? qsTr("Request sent") : qsTr("Add friend")
                            onClicked: {
                                if (lookup.relation === "friends")
                                    root.coop.invite(lookup.user.id)
                                else if (!result)
                                    root.friends.sendRequest(lookup.username)
                            }
                        }
                    }
                }
                Rectangle {
                    x: lookupLeft.x + lookupLeft.width + 22
                    y: 18
                    width: 1
                    height: parent.height - 36
                    color: "#24FFFFFF"
                }
                Column {
                    x: lookupLeft.x + lookupLeft.width + 45
                    y: 18
                    width: parent.width - x - 20
                    spacing: 6
                    Text {
                        width: parent.width
                        text: lookupPanel.notFound ? qsTr("Send %1 the code or link instead").arg(root.lookupName())
                            : qsTr("Or send the code or link")
                        color: "#FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 16
                        font.weight: Font.ExtraBold
                        elide: Text.ElideRight
                    }
                    Text {
                        width: parent.width
                        text: qsTr("They open the link, get OpenNOW if needed, and land in your lobby.")
                        color: "#A3FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 13
                        elide: Text.ElideRight
                    }
                    Item {
                        width: parent.width
                        height: 42
                        Rectangle {
                            id: lookupCode
                            y: 4
                            width: lookupCodeText.implicitWidth + 28
                            height: 38
                            radius: 10
                            color: "#59000000"
                            border.width: 1
                            border.color: "#24FFFFFF"
                            Text {
                                id: lookupCodeText
                                anchors.centerIn: parent
                                text: root.lobby ? String(root.lobby.code || "") : ""
                                color: "#FFFFFF"
                                font.family: DesktopTokens.monoFont
                                font.pixelSize: 18
                                font.weight: Font.Bold
                                font.letterSpacing: 1.44
                            }
                        }
                        Text {
                            x: lookupCode.width + 12
                            anchors.verticalCenter: lookupCode.verticalCenter
                            width: lookupCopy.x - x - 12
                            text: root.displayUrl()
                            color: "#7FD4FF"
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 13
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                        }
                        DesktopCoopHostButton {
                            id: lookupCopy
                            anchors.right: parent.right
                            y: 4
                            text: qsTr("Copy link"); glyph: "link"; glyphSize: 15; fill: "#FFFFFF"; hoverFill: "#14000000"; edge: "transparent"; ink: "#0B0F1A"
                            onClicked: root.coop.copyLink()
                        }
                    }
                }
            }

            Item { width: 1; height: root.showStrip ? (root.lookupVisible ? 22 : 26) : 34 }
            SectionLabel {
                visible: root.showStrip
                title: qsTr("PLAYERS")
                titleColor: "#A3FFFFFF"
                detail: {
                    const invited = root.cards.find(card => card.kind === "invited")
                    const count = qsTr("%1 OF %2").arg(root.coop.playerCount).arg(root.coop.maxPlayers)
                    return invited && !root.lookupVisible
                        ? qsTr("%1 OF %2 · P%3 HELD FOR %4").arg(root.coop.playerCount).arg(root.coop.maxPlayers)
                            .arg(invited.slot + 1).arg(String(invited.invite.to.displayName || invited.invite.to.username || "").toUpperCase())
                        : count
                }
            }
            Item { visible: root.showStrip; width: 1; height: 12 }
            Row {
                id: cardsRow
                width: parent.width
                spacing: 16
                Repeater {
                    model: root.cards
                    delegate: SlotCard {}
                }
            }

            Item { visible: uploadUsage.visible; width: 1; height: 18 }
            DesktopCoopUploadUsage {
                id: uploadUsage
                anchors.horizontalCenter: parent.horizontalCenter
            }
            Item { visible: root.uploadBarVisible; width: 1; height: 18 }
            Rectangle {
                id: uploadBar
                visible: root.uploadBarVisible
                width: parent.width
                height: Math.max(uploadMeter.height, uploadText.implicitHeight) + 28
                radius: 16
                color: "#14F5A623"
                border.width: 1
                border.color: "#59F5A623"
                readonly property real neededMbps: root.coop.uploadNeededKbps / 1000
                readonly property real scaleMbps: Math.max(neededMbps, root.capacityMbps) * 1.08
                readonly property int nextGuest: root.coop.guests.length + 1
                Column {
                    id: uploadMeter
                    x: 18
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(420, Math.round(uploadBar.width * 0.4))
                    spacing: 8
                    Item {
                        width: parent.width
                        height: 16
                        Text {
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 1
                            text: qsTr("HOST UPLOAD")
                            color: "#A3FFFFFF"
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 10
                            font.weight: Font.DemiBold
                            font.letterSpacing: 0.8
                        }
                        Row {
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            spacing: 4
                            Text {
                                text: qsTr("%1 Mbps").arg(root.mbps(uploadBar.neededMbps))
                                color: "#F5A623"
                                font.family: DesktopTokens.monoFont
                                font.pixelSize: 13
                                font.weight: Font.Bold
                            }
                            Text {
                                anchors.baseline: parent.children[0].baseline
                                text: qsTr("/ %1 Mbps").arg(root.mbps(root.capacityMbps))
                                color: "#52FFFFFF"
                                font.family: DesktopTokens.monoFont
                                font.pixelSize: 10
                                font.weight: Font.DemiBold
                            }
                        }
                    }
                    Item {
                        width: parent.width
                        height: 6
                        Rectangle { anchors.fill: parent; radius: 3; color: "#1AFFFFFF" }
                        Rectangle {
                            width: Math.round(parent.width * root.capacityMbps / uploadBar.scaleMbps)
                            height: 6
                            radius: 3
                            color: "#6EE7B7"
                        }
                        Rectangle {
                            x: Math.round(parent.width * root.capacityMbps / uploadBar.scaleMbps)
                            width: Math.max(0, Math.round(parent.width * (uploadBar.neededMbps - root.capacityMbps) / uploadBar.scaleMbps))
                            height: 6
                            radius: 3
                            color: "#F5A623"
                            Rectangle { width: 3; height: 6; color: "#F5A623" }
                        }
                        Rectangle {
                            x: Math.round(parent.width * root.capacityMbps / uploadBar.scaleMbps) - 1
                            y: -4
                            width: 2
                            height: 14
                            radius: 1
                            color: "#FFFFFF"
                        }
                    }
                }
                Text {
                    id: uploadText
                    x: uploadMeter.x + uploadMeter.width + 20
                    anchors.verticalCenter: parent.verticalCenter
                    width: openToggleRow.x - x - 20
                    text: {
                        const opened = root.coop.heldSlotOpened
                        const guest = uploadBar.nextGuest
                        const message = guest === 3
                            ? (opened ? qsTr("A 3rd guest needs %1 of your %2 Mbps and would make everyone stutter. P%3 is open anyway.")
                                : qsTr("A 3rd guest needs %1 of your %2 Mbps and would make everyone stutter. P%3 stays closed unless you open it."))
                            : guest === 2
                            ? (opened ? qsTr("A 2nd guest needs %1 of your %2 Mbps and would make everyone stutter. P%3 is open anyway.")
                                : qsTr("A 2nd guest needs %1 of your %2 Mbps and would make everyone stutter. P%3 stays closed unless you open it."))
                            : (opened ? qsTr("A guest needs %1 of your %2 Mbps and would make everyone stutter. P%3 is open anyway.")
                                : qsTr("A guest needs %1 of your %2 Mbps and would make everyone stutter. P%3 stays closed unless you open it."))
                        return message.arg(root.mbps(uploadBar.neededMbps)).arg(root.mbps(root.capacityMbps)).arg(guest + 1)
                    }
                    color: "#FFD166"
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 13
                    font.weight: Font.DemiBold
                    wrapMode: Text.WordWrap
                    lineHeight: 19
                    lineHeightMode: Text.FixedHeight
                }
                Row {
                    id: openToggleRow
                    anchors.right: parent.right
                    anchors.rightMargin: 18
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    visible: uploadBar.nextGuest < root.coop.maxPlayers
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: qsTr("Open P%1").arg(uploadBar.nextGuest + 1)
                        color: "#FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 13
                        font.weight: Font.ExtraBold
                    }
                    DesktopSettingsToggle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: implicitWidth * 0.85
                        height: implicitHeight * 0.85
                        Accessible.name: qsTr("Open P%1").arg(uploadBar.nextGuest + 1)
                        checked: root.coop.heldSlotOpened
                        onValueChangedByUser: value => root.coop.heldSlotOpened = value
                    }
                }
            }

            Item { visible: root.showStrip && !root.lookupVisible; width: 1; height: 24 }
            Row {
                visible: root.showStrip && !root.lookupVisible
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 10
                TextAction {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Not on OpenNOW yet? Send the code")
                    fontSize: 14
                    tone: "#A3FFFFFF"
                    Accessible.description: qsTr("Look someone up by username")
                    onClicked: root.openLookup()
                }
                Rectangle {
                    width: codeText.implicitWidth + 28
                    height: 38
                    radius: 10
                    color: "#59000000"
                    border.width: 1
                    border.color: "#24FFFFFF"
                    Text {
                        id: codeText
                        anchors.centerIn: parent
                        text: root.lobby ? String(root.lobby.code || "") : ""
                        color: "#FFFFFF"
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: 18
                        font.weight: Font.Bold
                        font.letterSpacing: 1.44
                    }
                }
                DesktopCoopHostButton {
                    text: qsTr("Copy link"); glyph: "link"; glyphSize: 15; fill: "#FFFFFF"; hoverFill: "#14000000"; edge: "transparent"; ink: "#0B0F1A"
                    onClicked: root.coop.copyLink()
                }
                DesktopCoopHostButton {
                    text: qsTr("Copy code"); glyph: "copy"; glyphSize: 15; fill: "#14FFFFFF"; edge: "#14FFFFFF"
                    onClicked: root.coop.copyCode()
                }
                DesktopCoopHostButton {
                    text: qsTr("New code"); glyph: "refresh"; glyphSize: 15; ink: "#A3FFFFFF"
                    onClicked: root.coop.regenerateCode()
                }
            }

            Item { width: 1; height: root.showStrip ? (root.lookupVisible ? 24 : 30) : 22 }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 10
                DesktopCoopHostButton {
                    id: endButton
                    objectName: "coopLobbyEnd"
                    text: qsTr("End co-op"); glyph: "logout"; fontSize: 15
                    fill: "#1AFF8A80"; edge: "#59FF8A80"; ink: "#FFB4AE"
                    onClicked: root.coop.requestEndCoop()
                }
                DesktopCoopHostButton {
                    id: backButton
                    objectName: "coopLobbyBack"
                    text: qsTr("Back to game"); keyText: "Esc"; fontSize: 15
                    onClicked: root.close()
                }
                DesktopCoopHostButton {
                    id: startButton
                    objectName: "coopLobbyStart"
                    visible: root.lobby !== null && root.lobby.state !== "streaming"
                    text: qsTr("Start playing together"); glyph: "play"; glyphSize: 12; keyText: "Enter"; fontSize: 15
                    fill: "#FFFFFF"; hoverFill: "#14000000"; edge: "#7FD4FF"; edgeWidth: 2; ink: "#0A0D14"
                    onClicked: root.coop.startPlayingTogether()
                }
            }
        }
    }

    onOpenedChanged: if (opened) {
        showAllFriends = false
        lookupOpen = false
        Qt.callLater(() => {
            if (!root.opened)
                return
            if (startButton.visible)
                startButton.forceActiveFocus()
            else
                backButton.forceActiveFocus()
        })
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.close()
        } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && startButton.visible) {
            root.coop.startPlayingTogether()
        } else {
            return
        }
        event.accepted = true
    }
}
