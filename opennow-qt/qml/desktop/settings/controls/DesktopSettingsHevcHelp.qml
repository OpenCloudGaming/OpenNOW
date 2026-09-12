pragma ComponentBehavior: Bound
import QtQuick
import OpenNOW

Item {
    id: help
    required property bool runtimeReady
    required property var capabilities
    property string platformName: Qt.platform.os
    signal openStoreRequested(string url)

    objectName: "windowsHevcHelp"
    visible: platformName === "windows" && runtimeReady
        && (capabilities.videoBackends || []).some(backend => backend.backend === "d3d11"
            && backend.available && (backend.codecs || []).some(codec => codec.codec === "h265"
                && codec.available === false))
    implicitHeight: visible ? content.implicitHeight + DesktopTokens.px(32) : 0

    Column {
        id: content
        x: DesktopTokens.px(20)
        y: DesktopTokens.px(16)
        width: Math.max(0, parent.width - x * 2)
        spacing: DesktopTokens.px(12)

        Text {
            width: parent.width
            text: qsTr("H.265 / HEVC is unavailable")
            color: Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.px(14)
            font.weight: Font.Bold
            wrapMode: Text.WordWrap
        }
        Text {
            width: parent.width
            text: qsTr("If Microsoft's HEVC extension is missing, install it to use H.265. Try the free version first. If it is unavailable for you, Microsoft also offers a paid version.")
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.px(13)
            font.weight: Font.DemiBold
            wrapMode: Text.WordWrap
        }
        Repeater {
            model: [
                {productId: "9N4WGH0Z6VHQ", label: qsTr("Free HEVC extension")},
                {productId: "9NMZLZ57R3T7", label: qsTr("Paid HEVC extension")}
            ]
            delegate: Flow {
                id: offer
                required property var modelData
                width: content.width
                spacing: DesktopTokens.px(12)

                DesktopSettingsButton {
                    objectName: "hevcStore-" + offer.modelData.productId
                    text: offer.modelData.label
                    onClicked: help.openStoreRequested("https://apps.microsoft.com/detail/" + offer.modelData.productId.toLowerCase())
                }
                TextEdit {
                    objectName: "hevcCommand-" + offer.modelData.productId
                    width: Math.min(implicitWidth, content.width)
                    text: "winget install " + offer.modelData.productId
                    readOnly: true
                    selectByMouse: true
                    activeFocusOnTab: true
                    color: Theme.label
                    selectionColor: Theme.focus
                    selectedTextColor: Theme.focusText
                    font.family: Theme.monoFont
                    font.pixelSize: DesktopTokens.px(13)
                    font.weight: Font.Medium
                    topPadding: DesktopTokens.px(10)
                    wrapMode: TextEdit.Wrap
                    Accessible.name: text
                }
            }
        }
        Text {
            width: parent.width
            text: qsTr("Your GPU still needs hardware HEVC decoding support. Installing the extension will not make H.265 work on hardware that cannot decode it. Restart OpenNOW after installation to check again.")
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.px(13)
            font.weight: Font.DemiBold
            wrapMode: Text.WordWrap
        }
    }
}
