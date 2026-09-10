pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

Rectangle {
    id: root
    objectName: "onboardingMacNetwork"
    property var controller: MacAwdl
    readonly property bool awdlEnabled: controller.state === MacAwdlController.Enabled
    readonly property bool awdlDisabled: controller.state === MacAwdlController.Disabled
    readonly property color accent: Theme.accentColor(awdlDisabled ? "green" : "amber")
    property bool restoring: false
    visible: controller.state !== MacAwdlController.Unsupported
    implicitHeight: content.implicitHeight + DesktopTokens.px(40)
    radius: DesktopTokens.px(16)
    color: Theme.lightMode ? Theme.glass : "#C70B0F1A"
    border.color: Qt.rgba(accent.r, accent.g, accent.b, 0.35)

    function refreshIfVisible() {
        if (visible && !controller.busy)
            controller.refresh()
    }

    Component.onCompleted: refreshIfVisible()
    onVisibleChanged: refreshIfVisible()

    Timer {
        interval: 3000
        repeat: true
        running: root.visible && !root.controller.busy
        onTriggered: root.controller.refresh()
    }

    component Copy: Text {
        width: parent.width
        color: Theme.textMuted
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.px(13)
        wrapMode: Text.Wrap
        lineHeightMode: Text.FixedHeight
        lineHeight: DesktopTokens.px(18)
    }

    component Action: Button {
        id: action
        property bool primary: false
        padding: DesktopTokens.px(14)
        implicitWidth: actionText.implicitWidth + leftPadding + rightPadding
        implicitHeight: DesktopTokens.px(38)
        focusPolicy: Qt.StrongFocus
        contentItem: Text {
            id: actionText
            text: action.text
            color: action.primary ? Theme.contrastText(root.accent) : Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.px(13)
            font.weight: Font.ExtraBold
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: DesktopTokens.px(9)
            color: action.primary ? root.accent : action.hovered ? DesktopTokens.raisedStrong : DesktopTokens.raised
            border.color: action.activeFocus ? Theme.focus : Theme.seam
            border.width: action.activeFocus ? 2 : 1
        }
        opacity: enabled ? 1 : 0.5
    }

    Column {
        id: content
        x: DesktopTokens.px(22)
        y: DesktopTokens.px(20)
        width: parent.width - DesktopTokens.px(44)
        spacing: DesktopTokens.px(14)

        RowLayout {
            width: parent.width
            spacing: DesktopTokens.px(12)
            DesktopSettingsIcon {
                Layout.preferredWidth: DesktopTokens.px(20)
                Layout.preferredHeight: DesktopTokens.px(20)
                glyph: "globe"
                ink: root.accent
            }
            Copy {
                Layout.fillWidth: true
                text: qsTr("Required macOS network setup")
                color: Theme.label
                font.pixelSize: DesktopTokens.px(20)
                font.weight: Font.Black
                lineHeight: DesktopTokens.px(24)
                Accessible.role: Accessible.Heading
            }
        }

        Copy {
            text: qsTr("Disable Apple Wireless Direct Link (AWDL) to finish setup on this Mac. AWDL shares the Wi-Fi radio with AirDrop and related features and can contribute to streaming latency spikes. Disabling it is not a guaranteed fix for stutter.")
        }

        GridLayout {
            width: parent.width
            columns: root.width >= DesktopTokens.px(760) ? 2 : 1
            columnSpacing: DesktopTokens.px(24)
            rowSpacing: DesktopTokens.px(12)
            Column {
                Layout.fillWidth: true
                spacing: DesktopTokens.px(4)
                Copy {
                    objectName: "onboardingAwdlStatus"
                    text: root.controller.busy ? qsTr("Waiting for macOS authorization…")
                        : root.awdlEnabled ? qsTr("AWDL is enabled")
                        : root.awdlDisabled ? qsTr("AWDL is down")
                        : root.controller.state === MacAwdlController.Unavailable ? qsTr("The AWDL interface was not found")
                        : qsTr("AWDL status is unavailable")
                    color: root.accent
                    font.weight: Font.ExtraBold
                    Accessible.role: Accessible.StaticText
                }
                Copy {
                    text: root.controller.state === MacAwdlController.Unavailable
                        ? qsTr("No AWDL interface is present, so no network change is required.")
                        : qsTr("macOS may re-enable AWDL after sleep or restart. OpenNOW checks its status here and again before saving setup completion.")
                    font.pixelSize: DesktopTokens.px(12)
                    lineHeight: DesktopTokens.px(16)
                }
            }
            RowLayout {
                Layout.alignment: Qt.AlignLeft
                spacing: DesktopTokens.px(10)
                Action {
                    objectName: "onboardingAwdlChange"
                    primary: true
                    text: root.awdlDisabled ? qsTr("Re-enable AWDL…") : qsTr("Disable AWDL…")
                    enabled: !root.controller.busy && (root.awdlEnabled || root.awdlDisabled)
                    onClicked: {
                        root.restoring = root.awdlDisabled
                        confirmation.open()
                    }
                }
                Action {
                    objectName: "onboardingAwdlRefresh"
                    text: qsTr("Refresh")
                    enabled: !root.controller.busy
                    onClicked: root.controller.refresh()
                }
            }
        }

        Copy {
            text: qsTr("Disabling AWDL affects all users on this Mac and can interrupt AirDrop, AirPlay, Sidecar and other Continuity features. It requires administrator authorization. Re-enabling it restores access to those features but blocks setup completion until AWDL is disabled again.")
            font.pixelSize: DesktopTokens.px(12)
            lineHeight: DesktopTokens.px(16)
        }

        Copy {
            objectName: "onboardingAwdlError"
            visible: root.controller.error !== ""
            text: root.controller.error
            color: Theme.accentColor("coral")
            Accessible.role: Accessible.AlertMessage
        }
    }

    Dialog {
        id: confirmation
        objectName: "onboardingAwdlConfirmation"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(DesktopTokens.px(520), root.width - DesktopTokens.px(32))
        padding: DesktopTokens.px(22)
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape
        title: root.restoring ? qsTr("Re-enable Apple Wireless Direct Link?") : qsTr("Disable Apple Wireless Direct Link?")
        header: Copy {
            text: confirmation.title
            padding: DesktopTokens.px(22)
            bottomPadding: 0
            color: Theme.label
            font.pixelSize: DesktopTokens.px(20)
            font.weight: Font.Black
            lineHeight: DesktopTokens.px(26)
        }
        background: Rectangle { radius: DesktopTokens.px(16); color: Theme.shell; border.color: Theme.seam }
        contentItem: Copy {
            text: root.restoring
                ? qsTr("macOS will ask for administrator authorization to bring awdl0 up. This lets AirDrop and related Apple features use AWDL again.")
                : qsTr("macOS will ask for administrator authorization to bring awdl0 down for all users. AirDrop, AirPlay, Sidecar and other Continuity features may stop working. AWDL must be down to finish setup. OpenNOW will not keep it disabled in the background.")
        }
        footer: DialogButtonBox {
            padding: DesktopTokens.px(16)
            background: Item {}
            Action {
                objectName: "onboardingAwdlCancel"
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
            Action {
                objectName: "onboardingAwdlConfirm"
                primary: true
                text: root.restoring ? qsTr("Re-enable AWDL") : qsTr("Disable AWDL")
                enabled: !root.controller.busy
                DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
            }
        }
        onAccepted: {
            if (root.restoring)
                root.controller.enable()
            else
                root.controller.disable()
        }
    }
}
