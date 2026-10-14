import QtQuick
import QtQuick.Layouts
import OpenNOW

DesktopGameDetailsDialog {
    id: root
    objectName: "desktopSourceDetails"
    partName: "sourceDetails"
    property var library: ShellStore.sourceLibraryOwnerState
    readonly property var store: library.sources
    readonly property var session: ShellStore.sourceSession
    readonly property var source: library.source
    readonly property var details: library.details
    readonly property bool requested: details !== null || library.detailsRequestId !== "" || library.detailsError !== ""
    property var preview: null
    readonly property var variant: library.defaultVariant()
    readonly property string sourceName: source ? String(source.name || source.id) : ""
    readonly property var authState: source ? store.authState(source.id) : null
    readonly property bool playEnabled: library.playbackSupported() && ShellStore.sourcePlaybackAvailable
        && variant !== null && !library.playRequested && !ShellStore.streamBusy
        && ShellStore.activeSession === null
    readonly property string notice: {
        if (library.detailsError !== "")
            return library.detailsError
        if (!details)
            return qsTr("Loading…")
        if (!library.playbackSupported())
            return qsTr("This service only lists games. They can't be played from OpenNOW.")
        if (!ShellStore.sourcePlaybackAvailable)
            return qsTr("This device can't play games from this service.")
        if (variant === null)
            return qsTr("This game isn't available to play right now.")
        if (library.launchDecision && library.launchDecision.state === "blocked")
            return String(library.launchDecision.message || qsTr("The service can't start this game right now."))
        if (session.active || ShellStore.activeSession !== null)
            return qsTr("Finish your current session before starting another.")
        return ""
    }

    function show(item) {
        const requestedDetails = library.openDetails(item.ref.localId)
        preview = requestedDetails ? item : null
        return requestedDetails
    }

    game: details ? details.game : preview
    badgeText: sourceName.toUpperCase()
    metaText: game ? [game.subtitle, game.availability !== "available" ? DesktopTokens.sourceAvailabilityText(game.availability) : ""]
        .filter(Boolean).join(" · ") : ""
    initialFocusItem: playEnabled ? playAction : null
    onRequestedChanged: if (!requested) preview = null
    onCloseRequested: library.closeDetails()
    Connections {
        target: root.library
        function onDetailsChanged() { if (root.opened && root.details) root.focusInitial() }
    }

    Text {
        objectName: "desktopSourcePlayNote"
        width: parent.width
        visible: text !== ""
        text: root.notice
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: root.library.detailsError !== "" ? (Theme.lightMode ? Qt.darker(DesktopTokens.danger, 2) : DesktopTokens.danger) : Theme.textMuted
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.captionSize
    }
    RowLayout {
        objectName: "sourceDetailsPrimaryActions"
        width: parent.width
        spacing: DesktopTokens.px(10)
        DesktopButton {
            id: playAction
            objectName: "desktopSourcePlay"
            Layout.fillWidth: true; Layout.minimumWidth: 0; Layout.preferredHeight: DesktopTokens.px(52)
            font.pixelSize: DesktopTokens.captionSize
            leftPadding: DesktopTokens.px(14); rightPadding: DesktopTokens.px(14)
            primary: true; glyph: "desktop-play.svg"; shortcutText: qsTr("ENTER"); shortcutSequence: "Enter"
            text: root.library.playRequested ? qsTr("Checking…") : qsTr("Play")
            enabled: root.playEnabled
            onClicked: root.library.play()
        }
    }
    Text {
        objectName: "desktopSourceDescription"
        width: parent.width
        visible: text !== ""
        text: root.details ? root.details.description : ""
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        color: Theme.textMuted
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.bodySize
    }
    GridLayout {
        id: summaryGrid
        objectName: "sourceDetailsSummary"
        width: parent.width
        columns: width < DesktopTokens.px(620) ? 2 : 4
        uniformCellWidths: columns === 2
        columnSpacing: DesktopTokens.px(10); rowSpacing: DesktopTokens.px(10)
        DesktopGameDetailsSummaryCard {
            glyph: "globe"
            title: root.sourceName
            detail: root.authState && root.authState.state === "signed-in"
                ? String(root.authState.account && root.authState.account.name || "") : ""
        }
        DesktopGameDetailsSummaryCard {
            objectName: "sourceDetailsVariant"
            visible: root.variant !== null
            glyph: "grid"
            title: root.variant ? String(root.variant.label || "") : ""
            detail: root.variant ? DesktopTokens.sourceAvailabilityText(root.variant.availability) : ""
        }
        DesktopButton {
            objectName: "sourceDetailsSettings"
            visible: root.store.has(root.source, "settings.v2")
            Layout.fillWidth: summaryGrid.columns === 2
            Layout.preferredWidth: Math.max(implicitWidth, DesktopTokens.px(68)); Layout.preferredHeight: DesktopTokens.px(68)
            font.pixelSize: DesktopTokens.captionSize
            text: qsTr("Tune"); themedGlyph: "sliders"; leftPadding: DesktopTokens.px(6); rightPadding: DesktopTokens.px(6)
            Accessible.name: qsTr("Service settings")
            onClicked: {
                root.library.closeDetails()
                AppController.navigate("settings-plugins")
            }
        }
    }
    Keys.onReturnPressed: if (playAction.enabled) root.library.play()
}
