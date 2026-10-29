import QtQuick
import QtQuick.Shapes

// The ZestBay logo (traced from images/logo/zestbay-master.png by
// images/trace-logo.py): a lime zest arc and an orange cable that loops into
// a zest slice and ends in an audio jack. `size` is the height; withName
// adds the "ZestBay" wordmark (About screen only).
Item {
    id: root
    property real size: 22
    property bool withName: false

    readonly property var box: withName ? Glyphs.logoFullBox : Glyphs.logoMarkBox
    implicitHeight: size
    implicitWidth: size * box[2] / box[3]

    // Antialiasing: Qt 6.6+ curve renderer (crisp at any scale, no layer);
    // older Qt falls back to a multisampled layer.
    property bool _curve: false

    Item {
        width: root.box[2]
        height: root.box[3]
        scale: root.size / root.box[3]
        transformOrigin: Item.TopLeft
        layer.enabled: !root._curve
        layer.samples: 4
        layer.smooth: true

        Shape {
            id: shape
            x: -root.box[0]
            y: -root.box[1]
            Component.onCompleted: {
                if (shape.preferredRendererType !== undefined) {
                    shape.preferredRendererType = Shape.CurveRenderer
                    root._curve = true
                }
            }

            ShapePath {
                strokeColor: "transparent"
                fillColor: Theme.logoLime
                PathSvg { path: Glyphs.logoLimePath }
            }
            ShapePath {
                strokeColor: "transparent"
                fillColor: Theme.logoOrange
                PathSvg { path: Glyphs.logoOrangePath }
            }
            ShapePath {
                strokeColor: "transparent"
                fillColor: Theme.logoLime
                PathSvg { path: root.withName ? Glyphs.logoWordLimePath : "" }
            }
            ShapePath {
                strokeColor: "transparent"
                fillColor: Theme.logoOrange
                PathSvg { path: root.withName ? Glyphs.logoWordOrangePath : "" }
            }
        }
    }
}
