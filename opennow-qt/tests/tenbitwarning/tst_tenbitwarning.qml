import QtQuick
import QtQuick.Controls
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "TenBitWarning"
    width: 960
    height: 720
    when: windowShown

    QtObject {
        id: store
        property var settings: ({})
        property var writes: []
        function applySetting(key, value) {
            const updated = Object.assign({}, settings)
            updated[key] = value
            settings = updated
        }
        function setSetting(key, value) {
            writes = writes.concat([{key:key, value:value}])
        }
    }
    Component {
        id: dialogComponent
        TenBitWarningDialog { settingsStore: store }
    }
    property var dialog

    function init() {
        store.settings = {colorQuality:"10bit_420"}
        store.writes = []
        DesktopTokens.uiScale = 1
        dialog = createTemporaryObject(dialogComponent, testCase)
        verify(dialog)
    }

    function cleanup() {
        dialog.close()
        tryCompare(dialog, "visible", false)
        DesktopTokens.uiScale = 1
    }

    function test_selection_data() {
        return [
            {tag:"ten-bit-420", previous:"8bit_420", value:"10bit_420", shown:true},
            {tag:"ten-bit-444", previous:"8bit_420", value:"10bit_444", shown:true},
            {tag:"change-chroma", previous:"10bit_420", value:"10bit_444", shown:true},
            {tag:"eight-bit", previous:"10bit_420", value:"8bit_420", shown:false},
            {tag:"eight-bit-444", previous:"10bit_444", value:"8bit_444", shown:false},
            {tag:"unchanged", previous:"10bit_420", value:"10bit_420", shown:false}
        ]
    }

    function test_selection(data) {
        verify(!dialog.visible)
        dialog.notifySelection(data.previous, data.value)
        tryCompare(dialog, "visible", data.shown)
        compare(store.writes.length, 0)
    }

    function test_dismissAndRepeat() {
        dialog.notifySelection("8bit_420", "10bit_420")
        tryCompare(dialog, "opened", true)
        const button = findChild(dialog, "tenBitWarningDismiss")
        verify(button)
        mouseClick(button)
        tryCompare(dialog, "visible", false)
        compare(store.settings.colorQuality, "10bit_420")
        compare(store.writes.length, 0)
        dialog.notifySelection("8bit_420", "10bit_420")
        tryCompare(dialog, "opened", true)
        keyClick(Qt.Key_Escape)
        tryCompare(dialog, "visible", false)
        compare(store.writes.length, 0)
    }

    function test_optOut_data() {
        return [{tag:"button", escape:false}, {tag:"escape", escape:true}]
    }

    function test_optOut(data) {
        dialog.notifySelection("8bit_420", "10bit_420")
        tryCompare(dialog, "opened", true)
        const checkbox = findChild(dialog, "tenBitWarningDontNotify")
        verify(checkbox)
        mouseClick(checkbox)
        verify(checkbox.checked)
        if (data.escape)
            keyClick(Qt.Key_Escape)
        else
            mouseClick(findChild(dialog, "tenBitWarningDismiss"))
        tryCompare(dialog, "visible", false)
        compare(store.writes, [{key:"suppressTenBitWarning", value:true}])
        compare(store.settings.suppressTenBitWarning, true)
        const recreated = createTemporaryObject(dialogComponent, testCase)
        recreated.notifySelection("8bit_420", "10bit_444")
        verify(!recreated.visible)
    }

    function test_scaledLayout_data() {
        return [{tag:"default", scale:1}, {tag:"125-percent", scale:1.25}, {tag:"150-percent", scale:1.5}]
    }

    function test_scaledLayout(data) {
        DesktopTokens.uiScale = data.scale
        dialog.notifySelection("8bit_420", "10bit_420")
        tryCompare(dialog, "opened", true)
        verify(waitForPolish(dialog.contentItem))
        verify(dialog.width <= width)
        verify(dialog.height <= height)
        const checkbox = findChild(dialog, "tenBitWarningDontNotify")
        verify(checkbox.y + checkbox.height <= dialog.contentItem.height)
        verify(dialog.contentItem.height >= dialog.contentItem.implicitHeight)
    }
}
