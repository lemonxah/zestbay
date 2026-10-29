import QtQuick
import QtQuick.Controls

// Root for every top-level window: theme colours, palette and default font.
ApplicationWindow {
    color: Theme.bgPanel
    font.family: Theme.fontSans
    font.pixelSize: Theme.sizeBody

    palette.window: Theme.bgPanel
    palette.windowText: Theme.textPrimary
    palette.base: Theme.bgSurface
    palette.alternateBase: Theme.bgRaised
    palette.button: Theme.bgRaised
    palette.buttonText: Theme.textPrimary
    palette.text: Theme.textPrimary
    palette.placeholderText: Theme.textPlaceholder
    palette.highlight: Theme.accent
    palette.highlightedText: Theme.textOnAccent
    palette.mid: Theme.borderStrong
    palette.light: Theme.borderHairline
    palette.dark: Theme.borderStrong
    palette.link: Theme.accent
    palette.toolTipBase: Theme.bgHover
    palette.toolTipText: Theme.textPrimary
}
