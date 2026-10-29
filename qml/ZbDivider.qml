import QtQuick

// 1 px rule. Hairline by default; vertical = true for a 1-px-wide divider
// (toolbar dividers use strong: true, 20 px tall).
Rectangle {
    property bool vertical: false
    property bool strong: false
    implicitWidth: vertical ? 1 : 0
    implicitHeight: vertical ? 20 : 1
    color: strong ? Theme.borderStrong : Theme.borderHairline
}
