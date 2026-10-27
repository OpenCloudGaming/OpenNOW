import QtQuick
import OpenNOW

QtObject {
    property string sink: ""
    property var stage: null
    property var target: null
    property var reference: null
    property int phase: 0
    property int sample: 0
    readonly property var titles: ["<b>Portal 2 & Friends</b>",
        "Portal <img src='qrc:/qt/qml/OpenNOW/res/icons/nav-home.svg'> & Friends"]
    property Component detailsComponent: Component { GameDetailScreen {} }
    property Component choiceComponent: Component { ConsoleChoiceSheet { opened: true } }
    property Component exitComponent: Component {
        DesktopStreamExitConfirm { opened: true; switchTargetName: "Fixture account" }
    }
    property Component menuComponent: Component { DesktopInStreamMenu { opened: true } }
    property Component startingComponent: Component { DesktopSessionStarting {} }
    property Component referenceComponent: Component { Text { visible: false; textFormat: Text.PlainText } }

    function check(value, message) { if (!value) throw new Error("Literal game title " + sink + ": " + message) }
    function namedChild(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = namedChild(child, name)
            if (found) return found
        }
        return null
    }
    function run(host) {
        const components = {"game-detail": detailsComponent, "console-choice": choiceComponent,
            "console-choice-description": choiceComponent,
            "desktop-exit": exitComponent, "desktop-menu": menuComponent, "desktop-starting": startingComponent}
        const names = {"game-detail": "consoleGameDetailTitle", "console-choice": "consoleChoiceTitle",
            "console-choice-description": "consoleChoiceDescription",
            "desktop-exit": "accountSwitchBody", "desktop-menu": "streamMenuGameTitle",
            "desktop-starting": "sessionLaunchGameTitle"}
        check(components[sink] !== undefined, "unknown sink")
        stage = components[sink].createObject(host, {width: host.width, height: host.height, z: 1000})
        check(stage !== null, "could not create the production component")
        target = namedChild(stage, names[sink])
        check(target !== null, "missing the production Text consumer")
        reference = referenceComponent.createObject(host)
        return true
    }
    function advance() {
        if (phase === 0) {
            ShellStore.selectedGame = {title: titles[sample]}
            if (sink === "console-choice") stage.title = titles[sample]
            if (sink === "console-choice-description") stage.description = titles[sample]
            phase = 1
            return 0
        }
        if (phase === 1) {
            check(target.text.indexOf(titles[sample]) >= 0, "the Text consumer lost or transformed the title")
            check(target.textFormat === Text.PlainText, "the Text consumer interprets title markup")
            reference.font = target.font
            reference.width = target.width
            reference.wrapMode = target.wrapMode
            reference.maximumLineCount = target.maximumLineCount
            reference.elide = target.elide
            reference.leftPadding = target.leftPadding
            reference.rightPadding = target.rightPadding
            reference.text = target.text
            phase = 2
            return 0
        }
        if (phase === 2) {
            check(target.implicitWidth > 0 && Math.abs(target.implicitWidth - reference.implicitWidth) < 0.5,
                "rendered title metrics differ from the literal PlainText reference")
            if (++sample < titles.length) {
                phase = 0
                return 0
            }
            console.log("LITERAL GAME TITLE PASS " + sink + " markup, image syntax, PlainText layout")
            return 1
        }
        return -1
    }
}
