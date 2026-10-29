import QtQuick
import QtQuick.Controls

// Numeric value field: 64x26, mono 12, right-aligned, radius 6.
TextField {
    id: control
    implicitWidth: 64
    implicitHeight: Theme.compactButtonHeight
    horizontalAlignment: TextInput.AlignRight
    verticalAlignment: TextInput.AlignVCenter
    leftPadding: 6
    rightPadding: 8
    selectByMouse: true
    font.family: Theme.fontMono
    font.pixelSize: Theme.sizeValue
    color: Theme.textPrimary
    selectionColor: Theme.accentRing
    selectedTextColor: Theme.textPrimary
    background: Rectangle {
        radius: Theme.radiusSmall
        color: Theme.bgInset
        border.width: 1
        border.color: control.activeFocus ? Theme.accent : Theme.borderStrong
    }
}
