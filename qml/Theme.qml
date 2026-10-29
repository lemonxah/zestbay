pragma Singleton
import QtQuick

// "Zest Dark" design tokens — design/DESIGN-BIBLE.md is the reference.
// Every colour, size and font in the UI comes from here; never use a literal.
QtObject {
    id: theme

    // ─── Mode ───────────────────────────────────────────────────────────
    // "system" follows the desktop; "dark" / "light" override it.
    property string mode: "system"

    // The desktop's colour scheme (Qt 6.5+). When Qt can't tell (older Qt,
    // or Unknown: e.g. qt5ct/qt6ct on sway-style compositors, issue #15),
    // judge by the system palette, which qt6ct and similar tools do set.
    readonly property bool systemDark: Qt.ColorScheme !== undefined
                                       && Qt.styleHints.colorScheme !== Qt.ColorScheme.Unknown
        ? Qt.styleHints.colorScheme === Qt.ColorScheme.Dark
        : systemPalette.window.hslLightness < 0.5
    readonly property bool dark: mode === "dark" || (mode === "system" && systemDark)

    property SystemPalette systemPalette: SystemPalette {}

    // ─── Fonts (bundled IBM Plex, OFL; system fallback if loading fails) ─
    property FontLoader _sans400: FontLoader { source: "../fonts/IBMPlexSans-Regular.ttf" }
    property FontLoader _sans500: FontLoader { source: "../fonts/IBMPlexSans-Medium.ttf" }
    property FontLoader _sans600: FontLoader { source: "../fonts/IBMPlexSans-SemiBold.ttf" }
    property FontLoader _mono400: FontLoader { source: "../fonts/IBMPlexMono-Regular.ttf" }
    property FontLoader _mono500: FontLoader { source: "../fonts/IBMPlexMono-Medium.ttf" }

    readonly property string fontSans: _sans400.status === FontLoader.Ready ? _sans400.font.family : "sans-serif"
    readonly property string fontMono: _mono400.status === FontLoader.Ready ? _mono400.font.family : "monospace"

    // Type scale (px). Weights: 400 Normal, 500 Medium, 600 DemiBold.
    readonly property int sizeTitle: 18        // window / dialog title, 600
    readonly property int sizeHero: 32         // CPU hero number, 600
    readonly property int sizeListPrimary: 14  // plugin list item name, 600
    readonly property int sizeBody: 13         // body, row titles (500)
    readonly property int sizeButton: 12       // buttons (primary 600, else 500)
    readonly property int sizeCaption: 12      // descriptions, captions
    readonly property int sizeNodeTitle: 12    // node title, 600
    readonly property int sizeSection: 11      // UPPERCASE section labels, 600
    readonly property int sizeMono: 11         // port names, values, counts, readouts
    readonly property int sizeValue: 12        // value fields, table numbers (mono)
    readonly property int sizeBadge: 10        // badges / tags (mono 500)
    readonly property int sizeNodeTag: 9       // format tag inside node headers
    readonly property int sizeTableHeader: 10  // mono UPPERCASE

    // ─── Surfaces and borders ───────────────────────────────────────────
    readonly property color bgCanvas:       dark ? "#121418" : "#e8eaed"
    readonly property color bgPanel:        dark ? "#1a1d23" : "#f6f7f8"
    readonly property color bgSurface:      dark ? "#1f232a" : "#ffffff"
    readonly property color bgMenu:         dark ? "#22262e" : "#ffffff"
    readonly property color bgRaised:       dark ? "#22262e" : "#ffffff"
    readonly property color bgInset:        dark ? "#121418" : "#ffffff"
    readonly property color bgHover:        dark ? "#262b33" : "#eef0f2"
    readonly property color bgSubtle:       dark ? "#2a2f38" : "#eef0f2"
    readonly property color borderStrong:   dark ? "#2f353f" : "#d4d8de"
    readonly property color borderHairline: dark ? "#2a2f38" : "#e3e6ea"
    readonly property color borderPanel:    dark ? "#262b33" : "#dde0e5"
    // Canvas dot grid; brighter than the spec's #262b33 / #cfd3d9, which
    // barely showed on real displays.
    readonly property color gridDot:        dark ? "#3a404b" : "#b9bec7"
    // Menu separators: strong in dark, hairline in light.
    readonly property color menuSeparator:  dark ? borderStrong : borderHairline

    // ─── Text ───────────────────────────────────────────────────────────
    readonly property color textPrimary:     dark ? "#e8eaee" : "#1a1d23"
    readonly property color textStrong:      dark ? "#f2f3f5" : "#14161a"
    readonly property color textSecondary:   dark ? "#aab1bc" : "#4b535e"
    readonly property color textIcon:        dark ? "#c9cfd8" : "#3a414b"
    readonly property color textMuted:       dark ? "#8b93a0" : "#646c78"
    readonly property color textPlaceholder: "#7b8390"
    readonly property color textDisabled:    dark ? "#6f7784" : "#9aa1ad"
    readonly property color textOnAccent:    dark ? "#141413" : "#ffffff"
    readonly property color textDestructive: dark ? "#e66767" : "#c23434"

    // ─── Accent (the only accent; see the bible for where it may be used) ─
    readonly property color accent:      dark ? "#c9e265" : "#5d7a00"
    readonly property color accentFaint: dark ? Qt.rgba(201/255, 226/255, 101/255, 0.14) : Qt.rgba(93/255, 122/255, 0, 0.12)
    readonly property color accentRing:  dark ? Qt.rgba(201/255, 226/255, 101/255, 0.26) : Qt.rgba(93/255, 122/255, 0, 0.22)
    readonly property color accentHover: dark ? "#e3f29a" : "#3f5400"
    // Logo colours (brand orange and lime). Light mode uses slightly deeper
    // shades: the brand lime nearly disappears on white.
    readonly property color logoOrange:  dark ? "#f48d11" : "#e07c00"
    readonly property color logoLime:    dark ? "#96dc2e" : "#6aaa12"

    // ─── Signal colours (pins and wires) ────────────────────────────────
    readonly property color sigAudio: dark ? "#63b3ff" : "#2a78d6"
    readonly property color sigMidi:  dark ? "#e879b5" : "#d55181"
    readonly property color sigVideo: dark ? "#f0a040" : "#c98500"
    readonly property real wireOpacity: dark ? 0.8 : 0.85
    readonly property real wireWidth: 1.75
    readonly property real wireWidthActive: 2.25

    // ─── Node categories (header glyph chip) ────────────────────────────
    readonly property color catHardware: dark ? "#39c6b0" : "#0f8f7c"
    readonly property color catVirtual:  dark ? "#5aa9ff" : "#2a78d6"
    readonly property color catApp:      dark ? "#f0a040" : "#d95926"
    readonly property color catPlugin:   dark ? "#a78bfa" : "#6250d6"
    readonly property color catMidi:     dark ? "#e879b5" : "#c2459a"
    readonly property real chipAlpha:  dark ? 0.16 : 0.12   // glyph chip background
    readonly property real badgeAlpha: dark ? 0.14 : 0.12   // badge background

    // ─── Badges (rule endpoints and plugin formats) ─────────────────────
    // Background = category colour at badgeAlpha (use Theme.tint()).
    readonly property color badgeTextTeal:   dark ? "#5fd6c3" : "#0b7a69"  // Source, CLAP
    readonly property color badgeTextBlue:   dark ? "#8fc3ff" : "#1f5fae"  // Sink, LV2
    readonly property color badgeTextViolet: dark ? "#b9a5fb" : "#4f3fb8"  // Plugin
    readonly property color badgeTextOrange: dark ? "#f5b35a" : "#b3461a"  // App Out, VST3

    // ─── Chart (CPU usage) ──────────────────────────────────────────────
    // Fixed slots: 0 = process CPU, then plugins in order. Never reassign a
    // slot when a series disappears; beyond eight series fold into "Other".
    readonly property var chartSeries: dark
        ? ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9", "#e66767"]
        : ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"]
    readonly property color chartOther:    textMuted   // the folded "Other" series
    readonly property color chartGrid:     dark ? "#262b33" : "#e3e6ea"
    readonly property color chartBaseline: dark ? "#383c45" : "#c9ced6"

    // ─── Switch ─────────────────────────────────────────────────────────
    // Tooltip fill: bg.hover in dark, white (bgMenu) in light.
    readonly property color bgTooltip: dark ? bgHover : bgMenu

    readonly property color switchTrackOff: dark ? borderStrong : "#cfd3d9"
    readonly property color switchKnobOff:  dark ? "#9aa1ad" : "#ffffff"

    // ─── Shadows (colour, vertical offset, blur radius) ─────────────────
    readonly property color shadowNode: dark ? Qt.rgba(0, 0, 0, 0.35) : Qt.rgba(20/255, 24/255, 32/255, 0.10)
    readonly property int shadowNodeY: dark ? 8 : 4
    readonly property int shadowNodeBlur: dark ? 24 : 14
    readonly property color shadowMenu: dark ? Qt.rgba(0, 0, 0, 0.45) : Qt.rgba(20/255, 24/255, 32/255, 0.18)
    readonly property int shadowMenuY: 12
    readonly property int shadowMenuBlur: 32

    // ─── Shape ──────────────────────────────────────────────────────────
    readonly property int radiusWindow: 12
    readonly property int radiusCard: 10     // node, card list, chart card, menu
    readonly property int radiusControl: 8   // button, input, search, tooltip
    readonly property int radiusSmall: 6     // small button, menu item, value field, badge
    readonly property int radiusChip: 5      // glyph chip
    readonly property int radiusTag: 4

    // ─── Spacing and control heights ────────────────────────────────────
    readonly property int grid: 4
    readonly property int dialogPadding: 24
    readonly property int cardPadding: 14
    readonly property int toolbarHeight: 52
    readonly property int toolbarPadding: 16
    readonly property int statusBarHeight: 32
    readonly property int controlHeight: 32     // toolbar controls, dialog buttons
    readonly property int searchHeightLarge: 40 // Add Plugin search
    readonly property int rowRule: 52
    readonly property int rowPlugin: 64
    readonly property int rowTable: 44
    readonly property int rowTableHeader: 30
    readonly property int menuItemHeight: 32
    readonly property int menuWidth: 220
    readonly property int compactButtonHeight: 26  // parameter rows, dense lists
    readonly property int segmentedOuter: 30
    readonly property int segmentedInner: 24
    readonly property int scrollBarWidth: 10   // ZbScrollBar, beside the content
    readonly property int scrollGap: 4         // gap between content and scroll bar
    readonly property int wheelLineHeight: 72  // px per wheel "line" (ZbWheelScroll); 3 lines = 216 px/notch

    // ─── Node card geometry (patchbay canvas) ───────────────────────────
    readonly property int nodeWidth: 184
    readonly property int nodeWidthPlugin: 208
    readonly property int nodeHeader: 34
    readonly property int nodePortRow: 22
    readonly property int nodePortsTop: 4
    readonly property int nodePortsBottom: 8
    readonly property int nodeFooter: 34
    readonly property int nodeFooterButton: 22
    readonly property int nodePin: 10
    readonly property int nodePinHover: 12
    readonly property int nodeChip: 18
    readonly property int nodeGlyph: 12
    readonly property int nodePadX: 14        // port label inset
    readonly property int gridPitch: 24
    readonly property int gridOffset: 12

    // ─── Helpers ────────────────────────────────────────────────────────
    // A colour at a given alpha (badge / chip backgrounds).
    function tint(c, alpha) {
        return Qt.rgba(c.r, c.g, c.b, alpha)
    }
}
