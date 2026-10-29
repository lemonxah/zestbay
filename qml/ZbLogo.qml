import QtQuick
import QtQuick.Shapes

// The ZestBay mark (logo "B · Patch"): two pins joined by the patch curve.
Item {
    id: root
    property real size: 22
    property color color: Theme.logoMark

    implicitWidth: size
    implicitHeight: size

    Shape {
        width: 64
        height: 64
        scale: root.size / 64
        transformOrigin: Item.TopLeft
        layer.enabled: true
        layer.samples: 4
        layer.smooth: true

        ShapePath {
            strokeColor: root.color
            strokeWidth: Glyphs.logoWireWidth
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            PathSvg { path: Glyphs.logoWire }
        }
        ShapePath {
            strokeColor: "transparent"
            fillColor: root.color
            PathSvg {
                readonly property real r: Glyphs.logoPinRadius
                path: "M" + (19 - r) + " 19a" + r + " " + r + " 0 1 0 " + 2 * r + " 0a" + r + " " + r + " 0 1 0 " + -2 * r + " 0"
                    + "M" + (45 - r) + " 45a" + r + " " + r + " 0 1 0 " + 2 * r + " 0a" + r + " " + r + " 0 1 0 " + -2 * r + " 0"
            }
        }
    }
}
