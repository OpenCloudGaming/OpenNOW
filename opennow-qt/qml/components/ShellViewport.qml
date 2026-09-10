import QtQuick

FocusScope {
    property bool desktopSurfaceActive: false

    scale: desktopSurfaceActive ? 1 : Math.min(parent.width / 1920, parent.height / 1080)
    width: scale > 0 ? parent.width / scale : 0
    height: scale > 0 ? parent.height / scale : 0
    transformOrigin: Item.TopLeft
}
