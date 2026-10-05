import QtQuick
import OpenNOW

QtObject {
    id: root
    property var controllers: ControllerInput.controllers
    readonly property var pad: controllers && controllers.length > 0 ? controllers[0] : null
    readonly property bool keyboard: pad === null
    readonly property string family: keyboard ? "keyboard"
        : String(pad.family || "") === "playstation" ? "playstation" : "xbox"
    readonly property bool playstation: family === "playstation"

    function button(name) {
        if (keyboard)
            return ({A: "Enter", B: "Esc", GUIDE: "Ctrl+G"})[name] || name
        if (playstation)
            return ({A: "✕", B: "○", X: "□", Y: "△", GUIDE: "PS", MENU: "Options", VIEW: "Create"})[name] || name
        return name
    }

    component Hint: Row {
        id: hint
        property var prompts: null
        property string button: ""
        property string label: ""
        property color glyphColor: Theme.face
        property color labelColor: Theme.label
        property color keyInk: Theme.label
        property real glyphSize: 26
        readonly property bool keyboard: !prompts || prompts.keyboard
        readonly property string prompt: prompts ? prompts.button(button) : button
        spacing: 8
        Accessible.role: Accessible.StaticText
        Accessible.name: label !== "" ? prompt + ", " + label : prompt

        ControllerGlyph {
            anchors.verticalCenter: parent.verticalCenter
            visible: !hint.keyboard
            glyph: hint.prompt
            label: ""
            glyphColor: hint.glyphColor
            glyphSize: hint.glyphSize
        }
        KeyboardGlyph {
            anchors.verticalCenter: parent.verticalCenter
            visible: hint.keyboard
            shortcut: hint.prompt
            keySize: hint.glyphSize
            ink: hint.keyInk
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: hint.label !== ""
            text: hint.label
            color: hint.labelColor
            font.family: Theme.bodyFont
            font.pixelSize: 16
            font.weight: Font.Bold
        }
    }
}
