import QtQuick
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "HevcHelp"
    visible: true
    width: 960
    height: 900
    when: windowShown

    Component {
        id: helpComponent
        DesktopSettingsHevcHelp {
            width: testCase.width
            runtimeReady: true
            platformName: "windows"
            capabilities: ({videoBackends: [{backend: "d3d11", available: true,
                codecs: [{codec: "h265", available: false}]}]})
        }
    }
    SignalSpy { id: storeSpy; signalName: "openStoreRequested" }
    property var help

    function init() {
        help = createTemporaryObject(helpComponent, testCase)
        verify(help)
        storeSpy.target = help
        storeSpy.clear()
    }

    function cleanup() {
        DesktopTokens.uiScale = 1
    }

    function test_detection_data() {
        return [
            {tag: "windows-missing", platform: "windows", ready: true, backends: [{backend: "d3d11", available: true, codecs: [{codec: "h265", available: false}]}], shown: true},
            {tag: "windows-supported", platform: "windows", ready: true, backends: [{backend: "d3d11", available: true, codecs: [{codec: "h265", available: true}]}], shown: false},
            {tag: "pending-probe", platform: "windows", ready: false, backends: [{backend: "d3d11", available: true, codecs: [{codec: "h265", available: false}]}], shown: false},
            {tag: "linux", platform: "linux", ready: true, backends: [{backend: "d3d11", available: true, codecs: [{codec: "h265", available: false}]}], shown: false},
            {tag: "macos", platform: "osx", ready: true, backends: [{backend: "d3d11", available: true, codecs: [{codec: "h265", available: false}]}], shown: false},
            {tag: "empty-probe", platform: "windows", ready: true, backends: [], shown: false},
            {tag: "backend-failed", platform: "windows", ready: true, backends: [{backend: "d3d11", available: false, codecs: [{codec: "h265", available: false}]}], shown: false},
            {tag: "unknown-codec", platform: "windows", ready: true, backends: [{backend: "d3d11", available: true, codecs: [{codec: "h264", available: true}]}], shown: false},
            {tag: "other-backend", platform: "windows", ready: true, backends: [{backend: "vulkan", available: true, codecs: [{codec: "h265", available: false}]}], shown: false}
        ]
    }

    function test_detection(data) {
        help.platformName = data.platform
        help.runtimeReady = data.ready
        help.capabilities = {videoBackends: data.backends}
        compare(help.visible, data.shown)
        compare(help.implicitHeight > 0, data.shown)
    }

    function test_reprobe() {
        verify(help.visible)
        help.runtimeReady = false
        verify(!help.visible)
        help.capabilities = {videoBackends: [{backend: "d3d11", available: true,
            codecs: [{codec: "h265", available: true}]}]}
        help.runtimeReady = true
        verify(!help.visible)
        help.capabilities = {}
        verify(!help.visible)
    }

    function test_storeLinks_data() {
        return [{tag: "free", productId: "9N4WGH0Z6VHQ"}, {tag: "paid", productId: "9NMZLZ57R3T7"}]
    }

    function test_storeLinks(data) {
        const button = findChild(help, "hevcStore-" + data.productId)
        verify(button)
        mouseClick(button)
        compare(storeSpy.count, 1)
        compare(storeSpy.signalArguments[0][0], "https://apps.microsoft.com/detail/" + data.productId.toLowerCase())
        const command = findChild(help, "hevcCommand-" + data.productId)
        compare(command.text, "winget install " + data.productId)
        verify(command.readOnly && command.selectByMouse)
        command.selectAll()
        compare(command.selectedText, command.text)
    }

    function test_scaledLayout_data() {
        return [{tag: "normal", scale: 1, width: 900}, {tag: "scaled", scale: 1.5, width: 600}]
    }

    function test_scaledLayout(data) {
        DesktopTokens.uiScale = data.scale
        help.width = data.width
        waitForRendering(help)
        verify(help.implicitHeight > 0)
        for (const productId of ["9N4WGH0Z6VHQ", "9NMZLZ57R3T7"]) {
            for (const prefix of ["hevcStore-", "hevcCommand-"]) {
                const item = findChild(help, prefix + productId)
                const position = item.mapToItem(help, 0, 0)
                verify(position.x >= 0 && position.x + item.width <= help.width)
                verify(position.y >= 0 && position.y + item.height <= help.height)
            }
        }
    }
}
