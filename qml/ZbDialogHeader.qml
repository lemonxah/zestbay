import QtQuick
import QtQuick.Layouts

// Dialog header: title 18/600, optional subtitle 12 muted, right-side
// controls (e.g. a labelled master switch) via the default property.
// Dialogs keep the window manager's frame, so there is no in-dialog close.
Item {
    id: root
    property string title
    property string subtitle: ""
    default property alias trailing: trailingRow.data

    implicitHeight: col.implicitHeight
    Layout.fillWidth: true

    ColumnLayout {
        id: col
        anchors.left: parent.left
        anchors.right: trailingRow.left
        anchors.rightMargin: 12
        spacing: 4
        Text {
            text: root.title
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeTitle
            font.weight: Font.DemiBold
            font.letterSpacing: -Theme.sizeTitle * 0.01
            color: Theme.textPrimary
            elide: Text.ElideRight
            Layout.fillWidth: true
        }
        Text {
            visible: root.subtitle.length > 0
            text: root.subtitle
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeCaption
            color: Theme.textMuted
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }
    }

    Row {
        id: trailingRow
        anchors.right: parent.right
        anchors.top: parent.top
        spacing: 12
    }
}
