import QtQuick

// Mouse-wheel scrolling for a Flickable / ListView: a fixed, immediate step
// per notch (the desktop's "lines per wheel step" x Theme.wheelLineHeight)
// instead of Flickable's slow velocity-based wheel; touchpads scroll by their
// own pixel deltas. Declare it inside the view:
//
//   ListView { id: list; ...; ZbWheelScroll { flickable: list } }
WheelHandler {
    id: handler
    property Flickable flickable
    target: null
    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad

    onWheel: (event) => {
        var f = handler.flickable
        if (!f || f.contentHeight <= f.height) return
        var dy
        if (event.pixelDelta.y !== 0)
            dy = event.pixelDelta.y
        else
            dy = event.angleDelta.y / 120 * Qt.styleHints.wheelScrollLines * Theme.wheelLineHeight
        var minY = f.originY - f.topMargin
        var maxY = f.originY + f.contentHeight + f.bottomMargin - f.height
        f.contentY = Math.max(minY, Math.min(maxY, f.contentY - dy))
        event.accepted = true
    }
}
