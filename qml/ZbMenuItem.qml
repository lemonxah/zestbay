import QtQuick
import QtQuick.Controls

// Menu item: 32 tall, radius 6, 10 px side padding, 13/400.
// destructive = text.destructive; shortcutHint = right-aligned mono 10 muted.
// checkable items show a compact switch (same look as ZbSwitch compact) on
// the right; clicking the row emits toggled() and keeps the menu open (a
// normal trigger would close it). Handle toggles with onToggled; keep
// `checked` bound to the real state.
MenuItem {
    id: control
    property bool destructive: false
    property string shortcutHint: ""
    // Pick-one lists (e.g. a filter): marks the current choice with a check;
    // the item still triggers and closes the menu. Use instead of checkable.
    property bool selected: false

    implicitHeight: Theme.menuItemHeight
    implicitWidth: Theme.menuWidth - 12
    leftPadding: 10
    rightPadding: 10
    font.family: Theme.fontSans
    font.pixelSize: Theme.sizeBody

    contentItem: Item {
        implicitHeight: label.implicitHeight
        Text {
            id: label
            anchors.left: parent.left
            anchors.right: selectedMark.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            text: control.text
            font: control.font
            elide: Text.ElideRight
            color: !control.enabled ? Theme.textDisabled
                 : control.destructive ? Theme.textDestructive
                 : Theme.textPrimary
        }
        ZbIcon {
            id: selectedMark
            visible: control.selected
            anchors.right: hint.left
            anchors.rightMargin: visible && control.shortcutHint.length > 0 ? 8 : 0
            anchors.verticalCenter: parent.verticalCenter
            width: visible ? 14 : 0
            name: "check"
            size: 14
            color: Theme.textPrimary
        }
        Text {
            id: hint
            anchors.right: check.left
            anchors.rightMargin: check.visible ? 8 : 0
            anchors.verticalCenter: parent.verticalCenter
            text: control.shortcutHint
            font.family: Theme.fontMono
            font.pixelSize: Theme.sizeBadge
            color: Theme.textMuted
        }
        // Compact switch: 32x18 track, 14 px knob (bible §4 "Switch").
        Rectangle {
            id: check
            visible: control.checkable
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: visible ? 32 : 0
            height: 18
            radius: height / 2
            color: control.checked ? Theme.accent : Theme.switchTrackOff
            opacity: control.enabled ? 1 : 0.5
            Behavior on color { ColorAnimation { duration: 120 } }

            Rectangle {
                width: 14
                height: 14
                radius: 7
                anchors.verticalCenter: parent.verticalCenter
                x: control.checked ? parent.width - width - 2 : 2
                color: control.checked ? Theme.textOnAccent : Theme.switchKnobOff
                Behavior on x { NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }
            }
        }
    }

    // Toggles stay open: flip without triggered(), which closes the menu.
    MouseArea {
        anchors.fill: parent
        enabled: control.checkable && control.enabled
        visible: enabled
        cursorShape: Qt.PointingHandCursor
        onClicked: control.toggled()
    }

    indicator: Item {}
    arrow: Item {}

    background: Rectangle {
        implicitHeight: Theme.menuItemHeight
        radius: Theme.radiusSmall
        color: control.highlighted && control.enabled ? Theme.bgHover : "transparent"
    }
}
