import QtQuick
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "SessionStartingContrast"
    width: 1000
    height: 700
    when: windowShown
    visible: true

    DesktopSessionStarting {
        id: screen
        anchors.fill: parent
    }

    DesktopButton {
        id: shellButton
        visible: false
        text: "Shell button"
    }

    function init() {
        ShellStore.previewThemePack = ""
        ShellStore.streamState = "preparing"
        AppController.route = "inserting"
        AppController.overlay = ""
    }

    function descendants(item) {
        let result = []
        for (const child of item.children || [])
            result = result.concat([child], descendants(child))
        return result
    }

    function textItem(text) {
        const item = descendants(screen).find(item => item.text === text && item.color !== undefined)
        verify(item !== undefined, "Missing text: " + text)
        return item
    }

    function contrastOnDark(color) {
        const background = Qt.color("#04060A")
        const linear = value => value <= 0.04045 ? value / 12.92 : Math.pow((value + 0.055) / 1.055, 2.4)
        const luminance = value => linear(value.r) * 0.2126 + linear(value.g) * 0.7152 + linear(value.b) * 0.0722
        const composited = Qt.rgba(color.r * color.a + background.r * (1 - color.a),
                                  color.g * color.a + background.g * (1 - color.a),
                                  color.b * color.a + background.b * (1 - color.a), 1)
        return (luminance(composited) + 0.05) / (luminance(background) + 0.05)
    }

    function test_launchPalette_data() {
        const rows = []
        for (const pack of Theme.packs) {
            for (const mode of ["light", "dark", "auto"]) {
                for (const accent of ["pack"].concat(Theme.accentChoices))
                    rows.push({tag: pack.id + "-" + mode + "-" + accent, pack: pack.id, mode: mode, accent: accent})
            }
        }
        return rows
    }

    function test_launchPalette(data) {
        ShellStore.settings = {themePack: data.pack, appTheme: data.mode,
            appAccentColor: data.accent, themeAccentOverride: data.accent !== "pack"}
        compare(textItem("Dead by Daylight").color, Theme.mediaForeground)
        compare(textItem("Queue position 21").color, Theme.mediaForeground)
        compare(textItem("OpenNOW").color, Theme.mediaForeground)
        compare(textItem(screen.detailText).color.toString(), Theme.mediaMuted.toString())
        compare(textItem("STARTING SESSION").color, Theme.mediaAccent)
        compare(textItem("Cancel session").color, Theme.mediaForeground)
        for (const text of ["Dead by Daylight", "Queue position 21", "OpenNOW",
                            screen.detailText, "STARTING SESSION", "Cancel session"])
            verify(contrastOnDark(textItem(text).color) >= 4.5, "Insufficient contrast: " + text)

        const shortcut = descendants(screen).find(item => item.shortcut === "Esc")
        verify(shortcut !== undefined)
        compare(shortcut.ink, Theme.mediaMuted)
        verify(contrastOnDark(shortcut.ink) >= 4.5)

        const cancel = descendants(screen).find(item => item.text === "Cancel session" && item.onMediaBackground !== undefined)
        cancel.forceActiveFocus()
        verify(descendants(cancel.background).some(item => item.visible && item.border
            && item.border.width === 2 && item.border.color.toString() === Theme.mediaAccent.toString()))

        const shellText = descendants(shellButton).find(item => item.text === shellButton.text && item.color !== undefined)
        compare(shellText.color, DesktopTokens.textHigh)
    }

    function test_failureAndReconnect() {
        ShellStore.settings = {themePack: "nocturne", appTheme: "light"}
        ShellStore.streamState = "failed"
        compare(textItem("Session could not start").color, DesktopTokens.danger)
        compare(textItem("Try again").color, Qt.color("#0A0D14"))
        compare(textItem("Cancel session").color, Theme.mediaForeground)
        ShellStore.streamState = "reconnecting"
        compare(textItem("Reconnecting to your session").color, Theme.mediaForeground)
        ShellStore.streamState = "stopping"
        compare(textItem("Closing your session").color, Theme.mediaForeground)
    }
}
