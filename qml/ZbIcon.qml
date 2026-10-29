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

    Shape {
        visible: root.glyph !== null
        width: root.glyph ? root.glyph.vb : 0
        height: width
        scale: root.glyph ? root.size / root.glyph.vb : 1
        transformOrigin: Item.TopLeft
        layer.enabled: true
        layer.samples: 4
        layer.smooth: true

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
