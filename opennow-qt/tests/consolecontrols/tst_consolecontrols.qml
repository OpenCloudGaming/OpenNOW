import QtQuick
import QtQuick.Controls
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "ConsoleControls"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    Component {
        id: warningComponent
        ConsoleWarningSheet {
            width: testCase.width
            height: testCase.height
            title: "End this session?"
            message: "Your game will close."
            safeText: "Keep playing"
            actionText: "End session"
            safeButtonObjectName: "safeAction"
            actionButtonObjectName: "destructiveAction"
            checkboxObjectName: "noticeCheckbox"
        }
    }

    Component {
        id: choiceComponent
        ConsoleChoiceSheet {
            width: testCase.width
            height: testCase.height
            title: "Choose a resolution"
            options: [
                {label:"720p", value:"1280x720"},
                {label:"1080p", value:"1920x1080"},
                {label:"4K", value:"3840x2160", disabled:true, detail:"Unavailable on this device"}
            ]
            currentIndex: 1
        }
    }

    SignalSpy { id: safeSpy; signalName: "safeRequested" }
    SignalSpy { id: actionSpy; signalName: "actionRequested" }
    SignalSpy { id: chosenSpy; signalName: "chosen" }
    SignalSpy { id: dismissedSpy; signalName: "dismissed" }

    function init() {
        AppController.reducedMotion = true
        safeSpy.clear()
        actionSpy.clear()
        chosenSpy.clear()
        dismissedSpy.clear()
    }

    function cleanup() {
        safeSpy.target = null
        actionSpy.target = null
        chosenSpy.target = null
        dismissedSpy.target = null
        AppController.reducedMotion = true
    }

    function openWarning(properties) {
        const sheet = createTemporaryObject(warningComponent, testCase, properties || {})
        verify(sheet !== null)
        safeSpy.target = sheet
        actionSpy.target = sheet
        sheet.opened = true
        tryCompare(findChild(sheet, "safeAction"), "activeFocus", true)
        return sheet
    }

    function test_enterActivatesSafeDefault_data() {
        return [{tag:"return", key:Qt.Key_Return}, {tag:"keypad-enter", key:Qt.Key_Enter}, {tag:"space", key:Qt.Key_Space}]
    }

    function test_enterActivatesSafeDefault(data) {
        const sheet = openWarning()
        keyClick(data.key)
        compare(safeSpy.count, 1)
        compare(actionSpy.count, 0)
        verify(sheet.opened)
    }

    function test_destructiveActionRequiresFocus() {
        const sheet = openWarning()
        keyClick(Qt.Key_Tab)
        tryCompare(findChild(sheet, "destructiveAction"), "activeFocus", true)
        keyClick(Qt.Key_Return)
        compare(actionSpy.count, 1)
        compare(safeSpy.count, 0)
    }

    function test_initiallyOpenWarningHasSafeFocus() {
        const sheet = createTemporaryObject(warningComponent, testCase, {opened:true})
        verify(sheet !== null)
        tryCompare(findChild(sheet, "safeAction"), "activeFocus", true)
    }

    function test_backAlwaysChoosesSafeAction() {
        const sheet = openWarning()
        keyClick(Qt.Key_Tab)
        keyClick(Qt.Key_Escape)
        compare(safeSpy.count, 1)
        compare(actionSpy.count, 0)
    }

    function test_focusStaysInWarning() {
        const sheet = openWarning()
        keyClick(Qt.Key_Tab)
        keyClick(Qt.Key_Tab)
        tryCompare(findChild(sheet, "safeAction"), "activeFocus", true)
        keyClick(Qt.Key_Backtab)
        tryCompare(findChild(sheet, "destructiveAction"), "activeFocus", true)
    }

    function test_informationalNoticeKeepsCheckboxState() {
        const sheet = openWarning({actionText:"", safeText:"Got it", checkboxText:"Don't notify me again"})
        const checkbox = findChild(sheet, "noticeCheckbox")
        verify(checkbox !== null)
        mouseClick(checkbox)
        verify(sheet.checked)
        keyClick(Qt.Key_Escape)
        compare(safeSpy.count, 1)
        compare(actionSpy.count, 0)
        verify(sheet.checked)
    }

    function test_choiceStartsAtSavedSelection() {
        const sheet = createTemporaryObject(choiceComponent, testCase)
        verify(sheet !== null)
        chosenSpy.target = sheet
        sheet.opened = true
        tryCompare(sheet, "focusedIndex", 1)
        keyClick(Qt.Key_Return)
        compare(chosenSpy.count, 1)
        compare(chosenSpy.signalArguments[0][0], 1)
        compare(sheet.currentIndex, 1)
    }

    function test_choiceCancelDoesNotCommit() {
        const sheet = createTemporaryObject(choiceComponent, testCase)
        chosenSpy.target = sheet
        dismissedSpy.target = sheet
        sheet.opened = true
        tryCompare(sheet, "focusedIndex", 1)
        keyClick(Qt.Key_Up)
        keyClick(Qt.Key_Escape)
        compare(dismissedSpy.count, 1)
        compare(chosenSpy.count, 0)
        compare(sheet.currentIndex, 1)
    }

    function test_disabledChoiceCannotBeChosen() {
        const sheet = createTemporaryObject(choiceComponent, testCase, {currentIndex:2})
        chosenSpy.target = sheet
        sheet.opened = true
        tryVerify(() => sheet.focusedIndex >= 0 && sheet.focusedIndex < 2)
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Return)
        verify(chosenSpy.count <= 1)
        if (chosenSpy.count)
            verify(chosenSpy.signalArguments[0][0] < 2)
    }

    function test_choiceListCanShrinkWhileOpen() {
        const sheet = createTemporaryObject(choiceComponent, testCase)
        chosenSpy.target = sheet
        sheet.opened = true
        tryCompare(sheet, "focusedIndex", 1)
        sheet.options = [{label:"Automatic", value:"auto"}]
        tryCompare(sheet, "focusedIndex", 0)
        keyClick(Qt.Key_Return)
        compare(chosenSpy.count, 1)
        compare(chosenSpy.signalArguments[0][0], 0)
        sheet.options = []
        keyClick(Qt.Key_Return)
        compare(chosenSpy.count, 1)
    }

    function test_interruptedWarningAndReducedMotion() {
        AppController.reducedMotion = false
        const sheet = openWarning()
        sheet.opened = false
        verify(!sheet.enabled)
        sheet.opened = true
        AppController.reducedMotion = true
        tryCompare(sheet, "visible", true)
        tryCompare(findChild(sheet, "safeAction"), "activeFocus", true)
        sheet.opened = false
        tryCompare(sheet, "visible", false)
        compare(actionSpy.count, 0)
    }

    function test_longWarningKeepsActionsVisible() {
        const sheet = openWarning({message:"A long provider error. ".repeat(150),
            detail:Array.from({length:11}, (_, index) => "Detail line " + index).join("\n")})
        const content = findChild(sheet, "consoleWarningSheetContent")
        const safe = findChild(sheet, "safeAction")
        verify(content !== null)
        tryVerify(() => content.contentHeight > content.height)
        verify(content.clip)
        const contentBottom = content.mapToItem(sheet, 0, content.height).y
        const safeTop = safe.mapToItem(sheet, 0, 0).y
        verify(contentBottom < safeTop)
        verify(safeTop + safe.height <= sheet.height)
        keyClick(Qt.Key_PageDown)
        tryVerify(() => content.contentY > 0)
        verify(safe.activeFocus)
        keyClick(Qt.Key_PageUp)
        tryCompare(content, "contentY", 0)
        compare(actionSpy.count, 0)
    }

    function focusedHalo(sheet) {
        const list = findChild(sheet, "consoleChoiceList")
        const item = list.itemAtIndex(sheet.focusedIndex)
        verify(item !== null)
        const stack = [item]
        while (stack.length) {
            const next = stack.pop()
            if (next.objectName === "consoleChoiceFocusHalo")
                return {list: list, halo: next}
            for (const child of next.children)
                stack.push(child)
        }
        return {list: list, halo: null}
    }

    function verifyRingInsideList(sheet) {
        waitForRendering(sheet)
        const found = focusedHalo(sheet)
        verify(found.halo !== null)
        verify(found.halo.visible)
        const topLeft = found.halo.mapToItem(found.list, 0, 0)
        const bottomRight = found.halo.mapToItem(found.list, found.halo.width, found.halo.height)
        verify(topLeft.x >= 0, "ring clipped on the left at " + topLeft.x)
        verify(topLeft.y >= 0, "ring clipped at the top at " + topLeft.y)
        verify(bottomRight.x <= found.list.width, "ring clipped on the right at " + bottomRight.x)
        verify(bottomRight.y <= found.list.height, "ring clipped at the bottom at " + bottomRight.y)
    }

    function test_focusRingStaysInsideScrollingList() {
        const options = Array.from({length: 24}, (_, index) => ({label: "Option " + index, value: index,
            detail: index % 3 === 0 ? "Detail for option " + index : ""}))
        const sheet = createTemporaryObject(choiceComponent, testCase, {options: options, currentIndex: -1})
        sheet.opened = true
        tryCompare(sheet, "focusedIndex", 0)
        verifyRingInsideList(sheet)
        for (let step = 0; step < 23; ++step)
            keyClick(Qt.Key_Down)
        compare(sheet.focusedIndex, 23)
        verifyRingInsideList(sheet)
        for (let step = 0; step < 11; ++step)
            keyClick(Qt.Key_Up)
        compare(sheet.focusedIndex, 12)
        verifyRingInsideList(sheet)
        for (let step = 0; step < 12; ++step)
            keyClick(Qt.Key_Up)
        compare(sheet.focusedIndex, 0)
        verifyRingInsideList(sheet)
    }

    function test_focusRingSurvivesOptionRefresh() {
        const sheet = createTemporaryObject(choiceComponent, testCase, {currentIndex: -1})
        sheet.opened = true
        tryCompare(sheet, "focusedIndex", 0)
        keyClick(Qt.Key_Down)
        sheet.options = sheet.options.map(option => Object.assign({}, option, {detail: "Refreshed"}))
        compare(sheet.focusedIndex, 1)
        tryVerify(() => sheet.activeFocus)
        verifyRingInsideList(sheet)
    }
}
