import QtQuick
import QtQuick.Window
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "ConsoleLayout"
    when: windowShown

    Component {
        id: hostComponent
        Window {
            width: 1280
            height: 800
            visible: true
            property alias viewport: viewport
            property alias chrome: chrome
            ShellViewport {
                id: viewport
                Rectangle { anchors.fill: parent; color: Theme.shell }
                AppChrome { id: chrome; anchors.fill: parent }
            }
        }
    }

    function verifyLayout(host) {
        const viewport = host.viewport
        const origin = viewport.mapToItem(host.contentItem, 0, 0)
        const edge = viewport.mapToItem(host.contentItem, viewport.width, viewport.height)
        fuzzyCompare(origin.x, 0, 0.01)
        fuzzyCompare(origin.y, 0, 0.01)
        fuzzyCompare(edge.x, host.width, 0.01)
        fuzzyCompare(edge.y, host.height, 0.01)
        if (!viewport.desktopSurfaceActive) {
            verify(viewport.width >= 1920 - 0.01)
            verify(viewport.height >= 1080 - 0.01)
        }
        const status = findChild(host.chrome, "consoleStatusPanel")
        const clock = findChild(host.chrome, "consoleClock")
        verify(status !== null)
        verify(clock !== null)
        const center = clock.mapToItem(status, clock.width / 2, clock.height / 2)
        fuzzyCompare(center.y, status.height / 2, 0.01)
        for (const child of host.chrome.children) {
            if (!child.visible || child.width <= 0 || child.height <= 0)
                continue
            verify(child.x >= 0)
            verify(child.y >= 0)
            verify(child.x + child.width <= viewport.width)
            verify(child.y + child.height <= viewport.height)
        }
    }

    function test_resolutions_data() {
        return [
            {tag: "960x540", w: 960, h: 540},
            {tag: "1280x720", w: 1280, h: 720},
            {tag: "1280x800", w: 1280, h: 800},
            {tag: "1920x1080", w: 1920, h: 1080},
            {tag: "1920x1200", w: 1920, h: 1200},
            {tag: "2560x1440", w: 2560, h: 1440},
            {tag: "2560x1600", w: 2560, h: 1600},
            {tag: "3840x2160", w: 3840, h: 2160},
            {tag: "3840x2400", w: 3840, h: 2400},
            {tag: "ultrawide", w: 3440, h: 1440},
            {tag: "arbitrary-window", w: 1371, h: 917}
        ]
    }

    function test_resolutions(data) {
        const host = createTemporaryObject(hostComponent, testCase, {width: data.w, height: data.h})
        verify(host !== null)
        waitForRendering(host.chrome)
        verifyLayout(host)
    }

    function test_resizeAndModeSwitch() {
        const host = createTemporaryObject(hostComponent, testCase)
        for (const size of test_resolutions_data()) {
            host.width = size.w
            host.height = size.h
            waitForRendering(host.chrome)
            verifyLayout(host)
        }
        host.viewport.desktopSurfaceActive = true
        compare(host.viewport.scale, 1)
        compare(host.viewport.width, host.width)
        compare(host.viewport.height, host.height)
        host.viewport.desktopSurfaceActive = false
        waitForRendering(host.chrome)
        verifyLayout(host)
    }

    function test_clockUpdates() {
        const host = createTemporaryObject(hostComponent, testCase)
        const clock = findChild(host.chrome, "consoleClock")
        for (const date of [new Date(2026, 8, 10, 19, 0), new Date(2026, 8, 10, 23, 59), new Date(2026, 8, 11, 0, 0)]) {
            host.chrome.now = date
            tryCompare(clock, "text", Qt.formatDateTime(date, "hh:mm | MM/dd"))
            waitForRendering(host.chrome)
            verifyLayout(host)
        }
    }

    function test_fullscreenRoundTrip() {
        const host = createTemporaryObject(hostComponent, testCase)
        host.showFullScreen()
        tryCompare(host, "visibility", Window.FullScreen)
        waitForRendering(host.chrome)
        verifyLayout(host)
        host.showNormal()
        tryCompare(host, "visibility", Window.Windowed)
        waitForRendering(host.chrome)
        verifyLayout(host)
    }
}
