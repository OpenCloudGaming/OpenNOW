import QtQuick
import QtTest
import OpenNOW.ThemeTests

TestCase {
    name: "ThemePalette"

    SettingsState {
        id: settingsState
        coreClient: null
        appController: AppController
        i18n: null
        ready: false
        subscription: null
        nativeRuntimeReady: false
        nativeRuntimeCapabilities: ({})
        refreshAccountServices: function() {}
        refreshStreamerDetection: function() {}
        syncDiscordPresence: function() {}
        syncTelemetry: function() {}
        lastError: ""
    }

    function init() {
        ShellStore.previewThemePack = ""
        ShellStore.settings = {themePack:"nocturne", appTheme:"dark", appAccentColor:"green", themeAccentOverride:false}
        settingsState.settings = ShellStore.settings
    }

    function luminance(color) {
        const linear = value => value <= 0.04045 ? value / 12.92 : Math.pow((value + 0.055) / 1.055, 2.4)
        return linear(color.r) * 0.2126 + linear(color.g) * 0.7152 + linear(color.b) * 0.0722
    }

    function contrast(first, second) {
        const a = luminance(first)
        const b = luminance(second)
        return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)
    }

    function test_palette_data() {
        const rows = []
        for (const pack of Theme.packs) {
            for (const mode of ["light", "dark"]) {
                for (const accent of ["pack"].concat(Theme.accentChoices))
                    rows.push({tag:pack.id + "-" + mode + "-" + accent, pack:pack.id, mode:mode, accent:accent})
            }
        }
        return rows
    }

    function test_palette(data) {
        ShellStore.settings = {themePack:data.pack, appTheme:data.mode, appAccentColor:data.accent, themeAccentOverride:data.accent !== "pack"}
        compare(Theme.lightMode, data.mode === "light")
        verify(contrast(Theme.label, Theme.shell) >= 7)
        verify(contrast(Theme.focus, Theme.shell) >= 3)
        verify(contrast(Theme.focusText, Theme.focus) >= 4.5)
        compare(Theme.glass.r.toFixed(2), Theme.shell.r.toFixed(2))
        compare(Theme.focus, data.accent === "pack" ? Theme.packAccent : Theme.customAccent)
    }

    function test_autoFollowsSystem() {
        for (const pack of Theme.packs) {
            ShellStore.settings = {themePack:pack.id, appTheme:"auto"}
            compare(Theme.lightMode, Qt.styleHints.colorScheme === Qt.Light)
        }
    }

    function test_previewRestoresOverrides() {
        ShellStore.settings = {themePack:"aurora", appTheme:"dark", appAccentColor:"violet", themeAccentOverride:true}
        const originalShell = Theme.shell
        const originalAccent = Theme.focus
        ShellStore.previewThemePack = "bone"
        compare(Theme.lightMode, true)
        compare(Theme.accentOverridden, false)
        compare(Theme.focus, Theme.packAccent)
        ShellStore.previewThemePack = ""
        compare(Theme.lightMode, false)
        compare(Theme.shell, originalShell)
        compare(Theme.focus, originalAccent)
        ShellStore.settings = {themePack:"bone", appTheme:"light"}
        ShellStore.previewThemePack = "aurora"
        compare(Theme.lightMode, false)
        ShellStore.previewThemePack = ""
        compare(Theme.lightMode, true)
    }

    function test_optimisticSettingsMatchPersistedCoupling() {
        for (const pack of Theme.packs) {
            settingsState.applySetting("appAccentColor", "rose")
            compare(settingsState.settings.themeAccentOverride, true)
            settingsState.applySetting("themePack", pack.id)
            compare(settingsState.settings.themeAccentOverride, false)
            compare(settingsState.settings.appTheme, pack.category === "Light" ? "light" : "dark")
            settingsState.applySetting("appTheme", "auto")
            settingsState.applySetting("appAccentColor", "violet")
            compare(settingsState.settings.appTheme, "auto")
            compare(settingsState.settings.themePack, pack.id)
            compare(settingsState.settings.themeAccentOverride, true)
        }
    }
}
