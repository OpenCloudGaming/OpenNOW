import QtQuick
import QtQuick.Controls
import OpenNOW

Column {
    id: root
    objectName: "desktopSourceSettings-" + sourceId
    required property string sourceId
    required property real availableWidth
    readonly property var store: ShellStore.sourceOwnerState
    readonly property var source: store.sourceById(sourceId)
    readonly property var authState: store.authState(sourceId)
    readonly property var view: store.settingsViews[sourceId] || null
    readonly property string error: store.settingsErrors[sourceId] || ""
    readonly property var streamView: store.streamSettingsViews[sourceId] || null
    readonly property string streamError: store.streamSettingsErrors[sourceId] || ""
    width: availableWidth
    visible: source !== null && source.enabled === true

    Component.onCompleted: store.loadSettings(sourceId)
    onAuthStateChanged: store.loadSettings(sourceId)

    DesktopSettingsRow {
        objectName: "desktopSourceAccount-" + root.sourceId
        width: parent.width; paperStyle: true
        visible: root.authState !== null && root.authState.state !== "not-required"
        textFormat: Text.PlainText
        glyph: "person"
        title: root.authState && root.authState.state === "signed-in"
            ? String(root.authState.account && root.authState.account.name || "") : qsTr("Not signed in")
        description: root.authState && root.authState.state === "signed-in"
            ? qsTr("Account for this service") : qsTr("Sign in from Home after choosing this service.")
        DesktopSettingsButton {
            visible: root.authState !== null && root.authState.state === "signed-in"
            text: qsTr("Sign out")
            onClicked: root.store.signOut(root.sourceId)
        }
    }

    Repeater {
        model: root.view ? root.view.settings : []
        delegate: DesktopSourceSettingRow {
            width: root.width
            domain: "provider"
            sourceId: root.sourceId
        }
    }

    Text {
        visible: root.error !== ""
        width: parent.width
        leftPadding: DesktopTokens.settingsInset; rightPadding: DesktopTokens.settingsInset
        topPadding: DesktopTokens.px(6); bottomPadding: DesktopTokens.px(10)
        text: root.error
        textFormat: Text.PlainText
        color: Theme.coral; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
        wrapMode: Text.WordWrap
    }

    Text {
        objectName: "desktopSourceStreamSettingsTitle-" + root.sourceId
        visible: root.streamView !== null && root.streamView.settings.length > 0
        width: parent.width
        leftPadding: DesktopTokens.settingsInset; rightPadding: DesktopTokens.settingsInset
        topPadding: DesktopTokens.px(12); bottomPadding: DesktopTokens.px(4)
        text: qsTr("Stream quality for this service")
        color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
        font.weight: Font.DemiBold
    }

    Repeater {
        model: root.streamView ? root.streamView.settings : []
        delegate: DesktopSourceSettingRow {
            width: root.width
            domain: "stream"
            sourceId: root.sourceId
        }
    }

    Text {
        visible: root.streamError !== ""
        width: parent.width
        leftPadding: DesktopTokens.settingsInset; rightPadding: DesktopTokens.settingsInset
        topPadding: DesktopTokens.px(6); bottomPadding: DesktopTokens.px(10)
        text: root.streamError
        textFormat: Text.PlainText
        color: Theme.coral; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
        wrapMode: Text.WordWrap
    }
}
