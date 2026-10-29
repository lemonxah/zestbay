import QtQuick
import QtQuick.Controls

// Pill switch. Standard 40x22 (knob 18); compact 32x18 (knob 14) for lists.
// Set `text` for a labelled switch (label left of the track).
Switch {
    id: control
    property bool compact: false
    readonly property int trackW: compact ? 32 : 40
    readonly property int trackH: compact ? 18 : 22
    readonly property int knob: compact ? 14 : 18

    hoverEnabled: true
    focusPolicy: Qt.TabFocus
    spacing: 8
    padding: 0
    font.family: Theme.fontSans
    font.pixelSize: Theme.sizeBody
    font.weight: Font.Medium

    indicator: Rectangle {
        x: control.text.length > 0 ? control.width - width : 0
        anchors.verticalCenter: parent.verticalCenter
        implicitWidth: control.trackW
        implicitHeight: control.trackH
        radius: height / 2
        color: control.checked ? Theme.accent : Theme.switchTrackOff
        opacity: control.enabled ? 1 : 0.5
        Behavior on color { ColorAnimation { duration: 120 } }

        Rectangle {
            width: control.knob
            height: control.knob
            radius: width / 2
            anchors.verticalCenter: parent.verticalCenter
            x: control.checked ? parent.width - width - 2 : 2
            color: control.checked ? Theme.textOnAccent : Theme.switchKnobOff
            Behavior on x { NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }
        }

        Rectangle {
            visible: control.visualFocus
            anchors.fill: parent
            anchors.margins: -3
            radius: height / 2
            color: "transparent"
            border.width: 2
            border.color: Theme.accentRing
        }
    }

    contentItem: Text {
        visible: control.text.length > 0
        text: control.text
        font: control.font
        color: Theme.textPrimary
        verticalAlignment: Text.AlignVCenter
        rightPadding: control.text.length > 0 ? control.trackW + control.spacing : 0
    }

    implicitWidth: text.length > 0 ? contentItem.implicitWidth : trackW
    implicitHeight: Math.max(trackH, text.length > 0 ? contentItem.implicitHeight : 0)
}
