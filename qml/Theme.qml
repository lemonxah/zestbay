pragma Singleton
import QtQuick

QtObject {
    id: theme

    property string mode: "system"

    property int desktopScheme: 0
    readonly property bool systemDark: desktopScheme === 1 ? true
        : desktopScheme === 2 ? false
        : Qt.ColorScheme !== undefined
          && Qt.styleHints.colorScheme !== Qt.ColorScheme.Unknown
        ? Qt.styleHints.colorScheme === Qt.ColorScheme.Dark
        : systemPalette.window.hslLightness < 0.5
    readonly property bool dark: mode === "dark" || (mode === "system" && systemDark)

    property SystemPalette systemPalette: SystemPalette {}

    property string palette: "zest"
    signal colorsChanged()
    onDarkChanged: colorsChanged()
    onPaletteChanged: colorsChanged()
    onCustomPalettesChanged: colorsChanged()
    property var customPalettes: ({})
    property var customIds: []
    property var customNames: []
    readonly property var paletteIds: ["zest", "catppuccin", "gruvbox", "nord", "solarized"].concat(customIds)
    readonly property var paletteNames: ["Zest", "Catppuccin", "Gruvbox", "Nord", "Solarized"].concat(customNames)
    readonly property var palettes: ({
        catppuccin: {
            dark: { bgCanvas: "#11111b", bgPanel: "#181825", bgSurface: "#1e1e2e", bgMenu: "#1e1e2e", bgRaised: "#232334", bgInset: "#11111b", bgHover: "#2a2b3d", bgSubtle: "#313244", borderStrong: "#45475a", borderHairline: "#313244", borderPanel: "#2a2b3d", gridDot: "#45475a", textPrimary: "#cdd6f4", textStrong: "#e2e8fb", textSecondary: "#a6adc8", textIcon: "#bac2de", textMuted: "#9399b2", textDisabled: "#6c7086", textOnAccent: "#11111b", textDestructive: "#f38ba8", accent: "#cba6f7", accentHover: "#dcc2fa", sigAudio: "#89b4fa", sigMidi: "#f5c2e7", sigVideo: "#fab387", catHardware: "#94e2d5", catVirtual: "#74c7ec", catApp: "#fab387", catPlugin: "#b4befe", catMidi: "#f5c2e7", catSubgraph: "#a6e3a1" },
            light: { bgCanvas: "#dce0e8", bgPanel: "#e6e9ef", bgSurface: "#eff1f5", bgMenu: "#eff1f5", bgRaised: "#eff1f5", bgInset: "#f7f8fa", bgHover: "#e6e9ef", bgSubtle: "#e6e9ef", borderStrong: "#bcc0cc", borderHairline: "#ccd0da", borderPanel: "#ccd0da", gridDot: "#acb0be", textPrimary: "#4c4f69", textStrong: "#3a3d55", textSecondary: "#5c5f77", textIcon: "#4c4f69", textMuted: "#6c6f85", textDisabled: "#9ca0b0", textOnAccent: "#ffffff", textDestructive: "#d20f39", accent: "#8839ef", accentHover: "#6a1fd0", sigAudio: "#1e66f5", sigMidi: "#d9479f", sigVideo: "#fe640b", catHardware: "#179299", catVirtual: "#209fb5", catApp: "#fe640b", catPlugin: "#7287fd", catMidi: "#d9479f", catSubgraph: "#40a02b" }
        },
        gruvbox: {
            dark: { bgCanvas: "#1d2021", bgPanel: "#282828", bgSurface: "#32302f", bgMenu: "#32302f", bgRaised: "#32302f", bgInset: "#1d2021", bgHover: "#3c3836", bgSubtle: "#3c3836", borderStrong: "#504945", borderHairline: "#3c3836", borderPanel: "#3c3836", gridDot: "#504945", textPrimary: "#ebdbb2", textStrong: "#fbf1c7", textSecondary: "#bdae93", textIcon: "#d5c4a1", textMuted: "#a89984", textDisabled: "#7c6f64", textOnAccent: "#1d2021", textDestructive: "#fb4934", accent: "#b8bb26", accentHover: "#cfd23f", sigAudio: "#83a598", sigMidi: "#d3869b", sigVideo: "#fe8019", catHardware: "#8ec07c", catVirtual: "#83a598", catApp: "#fe8019", catPlugin: "#fabd2f", catMidi: "#d3869b", catSubgraph: "#b8bb26" },
            light: { bgCanvas: "#ebdbb2", bgPanel: "#f2e5bc", bgSurface: "#fbf1c7", bgMenu: "#fbf1c7", bgRaised: "#fbf1c7", bgInset: "#f9f5d7", bgHover: "#f2e5bc", bgSubtle: "#f2e5bc", borderStrong: "#d5c4a1", borderHairline: "#e5d5ad", borderPanel: "#d5c4a1", gridDot: "#bdae93", textPrimary: "#3c3836", textStrong: "#282828", textSecondary: "#504945", textIcon: "#3c3836", textMuted: "#7c6f64", textDisabled: "#a89984", textOnAccent: "#fbf1c7", textDestructive: "#9d0006", accent: "#79740e", accentHover: "#5a560a", sigAudio: "#076678", sigMidi: "#8f3f71", sigVideo: "#af3a03", catHardware: "#427b58", catVirtual: "#076678", catApp: "#af3a03", catPlugin: "#b57614", catMidi: "#8f3f71", catSubgraph: "#79740e" }
        },
        nord: {
            dark: { bgCanvas: "#242933", bgPanel: "#2a303b", bgSurface: "#2e3440", bgMenu: "#333a47", bgRaised: "#333a47", bgInset: "#242933", bgHover: "#3b4252", bgSubtle: "#3b4252", borderStrong: "#434c5e", borderHairline: "#3b4252", borderPanel: "#3b4252", gridDot: "#4c566a", textPrimary: "#e5e9f0", textStrong: "#eceff4", textSecondary: "#aeb7c6", textIcon: "#d8dee9", textMuted: "#8f99ab", textDisabled: "#616e88", textOnAccent: "#2e3440", textDestructive: "#bf616a", accent: "#88c0d0", accentHover: "#a3d3e0", sigAudio: "#81a1c1", sigMidi: "#b48ead", sigVideo: "#d08770", catHardware: "#8fbcbb", catVirtual: "#5e81ac", catApp: "#d08770", catPlugin: "#ebcb8b", catMidi: "#b48ead", catSubgraph: "#a3be8c" },
            light: { bgCanvas: "#d8dee9", bgPanel: "#e5e9f0", bgSurface: "#eceff4", bgMenu: "#f4f6f9", bgRaised: "#f4f6f9", bgInset: "#f8f9fb", bgHover: "#e5e9f0", bgSubtle: "#e5e9f0", borderStrong: "#c2cad8", borderHairline: "#d8dee9", borderPanel: "#cfd6e2", gridDot: "#aab4c5", textPrimary: "#2e3440", textStrong: "#242933", textSecondary: "#434c5e", textIcon: "#3b4252", textMuted: "#4c566a", textDisabled: "#8f99ab", textOnAccent: "#ffffff", textDestructive: "#a5434d", accent: "#4c8a99", accentHover: "#3a7080", sigAudio: "#5e81ac", sigMidi: "#9a6f93", sigVideo: "#c06a4d", catHardware: "#4f8f8e", catVirtual: "#5e81ac", catApp: "#c06a4d", catPlugin: "#a8823a", catMidi: "#9a6f93", catSubgraph: "#6f9058" }
        },
        solarized: {
            dark: { bgCanvas: "#00252e", bgPanel: "#002b36", bgSurface: "#073642", bgMenu: "#073642", bgRaised: "#0a3f4c", bgInset: "#002b36", bgHover: "#0d4452", bgSubtle: "#0d4452", borderStrong: "#1c5361", borderHairline: "#0d4452", borderPanel: "#0d4452", gridDot: "#2a5b66", textPrimary: "#e2dcc8", textStrong: "#fdf6e3", textSecondary: "#93a1a1", textIcon: "#b4bcb8", textMuted: "#839496", textDisabled: "#586e75", textOnAccent: "#002b36", textDestructive: "#dc322f", accent: "#b58900", accentHover: "#d4a514", sigAudio: "#268bd2", sigMidi: "#d33682", sigVideo: "#cb4b16", catHardware: "#2aa198", catVirtual: "#268bd2", catApp: "#cb4b16", catPlugin: "#6c71c4", catMidi: "#d33682", catSubgraph: "#859900" },
            light: { bgCanvas: "#eee8d5", bgPanel: "#f5efdc", bgSurface: "#fdf6e3", bgMenu: "#fdf6e3", bgRaised: "#fdf6e3", bgInset: "#fffbf0", bgHover: "#f2ecd9", bgSubtle: "#f2ecd9", borderStrong: "#d6cfb8", borderHairline: "#e6dfca", borderPanel: "#e0d9c3", gridDot: "#c2bba5", textPrimary: "#073642", textStrong: "#002b36", textSecondary: "#586e75", textIcon: "#2f4a52", textMuted: "#657b83", textDisabled: "#93a1a1", textOnAccent: "#fdf6e3", textDestructive: "#dc322f", accent: "#9c7600", accentHover: "#7d5f00", sigAudio: "#268bd2", sigMidi: "#d33682", sigVideo: "#cb4b16", catHardware: "#2aa198", catVirtual: "#268bd2", catApp: "#cb4b16", catPlugin: "#6c71c4", catMidi: "#d33682", catSubgraph: "#859900" }
        }
    })

    function pick(name, fallback) {
        var p = palettes[palette] || customPalettes[palette];
        if (!p)
            return fallback;
        var v = p[dark ? "dark" : "light"][name];
        return v !== undefined ? v : fallback;
    }


    property FontLoader _sans400: FontLoader { source: "../fonts/IBMPlexSans-Regular.ttf" }
    property FontLoader _sans500: FontLoader { source: "../fonts/IBMPlexSans-Medium.ttf" }
    property FontLoader _sans600: FontLoader { source: "../fonts/IBMPlexSans-SemiBold.ttf" }
    property FontLoader _mono400: FontLoader { source: "../fonts/IBMPlexMono-Regular.ttf" }
    property FontLoader _mono500: FontLoader { source: "../fonts/IBMPlexMono-Medium.ttf" }

    readonly property string fontSans: _sans400.status === FontLoader.Ready ? _sans400.font.family : "sans-serif"
    readonly property string fontMono: _mono400.status === FontLoader.Ready ? _mono400.font.family : "monospace"

    readonly property int sizeTitle: 18
    readonly property int sizeHero: 32
    readonly property int sizeListPrimary: 14
    readonly property int sizeBody: 13
    readonly property int sizeButton: 12
    readonly property int sizeCaption: 12
    readonly property int sizeNodeTitle: 12
    readonly property int sizeSection: 11
    readonly property int sizeMono: 11
    readonly property int sizeValue: 12
    readonly property int sizeBadge: 10
    readonly property int sizeNodeTag: 9
    readonly property int sizeTableHeader: 10

    readonly property color bgCanvas:       pick("bgCanvas", dark ? "#121418" : "#e8eaed")
    readonly property color bgPanel:        pick("bgPanel", dark ? "#1a1d23" : "#f6f7f8")
    readonly property color bgSurface:      pick("bgSurface", dark ? "#1f232a" : "#ffffff")
    readonly property color bgMenu:         pick("bgMenu", dark ? "#22262e" : "#ffffff")
    readonly property color bgRaised:       pick("bgRaised", dark ? "#22262e" : "#ffffff")
    readonly property color bgInset:        pick("bgInset", dark ? "#121418" : "#ffffff")
    readonly property color bgHover:        pick("bgHover", dark ? "#262b33" : "#eef0f2")
    readonly property color bgSubtle:       pick("bgSubtle", dark ? "#2a2f38" : "#eef0f2")
    readonly property color borderStrong:   pick("borderStrong", dark ? "#2f353f" : "#d4d8de")
    readonly property color borderHairline: pick("borderHairline", dark ? "#2a2f38" : "#e3e6ea")
    readonly property color borderPanel:    pick("borderPanel", dark ? "#262b33" : "#dde0e5")
    readonly property color gridDot:        pick("gridDot", dark ? "#3a404b" : "#b9bec7")
    readonly property color menuSeparator:  dark ? borderStrong : borderHairline

    readonly property color textPrimary:     pick("textPrimary", dark ? "#e8eaee" : "#1a1d23")
    readonly property color textStrong:      pick("textStrong", dark ? "#f2f3f5" : "#14161a")
    readonly property color textSecondary:   pick("textSecondary", dark ? "#aab1bc" : "#4b535e")
    readonly property color textIcon:        pick("textIcon", dark ? "#c9cfd8" : "#3a414b")
    readonly property color textMuted:       pick("textMuted", dark ? "#8b93a0" : "#646c78")
    readonly property color textPlaceholder: "#7b8390"
    readonly property color textDisabled:    pick("textDisabled", dark ? "#6f7784" : "#9aa1ad")
    readonly property color textOnAccent:    pick("textOnAccent", dark ? "#141413" : "#ffffff")
    readonly property color textDestructive: pick("textDestructive", dark ? "#e66767" : "#c23434")
    readonly property color accent:      pick("accent", dark ? "#c9e265" : "#5d7a00")
    readonly property color accentFaint: tint(accent, dark ? 0.14 : 0.12)
    readonly property color accentRing:  tint(accent, dark ? 0.26 : 0.22)
    readonly property color accentHover: pick("accentHover", dark ? "#e3f29a" : "#3f5400")
    readonly property color bypass:      dark ? "#f0a040" : "#b36b00"
    readonly property color bypassFaint: dark ? Qt.rgba(240/255, 160/255, 64/255, 0.16) : Qt.rgba(179/255, 107/255, 0, 0.12)

    readonly property color logoOrange:  dark ? "#f48d11" : "#e07c00"
    readonly property color logoLime:    dark ? "#96dc2e" : "#6aaa12"

    readonly property color sigAudio: pick("sigAudio", dark ? "#63b3ff" : "#2a78d6")
    readonly property color sigMidi:  pick("sigMidi", dark ? "#e879b5" : "#d55181")
    readonly property color sigVideo: pick("sigVideo", dark ? "#f0a040" : "#c98500")
    readonly property color meterLow:   dark ? "#199e70" : "#1baf7a"
    readonly property color meterHot:   dark ? "#c98500" : "#eda100"
    readonly property color meterClip:  textDestructive
    readonly property color meterTrack: bgSubtle
    readonly property real wireOpacity: dark ? 0.8 : 0.85
    readonly property real wireWidth: 1.75
    readonly property real wireWidthActive: 2.25

    readonly property color catHardware: pick("catHardware", dark ? "#39c6b0" : "#0f8f7c")
    readonly property color catVirtual:  pick("catVirtual", dark ? "#5aa9ff" : "#2a78d6")
    readonly property color catApp:      pick("catApp", dark ? "#f0a040" : "#d95926")
    readonly property color catPlugin:   pick("catPlugin", dark ? "#a78bfa" : "#6250d6")
    readonly property color catMidi:     pick("catMidi", dark ? "#e879b5" : "#c2459a")
    readonly property color catSubgraph: pick("catSubgraph", dark ? "#96dc2e" : "#5f9a10")
    readonly property real chipAlpha:  dark ? 0.16 : 0.12
    readonly property real badgeAlpha: dark ? 0.14 : 0.12

    readonly property color badgeTextTeal:   dark ? "#5fd6c3" : "#0b7a69"
    readonly property color badgeTextBlue:   dark ? "#8fc3ff" : "#1f5fae"
    readonly property color badgeTextViolet: dark ? "#b9a5fb" : "#4f3fb8"
    readonly property color badgeTextOrange: dark ? "#f5b35a" : "#b3461a"

    readonly property var chartSeries: dark
        ? ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9", "#e66767"]
        : ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"]
    readonly property color chartOther:    textMuted
    readonly property color chartGrid:     dark ? "#262b33" : "#e3e6ea"
    readonly property color chartBaseline: dark ? "#383c45" : "#c9ced6"

    readonly property color bgTooltip: dark ? bgHover : bgMenu

    readonly property color switchTrackOff: dark ? borderStrong : "#cfd3d9"
    readonly property color switchKnobOff:  dark ? "#9aa1ad" : "#ffffff"

    readonly property color shadowNode: dark ? Qt.rgba(0, 0, 0, 0.35) : Qt.rgba(20/255, 24/255, 32/255, 0.10)
    readonly property int shadowNodeY: dark ? 8 : 4
    readonly property int shadowNodeBlur: dark ? 24 : 14
    readonly property color shadowMenu: dark ? Qt.rgba(0, 0, 0, 0.45) : Qt.rgba(20/255, 24/255, 32/255, 0.18)
    readonly property int shadowMenuY: 12
    readonly property int shadowMenuBlur: 32

    readonly property int radiusWindow: 12
    readonly property int radiusCard: 10
    readonly property int radiusControl: 8
    readonly property int radiusSmall: 6
    readonly property int radiusChip: 5
    readonly property int radiusTag: 4

    readonly property int grid: 4
    readonly property int dialogPadding: 24
    readonly property int cardPadding: 14
    readonly property int toolbarHeight: 52
    readonly property int toolbarPadding: 16
    readonly property int statusBarHeight: 32
    readonly property int controlHeight: 32
    readonly property int searchHeightLarge: 40
    readonly property int rowRule: 52
    readonly property int rowPlugin: 64
    readonly property int rowTable: 44
    readonly property int rowTableHeader: 30
    readonly property int menuItemHeight: 32
    readonly property int menuWidth: 220
    readonly property int compactButtonHeight: 26
    readonly property int segmentedOuter: 30
    readonly property int segmentedInner: 24
    readonly property int scrollBarWidth: 10
    readonly property int scrollGap: 4
    readonly property int wheelLineHeight: 72

    property int cardRow: 22
    readonly property real nodeScale: Math.max(0.7, Math.min(1.45, cardRow / 22))
    readonly property real nodeWidthScale: 1 + (nodeScale - 1) * 0.6
    readonly property int nodeWidth: Math.round(184 * nodeWidthScale)
    readonly property int nodeWidthPlugin: Math.round(208 * nodeWidthScale)
    readonly property int nodeHeader: Math.round(34 * nodeScale)
    readonly property int nodePortRow: Math.round(22 * nodeScale)
    readonly property int nodePortsTop: Math.round(4 * nodeScale)
    readonly property int nodePortsBottom: Math.round(8 * nodeScale)
    readonly property int nodeFooter: Math.round(34 * nodeScale)
    readonly property int nodeFooterButton: 22
    readonly property int nodeFooterBtn: Math.round(22 * nodeScale)
    readonly property int nodePresetRow: Math.round(30 * nodeScale)
    readonly property int nodePin: Math.max(8, Math.round(10 * nodeScale))
    readonly property int nodePinHover: nodePin + 2
    readonly property int nodeChip: Math.max(14, Math.round(18 * nodeScale))
    readonly property int nodeGlyph: Math.max(10, Math.round(12 * nodeScale))
    readonly property int nodePadX: Math.round(14 * nodeScale)
    readonly property int gridPitch: 24
    readonly property int gridOffset: 12

    function tint(c, alpha) {
        return Qt.rgba(c.r, c.g, c.b, alpha)
    }
}
