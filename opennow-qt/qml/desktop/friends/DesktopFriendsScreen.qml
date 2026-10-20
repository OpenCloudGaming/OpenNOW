import QtQuick
import OpenNOW

FocusScope {
    id: screen
    objectName: "desktopFriendsScreen"
    anchors.fill: parent
    readonly property var friends: ShellStore.friends
    readonly property var coop: ShellStore.coop
    property string route: "friends"
    property string filter: "all"
    property string query: ""
    property string selectedUserId: ""
    property string manageUserId: ""
    property bool manageMenuOpen: false
    property string addText: ""
    property real nowMs: Date.now()
    property Item searchField: null
    // The early-access card: opens by itself until the linked user dismisses it, or from the header pill.
    property bool earlyAccessReopened: false
    readonly property bool earlyAccessOpen: friends.linked && (friends.earlyAccessPending || earlyAccessReopened)
    readonly property Item screenItem: screen
    readonly property string page: !friends.linked ? (friends.link ? "linking" : "notLinked")
        : route === "friends-add" ? "add"
        : route === "friends-requests" ? "requests"
        : route === "friends-invites" ? "invites"
        : friends.welcomePending ? "welcome" : "list"
    readonly property int inviteCount: coop.incomingInvites.length + coop.inviteHistory.length
    readonly property string friendsSubtitle: {
        switch (page) {
        case "notLinked": return qsTr("Not linked")
        case "linking": return qsTr("Linking Discord")
        case "welcome": return qsTr("Linked")
        case "add": return qsTr("Add a friend")
        case "requests": return qsTr("Requests")
        case "invites": return inviteCount === 1 ? qsTr("1 invite") : qsTr("%1 invites").arg(inviteCount)
        }
        return qsTr("%1 online").arg(friends.onlineCount)
    }
    readonly property Component headerActions: page === "list" || page === "invites" ? headerComponent : null
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Friends")
    signal firstResultRequested()

    function syncRoute() {
        if (AppController.route.indexOf("friends") === 0)
            route = AppController.route
    }
    function showFriend(userId) {
        filter = "all"
        query = ""
        selectedUserId = userId
        AppController.navigate("friends")
    }
    function openManage(userId) {
        manageUserId = userId
        manageMenuOpen = true
        AppController.navigate("friends-requests")
    }
    function closeEarlyAccess() {
        friends.dismissEarlyAccess()
        earlyAccessReopened = false
        if (pageLoader.item)
            pageLoader.item.forceActiveFocus()
    }
    function setFilter(value) {
        filter = value
        if (route !== "friends")
            AppController.navigate("friends")
    }
    Component.onCompleted: syncRoute()
    Connections {
        target: AppController
        function onRouteChanged() { screen.syncRoute() }
    }
    Timer {
        interval: 1000
        repeat: true
        running: screen.visible && (screen.page === "list" || screen.page === "invites" || screen.page === "requests")
        onTriggered: screen.nowMs = Date.now()
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Slash && screen.searchField) {
            screen.searchField.forceActiveFocus()
            event.accepted = true
        }
    }

    Loader {
        id: pageLoader
        anchors.fill: parent
        focus: true
        sourceComponent: {
            switch (screen.page) {
            case "notLinked": return notLinkedPage
            case "linking": return linkingPage
            case "welcome": return welcomePage
            case "add": return addPage
            case "requests": return requestsPage
            case "invites": return invitesPage
            }
            return listPage
        }
        onLoaded: if (!screen.earlyAccessOpen) item.forceActiveFocus()
    }
    DesktopFriendsEarlyAccess {
        anchors.fill: parent
        z: 10
        opened: screen.visible && screen.earlyAccessOpen
        onCloseRequested: screen.closeEarlyAccess()
    }
    Component { id: notLinkedPage; DesktopFriendsNotLinked {} }
    Component { id: linkingPage; DesktopFriendsLinking {} }
    Component { id: welcomePage; DesktopFriendsWelcome { screen: screenItem } }
    Component { id: listPage; DesktopFriendsList { screen: screenItem } }
    Component { id: invitesPage; DesktopFriendsInvites { screen: screenItem } }
    Component { id: addPage; DesktopFriendsAdd { screen: screenItem } }
    Component { id: requestsPage; DesktopFriendsRequests { screen: screenItem } }

    Component {
        id: headerComponent
        Item {
            id: headerBar
            readonly property real fixedWidth: earlyAccessPill.implicitWidth + requestsButton.implicitWidth + addButton.implicitWidth + DesktopTokens.px(20)
            readonly property real searchRoom: width - fixedWidth - DesktopTokens.px(10)
            readonly property bool chipsFit: searchRoom - chipBox.width - DesktopTokens.px(10) >= DesktopTokens.px(200)
            implicitWidth: headerRow.implicitWidth
            implicitHeight: DesktopTokens.px(40)
            Row {
                id: headerRow
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: DesktopTokens.px(10)
                DesktopFriendsButton {
                    id: earlyAccessPill
                    objectName: "friendsEarlyAccessPill"
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Early access")
                    kind: "outline"
                    controlHeight: 28; cornerRadius: 14; sidePadding: 12; fontSize: 12
                    Accessible.name: qsTr("Early access. What to expect")
                    background: Rectangle {
                        radius: height / 2
                        color: DesktopFriendsFormat.tint(DesktopFriendsFormat.amber, earlyAccessPill.hovered ? 0.18 : 0.10)
                        border.width: earlyAccessPill.visualFocus ? 2 : 1
                        border.color: earlyAccessPill.visualFocus ? DesktopTokens.focus : DesktopFriendsFormat.tint(DesktopFriendsFormat.amber, 0.4)
                    }
                    contentItem: DesktopFriendsLabel {
                        text: earlyAccessPill.text
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                        mono: true; size: 10; line: 12; tracking: 0.08
                        color: DesktopFriendsFormat.amber
                        font.weight: Font.Bold
                        font.capitalization: Font.AllUppercase
                    }
                    onClicked: screen.earlyAccessReopened = true
                }
                Rectangle {
                    id: chipBox
                    visible: headerBar.chipsFit
                    anchors.verticalCenter: parent.verticalCenter
                    width: chips.implicitWidth + DesktopTokens.px(8)
                    height: DesktopTokens.px(40)
                    radius: DesktopTokens.px(12)
                    color: DesktopTokens.raised
                    border.width: 1
                    border.color: DesktopTokens.seamSoft
                    Row {
                        id: chips
                        x: DesktopTokens.px(4)
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: DesktopTokens.px(4)
                        Repeater {
                            model: [{key: "all", label: qsTr("All")}, {key: "online", label: qsTr("Online")},
                                {key: "playing", label: qsTr("Playing")}]
                            delegate: DesktopFriendsButton {
                                required property var modelData
                                readonly property bool chosen: screen.filter === modelData.key
                                text: modelData.label
                                controlHeight: 32; cornerRadius: 9; sidePadding: 14; fontSize: 14
                                kind: chosen ? "accent" : "outline"
                                checkable: true
                                checked: chosen
                                Accessible.role: Accessible.RadioButton
                                background: Rectangle {
                                    radius: DesktopTokens.px(9)
                                    color: parent.chosen ? DesktopFriendsFormat.accent : parent.hovered ? DesktopTokens.raised : "transparent"
                                    border.width: parent.visualFocus ? 2 : 0
                                    border.color: DesktopTokens.focus
                                }
                                onClicked: screen.setFilter(modelData.key)
                            }
                        }
                    }
                }
                Rectangle {
                    visible: width >= DesktopTokens.px(120)
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(DesktopTokens.px(240), headerBar.searchRoom - (chipBox.visible ? chipBox.width + DesktopTokens.px(10) : 0))
                    height: DesktopTokens.px(38)
                    radius: DesktopTokens.px(12)
                    color: DesktopFriendsFormat.fieldFill
                    border.width: 1
                    border.color: searchInput.activeFocus ? DesktopFriendsFormat.accent : DesktopTokens.seam
                    DesktopFriendsIcon {
                        x: DesktopTokens.px(14)
                        anchors.verticalCenter: parent.verticalCenter
                        width: DesktopTokens.px(16); height: width
                        glyph: "search"; ink: DesktopTokens.textMuted
                    }
                    TextInput {
                        id: searchInput
                        objectName: "friendsSearch"
                        x: DesktopTokens.px(40)
                        width: parent.width - x - DesktopTokens.px(14)
                        anchors.verticalCenter: parent.verticalCenter
                        text: screen.query
                        color: DesktopTokens.text
                        selectionColor: DesktopFriendsFormat.tint(DesktopFriendsFormat.accent, 0.4)
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: DesktopTokens.px(15)
                        clip: true
                        maximumLength: 32
                        Accessible.role: Accessible.EditableText
                        Accessible.name: qsTr("Search friends")
                        Component.onCompleted: screen.searchField = searchInput
                        onTextEdited: {
                            screen.query = text
                            if (screen.route !== "friends")
                                AppController.navigate("friends")
                        }
                        Keys.onReturnPressed: screen.firstResultRequested()
                        Keys.onEnterPressed: screen.firstResultRequested()
                        Keys.onEscapePressed: event => {
                            if (text === "") {
                                event.accepted = false
                                return
                            }
                            screen.query = ""
                        }
                        DesktopFriendsLabel {
                            visible: searchInput.text === ""
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("Search friends")
                            size: 15; line: 20
                            color: DesktopTokens.textMuted
                        }
                    }
                }
                DesktopFriendsButton {
                    id: requestsButton
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Requests")
                    kind: "secondary"
                    controlHeight: 38; cornerRadius: 10; sidePadding: 16; fontSize: 14
                    badge: screen.friends.requestCount
                    Accessible.name: screen.friends.requestCount > 0
                        ? qsTr("Requests, %1 waiting").arg(screen.friends.requestCount) : qsTr("Requests")
                    onClicked: AppController.navigate("friends-requests")
                }
                DesktopFriendsButton {
                    id: addButton
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Add friend")
                    glyph: "userPlus"; glyphSize: 16
                    kind: "primary"
                    controlHeight: 38; cornerRadius: 10; sidePadding: 16; fontSize: 14
                    onClicked: AppController.navigate("friends-add")
                }
            }
        }
    }
}
