pragma Singleton
import QtQuick

QtObject {
    id: theme

    // System theme detection via Qt.styleHints (Qt 6.5+)
    // This binding automatically re-evaluates when colorScheme changes
    readonly property bool dark: Qt.styleHints.colorScheme === Qt.ColorScheme.Dark

    // ─── Window / Base backgrounds ───
    readonly property color windowBg:       dark ? "#1e1e1e" : "#f5f5f5"
    readonly property color panelBg:        dark ? "#282828" : "#ffffff"
    readonly property color surfaceBg:      dark ? "#1a1a1a" : "#e8e8e8"

    // ─── Row / List backgrounds ───
    readonly property color rowEven:        dark ? "#2a2a2a" : "#ffffff"
    readonly property color rowOdd:         dark ? "#252525" : "#f0f0f0"
    readonly property color rowHover:       dark ? "#3a3a3a" : "#e0e0e0"
    readonly property color rowSelected:    dark ? "#404060" : "#c0c8e8"

    // ─── Borders / Separators ───
    readonly property color separator:      dark ? "#3c3c3c" : "#d0d0d0"
    readonly property color separatorLight: dark ? "#2c2c2c" : "#e0e0e0"
    readonly property color border:         dark ? "#333333" : "#c0c0c0"
    readonly property color borderMuted:    dark ? "#555555" : "#aaaaaa"

    // ─── Text ───
    readonly property color textPrimary:    dark ? "#ffffff" : "#1a1a1a"
    readonly property color textSecondary:  dark ? "#bbbbbb" : "#555555"
    readonly property color textMuted:      dark ? "#999999" : "#777777"
    readonly property color textDisabled:   dark ? "#555555" : "#aaaaaa"

    // ─── Buttons / Controls ───
    readonly property color buttonBg:       dark ? "#373737" : "#e8e8e8"
    readonly property color buttonBorder:   dark ? "#5a5a5a" : "#b0b0b0"
    readonly property color buttonDisabledBg: dark ? "#2a2a2a" : "#f0f0f0"
    readonly property color buttonDisabledBorder: dark ? "#3a3a3a" : "#d0d0d0"
    readonly property color buttonActiveBg:      dark ? "#1a4a2a" : "#c8f0d8"
    readonly property color buttonActiveBorder:  dark ? "#40b060" : "#40a050"
    readonly property color buttonActiveText:    dark ? "#60e080" : "#1a6030"
    readonly property color buttonOffBg:         dark ? "#4a1a1a" : "#f0d0d0"
    readonly property color buttonOffBorder:     dark ? "#cc4444" : "#cc4444"
    readonly property color buttonOffText:       dark ? "#ff6666" : "#aa2222"

    // ─── Destructive / Delete ───
    readonly property color deleteBg:       dark ? "#5c2020" : "#ffe0e0"
    readonly property color deleteBorder:   dark ? "#cc4444" : "#cc4444"
    readonly property color deleteIcon:     dark ? "#ff6666" : "#cc2222"
    readonly property color deleteIconMuted: dark ? "#999999" : "#888888"

    // ─── Status indicators ───
    readonly property color statusActive:   dark ? "#60c060" : "#28a028"
    readonly property color statusBypassed: dark ? "#e0a040" : "#c08020"
    readonly property color statusError:    dark ? "#e06060" : "#cc2020"

    // ─── Graph: Node colors ───
    readonly property color nodeBg:         dark ? "#282828" : "#ffffff"
    readonly property color nodeBorder:     dark ? "#3c3c3c" : "#c0c0c0"
    readonly property color colSink:        "#4682B4"
    readonly property color colSource:      "#3CB371"
    readonly property color colVirtualSink: "#2E5A88"
    readonly property color colVirtualSource: "#2A7A52"
    readonly property color colStreamOut:   "#FFA500"
    readonly property color colStreamIn:    "#BA55D3"
    readonly property color colDuplex:      "#FFD700"
    readonly property color colJack:        "#E04040"
    readonly property color colLv2:         "#00BFFF"
    readonly property color colDefault:     "#808080"
    readonly property color colDefaultOutline: "#00FF88"

    // ─── Graph: Port colors ───
    readonly property color colPortIn:      "#6495ED"
    readonly property color colPortOut:     "#90EE90"
    readonly property color colMidi:        "#FF69B4"
    readonly property color colMidiPort:    "#FF69B4"

    // ─── Graph: Link colors ───
    readonly property color colLinkActive:     "#32CD32"
    readonly property color colLinkInactive:   dark ? "#555555" : "#aaaaaa"
    readonly property color colLinkMidi:       "#FF69B4"
    readonly property color colLinkConnecting: "#FFFF00"
    readonly property color colLinkSelected:   "#FF4444"

    // ─── Graph: Selection ───
    readonly property color selectionOutline:  "#FFFF00"
    readonly property real  selectionFillAlpha: 0.08

    // ─── Graph: Snap guides ───
    readonly property color snapGuide:      "#00AAFF"

    // ─── Chart / Canvas ───
    readonly property color chartBg:        dark ? "#1a1a1a" : "#f0f0f0"
    readonly property color chartBorder:    dark ? "#333333" : "#c0c0c0"
    readonly property color chartLine:      "#4CAF50"
    readonly property color chartFill:      Qt.rgba(76/255, 175/255, 80/255, 0.15)
    readonly property color chartGrid25:    dark ? "#4466AA" : "#8899cc"
    readonly property color chartGrid50:    dark ? "#AA4444" : "#cc6666"
    readonly property color chartGridLight: dark ? "#334455" : "#bbccdd"

    // ─── CPU / DSP thresholds ───
    readonly property color dspLow:         dark ? "#88CC88" : "#228822"
    readonly property color dspMedium:      dark ? "#FFAA44" : "#cc8800"
    readonly property color dspHigh:        dark ? "#FF4444" : "#cc0000"
    readonly property color dspBarLow:      "#4CAF50"
    readonly property color dspBarMedium:   dark ? "#CC8833" : "#cc8833"
    readonly property color dspBarHigh:     dark ? "#CC3333" : "#cc3333"

    // ─── Plugin format badges ───
    readonly property color badgeClapBg:    dark ? "#1a3a2a" : "#d0f0e0"
    readonly property color badgeClapText:  dark ? "#60e0a0" : "#208060"
    readonly property color badgeVst3Bg:    dark ? "#3a2a1a" : "#f0e0d0"
    readonly property color badgeVst3Text:  dark ? "#e0a060" : "#a06020"
    readonly property color badgeLv2Bg:     dark ? "#1a3a5a" : "#d0e0f0"
    readonly property color badgeLv2Text:   dark ? "#60a0e0" : "#2060a0"
    readonly property color badgeNoUiBg:    dark ? "#3a3a1a" : "#f0f0d0"
    readonly property color badgeNoUiText:  dark ? "#a0a060" : "#808020"

    // ─── Default node badge ───
    readonly property color defaultBadgeBg:   "#004422"
    readonly property color defaultBadgeText: "#00FF88"

    // ─── Gradient fades (for scroll hints) ───
    readonly property color fadeColor:      dark ? "#1e1e1e" : "#f5f5f5"

    // ─── Preferences section input bg ───
    readonly property color inputBg:        dark ? "#2c2c2c" : "#e8e8e8"
}
