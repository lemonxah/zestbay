import QtQuick
import QtQuick.Controls

// Tooltip: bg.hover (dark) / white (light), strong border, radius 8, 11 px.
ToolTip {
    id: control
    delay: 400
    padding: 8
    leftPadding: 10
    rightPadding: 10
    font.family: Theme.fontSans
    font.pixelSize: 11
    contentItem: Text {
        text: control.text
        font: control.font
        color: Theme.textPrimary
        wrapMode: Text.WordWrap
    }
    background: Rectangle {
        radius: Theme.radiusControl
        color: Theme.bgTooltip
        border.width: 1
        border.color: Theme.borderStrong
    }
}
