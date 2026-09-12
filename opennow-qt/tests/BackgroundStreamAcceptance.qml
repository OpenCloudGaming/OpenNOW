import QtQuick
import OpenNOW

QtObject {
    id: fixture
    property var savedSettings: ({})
    property var commands: []
    property int reminders: 0
    property BackgroundStreamState policy: BackgroundStreamState {
        settings: fixture.savedSettings
        applicationActive: true
        streaming: false
        nativeRuntimeReady: false
        sendNativeCommand: function(type, params, operation) {
            fixture.commands.push({type: type, muted: params.muted, operation: operation})
            return "background-test"
        }
        onReminderRequested: fixture.reminders += 1
    }
    property Component pageComponent: Component {
        DesktopSettingsAudioPage {
            availableWidth: 960
            settingsScreen: fixture
        }
    }

    function valueSetting(key, fallback) {
        return savedSettings[key] === undefined ? fallback : savedSettings[key]
    }
    function setSetting(key, value) {
        savedSettings = Object.assign({}, savedSettings, {[key]: value})
    }
    function check(ok, message) {
        if (!ok) throw new Error("Background stream: " + message)
    }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }
    function lastMuted() { return commands[commands.length - 1].muted }

    function run(parent) {
        const page = pageComponent.createObject(parent)
        check(page !== null, "Audio settings must load")
        const mute = find(page, "muteWhenOutOfFocusToggle")
        const reminder = find(page, "backgroundStreamReminderToggle")
        check(!mute.checked && !reminder.checked, "both settings must default off")
        policy.applicationActive = false
        check(commands.length === 0 && !policy.reminderTimer.running, "disabled policy must stay idle")
        mute.clicked()
        check(savedSettings.muteWhenOutOfFocus === true && mute.checked, "mute toggle must persist")
        check(policy.audioMuted && commands.length === 0, "remember mute before runtime starts")
        policy.nativeRuntimeReady = true
        check(lastMuted() && commands[0].type === "setAudioMuted", "apply mute on runtime readiness")
        policy.streaming = true
        check(!policy.reminderTimer.running, "mute must not enable reminders")
        policy.applicationActive = true
        check(!lastMuted(), "restore audio on focus gain")
        const foregroundCommands = commands.length
        check(AppController.showOverlay("desktop-stream-menu"), "local stream menu must open")
        AppController.showOverlay("")
        check(commands.length === foregroundCommands, "local overlays must not change playback mute")
        policy.applicationActive = false
        check(lastMuted(), "mute again on focus loss")
        mute.clicked()
        check(!lastMuted() && !mute.checked, "disabling while backgrounded must unmute immediately")
        reminder.clicked()
        check(reminder.checked && policy.reminderTimer.running, "reminder is independently selectable")
        check(policy.reminderTimer.interval === 300000 && policy.reminderTimer.repeat,
              "reminders must repeat every five minutes")
        policy.reminderTimer.triggered()
        check(reminders === 1, "background stream must request attention")
        policy.applicationActive = true
        policy.reminderTimer.triggered()
        check(!policy.reminderTimer.running && reminders === 1, "focus gain must stop reminders")
        policy.applicationActive = false
        check(policy.reminderTimer.running, "new background period must start a fresh timer")
        policy.streaming = false
        policy.reminderTimer.triggered()
        check(!policy.reminderTimer.running && reminders === 1, "ended stream must stop reminders")
        policy.streaming = true
        reminder.clicked()
        check(!policy.reminderTimer.running, "disabling reminders must stop the timer")
        mute.clicked()
        policy.nativeRuntimeReady = false
        const beforeRestart = commands.length
        policy.applicationActive = true
        check(commands.length === beforeRestart, "offline focus changes must not send commands")
        policy.nativeRuntimeReady = true
        check(!lastMuted() && commands.length === beforeRestart + 1, "runtime restart must restore current focus policy")
        page.destroy()
        return true
    }
}
