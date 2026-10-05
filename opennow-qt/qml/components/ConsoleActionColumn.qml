import QtQuick
import OpenNOW

Column {
    id: root
    property Item returnTarget: null
    spacing: 12

    function focusableItems() {
        const items = []
        for (let index = 0; index < children.length; ++index) {
            const item = children[index]
            if (item.visible && item.enabled && item.focusPolicy !== undefined && item.focusPolicy !== Qt.NoFocus)
                items.push(item)
        }
        return items
    }

    function focusFirst() {
        const items = focusableItems()
        if (items.length > 0)
            items[0].forceActiveFocus()
        else if (returnTarget)
            returnTarget.forceActiveFocus()
    }

    function move(delta) {
        const items = focusableItems()
        const index = items.findIndex(item => item.activeFocus)
        const next = index < 0 ? 0 : index + delta
        if (next >= 0 && next < items.length)
            items[next].forceActiveFocus()
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Down || event.key === Qt.Key_Up) {
            move(event.key === Qt.Key_Down ? 1 : -1)
            event.accepted = true
        } else if (returnTarget && (event.key === Qt.Key_Left || event.key === Qt.Key_Escape || event.key === Qt.Key_Back)) {
            if (!event.isAutoRepeat)
                returnTarget.forceActiveFocus()
            event.accepted = true
        }
    }
}
