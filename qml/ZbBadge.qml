import QtQuick

Rectangle {
    id: root
    property string text
    property string tone: "neutral"
    property bool small: false

    function formatTone(fmt) {
        return fmt === "CLAP" ? "teal" : (fmt === "VST3" || fmt === "VST2") ? "orange" : fmt === "LADSPA" ? "violet" : fmt === "LV2" ? "blue" : "neutral"
    }

    readonly property color _base: tone === "teal" ? Theme.catHardware
                                 : tone === "blue" ? Theme.catVirtual
                                 : tone === "violet" ? Theme.catPlugin
                                 : tone === "orange" ? Theme.catApp
                                 : tone === "danger" ? Theme.textDestructive
                                 : Theme.textMuted
    readonly property color textColor: tone === "teal" ? Theme.badgeTextTeal
                                     : tone === "blue" ? Theme.badgeTextBlue
                                     : tone === "violet" ? Theme.badgeTextViolet
                                     : tone === "orange" ? Theme.badgeTextOrange
                                     : tone === "danger" ? Theme.textDestructive
                                     : Theme.textMuted

    implicitWidth: label.implicitWidth + (small ? 8 : 12)
    implicitHeight: small ? 16 : 18
    radius: small ? Theme.radiusTag : Theme.radiusSmall
    color: tone === "neutral" ? "transparent" : Theme.tint(_base, Theme.badgeAlpha)
    border.width: tone === "neutral" ? 1 : 0
    border.color: Theme.borderStrong

    Text {
        id: label
        anchors.centerIn: parent
        text: root.text
        font.family: Theme.fontMono
        font.pixelSize: root.small ? Theme.sizeNodeTag : Theme.sizeBadge
        font.weight: Font.Medium
        color: root.textColor
    }
}
