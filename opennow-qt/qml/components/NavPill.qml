import QtQuick
import QtQuick.Controls
import OpenNOW

GlassPanel {
    id: root
    property string currentRoute: "home"
    signal routeRequested(string route)
    implicitWidth: navRow.implicitWidth + 24
    implicitHeight: 72
    width: implicitWidth
    height: implicitHeight
    panelRadius: 36
    strong: true

    readonly property var destinations: [
        { route: "home", icon: "desktop-nav-home", label: qsTr("Home") },
        { route: "library", icon: "desktop-nav-library", label: qsTr("Library") },
        { route: "store", icon: "desktop-nav-store", label: qsTr("Store") },
        { route: "friends", icon: "desktop-nav-friends", label: qsTr("Friends") },
        { route: "settings", icon: "desktop-nav-settings", label: qsTr("Settings") },
        { route: "computer", icon: "", label: qsTr("Computer mode") }
    ]

    function selected(route) {
        if (route === "settings")
            return root.currentRoute.indexOf("settings") === 0 || root.currentRoute === "controllers"
        return root.currentRoute === route
    }

    Row {
        id: navRow
        anchors.centerIn: parent
        spacing: 4

        ControllerGlyph {
            anchors.verticalCenter: parent.verticalCenter
            glyph: "LB"
            label: ""
            glyphSize: 30
        }

        Repeater {
            model: root.destinations
            ItemDelegate {
                id: destination
                required property var modelData
                readonly property bool current: root.selected(modelData.route)
                readonly property color ink: current ? Theme.faceText : Theme.label
                anchors.verticalCenter: parent.verticalCenter
                width: current ? destinationRow.implicitWidth + 44 : 68
                height: 52
                padding: 0
                focusPolicy: Qt.StrongFocus
                Accessible.name: modelData.label
                Accessible.role: Accessible.Button
                onClicked: root.routeRequested(modelData.route)
                Keys.onReturnPressed: clicked()

                background: Rectangle {
                    radius: height / 2
                    color: destination.current ? Theme.face : "transparent"
                    border.width: destination.visualFocus ? 3 : 0
                    border.color: Theme.focus
                }
                contentItem: Item {
                    Row {
                        id: destinationRow
                        anchors.centerIn: parent
                        spacing: 10
                        Item {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 26
                            height: 26
                            Image {
                                anchors.fill: parent
                                visible: destination.modelData.icon !== ""
                                source: destination.modelData.icon === "" ? ""
                                    : "qrc:/qt/qml/OpenNOW/res/icons/" + destination.modelData.icon
                                        + (destination.current !== Theme.lightMode ? "-on-light" : "") + ".svg"
                                sourceSize: Qt.size(52, 52)
                                fillMode: Image.PreserveAspectFit
                            }
                            Item {
                                anchors.fill: parent
                                visible: destination.modelData.icon === ""
                                Rectangle {
                                    x: 2; y: 3; width: 22; height: 15; radius: 3
                                    color: "transparent"
                                    border.color: destination.ink
                                    border.width: 2
                                }
                                Rectangle { x: 12; y: 18; width: 2; height: 4; color: destination.ink }
                                Rectangle { x: 7; y: 22; width: 12; height: 2; radius: 1; color: destination.ink }
                            }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: destination.current
                            text: destination.modelData.label
                            color: destination.ink
                            font.family: Theme.displayFont
                            font.pixelSize: 19
                            font.weight: Font.Black
                        }
                    }
                }
            }
        }

        ControllerGlyph {
            anchors.verticalCenter: parent.verticalCenter
            glyph: "RB"
            label: ""
            glyphSize: 30
        }
    }
}
