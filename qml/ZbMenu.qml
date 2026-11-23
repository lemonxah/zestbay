import QtQuick
import QtQuick.Controls

Menu {
    id: control
    implicitWidth: Theme.menuWidth
    padding: 6
    font.family: Theme.fontSans
    font.pixelSize: Theme.sizeBody
    delegate: ZbMenuItem {}
    overlap: 6

    background: Item {
        implicitWidth: Theme.menuWidth
        Repeater {
            model: 4
            Rectangle {
                required property int index
                anchors.fill: parent
                anchors.margins: -(index * 3 + 2)
                anchors.topMargin: -(index * 3 + 2) + Theme.shadowMenuY * 0.5
                radius: Theme.radiusCard + index * 3 + 2
                color: Theme.shadowMenu
                opacity: 0.18 - index * 0.04
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: Theme.radiusCard
            color: Theme.bgMenu
            border.width: 1
            border.color: Theme.borderStrong
        }
    }
}
