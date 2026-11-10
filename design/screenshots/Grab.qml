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
        var isMain = target === "main" || target === "main-presets" || target === "main-sessions" || target === "main-portmenu"
        var file = isMain ? "main.qml" : target + ".qml"
        var c = Qt.createComponent(qmlDir + file)
        if (c.status !== Component.Ready) {
            console.warn(c.errorString())
            Qt.quit()
            return
        }
        var w = isMain ? c.createObject(null) : c.createObject(null, { controller: controller })
        if (target === "PluginParams") w.openForNode(30)
        else if (!isMain && w.open) w.open()
        w.visible = true
        if (target === "main-sessions" || target === "main-portmenu") {
            // The whole window with the session dropdown / a port's menu open.
            menuShot.win = w
            menuShot.kind = target
            menuShot.start()
            return
        }
        if (target === "main-presets") {
            // The whole window with a plugin node's preset dropdown open.
            presetShot.win = w
            presetShot.start()
            return
        }
        grab.win = w
        grab.start()
    }

    function findByFunction(item, name) {
        if (typeof item[name] === "function") return item
        var kids = item.children
        for (var i = 0; i < kids.length; i++) {
            var r = findByFunction(kids[i], name)
            if (r) return r
        }
        return null
    }

    // Walk Items and their non-visual children (menus are only in `data`).
    function findMenu(item, wanted) {
        var d = item.data
        for (var i = 0; i < d.length; i++) {
            var o = d[i]
            if (o && typeof o.insertItem === "function" && typeof o.itemAt === "function") {
                for (var k = 0; k < o.count; k++) {
                    var it = o.itemAt(k)
                    if (it && it.text === wanted) return o
                }
            }
            if (o && o.data !== undefined) {
                var r = findMenu(o, wanted)
                if (r) return r
            }
        }
        return null
    }

    Timer {
        id: menuShot
        property var win
        property string kind
        interval: 2500
        onTriggered: {
            var graph = root.findByFunction(win.contentItem, "openPresetMenu")
            if (!graph) { console.warn("no graph view found"); Qt.quit(); return }
            var menu = null
            if (kind === "main-sessions") {
                menu = root.findMenu(win.header, "New session\u2026")
                if (!menu) { console.warn("no session menu"); Qt.quit(); return }
                var btn = menu.parent
                menu.popup(btn, 0, btn.height + 6)
                menuMirror.hostOffset = btn.mapToItem(graph, 0, btn.height + 6)
            } else {
                graph.contextNodeId = 30
                graph.contextNode = graph.findNodeData(30)
                graph.contextPortId = 3010
                menu = root.findMenu(graph, "Rename port\u2026")
                if (!menu) { console.warn("no port menu"); Qt.quit(); return }
                var pp = graph.portPositions[3010]
                menu.popup(graph, pp.cx * graph.zoom + graph.panX + 18, pp.cy * graph.zoom + graph.panY - 6)
                menuMirror.hostOffset = Qt.point(menu.x, menu.y)
            }
            menuMirror.menu = menu
            menuMirror.graph = graph
            menuMirror.win = win
            menuMirror.start()
        }
    }

    Timer {
        id: menuMirror
        property var menu
        property var graph
        property var win
        property point hostOffset
        interval: 600
        onTriggered: {
            var bg = menu.background, ct = menu.contentItem
            var bw = menu.width, bh = menu.height
            var cx = menu.leftPadding, cy = menu.topPadding
            var cw = ct.width, ch = ct.height
            bg.parent = graph
            ct.parent = graph
            bg.z = 100000
            ct.z = 100001
            bg.x = hostOffset.x; bg.y = hostOffset.y; bg.width = bw; bg.height = bh
            ct.x = hostOffset.x + cx; ct.y = hostOffset.y + cy; ct.width = cw; ct.height = ch
            grab.win = win
            grab.interval = 500
            grab.start()
        }
    }

    Timer {
        id: presetShot
        property var win
        interval: 2500
        onTriggered: {
            var graph = root.findByFunction(win.contentItem, "openPresetMenu")
            if (!graph) { console.warn("no graph view found"); Qt.quit(); return }
            graph.openPresetMenu(30)
            presetMirror.graph = graph
            presetMirror.win = win
            presetMirror.start()
        }
    }

    // Qt can't grab a popup (it lives in the window's popup layer), so move
    // the open menu's own items into the graph view; the normal grab then
    // includes them.
    Timer {
        id: presetMirror
        property var graph
        property var win
        interval: 600
        onTriggered: {
            var menu = null
            for (var i = 0; i < graph.data.length; i++) {
                var o = graph.data[i]
                if (o && typeof o.insertItem === "function" && o.opened) { menu = o; break }
            }
            if (!menu) { console.warn("no open menu found"); Qt.quit(); return }
            // Move the menu's own items into the graph view (offscreen
            // rendering has no shader effects to mirror them with).
            var mx = menu.x, my = menu.y
            var bg = menu.background, ct = menu.contentItem
            var bw = menu.width, bh = menu.height
            var cx = menu.leftPadding, cy = menu.topPadding
            var cw = ct.width, ch = ct.height
            bg.parent = graph
            ct.parent = graph
            bg.z = 100000
            ct.z = 100001
            bg.x = mx; bg.y = my; bg.width = bw; bg.height = bh
            ct.x = mx + cx; ct.y = my + cy; ct.width = cw; ct.height = ch
            grab.win = win
            grab.interval = 500
            grab.start()
        }
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
