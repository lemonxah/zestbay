import QtQuick
import QtQuick.Controls

// Button kinds (bible: Buttons):
//   "primary" accent fill, one per dialog / toolbar
//   "ghost"   raised fill + strong border (default)
//   "subtle"  inside cards and node footers, no border
// Set iconName for a leading glyph; with no text it becomes a square icon button.
// trailingIconName adds a glyph after the label (e.g. "chevronDown" for a
// dropdown). destructive = text.destructive label (ghost / icon buttons only;
// pair it with a confirmation step).
Button {
    id: control
    property string kind: "ghost"
    property string iconName: ""
    property real iconSize: kind === "subtle" ? 12 : 14
    property string trailingIconName: ""
    property bool compact: false          // 26 px tall (dense rows)
    property bool destructive: false
    property color textColor: kind === "primary" ? Theme.textOnAccent
                            : destructive ? Theme.textDestructive
                            : kind === "subtle" ? Theme.textIcon
                            : (text.length > 0 ? Theme.textPrimary : Theme.textMuted)

    readonly property bool iconOnly: text.length === 0 && iconName.length > 0
    readonly property int controlHeight: compact ? Theme.compactButtonHeight
                                       : kind === "subtle" ? Theme.nodeFooterButton
                                       : Theme.controlHeight

    implicitHeight: controlHeight
    implicitWidth: iconOnly ? controlHeight : Math.max(controlHeight, row.implicitWidth + (kind === "subtle" ? 16 : 24))
    hoverEnabled: true
    focusPolicy: Qt.TabFocus
    opacity: enabled ? 1 : 0.45

    font.family: Theme.fontSans
    font.pixelSize: kind === "subtle" ? 11 : Theme.sizeButton
    font.weight: kind === "primary" ? Font.DemiBold : Font.Medium

    contentItem: Item {
        implicitWidth: row.implicitWidth
        implicitHeight: row.implicitHeight
        Row {
            id: row
            anchors.centerIn: parent
            spacing: 6
            ZbIcon {
                visible: control.iconName.length > 0
                anchors.verticalCenter: parent.verticalCenter
                name: control.iconName
                size: control.iconSize
                color: control.textColor
            }
            Text {
                visible: control.text.length > 0
                anchors.verticalCenter: parent.verticalCenter
                text: control.text
                font: control.font
                color: control.textColor
            }
            ZbIcon {
                visible: control.trailingIconName.length > 0
                anchors.verticalCenter: parent.verticalCenter
                name: control.trailingIconName
                size: control.trailingIconName.indexOf("chevron") === 0 ? 8 : control.iconSize
                color: control.textColor
            }
        }
    }

    background: Rectangle {
        radius: control.kind === "subtle" || control.compact ? Theme.radiusSmall : Theme.radiusControl
        color: {
            if (control.kind === "primary")
                return control.hovered || control.down ? Theme.accentHover : Theme.accent
            if (control.kind === "subtle")
                return control.hovered || control.down ? Theme.borderStrong : Theme.bgSubtle
            return control.hovered || control.down ? Theme.bgHover : Theme.bgRaised
        }
        border.width: control.kind === "ghost" ? 1 : 0
        border.color: Theme.borderStrong

        // Focus ring: 2 px accent.ring outside the control.
        Rectangle {
            visible: control.visualFocus
            anchors.fill: parent
            anchors.margins: -3
            radius: parent.radius + 3
            color: "transparent"
            border.width: 2
            border.color: Theme.accentRing
        }
    }
}
