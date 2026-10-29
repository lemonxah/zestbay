import QtQuick
import QtQuick.Controls

// Search / filter field: magnifier, inset fill, accent border + ring on focus.
// `large` = 40 px tall with 14 px text (Add Plugin).
TextField {
    id: control
    property bool large: false

    implicitHeight: large ? Theme.searchHeightLarge : Theme.controlHeight
    leftPadding: 10 + (large ? 16 : 14) + 8
    rightPadding: 10
    verticalAlignment: TextInput.AlignVCenter
    selectByMouse: true
    font.family: Theme.fontSans
    font.pixelSize: large ? 14 : Theme.sizeBody
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

        ZbIcon {
            anchors.verticalCenter: parent.verticalCenter
            x: 10
            name: "search"
            size: control.large ? 16 : 14
            color: Theme.textMuted
        }
    }
}
