import QtQuick
import QtQuick.Controls

// Scroll bar that sits BESIDE the content, never over it (vertical by
// default; the patchbay canvas also uses it horizontally over its edges).
// Shown only while the content overflows. Use the pattern in
// design/DESIGN-BIBLE.md §5.3:
//
//   Item {                                   // the scroll area
//       ListView {
//           id: list
//           anchors { left: parent.left; top: parent.top; bottom: parent.bottom
//                     right: bar.visible ? bar.left : parent.right
//                     rightMargin: bar.visible ? Theme.scrollGap : 0 }
//           clip: true
//           ScrollBar.vertical: bar
//       }
//       ZbScrollBar {
//           id: bar
//           anchors { top: parent.top; bottom: parent.bottom; right: parent.right }
//       }
//   }
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
