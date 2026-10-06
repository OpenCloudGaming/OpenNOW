import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import OpenNOW

Column {
    id: page
    objectName: "desktopPluginsSettings"
    required property real availableWidth
    readonly property var store: ShellStore.pluginOwnerState
    property string expandedId: ""
    property string removeId: ""
    readonly property var removePlugin: store.pluginById(removeId)

    width: page.availableWidth
    spacing: DesktopTokens.px(12)

    function stateLabel(plugin) {
        if (!plugin)
            return ""
        if (page.store.busyId === plugin.id)
            return qsTr("Working…")
        if (plugin.state === "ready")
            return qsTr("Running")
        if (plugin.state === "starting")
            return qsTr("Starting…")
        if (plugin.state === "failed")
            return qsTr("Failed")
        return qsTr("Off")
    }

    function stateColor(plugin) {
        if (plugin && plugin.state === "failed")
            return Theme.coral
        if (plugin && plugin.state === "ready")
            return Theme.mint
        return Theme.textMuted
    }

    function summary(plugin) {
        return [qsTr("Version %1").arg(String(plugin.version || "")), String(plugin.publisher || ""),
            plugin.builtin === true ? qsTr("Built in") : qsTr("Community")].filter(part => part !== "").join(" · ")
    }

    function trustText(plugin) {
        return plugin.trust === "builtin"
            ? qsTr("Built into OpenNOW.")
            : qsTr("Unsigned native code. The publisher name is self-declared and is not verified.")
    }

    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        visible: !page.store.available
        DesktopSettingsRow {
            objectName: "pluginsUnavailable"
            width: parent.width; paperStyle: true; glyph: "puzzle"; showDivider: false
            title: qsTr("Plugins are unavailable")
            description: ShellStore.ready
                ? qsTr("This OpenNOW core does not support plugins.")
                : qsTr("Plugins appear when OpenNOW finishes starting.")
        }
    }

    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        visible: page.store.available
        DesktopSettingsSection {
            text: qsTr("INSTALL")
            description: qsTr("Plugins are native programs from their publishers, not from OpenNOW. Only install plugins you trust.")
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "folder"
            title: qsTr("Install from file")
            description: page.store.inspecting ? qsTr("Checking the package…")
                : qsTr("Choose a local .opennow-plugin package. To install a newer version, remove the current one first.")
            showDivider: page.store.installError !== ""
            DesktopSettingsButton {
                objectName: "pluginInstallButton"
                text: qsTr("Choose file…")
                enabled: ShellStore.ready && !page.store.inspecting && !page.store.committing
                onClicked: packageDialog.open()
            }
        }
        Text {
            objectName: "pluginInstallError"
            visible: page.store.installError !== ""
            width: parent.width
            leftPadding: DesktopTokens.settingsInset; rightPadding: DesktopTokens.settingsInset
            topPadding: DesktopTokens.px(10); bottomPadding: DesktopTokens.px(14)
            text: page.store.installError
            textFormat: Text.PlainText
            color: Theme.coral
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.captionSize
            font.weight: Font.DemiBold
            wrapMode: Text.WordWrap
        }
    }

    DesktopSettingsPanel {
        objectName: "pluginsInstalledPanel"
        width: parent.width; paperStyle: true
        visible: page.store.available
        DesktopSettingsSection {
            text: qsTr("INSTALLED")
            description: page.store.loading ? qsTr("Loading plugins…") : ""
        }
        Text {
            objectName: "pluginsError"
            visible: page.store.error !== ""
            width: parent.width
            leftPadding: DesktopTokens.settingsInset; rightPadding: DesktopTokens.settingsInset
            bottomPadding: DesktopTokens.px(12)
            text: page.store.error
            textFormat: Text.PlainText
            color: Theme.coral
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.captionSize
            font.weight: Font.DemiBold
            wrapMode: Text.WordWrap
        }
        Repeater {
            model: page.store.plugins
            delegate: Column {
                id: pluginEntry
                required property var modelData
                required property int index
                readonly property bool expanded: page.expandedId === modelData.id
                width: parent.width
                DesktopSettingsRow {
                    objectName: "pluginRow-" + pluginEntry.modelData.id
                    width: parent.width; paperStyle: true
                    textFormat: Text.PlainText
                    leadingLetter: String(pluginEntry.modelData.name || "?").charAt(0).toUpperCase()
                    title: String(pluginEntry.modelData.name || pluginEntry.modelData.id)
                    description: page.summary(pluginEntry.modelData)
                    expandable: true
                    expanded: pluginEntry.expanded
                    showDivider: pluginEntry.index < page.store.plugins.length - 1 || pluginEntry.expanded
                    onExpansionRequested: page.expandedId = pluginEntry.expanded ? "" : pluginEntry.modelData.id
                    Text {
                        objectName: "pluginState-" + pluginEntry.modelData.id
                        anchors.verticalCenter: parent.verticalCenter
                        text: page.stateLabel(pluginEntry.modelData)
                        color: page.stateColor(pluginEntry.modelData)
                        font.family: Theme.monoFont
                        font.pixelSize: DesktopTokens.monoSize
                        font.weight: Font.Bold
                    }
                    DesktopSettingsToggle {
                        objectName: "pluginToggle-" + pluginEntry.modelData.id
                        Accessible.name: qsTr("Turn on %1").arg(String(pluginEntry.modelData.name || ""))
                        checked: pluginEntry.modelData.enabled === true
                        enabled: pluginEntry.modelData.required !== true && page.store.busyId === "" && ShellStore.ready
                        onValueChangedByUser: value => page.store.setEnabled(pluginEntry.modelData.id, value)
                    }
                }
                DesktopSettingsDisclosure {
                    width: parent.width
                    expanded: pluginEntry.expanded
                    sourceComponent: Column {
                        width: page.availableWidth
                        leftPadding: DesktopTokens.settingsLabelInset
                        rightPadding: DesktopTokens.settingsInset
                        bottomPadding: DesktopTokens.settingsInset
                        spacing: DesktopTokens.px(8)
                        Text {
                            width: parent.width - parent.leftPadding - parent.rightPadding
                            visible: text !== ""
                            text: String(pluginEntry.modelData.description || "")
                            textFormat: Text.PlainText
                            color: Theme.label
                            font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                            wrapMode: Text.WordWrap
                        }
                        Text {
                            width: parent.width - parent.leftPadding - parent.rightPadding
                            text: qsTr("ID: %1").arg(String(pluginEntry.modelData.id))
                                + "\n" + qsTr("Capabilities: %1").arg((pluginEntry.modelData.capabilities || []).join(", ") || qsTr("None"))
                                + "\n" + page.trustText(pluginEntry.modelData)
                                + (pluginEntry.modelData.required === true ? "\n" + qsTr("Required by OpenNOW. It can't be turned off or removed.") : "")
                            textFormat: Text.PlainText
                            color: Theme.textMuted
                            font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                            wrapMode: Text.WordWrap
                        }
                        Text {
                            width: parent.width - parent.leftPadding - parent.rightPadding
                            visible: !!pluginEntry.modelData.lastError
                            text: pluginEntry.modelData.lastError
                                ? String(pluginEntry.modelData.lastError.message || pluginEntry.modelData.lastError.code || "") : ""
                            textFormat: Text.PlainText
                            color: Theme.coral
                            font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.DemiBold
                            wrapMode: Text.WordWrap
                        }
                        Row {
                            spacing: DesktopTokens.px(10)
                            topPadding: DesktopTokens.px(4)
                            DesktopSettingsButton {
                                objectName: "pluginBrowse-" + pluginEntry.modelData.id
                                text: qsTr("Browse catalog")
                                enabled: page.store.catalogAvailable && page.store.catalogReady(pluginEntry.modelData)
                                onClicked: page.store.openPreview(pluginEntry.modelData.id)
                            }
                            DesktopSettingsButton {
                                objectName: "pluginRemove-" + pluginEntry.modelData.id
                                visible: pluginEntry.modelData.builtin !== true && pluginEntry.modelData.required !== true
                                text: qsTr("Remove…")
                                danger: true
                                enabled: page.store.busyId === "" && ShellStore.ready
                                onClicked: {
                                    page.removeId = pluginEntry.modelData.id
                                    removeConfirmation.open()
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    DesktopSettingsPanel {
        objectName: "pluginCatalogPreview"
        width: parent.width; paperStyle: true
        visible: page.store.previewSourceId !== ""
        DesktopSettingsSection {
            text: qsTr("CATALOG PREVIEW")
            description: qsTr("Read-only titles from %1. They can't be played from OpenNOW.")
                .arg(page.store.previewPlugin ? String(page.store.previewPlugin.name || page.store.previewSourceId) : page.store.previewSourceId)
            DesktopSettingsButton {
                objectName: "pluginPreviewClose"
                text: qsTr("Close")
                onClicked: page.store.closePreview()
            }
        }
        Item {
            width: parent.width
            height: previewSearch.height + DesktopTokens.px(12)
            DesktopSettingsField {
                id: previewSearch
                objectName: "pluginPreviewSearch"
                x: DesktopTokens.settingsInset
                width: Math.min(DesktopTokens.px(420), parent.width - 2 * DesktopTokens.settingsInset)
                placeholderText: qsTr("Search this catalog")
                maximumLength: 512
                Accessible.name: qsTr("Search this catalog")
                onAccepted: page.store.searchPreview(text)
            }
            DesktopSettingsButton {
                anchors.left: previewSearch.right; anchors.leftMargin: DesktopTokens.px(10)
                anchors.verticalCenter: previewSearch.verticalCenter
                text: qsTr("Search")
                onClicked: page.store.searchPreview(previewSearch.text)
            }
        }
        Repeater {
            model: page.store.previewItems
            delegate: Item {
                required property var modelData
                width: parent.width
                height: DesktopTokens.px(44)
                Text {
                    x: DesktopTokens.settingsInset
                    width: parent.width - 2 * DesktopTokens.settingsInset - localIdText.width - DesktopTokens.px(16)
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData.title
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: Theme.label
                    font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize; font.weight: Font.Bold
                }
                Text {
                    id: localIdText
                    anchors.right: parent.right; anchors.rightMargin: DesktopTokens.settingsInset
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, DesktopTokens.px(220))
                    text: modelData.localId
                    textFormat: Text.PlainText
                    elide: Text.ElideMiddle
                    color: Theme.textMuted
                    font.family: Theme.monoFont; font.pixelSize: DesktopTokens.monoSize
                }
                Rectangle { anchors.bottom: parent.bottom; x: DesktopTokens.settingsInset; width: parent.width - 2 * x; height: 1; color: DesktopTokens.seamSoft }
            }
        }
        Item {
            width: parent.width
            height: previewFooter.implicitHeight + DesktopTokens.px(24)
            Text {
                id: previewFooter
                objectName: "pluginPreviewStatus"
                x: DesktopTokens.settingsInset
                y: DesktopTokens.px(12)
                width: parent.width - 2 * DesktopTokens.settingsInset - loadMore.width - DesktopTokens.px(16)
                text: page.store.previewError !== "" ? page.store.previewError
                    : page.store.previewWaiting ? qsTr("The plugin is restarting. Titles reload when it's ready.")
                    : page.store.previewLoading ? qsTr("Loading titles…")
                    : page.store.previewSummary
                textFormat: Text.PlainText
                color: page.store.previewError !== "" ? Theme.coral : Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                wrapMode: Text.WordWrap
            }
            DesktopSettingsButton {
                id: loadMore
                objectName: "pluginPreviewMore"
                anchors.right: parent.right; anchors.rightMargin: DesktopTokens.settingsInset
                y: DesktopTokens.px(6)
                visible: page.store.previewNextCursor !== null
                enabled: !page.store.previewLoading
                text: qsTr("Load more")
                onClicked: page.store.loadMorePreview()
            }
        }
    }

    FileDialog {
        id: packageDialog
        title: qsTr("Choose a plugin package")
        fileMode: FileDialog.OpenFile
        nameFilters: [qsTr("OpenNOW plugins (*.opennow-plugin)"), qsTr("All files (*)")]
        onAccepted: page.store.inspectPackage(selectedFile)
    }

    Connections {
        target: page.store
        function onInspectionChanged() {
            if (page.store.inspection !== null)
                installConsent.open()
            else if (installConsent.opened)
                installConsent.close()
        }
    }

    component DialogTitle: Text {
        padding: DesktopTokens.px(22)
        bottomPadding: 0
        color: Theme.label
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.px(20)
        font.weight: Font.ExtraBold
        wrapMode: Text.WordWrap
    }

    Dialog {
        id: installConsent
        objectName: "pluginInstallConsent"
        readonly property var candidate: page.store.inspection ? page.store.inspection.plugin || ({}) : ({})
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(DesktopTokens.px(560), parent.width - DesktopTokens.px(32))
        contentWidth: width - leftPadding - rightPadding
        padding: DesktopTokens.px(22)
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape
        title: qsTr("Install this plugin?")
        implicitHeight: header.implicitHeight + consentBody.implicitHeight + footer.implicitHeight
            + topPadding + bottomPadding
        header: DialogTitle { width: installConsent.width; text: installConsent.title }
        background: Rectangle { radius: DesktopTokens.px(16); color: Theme.shell; border.color: Theme.seam }
        contentItem: Column {
            id: consentBody
            spacing: DesktopTokens.px(12)
            Text {
                objectName: "pluginConsentDetails"
                width: installConsent.contentWidth
                text: String(installConsent.candidate.name || "") + "\n"
                    + qsTr("Version %1").arg(String(installConsent.candidate.version || "")) + "\n"
                    + qsTr("Publisher: %1 (self-declared, not verified)").arg(String(installConsent.candidate.publisher || "")) + "\n"
                    + qsTr("ID: %1").arg(String(installConsent.candidate.id || "")) + "\n"
                    + qsTr("Package SHA-256: %1").arg(String(page.store.inspection ? page.store.inspection.packageSha256 || "" : ""))
                textFormat: Text.PlainText
                color: Theme.label
                font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.captionSize
                wrapMode: Text.WrapAnywhere
            }
            Rectangle {
                width: installConsent.contentWidth
                height: warningText.implicitHeight + DesktopTokens.px(24)
                radius: DesktopTokens.px(12)
                color: Qt.rgba(Theme.coral.r, Theme.coral.g, Theme.coral.b, 0.12)
                border.color: Theme.coral
                Text {
                    id: warningText
                    objectName: "pluginConsentWarning"
                    x: DesktopTokens.px(12); y: DesktopTokens.px(12)
                    width: parent.width - DesktopTokens.px(24)
                    text: qsTr("This plugin is native code. It runs as your user and is not sandboxed. It can read your files, access credentials available to your user, and use the network. The publisher name is self-declared, and the package hash does not prove who made it.")
                    color: Theme.label
                    font.family: Theme.bodyFont
                    font.pixelSize: DesktopTokens.captionSize
                    font.weight: Font.DemiBold
                    wrapMode: Text.WordWrap
                }
            }
            Text {
                width: installConsent.contentWidth
                text: page.store.inspection && page.store.inspection.stale === true
                    ? qsTr("Your plugins changed after this package was checked. Cancel and choose the file again.")
                    : qsTr("The plugin is installed turned off. Its code runs for the first time when you turn it on.")
                color: page.store.inspection && page.store.inspection.stale === true ? Theme.coral : Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.captionSize
                wrapMode: Text.WordWrap
            }
        }
        footer: DialogButtonBox {
            padding: DesktopTokens.px(16)
            implicitHeight: DesktopTokens.controlHeight + topPadding + bottomPadding
            background: Item {}
            DesktopSettingsButton {
                id: consentCancel
                objectName: "pluginConsentCancel"
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
            DesktopSettingsButton {
                objectName: "pluginConsentInstall"
                text: qsTr("Install")
                primary: true
                enabled: page.store.inspection !== null && page.store.inspection.stale !== true && !page.store.committing
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
        }
        onOpened: consentCancel.forceActiveFocus()
        onAccepted: page.store.commitInstall()
        onRejected: page.store.cancelInstall()
    }

    Dialog {
        id: removeConfirmation
        objectName: "pluginRemoveConfirmation"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(DesktopTokens.px(520), parent.width - DesktopTokens.px(32))
        contentWidth: width - leftPadding - rightPadding
        padding: DesktopTokens.px(22)
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape
        title: qsTr("Remove this plugin?")
        implicitHeight: header.implicitHeight + removeBody.implicitHeight + footer.implicitHeight
            + topPadding + bottomPadding
        header: DialogTitle { width: removeConfirmation.width; text: removeConfirmation.title }
        background: Rectangle { radius: DesktopTokens.px(16); color: Theme.shell; border.color: Theme.seam }
        contentItem: Text {
            id: removeBody
            width: removeConfirmation.contentWidth
            text: qsTr("OpenNOW removes %1 and the plugin's data from this PC. To use it again, install its package file again.")
                .arg(page.removePlugin ? String(page.removePlugin.name || page.removeId) : page.removeId)
            textFormat: Text.PlainText
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.bodySize
            wrapMode: Text.WordWrap
        }
        footer: DialogButtonBox {
            padding: DesktopTokens.px(16)
            implicitHeight: DesktopTokens.controlHeight + topPadding + bottomPadding
            background: Item {}
            DesktopSettingsButton {
                id: removeCancel
                objectName: "pluginRemoveCancel"
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
            DesktopSettingsButton {
                objectName: "pluginRemoveConfirm"
                text: qsTr("Remove plugin")
                danger: true
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
        }
        onOpened: removeCancel.forceActiveFocus()
        onAccepted: {
            page.store.uninstall(page.removeId)
            if (page.expandedId === page.removeId)
                page.expandedId = ""
            page.removeId = ""
        }
        onRejected: page.removeId = ""
    }
}
