import QtQuick
import QtQuick.Shapes

// A stroke glyph from Glyphs.icons, drawn at `size` px in `color`.
Item {
    id: root
    property string name
    property color color: Theme.textIcon
    property real size: 14

    readonly property var glyph: Glyphs.icons[name] || null

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
            strokeColor: root.color
            strokeWidth: root.glyph ? root.glyph.sw : 1
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg { path: root.glyph ? root.glyph.d : "" }
        }
    }
}
