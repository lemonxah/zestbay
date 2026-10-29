pragma Singleton
import QtQuick

// Stroke icon set from the redesign (design/zestbay-redesign/design-source).
// Each glyph: vb = viewBox size (square), sw = stroke width in vb units,
// d = SVG path data. Drawn with round caps/joins, no fill, tinted per use.
// Used by ZbIcon (Shape) and by GraphView's canvas (ctx.path = d).
QtObject {
    // Circle as path data (SVG has no circle command).
    function _circle(cx, cy, r) {
        return "M" + (cx - r) + " " + cy + "a" + r + " " + r + " 0 1 0 " + (2 * r) + " 0"
             + "a" + r + " " + r + " 0 1 0 " + (-2 * r) + " 0"
    }

    readonly property var icons: ({
        // Node category glyphs (12 px)
        "speaker":    { vb: 12, sw: 1.5, d: "M1.5 4.5h2L7 1.5v9L3.5 7.5h-2zM9 4a3 3 0 0 1 0 4" },
        "mic":        { vb: 12, sw: 1.5, d: "M4 3a2 2 0 0 1 4 0v2a2 2 0 0 1-4 0zM2 6a4 4 0 0 0 8 0M6 10v1" },
        "headphones": { vb: 12, sw: 1.5, d: "M1.5 8.5V6a4.5 4.5 0 0 1 9 0v2.5M1.5 7.5h1.5v3H1.5zM9 7.5h1.5v3H9z" },
        "camera":     { vb: 12, sw: 1.5, d: "M2 3h4.5a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1h-4.5a1 1 0 0 1-1-1v-4a1 1 0 0 1 1-1zM7.5 5.5L11 3.5v5L7.5 6.5" },
        "wave":       { vb: 12, sw: 1.5, d: "M1 6h1.5l1.5-4 2 8 2-6 1.5 2H11" },
        "plug":       { vb: 12, sw: 1.5, d: "M4 1v3M8 1v3M2.5 4h7v1.5a3.5 3.5 0 0 1-7 0zM6 9v2" },
        "din":        { vb: 12, sw: 1.5, d: _circle(6, 6, 4.5) + "M3.6 6h0.01M6 3.8h0.01M8.4 6h0.01M6 8.2h0.01" },
        // Pin toggle in node headers; "pinFill" is the head filled in when pinned.
        "pin":        { vb: 12, sw: 1.5, d: "M4 1.5h4M5 1.5v3L3 7h6L7 4.5v-3M6 7v3.5" },
        "pinFill":    { vb: 12, sw: 1.5, d: "M5 1.5h2v3L9 7H3l2-2.5z" },
        // Toolbar / dialog glyphs
        "search":     { vb: 14, sw: 1.5, d: _circle(6, 6, 4.2) + "M9.2 9.2L12.5 12.5" },
        "plus":       { vb: 14, sw: 1.75, d: "M7 2.5v9M2.5 7h9" },
        "minus":      { vb: 14, sw: 1.75, d: "M3 7h8" },
        "fit":        { vb: 14, sw: 1.5, d: "M2 5.5V2h3.5M8.5 2H12v3.5M12 8.5V12H8.5M5.5 12H2V8.5" },
        // Snap to grid (toolbar).
        "grid":       { vb: 14, sw: 1.4, d: "M5 1.5v11M9 1.5v11M1.5 5h11M1.5 9h11" },
        "trash":      { vb: 14, sw: 1.4, d: "M2 3.5h10M5.5 3.5V2h3v1.5M3.5 3.5l.6 8h5.8l.6-8" },
        "reset":      { vb: 14, sw: 1.5, d: "M2.5 7a4.5 4.5 0 1 0 1.3-3.2M2.5 2.5v2.5H5" },
        "arrow":      { vb: 14, sw: 1.5, d: "M1 7h11M9 3.5L12.5 7 9 10.5" },
        // Gear (Preferences). The design source drew a sun here, which read
        // as a theme toggle.
        "settings":   { vb: 16, sw: 1.3, d: "M6.69 3.28 L7.00 1.68 L9.00 1.68 L9.31 3.28 L10.41 3.74 L11.76 2.82 L13.18 4.24 L12.26 5.59 L12.72 6.69 L14.32 7.00 L14.32 9.00 L12.72 9.31 L12.26 10.41 L13.18 11.76 L11.76 13.18 L10.41 12.26 L9.31 12.72 L9.00 14.32 L7.00 14.32 L6.69 12.72 L5.59 12.26 L4.24 13.18 L2.82 11.76 L3.74 10.41 L3.28 9.31 L1.68 9.00 L1.68 7.00 L3.28 6.69 L3.74 5.59 L2.82 4.24 L4.24 2.82 L5.59 3.74Z" + _circle(8, 8, 2.1) },
        "menu":       { vb: 16, sw: 1.5, d: "M3 4.5h10M3 8h10M3 11.5h10" },
        "close":      { vb: 16, sw: 1.6, d: "M4 4l8 8M12 4l-8 8" },
        "check":      { vb: 14, sw: 1.75, d: "M2.5 7.5l3 3 6-6.5" },
        "chevronDown":  { vb: 8, sw: 1.5, d: "M2 3l2 2 2-2" },
        "chevronRight": { vb: 8, sw: 1.5, d: "M3 2l2 2-2 2" }
    })

    // The logo mark ("B · Patch") on its 64x64 grid.
    readonly property string logoWire: "M19 19C47 19 17 45 45 45"
    readonly property real logoWireWidth: 5.5
    readonly property real logoPinRadius: 6.5
}
