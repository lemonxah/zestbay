import QtQuick
import QtQuick.Controls

// Plain text input (names, patterns): 32 px, sans 13, inset fill, strong
// border; accent border + ring on focus. For search use ZbSearchField, for
// numbers ZbValueField.
TextField {
    id: control
    implicitHeight: Theme.controlHeight
    leftPadding: 10
    rightPadding: 10
    verticalAlignment: TextInput.AlignVCenter
    selectByMouse: true
    font.family: Theme.fontSans
    font.pixelSize: Theme.sizeBody
    color: Theme.textPrimary
    placeholderTextColor: Theme.textPlaceholder
    selectionColor: Theme.accentRing
    selectedTextColor: Theme.textPrimary

    background: Rectangle {
        radius: Theme.radiusControl
        color: Theme.bgInset
        border.width: 1
        border.color: control.activeFocus ? Theme.accent : Theme.borderStrong
        Rectangle {
            visible: control.activeFocus
            anchors.fill: parent
            anchors.margins: -3
            radius: parent.radius + 3
            color: "transparent"
            border.width: 3
            border.color: Theme.accentRing
        }
    }
}
