import QtQuick
import QtQuick.Controls
import OpenNOW

GlassPanel {
    id: root
    property string currentRoute: "home"
    signal routeRequested(string route)
    implicitWidth: 640
    implicitHeight: 72
    panelRadius: 36
    strong: true

    readonly property var destinations: [
        { route: "home", icon: "nav-home.svg", label: "Home" },
        { route: "library", icon: "nav-library.svg", label: "Library" },
        { route: "store", icon: "nav-controller.svg", label: "Store" },
        { route: "friends", icon: "nav-friends.svg", label: "Friends" },
        { route: "settings", icon: "nav-settings.svg", label: "Settings" },
        { route: "computer", icon: "nav-computer.svg", label: qsTr("Computer mode") }
    ]

    function selected(route) {
        if (route === "settings")
            return root.currentRoute.indexOf("settings") === 0
        return root.currentRoute === route
    }

    Row {
        anchors.centerIn: parent
        spacing: 0

        ControllerGlyph {
            anchors.verticalCenter: parent.verticalCenter
            glyph: "LB"
            label: ""
            glyphSize: 24
        }

        Repeater {
            model: root.destinations
            ItemDelegate {
                id: destination
                required property var modelData
                width: 88
                height: 43
                padding: 0
                focusPolicy: Qt.StrongFocus
                Accessible.name: I18n.source(destination.modelData.label, I18n.revision)
                Accessible.role: Accessible.Button
                onClicked: root.routeRequested(modelData.route)
                Keys.onReturnPressed: clicked()

                background: Item {}
                contentItem: Column {
                    spacing: 5
                    anchors.centerIn: parent
                    Image {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 34
                        height: 34
                        source: "qrc:/qt/qml/OpenNOW/res/icons/" + destination.modelData.icon
                        sourceSize: Qt.size(34, 34)
                        fillMode: Image.PreserveAspectFit
                    }
                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 34
                        height: 3
                        radius: 2
                        color: root.selected(destination.modelData.route) ? Theme.face : "transparent"
                    }
                }
            }
        }

        ControllerGlyph {
            anchors.verticalCenter: parent.verticalCenter
            glyph: "RB"
            label: ""
            glyphSize: 24
        }
    }
}
