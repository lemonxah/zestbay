import QtQuick
import QtQuick.Controls

ScrollBar {
    id: control
    orientation: Qt.Vertical
    visible: size < 1
    policy: ScrollBar.AlwaysOn
    implicitWidth: orientation === Qt.Vertical ? Theme.scrollBarWidth : 100
    implicitHeight: orientation === Qt.Vertical ? 100 : Theme.scrollBarWidth
    padding: 2
    minimumSize: 0.08

    contentItem: Rectangle {
        implicitWidth: Theme.scrollBarWidth - 4
        implicitHeight: Theme.scrollBarWidth - 4
        radius: Math.min(width, height) / 2
        color: control.pressed ? Theme.textMuted
             : control.hovered ? Theme.textDisabled
             : Theme.borderStrong
    }

    background: Rectangle {
        radius: Math.min(width, height) / 2
        color: control.hovered || control.pressed ? Theme.bgSubtle : "transparent"
    }
}
