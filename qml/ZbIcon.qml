import QtQuick
import QtQuick.Shapes

// A glyph drawn at `size` px in `color`: a stroke glyph from Glyphs.icons,
// or a filled one from Glyphs.traced (fill: true).
Item {
    id: root
    property string name
    property color color: Theme.textIcon
    property real size: 14

    readonly property var glyph: Glyphs.icons[name] || Glyphs.traced[name] || null
    readonly property bool filled: glyph !== null && glyph.fill === true

    implicitWidth: size
    implicitHeight: size

    // Antialiasing: Qt 6.6+ curve renderer (crisp at any scale, no layer);
    // older Qt falls back to a multisampled layer.
    property bool _curve: false

    Shape {
        id: shape
        visible: root.glyph !== null
        width: root.glyph ? root.glyph.vb : 0
        height: width
        scale: root.glyph ? root.size / root.glyph.vb : 1
        transformOrigin: Item.TopLeft
        layer.enabled: !root._curve
        layer.samples: 4
        layer.smooth: true
        Component.onCompleted: {
            if (shape.preferredRendererType !== undefined) {
                shape.preferredRendererType = Shape.CurveRenderer
                root._curve = true
            }
        }

        ShapePath {
            strokeColor: root.filled ? "transparent" : root.color
            strokeWidth: root.glyph && root.glyph.sw ? root.glyph.sw : 1
            fillColor: root.filled ? root.color : "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: root.glyph ? root.glyph.d : "" }
        }
    }
}
