import QtQuick
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "ControllerIcons"
    when: windowShown
    visible: true
    width: 400
    height: 200

    Component { id: imageComponent; Image { width: 32; height: 32 } }
    Component { id: glyphComponent; ControllerGlyph { label: "" } }
    Component { id: buttonComponent; GlassButton { text: "Select" } }
    Component { id: settingsComponent; DesktopSettingsIcon { width: 24; height: 24 } }
    Component { id: desktopComponent; DesktopGlyph { width: 24; height: 24; icon: "desktop-gamepad.svg" } }

    function init() {
        ShellStore.settings = {appTheme: "dark"}
    }

    function test_assets_data() {
        const rows = []
        for (const glyph of Object.keys(InputPromptIcons.assets)) {
            for (const ink of ["white", "#111827"]) {
                rows.push({tag: glyph + ink, glyph: glyph, ink: ink})
            }
        }
        return rows
    }

    function test_assets(data) {
        const image = createTemporaryObject(imageComponent, testCase, {
            source: InputPromptIcons.sourceFor(data.glyph, data.ink)
        })
        verify(image !== null)
        verify(image.source.toString().indexOf("/input-prompts/") >= 0)
        compare(image.source.toString().endsWith("-dark.svg"), data.ink !== "white")
        tryCompare(image, "status", Image.Ready)
        verify(image.implicitWidth > 0)
    }

    function test_semanticsAndFallbacks() {
        verify(InputPromptIcons.sourceFor("MENU", "white").toString().endsWith("xbox_button_menu.svg"))
        verify(InputPromptIcons.sourceFor("VIEW", "white").toString().endsWith("xbox_button_view.svg"))
        for (const glyph of ["F3", "F11", "R", "+", "−", "↻", ""]) {
            compare(InputPromptIcons.sourceFor(glyph, "white").toString(), "")
            const prompt = createTemporaryObject(glyphComponent, testCase, {glyph: glyph})
            verify(prompt !== null)
            compare(prompt.glyph, glyph)
            const fallback = prompt.children[0].children[1]
            compare(fallback.visible, glyph !== "")
            compare(fallback.children[0].text, glyph)
        }
    }

    function test_promptSizingAndContrast() {
        const prompt = createTemporaryObject(glyphComponent, testCase, {glyph: "LB", glyphSize: 28})
        tryCompare(prompt, "width", 28)
        compare(prompt.height, 28)
        const image = prompt.children[0].children[0]
        tryCompare(image, "status", Image.Ready)
        prompt.glyphColor = "#111827"
        verify(image.source.toString().endsWith("xbox_lb-dark.svg"))
        prompt.label = "Previous tab"
        tryVerify(() => prompt.width > 28)
    }

    function test_buttonContrastAndActivation() {
        const button = createTemporaryObject(buttonComponent, testCase, {width: 200, primary: true})
        const prompt = button.contentItem.children[0]
        compare(prompt.glyph, "A")
        compare(prompt.glyphColor, Theme.faceText)
        const image = prompt.children[0].children[0]
        tryCompare(image, "status", Image.Ready)
        verify(image.source.toString().endsWith("-dark.svg"))
        let clicks = 0
        button.clicked.connect(() => ++clicks)
        button.forceActiveFocus()
        keyClick(Qt.Key_Space)
        compare(clicks, 1)
        button.primary = false
        compare(prompt.glyphColor, Theme.face)
        button.glyph = ""
        compare(prompt.visible, false)
    }

    function test_consoleButtonReturnActivation_data() {
        return [{tag: "return", key: Qt.Key_Return}, {tag: "enter", key: Qt.Key_Enter}]
    }

    function test_consoleButtonReturnActivation(data) {
        const button = createTemporaryObject(buttonComponent, testCase, {width: 200})
        let clicks = 0
        button.clicked.connect(() => ++clicks)
        button.forceActiveFocus()
        keyClick(data.key)
        compare(clicks, 1)
        button.enabled = false
        keyClick(data.key)
        compare(clicks, 1)
    }

    function test_desktopControllerIcons() {
        for (const glyph of ["controller", "xbox", "playstation"]) {
            const icon = createTemporaryObject(settingsComponent, testCase, {glyph: glyph})
            tryCompare(icon, "status", Image.Ready)
            verify(icon.source.toString().indexOf("/input-prompts/controller_") >= 0)
            icon.ink = "#111827"
            verify(icon.source.toString().endsWith("-dark.svg"))
        }
        const icon = createTemporaryObject(desktopComponent, testCase)
        tryCompare(icon, "status", Image.Ready)
        ShellStore.settings = {appTheme: "light"}
        tryVerify(() => icon.source.toString().endsWith("-dark.svg"))
    }
}
