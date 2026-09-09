import QtQuick
import QtQuick.Window
import OpenNOW

Image {
    id: root
    property string icon: ""
    property bool active: false
    readonly property string fileName: {
        // Static SVG variants avoid a shader/layer per navigation icon and
        // work identically with software rendering and the native scene graph.
        const lightVariants = ["desktop-sliders.svg","desktop-play-stroke.svg","desktop-user-plus.svg","desktop-logout.svg","desktop-nav-home.svg","desktop-expand.svg","desktop-star.svg","desktop-gamepad.svg","desktop-chevron-up.svg","desktop-rtx.svg","desktop-search.svg","desktop-nav-store.svg","desktop-collapse.svg","desktop-nav-settings.svg","desktop-plus.svg","desktop-nav-library.svg","desktop-coop.svg","desktop-nav-friends.svg","desktop-clock.svg"]
        if (Theme.lightMode && lightVariants.indexOf(root.icon) >= 0)
            return root.icon.replace(".svg", root.active && root.icon.indexOf("desktop-nav-") === 0
                ? "-active-on-light.svg" : "-on-light.svg")
        if (root.active && root.icon.indexOf("desktop-nav-") === 0)
            return root.icon.replace(".svg", "-active.svg")
        return root.icon
    }
    readonly property real dpr: Math.max(1, Screen.devicePixelRatio)

    source: root.icon === "desktop-gamepad.svg"
        ? InputPromptIcons.sourceFor("controller", DesktopTokens.textHigh)
        : root.icon === "" ? "" : "qrc:/qt/qml/OpenNOW/res/icons/" + root.fileName
    sourceSize: Qt.size(Math.max(1, Math.round(width * dpr)), Math.max(1, Math.round(height * dpr)))
    fillMode: Image.PreserveAspectFit
    smooth: false
    mipmap: false
    asynchronous: false
    cache: true
}
