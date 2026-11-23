import QtQuick

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
