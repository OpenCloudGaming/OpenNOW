import QtQuick
import QtQuick.Controls
import OpenNOW

// Use Qt's document renderer for headings, lists, tables, links and code blocks.
// Keep release content selectable without allowing it to edit app state.
TextArea {
    objectName: "releaseNotesDocument"
    readOnly: true
    selectByMouse: true
    textFormat: TextEdit.MarkdownText
    wrapMode: TextEdit.Wrap
    height: Math.ceil(contentHeight) + topPadding + bottomPadding
    padding: 0
    background: null
    palette.link: Theme.focus
    selectionColor: Theme.focus
    selectedTextColor: Theme.contrastText(Theme.focus)
    color: Theme.textMuted
    font.family: Theme.bodyFont
    font.pixelSize: 14
    onLinkActivated: link => AppController.openExternalUrl(link)
}
