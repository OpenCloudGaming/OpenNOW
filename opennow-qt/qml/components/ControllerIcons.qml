pragma Singleton
import QtQuick

QtObject {
    readonly property var assets: ({
        "A": "xbox_button_a",
        "B": "xbox_button_b",
        "X": "xbox_button_x",
        "Y": "xbox_button_y",
        "LB": "xbox_lb",
        "RB": "xbox_rb",
        "LT": "xbox_lt",
        "RT": "xbox_rt",
        "MENU": "xbox_button_menu",
        "VIEW": "xbox_button_view",
        "GUIDE": "xbox_guide",
        "controller": "controller_xboxseries",
        "xbox": "controller_xboxseries",
        "playstation": "controller_playstation5"
    })

    function sourceFor(glyph: string, ink: color): url {
        const asset = assets[glyph]
        if (!asset)
            return ""
        const dark = 0.299 * ink.r + 0.587 * ink.g + 0.114 * ink.b < 0.5
        return "qrc:/qt/qml/OpenNOW/res/input-prompts/" + asset + (dark ? "-dark.svg" : ".svg")
    }
}
