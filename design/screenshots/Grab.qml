import QtQuick
import ZestBay

// Screenshot harness (see render.sh). Arguments, last four:
//   <dark|light> <main|DialogFile> <out prefix> <qml dir URL>
// The main window is saved as <out>.0/1/2.png (toolbar, canvas, status
// bar) for render.sh to stack; a dialog as <out>.0.png.
Item {
    id: root
    readonly property var args: Qt.application.arguments
    readonly property string mode: args[args.length - 4]
    readonly property string target: args[args.length - 3]
    readonly property string out: args[args.length - 2]
    readonly property string qmlDir: args[args.length - 1]

    AppController { id: controller }

    // Never hang: give up if the grab hasn't finished in time.
    Timer { interval: 20000; running: true; onTriggered: Qt.quit() }

    Component.onCompleted: {
        // Theme lives in the qml/ directory, not visible from here.
        Qt.createQmlObject('import QtQuick; import "' + qmlDir + '"; QtObject { Component.onCompleted: Theme.mode = "'
                           + mode + '" }', root)
        var file = target === "main" ? "main.qml" : target + ".qml"
        var c = Qt.createComponent(qmlDir + file)
        if (c.status !== Component.Ready) {
            console.warn(c.errorString())
            Qt.quit()
            return
        }
        var w = target === "main" ? c.createObject(null) : c.createObject(null, { controller: controller })
        if (target === "PluginParams") w.openForNode(30)
        else if (target !== "main" && w.open) w.open()
        w.visible = true
        grab.win = w
        grab.start()
    }

    Timer {
        id: grab
        property var win
        interval: 2500   // let the CPU chart collect a few samples
        onTriggered: {
            var parts = []
            if (win.header) parts.push(win.header)
            var kids = win.contentItem.children
            for (var k = 0; k < kids.length; k++) {
                if (kids[k].visible && kids[k].width > 0 && kids[k].height > 0) { parts.push(kids[k]); break }
            }
            if (win.footer) parts.push(win.footer)
            var done = 0
            for (var i = 0; i < parts.length; i++) {
                (function(idx) {
                    parts[idx].grabToImage(function(r) {
                        r.saveToFile(root.out + "." + idx + ".png")
                        if (++done === parts.length) Qt.quit()
                    })
                })(i)
            }
        }
    }
}
