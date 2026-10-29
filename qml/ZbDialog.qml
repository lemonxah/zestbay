import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// In-window modal dialog (confirmations, rename, errors): title 18/600,
// content, footer with an optional secondary button left of the primary.
// Escape / secondary = reject(); primary = accept().
Dialog {
    id: control
    property string primaryText: "OK"
    property string secondaryText: ""     // empty = no secondary button
    property bool primaryEnabled: true    // e.g. false until required fields are filled

    modal: true
    padding: Theme.dialogPadding
    topPadding: 8
    font.family: Theme.fontSans
    font.pixelSize: Theme.sizeBody
    anchors.centerIn: parent

    background: Item {
        Repeater {
            model: 4
            Rectangle {
                required property int index
                anchors.fill: parent
                anchors.margins: -(index * 3 + 2)
                anchors.topMargin: -(index * 3 + 2) + Theme.shadowMenuY * 0.5
                radius: Theme.radiusWindow + index * 3 + 2
                color: Theme.shadowMenu
                opacity: 0.18 - index * 0.04
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: Theme.radiusWindow
            color: Theme.bgPanel
            border.width: 1
            border.color: Theme.borderStrong
        }
    }

    Overlay.modal: Rectangle { color: Theme.tint(Theme.bgCanvas, 0.6) }

    header: Text {
        text: control.title
        visible: text.length > 0
        topPadding: 20
        leftPadding: Theme.dialogPadding
        rightPadding: Theme.dialogPadding
        bottomPadding: 4
        font.family: Theme.fontSans
        font.pixelSize: Theme.sizeTitle
        font.weight: Font.DemiBold
        color: Theme.textPrimary
        elide: Text.ElideRight
    }

    footer: Item {
        implicitHeight: buttons.implicitHeight + 16 + 20
        RowLayout {
            id: buttons
            anchors.right: parent.right
            anchors.rightMargin: Theme.dialogPadding
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 20
            spacing: 8
            ZbButton {
                visible: control.secondaryText.length > 0
                text: control.secondaryText
                onClicked: control.reject()
            }
            ZbButton {
                kind: "primary"
                text: control.primaryText
                enabled: control.primaryEnabled
                onClicked: control.accept()
            }
        }
    }
}
