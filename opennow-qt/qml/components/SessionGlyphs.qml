import QtQuick
import OpenNOW

QtObject {
    readonly property var pad: ControllerInput.controllers.length > 0 ? ControllerInput.controllers[0] : null
    readonly property bool keyboard: AppController.inputMode !== "controller" || pad === null
    readonly property bool playstation: !keyboard && String(pad.family || "") === "playstation"

    function button(name) {
        if (keyboard)
            return ({A: "Enter", B: "Esc", GUIDE: "Ctrl+G", MENU: "Menu"})[name] || name
        if (playstation)
            return ({A: "✕", B: "○", X: "□", Y: "△", GUIDE: "PS", MENU: "Options", VIEW: "Create"})[name] || name
        return name
    }
}
