pragma Singleton
import QtQuick
import OpenNOW

QtObject {
    id: format

    readonly property color accent: Theme.focus
    readonly property color accentText: Theme.focusText
    readonly property color lobby: "#A78BFA"
    readonly property color amber: "#F5A623"
    readonly property color green: "#4ADE80"
    readonly property color mint: "#6EE7B7"
    readonly property color red: "#F87171"
    readonly property color offlineRing: "#6B7280"
    readonly property color faint: tint(DesktopTokens.text, 0.4)
    readonly property color rowFill: tint(DesktopTokens.text, 0.03)
    readonly property color fieldFill: Theme.lightMode ? Qt.rgba(1, 1, 1, 0.7) : Qt.rgba(0.043, 0.059, 0.102, 0.7)
    readonly property color popupFill: Theme.lightMode ? Qt.rgba(0.97, 0.98, 0.99, 0.96) : Qt.rgba(0.055, 0.063, 0.094, 0.94)
    readonly property color ring: Theme.lightMode ? "#FFFFFF" : "#0B0F1A"
    readonly property color avatarInk: "#0B0F1A"

    readonly property var palette: ["#F9A8D4", "#6EE7B7", "#FCD34D", "#FDBA74", "#C4B5FD", "#93C5FD", "#86EFAC", "#FDA4AF",
        "#A5F3FC", "#D6D3D1", "#FBCFE8", "#BEF264", "#7FD4FF", "#FDE68A", "#A7F3D0", "#F5D0FE", "#BAE6FD"]

    function tint(color, alpha) {
        return Qt.rgba(color.r, color.g, color.b, alpha)
    }
    function avatarColor(user) {
        const name = String(user && (user.username || user.id) || "")
        let hash = 0
        for (let index = 0; index < name.length; ++index)
            hash = (Math.imul(hash, 31) + name.charCodeAt(index)) >>> 0
        return palette[hash % palette.length]
    }
    function initial(user) {
        const name = ShellStore.friends.displayName(user)
        return name === "" ? "?" : name.charAt(0).toUpperCase()
    }
    function presence(friend) {
        return friend && friend.presence || {}
    }
    function inMyLobby(friend) {
        const lobby = presence(friend).lobby
        return Boolean(ShellStore.coop.isHost && lobby && lobby.id === ShellStore.coop.lobby.id)
    }
    function dotColor(friend) {
        const activity = ShellStore.friends.activity(friend)
        if (activity === "offline") return "transparent"
        if (activity === "in_lobby") return lobby
        if (activity === "playing") return accent
        if (activity === "in_queue" || activity === "idle") return amber
        return green
    }
    function gameTitle(game) {
        return game ? String(game.title || "") : ""
    }
    function gameArt(game) {
        return game ? DesktopTokens.decodeArtworkUrl(game.artUrl || game.imageUrl || game.heroImageUrl || "") : ""
    }
    function since(value, nowMs) {
        const start = Date.parse(String(value || ""))
        return isNaN(start) ? 0 : Math.max(0, nowMs - start)
    }
    function duration(ms) {
        const minutes = Math.max(1, Math.floor(ms / 60000))
        const hours = Math.floor(minutes / 60)
        if (hours === 0)
            return qsTr("%1 min").arg(minutes)
        return minutes % 60 === 0 ? qsTr("%1 h").arg(hours) : qsTr("%1 h %2 min").arg(hours).arg(minutes % 60)
    }
    function countdown(ms) {
        const seconds = Math.max(0, Math.ceil(ms / 1000))
        return "%1:%2".arg(Math.floor(seconds / 60)).arg(String(seconds % 60).padStart(2, "0"))
    }
    // {glyph, color, text, meta, strong}: the one-line activity of a friend row (C2 3).
    function activity(friend, nowMs) {
        const value = presence(friend)
        const game = gameTitle(value.game)
        switch (ShellStore.friends.activity(friend)) {
        case "in_lobby": {
            const lobbyInfo = value.lobby || {}
            return {glyph: "users", color: lobby, strong: true,
                text: inMyLobby(friend) ? qsTr("In your co-op lobby · %1").arg(game) : qsTr("In a co-op lobby · %1").arg(game),
                meta: lobbyInfo.max ? qsTr("· %1/%2 players").arg(lobbyInfo.size || 0).arg(lobbyInfo.max) : ""}
        }
        case "playing":
            return {glyph: "play", color: accent, strong: true, text: qsTr("Playing %1").arg(game),
                meta: value.since ? qsTr("· for %1").arg(duration(since(value.since, nowMs))) : ""}
        case "in_queue":
            return {glyph: "hourglass", color: amber, strong: true, text: qsTr("In queue · %1").arg(game),
                meta: value.queuePosition ? qsTr("· #%1 in line").arg(value.queuePosition) : ""}
        case "browsing_library":
            return {glyph: "library", color: DesktopTokens.textMuted, strong: false, text: qsTr("Browsing the library"), meta: ""}
        case "browsing_store":
            return {glyph: "store", color: DesktopTokens.textMuted, strong: false, text: qsTr("Browsing the store"), meta: ""}
        case "in_settings":
            return {glyph: "settings", color: DesktopTokens.textMuted, strong: false, text: qsTr("In settings"), meta: ""}
        case "idle":
            return {glyph: "moon", color: DesktopTokens.textMuted, strong: false, text: qsTr("Online · idle"),
                meta: value.since ? qsTr("· away %1").arg(duration(since(value.since, nowMs))) : ""}
        case "offline":
            return {glyph: "", color: DesktopTokens.textMuted, strong: false, text: lastSeen(value.lastSeenAt, nowMs), meta: ""}
        }
        return {glyph: "dot", color: DesktopTokens.textMuted, strong: false, text: qsTr("Online"), meta: ""}
    }
    // "just now", "5 min ago", "2 h ago", "yesterday" or "3 days ago".
    function agoText(value, nowMs) {
        const stamp = Date.parse(String(value || ""))
        if (isNaN(stamp))
            return ""
        const minutes = Math.floor(Math.max(0, nowMs - stamp) / 60000)
        if (minutes < 1)
            return qsTr("just now")
        if (minutes < 60)
            return qsTr("%1 min ago").arg(minutes)
        const hours = Math.floor(minutes / 60)
        if (hours < 24)
            return qsTr("%1 h ago").arg(hours)
        const days = Math.floor(hours / 24)
        return days === 1 ? qsTr("yesterday") : qsTr("%1 days ago").arg(days)
    }
    function lastSeen(value, nowMs) {
        const ago = agoText(value, nowMs)
        return ago === "" ? qsTr("Offline") : qsTr("Last seen %1").arg(ago)
    }
    // agoText within two days, then the weekday or the date, for request timestamps.
    function when(value, nowMs) {
        const stamp = Date.parse(String(value || ""))
        if (isNaN(stamp))
            return ""
        const hours = Math.max(0, nowMs - stamp) / 3600000
        if (hours < 48)
            return agoText(value, nowMs)
        const date = new Date(stamp)
        return hours < 7 * 24 ? Qt.locale().dayName(date.getDay(), Locale.LongFormat) : Qt.formatDate(date, "d MMM")
    }
    function ago(value, nowMs) {
        return duration(since(value, nowMs))
    }
}
