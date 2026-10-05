import QtQuick
import OpenNOW

QtObject {
    property bool recordingSection: false

    function check(ok, message) {
        if (!ok)
            throw new Error("Console spacing: " + message)
    }

    function find(item, name) {
        if (item.objectName === name)
            return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found)
                return found
        }
        return null
    }

    function near(actual, expected, label) {
        check(Math.abs(actual - expected) <= 0.5,
              label + ": expected " + expected + ", got " + actual)
    }

    function bounds(item, screen, expected, label) {
        check(item !== null, label + " exists")
        const position = item.mapToItem(screen, 0, 0)
        near(position.x, expected.x, label + " left")
        near(position.y, expected.y, label + " top")
        near(item.width, expected.width, label + " width")
        if (expected.height !== undefined)
            near(item.height, expected.height, label + " height")
    }

    function chromeBounds(screen) {
        const profile = find(screen, "consoleProfilePanel")
        const status = find(screen, "consoleStatusPanel")
        const title = find(screen, "consoleChromeTitle")
        const nav = find(screen, "consoleNavPill")
        check(profile && status && title && nav, "shared chrome geometry targets exist")
        const profilePosition = profile.mapToItem(screen, 0, 0)
        const statusPosition = status.mapToItem(screen, 0, 0)
        const titlePosition = title.mapToItem(screen, 0, 0)
        const navPosition = nav.mapToItem(screen, 0, 0)
        near(profilePosition.x, 64, "profile left margin")
        near(profilePosition.y, 36, "profile top margin")
        near(profile.height, 60, "profile height")
        near(statusPosition.y, 36, "status top margin")
        near(statusPosition.x + status.width, screen.width - 64, "status right margin")
        near(status.height, 60, "status height")
        near(titlePosition.x + title.width / 2,
             (profilePosition.x + profile.width + statusPosition.x) / 2,
             "title centered between populated pills")
        near(titlePosition.y + title.height / 2, 66, "title vertical center")
        near(navPosition.x + nav.width / 2, screen.width / 2, "navigation horizontal center")
        near(navPosition.y + nav.height, screen.height - 60, "navigation bottom margin")
        near(nav.height, 72, "navigation height")
    }

    function run(parent) {
        const loader = find(parent, "mainRouteLoader")
        check(loader !== null && loader.item !== null, "the destination is loaded")
        const screen = loader.item
        if (recordingSection) {
            check(screen.objectName === "consoleSettingsScreen", "Recording starts from Settings")
            screen.selectedSection = 7
            find(screen, "consoleSettingsList").forceLayout()
        }
        chromeBounds(screen)
        if (AppController.route === "home") {
            bounds(find(screen, "consoleHomeTilePanel"), screen,
                   {x:(screen.width - 1586) / 2, y:224, width:1586, height:626}, "Home tile panel")
            return true
        }
        if (AppController.route === "library") {
            bounds(find(screen, "consoleLibrarySearchField"), screen,
                   {x:96, y:126, width:440, height:60}, "library search")
            bounds(find(screen, "consoleLibraryDetailPanel"), screen,
                   {x:screen.width - 584, y:228, width:488, height:screen.height - 400}, "library detail panel")
            return true
        }
        check(screen.objectName === "consoleSettingsScreen", "the console settings screen is rendered")
        const sections = find(screen, "consoleSettingsSections")
        const rows = find(screen, "consoleSettingsList")
        check(sections && rows, "settings lists exist")
        bounds(sections.itemAtIndex(0), screen,
               {x:119, y:147, width:314}, "section rail content")
        bounds(rows.itemAtIndex(0), screen,
               {x:515, y:213, width:screen.width - 646}, "settings rows")
        if (AppController.route === "settings-streaming") {
            const first = rows.itemAtIndex(0)
            const second = rows.itemAtIndex(1)
            check(second !== null, "the second settings row is visible")
            near(first.height, 84, "settings row height")
            near(second.y - first.y, 88, "settings row pitch")
        }
        return true
    }
}
