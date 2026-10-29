import QtQuick

// Mono badge. tone: "teal" | "blue" | "violet" | "orange" | "danger" | "neutral".
// "danger" = text.destructive on a faint tint (e.g. "incompatible").
// Rule endpoints: Source teal, Sink blue, Plugin violet, App Out orange.
// Formats: LV2 blue, VST3 orange, CLAP teal. "neutral" = outline only
// ("no UI", the format tag in node headers). `small` = 9 px node-header tag.
Rectangle {
    id: root
    property string text
    property string tone: "neutral"
    property bool small: false

    // Tone for a plugin format string ("LV2", "VST3", "CLAP").
    function formatTone(fmt) {
        return fmt === "CLAP" ? "teal" : fmt === "VST3" ? "orange" : fmt === "LV2" ? "blue" : "neutral"
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
