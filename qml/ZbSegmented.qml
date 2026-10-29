import QtQuick

// Segmented control: inset track 30 px, segments 24 px, active = bg.subtle.
Rectangle {
    id: root
    property var model: []           // list of labels
    property int currentIndex: 0
    signal activated(int index)

    implicitHeight: Theme.segmentedOuter
    implicitWidth: row.implicitWidth + 6
    radius: Theme.radiusControl
    color: Theme.bgInset
    border.width: 1
    border.color: Theme.borderStrong

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 2
        Repeater {
            model: root.model
            delegate: Rectangle {
                required property int index
                required property var modelData
                readonly property bool active: index === root.currentIndex
                width: label.implicitWidth + 20
                height: Theme.segmentedInner
                radius: Theme.radiusSmall
                color: active ? Theme.bgSubtle : (mouse.containsMouse ? Theme.bgHover : "transparent")
                Text {
                    id: label
                    anchors.centerIn: parent
                    text: modelData
                    font.family: Theme.fontSans
                    font.pixelSize: Theme.sizeButton
                    font.weight: Font.Medium
                    color: parent.active ? Theme.textPrimary : Theme.textMuted
                }
                MouseArea {
                    id: mouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        root.currentIndex = index
                        root.activated(index)
                    }
                }
            }
        }
    }
}
