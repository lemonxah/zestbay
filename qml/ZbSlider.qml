import QtQuick
import QtQuick.Controls

// Slider: 4 px groove (strong unfilled, accent filled), 14 px accent handle
// with a 2 px surface ring. flush = no side padding, so the groove lines up
// with min/max labels placed under it.
Slider {
    id: control
    property bool flush: false
    hoverEnabled: true
    implicitHeight: 20
    leftPadding: flush ? 0 : 6
    rightPadding: flush ? 0 : 6

    background: Rectangle {
        x: control.leftPadding
        y: control.topPadding + control.availableHeight / 2 - height / 2
        width: control.availableWidth
        height: 4
        radius: 2
        color: Theme.borderStrong
        Rectangle {
            width: control.visualPosition * parent.width
            height: parent.height
            radius: 2
            color: Theme.accent
        }
    }

    handle: Rectangle {
        x: control.leftPadding + control.visualPosition * (control.availableWidth - width)
        y: control.topPadding + control.availableHeight / 2 - height / 2
        width: 14
        height: 14
        radius: 7
        color: Theme.accent
        border.width: 2
        border.color: Theme.bgSurface
        scale: control.pressed ? 1.1 : 1
        Rectangle {
            visible: control.visualFocus
            anchors.fill: parent
            anchors.margins: -4
            radius: width / 2
            color: "transparent"
            border.width: 2
            border.color: Theme.accentRing
        }
    }
}
