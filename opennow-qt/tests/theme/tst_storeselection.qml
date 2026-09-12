import QtQuick
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "DesktopStoreSelection"
    width: 1100
    height: 900
    visible: true
    when: windowShown

    DesktopGameModal {
        id: modal
        anchors.fill: parent
        opened: true
        onVariantSelected: index => {
            game = Object.assign({}, game, {selectedVariantIndex: index})
        }
    }

    SignalSpy { id: selection; target: modal; signalName: "variantSelected" }
    SignalSpy { id: launches; target: modal; signalName: "playRequested" }

    function init() {
        DesktopTokens.uiScale = 1
        ShellStore.settings = {appTheme: "dark"}
        modal.game = {title: "Multi Store Game", isAvailable: true, selectedVariantIndex: 0,
            variants: [{id: "1001", store: "Steam", inLibrary: false},
                       {id: "1003", store: "Xbox", inLibrary: true}]}
        selection.clear()
        launches.clear()
        waitForRendering(modal)
    }

    function cleanup() {
        DesktopTokens.uiScale = 1
    }

    function test_chooseStore_data() {
        return [{tag: "mouse", key: 0}, {tag: "return", key: Qt.Key_Return},
                {tag: "enter", key: Qt.Key_Enter}, {tag: "space", key: Qt.Key_Space}]
    }

    function test_chooseStore(data) {
        const xbox = findChild(modal, "desktopStoreVariant1")
        const steam = findChild(modal, "desktopStoreVariant0")
        verify(xbox !== null)
        verify(steam.checked)
        if (data.key) {
            xbox.forceActiveFocus()
            keyClick(data.key)
        } else {
            mouseClick(xbox)
        }
        compare(selection.count, 1)
        compare(selection.signalArguments[0][0], 1)
        compare(modal.selectedVariant.id, "1003")
        verify(xbox.checked)
        verify(!steam.checked)
        compare(modal.ownershipText, "OWNED ON XBOX")
        compare(launches.count, 0)
        waitForRendering(modal)
        mouseClick(findChild(modal, "desktopGamePlay"))
        compare(launches.count, 1)
        mouseClick(findChild(modal, "desktopStoreVariant0"))
        compare(modal.selectedVariant.id, "1001")
        verify(findChild(modal, "desktopStoreVariant0").checked)
        verify(!findChild(modal, "desktopStoreVariant1").checked)
    }

    function test_singleStoreHidesPicker() {
        modal.game = {title: "Xbox Game", isAvailable: true,
            variants: [{id: "1003", store: "Xbox", inLibrary: true}]}
        tryVerify(() => findChild(modal, "desktopStoreVariant1") === null)
        verify(!findChild(modal, "desktopStoreVariant0").visible)
        compare(modal.selectedVariant.id, "1003")
    }

    function test_clearingGameRemovesPicker() {
        modal.game = null
        tryVerify(() => findChild(modal, "desktopStoreVariant0") === null)
        compare(modal.selectedVariant, null)
        verify(!findChild(modal, "desktopGamePlay").enabled)
    }

    function test_storeLogos_data() {
        return [{tag: "dark", theme: "dark"}, {tag: "light", theme: "light"}]
    }

    function test_storeLogos(data) {
        ShellStore.settings = {appTheme: data.theme}
        modal.game = {title: "Multi Store Game", isAvailable: true, selectedVariantIndex: 2,
            variants: [{id: "1001", store: "Steam"}, {id: "1002", store: "Epic Games Store"},
                       {id: "1003", store: "Xbox", inLibrary: true}]}
        waitForRendering(modal)
        for (let index = 0; index < 3; ++index) {
            const logo = findChild(modal, "desktopStoreLogo" + index)
            verify(logo.visible)
            tryCompare(logo, "status", Image.Ready)
            compare(logo.source.toString(), DesktopTokens.storeIconUrl(modal.game.variants[index].store))
            if (index === 2 || data.theme === "light")
                compare(logo.parent.color, Qt.color("#202634"))
        }
    }

    function test_scaledPicker_data() {
        return [{tag: "default", scale: 1}, {tag: "125-percent", scale: 1.25},
                {tag: "150-percent", scale: 1.5}]
    }

    function test_scaledPicker(data) {
        DesktopTokens.uiScale = data.scale
        waitForRendering(modal)
        const xbox = findChild(modal, "desktopStoreVariant1")
        tryCompare(xbox, "height", DesktopTokens.px(36))
        verify(xbox.x + xbox.width <= xbox.parent.width)
        mouseClick(xbox)
        compare(modal.selectedVariant.id, "1003")
    }
}
