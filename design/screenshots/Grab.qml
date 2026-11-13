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
        var isMain = target === "main" || target.indexOf("main-") === 0
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
        // The CPU window with its meters row expanded.
        if (target === "CpuOverlay") {
            w.metersExpanded = true
            w.height += 180   // room for the meters listed under their row
        }
        w.visible = true
        if (target === "main-sessions" || target === "main-portmenu" || target === "main-metermenu" || target === "main-nodemeters") {
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
            } else if (kind === "main-metermenu") {
                // The canvas menu, with its Add meter submenu opened next.
                menu = root.findMenu(graph, "Add plugin\u2026")
                if (!menu) { console.warn("no canvas menu"); Qt.quit(); return }
                menu.popup(graph, 700, 110)
                menuMirror.hostOffset = Qt.point(menu.x, menu.y)
                subShot.menu = menu
                subShot.graph = graph
                subShot.win = win
                subShot.start()
                return
            } else if (kind === "main-nodemeters") {
                // Daw Sink's node menu: its snapped meter and Meter inputs/outputs.
                graph.contextNodeId = 20
                graph.contextNode = graph.findNodeData(20)
                menu = root.findMenu(graph, "Hide")
                if (!menu) { console.warn("no node menu"); Qt.quit(); return }
                var np = graph.nodePositions[20]
                menu.popup(graph, (np.x + graph.getNodeWidth(20)) * graph.zoom + graph.panX + 12, np.y * graph.zoom + graph.panY + 8)
                menuMirror.hostOffset = Qt.point(menu.x, menu.y)
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

    // Move an open menu's own items into the graph view at (x, y).
    function mirrorMenu(menu, graph, x, y, z) {
        var bg = menu.background, ct = menu.contentItem
        var bw = menu.width, bh = menu.height
        var cx = menu.leftPadding, cy = menu.topPadding
        var cw = ct.width, ch = ct.height
        bg.parent = graph
        ct.parent = graph
        bg.z = z
        ct.z = z + 1
        bg.x = x; bg.y = y; bg.width = bw; bg.height = bh
        ct.x = x + cx; ct.y = y + cy; ct.width = cw; ct.height = ch
    }

    Timer {
        id: menuMirror
        property var menu
        property var graph
        property var win
        property point hostOffset
        // A submenu shown beside `menu`, and where.
        property var sub: null
        property point subOffset
        interval: 600
        onTriggered: {
            root.mirrorMenu(menu, graph, hostOffset.x, hostOffset.y, 100000)
            if (sub)
                root.mirrorMenu(sub, graph, subOffset.x, subOffset.y, 100010)
            grab.win = win
            grab.interval = 500
            grab.start()
        }
    }

    // Open the submenu of the entry that has one, beside the open menu.
    Timer {
        id: subShot
        property var menu
        property var graph
        property var win
        interval: 600
        onTriggered: {
            var entry = null
            for (var k = 0; k < menu.count; k++) {
                var it = menu.itemAt(k)
                if (it && it.subMenu) { entry = it; break }
            }
            if (!entry) { console.warn("no submenu entry"); Qt.quit(); return }
            var sub = entry.subMenu
            entry.highlighted = true
            var ey = entry.mapToItem(menu.contentItem, 0, 0).y + menu.y + menu.topPadding - sub.topPadding
            var sx = menu.x + menu.width - 6
            sub.popup(graph, sx, ey)
            menuMirror.menu = menu
            menuMirror.sub = sub
            menuMirror.subOffset = Qt.point(sx, ey)
            menuMirror.graph = graph
            menuMirror.win = win
            menuMirror.start()
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
