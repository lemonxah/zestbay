import QtQuick

Rectangle {
    property bool vertical: false
    property bool strong: false
    implicitWidth: vertical ? 1 : 0
    implicitHeight: vertical ? 20 : 1
    color: strong ? Theme.borderStrong : Theme.borderHairline
}
