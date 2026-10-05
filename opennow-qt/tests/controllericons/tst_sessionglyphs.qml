import QtQuick
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "SessionGlyphs"
    width: 640
    height: 200
    visible: true
    when: windowShown

    readonly property var dualSense: ({name: "DualSense Wireless Controller", family: "playstation", batteryPercent: 82})
    readonly property var xboxPad: ({name: "Xbox Wireless Controller", family: "xbox", batteryPercent: 40})
    readonly property var genericPad: ({name: "8BitDo", family: "generic", batteryPercent: -1})

    function prompts(controllers) {
        const component = Qt.createComponent(Qt.resolvedUrl("../../qml/components/SessionGlyphs.qml"))
        verify(component.status === Component.Ready, component.errorString())
        const object = component.createObject(testCase, {controllers: controllers})
        verify(object !== null)
        return object
    }

    function hint(glyphs, button) {
        const object = Qt.createQmlObject(
            'import QtQuick; import "../../qml/components" as Session; Session.SessionGlyphs.Hint { label: "Session" }',
            testCase, Qt.resolvedUrl("sessionhint.qml"))
        object.prompts = glyphs
        object.button = button
        return object
    }

    function test_playstationPadKeepsPadPromptsAfterPointerActivity() {
        verify(AppController.inputMode !== "controller", "fixture models pointer or keyboard activity")
        const glyphs = prompts([dualSense])
        verify(!glyphs.keyboard)
        verify(glyphs.playstation)
        compare(glyphs.button("GUIDE"), "PS")
        compare(glyphs.button("A"), "✕")
        compare(glyphs.button("B"), "○")
        glyphs.destroy()
    }

    function test_xboxAndGenericPadsUsePositionalLabels() {
        for (const pad of [xboxPad, genericPad]) {
            const glyphs = prompts([pad])
            verify(!glyphs.keyboard)
            verify(!glyphs.playstation)
            compare(glyphs.button("GUIDE"), "GUIDE")
            compare(glyphs.button("A"), "A")
            glyphs.destroy()
        }
    }

    function test_firstConnectedControllerOwnsTheFamily() {
        const glyphs = prompts([dualSense, xboxPad])
        compare(glyphs.family, "playstation")
        glyphs.controllers = [xboxPad]
        compare(glyphs.family, "xbox")
        glyphs.destroy()
    }

    function test_keyboardFallbackOnlyWithoutControllers() {
        const glyphs = prompts([])
        verify(glyphs.keyboard)
        compare(glyphs.button("GUIDE"), "Ctrl+G")
        compare(glyphs.button("B"), "Esc")
        glyphs.controllers = [dualSense]
        verify(!glyphs.keyboard)
        glyphs.destroy()
    }

    function test_hintDrawsPadGlyphOrFullKeyboardChord() {
        const glyphs = prompts([dualSense])
        const item = hint(glyphs, "GUIDE")
        compare(item.prompt, "PS")
        verify(item.children[0].visible, "controller glyph shown for a pad")
        verify(!item.children[1].visible, "keyboard caps hidden for a pad")

        glyphs.controllers = []
        compare(item.prompt, "Ctrl+G")
        verify(!item.children[0].visible, "single-key controller glyph never draws a chord")
        verify(item.children[1].visible)
        compare(item.children[1].shortcut, "Ctrl+G")
        compare(InputPromptIcons.keysFor(item.prompt).length, 2)
        waitForRendering(item)
        verify(item.children[1].width > item.children[1].keySize, "both caps of the chord are laid out")
        item.destroy()
        glyphs.destroy()
    }
}
