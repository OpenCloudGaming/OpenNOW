import QtQuick
import OpenNOW

Rectangle {
    id: root
    color: Theme.shell
    clip: true

    Image {
        readonly property real coverScale: Math.max(root.width / 1920, root.height / 620)
        width: 1920 * coverScale
        height: 620 * coverScale
        x: (root.width - width) / 2
        y: (root.height - height) * 0.3
        source: "qrc:/qt/qml/OpenNOW/res/brand/signin-hero.jpg"
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0; color: Theme.lightMode ? "#D9EDF3F8" : "#CC0D0F1A" }
            GradientStop { position: 0.5; color: Theme.lightMode ? "#EBEDF3F8" : "#E60D0F1A" }
            GradientStop { position: 1; color: Theme.lightMode ? "#F7EDF3F8" : "#F70D0F1A" }
        }
    }
}
