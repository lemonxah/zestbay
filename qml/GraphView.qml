import QtQuick
import QtQuick.Controls

Item {
    id: graphView

    required property var controller

    signal openPluginBrowser()
    signal openPluginParams(int nodeId)

    property real zoom: 1.0
    property real panX: 0
    property real panY: 0
    property bool viewportLoaded: false

    property var nodePositions: ({})
    property var layoutCursors: ({ "source": {x: 50, y: 50}, "stream": {x: 350, y: 50}, "sink": {x: 650, y: 50} })

    property int dragNodeId: -1
    property real dragOffsetX: 0
    property real dragOffsetY: 0

    property int connectFromPortId: -1
    property string connectFromDir: ""
    property real connectMouseX: 0
    property real connectMouseY: 0

    property var nodes: []
    property var links: []
    property var portsByNode: ({})
    property var portPositions: ({})
    property var portMediaTypes: ({})
    property int refreshCount: 0

    property var savedLayout: ({})
    property bool layoutLoaded: false

    property var hiddenNodes: ({})
    property bool hiddenLoaded: false

    property bool selectDragging: false
    property real selectStartX: 0
    property real selectStartY: 0
    property real selectEndX: 0
    property real selectEndY: 0
    property var selectedLinks: ({})
    property var selectedNodes: ({})
    property bool groupDragging: false
    property real groupDragLastX: 0
    property real groupDragLastY: 0

    property int contextNodeId: -1
    property var contextNode: null
    property var pendingPluginPosition: null
    property string defaultNodeKey: ""

    // Snap guides: drawn while dragging
    readonly property real snapThreshold: 5  // pixels in canvas space
    property var activeSnapLines: []  // [{axis:"x"|"y", pos: number}]

    readonly property real minNodeWidth: 180
    readonly property real maxNodeWidth: 400
    property var nodeWidths: ({})
    readonly property real headerHeight: 26
    readonly property real portHeight: 18
    readonly property real portSpacing: 3
    readonly property real portRadius: 5
    readonly property real nodePadding: 8
    readonly property real buttonRowHeight: 22

    readonly property color colSink: "#4682B4"
    readonly property color colSource: "#3CB371"
    readonly property color colVirtualSink: "#2E5A88"
    readonly property color colVirtualSource: "#2A7A52"
    readonly property color colStreamOut: "#FFA500"
    readonly property color colStreamIn: "#BA55D3"
    readonly property color colDuplex: "#FFD700"
    readonly property color colJack: "#E04040"
    readonly property color colLv2: "#00BFFF"
    readonly property color colDefault: "#808080"
    readonly property color colNodeBg: "#282828"
    readonly property color colNodeBorder: "#3c3c3c"
    readonly property color colPortIn: "#6495ED"
    readonly property color colPortOut: "#90EE90"
    readonly property color colMidi: "#FF69B4"
    readonly property color colMidiPort: "#FF69B4"
    readonly property color colLinkActive: "#32CD32"
    readonly property color colLinkInactive: "#555555"
    readonly property color colLinkMidi: "#FF69B4"
    readonly property color colLinkConnecting: "#FFFF00"
    readonly property color colDefaultOutline: "#00FF88"

    function refreshData() {
        if (!viewportLoaded) {
            try {
                var vp = JSON.parse(controller.get_viewport_json())
                if (vp.panX !== undefined) panX = vp.panX
                if (vp.panY !== undefined) panY = vp.panY
                if (vp.zoom !== undefined) zoom = vp.zoom
            } catch(e) {}
            viewportLoaded = true
        }

        if (!layoutLoaded) {
            try {
                savedLayout = JSON.parse(controller.get_layout_json())
            } catch(e) {
                savedLayout = {}
            }
            layoutLoaded = true
            try {
                var defNode = controller.get_default_node()
                if (defNode) defaultNodeKey = defNode
            } catch(e) {}
        }
        if (!hiddenLoaded) {
            try {
                var hiddenArr = JSON.parse(controller.get_hidden_json())
                var hSet = {}
                for (var hi = 0; hi < hiddenArr.length; hi++) {
                    hSet[hiddenArr[hi]] = true
                }
                hiddenNodes = hSet
            } catch(e) {
                hiddenNodes = {}
            }
            hiddenLoaded = true
        }

        try {
            var nodesJson = controller.get_nodes_json()
            var linksJson = controller.get_links_json()
            nodes = JSON.parse(nodesJson)
            links = JSON.parse(linksJson)
        } catch(e) {
            nodes = []
            links = []
        }

        var newPorts = {}
        for (var i = 0; i < nodes.length; i++) {
            try {
                newPorts[nodes[i].id] = JSON.parse(controller.get_ports_json(nodes[i].id))
            } catch(e) {
                newPorts[nodes[i].id] = []
            }
        }
        portsByNode = newPorts

        var newPortMedia = {}
        for (var nid in newPorts) {
            var pp = newPorts[nid]
            for (var pi2 = 0; pi2 < pp.length; pi2++) {
                newPortMedia[pp[pi2].id] = pp[pi2].mediaType || "Unknown"
            }
        }
        portMediaTypes = newPortMedia

        for (var ni = 0; ni < nodes.length; ni++) {
            var n = nodes[ni]
            if (!(n.id in nodePositions)) {
                var key = n.layoutKey || ""
                if (key && savedLayout[key]) {
                    var saved = savedLayout[key]
                    nodePositions[n.id] = { x: saved[0], y: saved[1] }
                } else if (n.type === "Plugin" && pendingPluginPosition) {
                    nodePositions[n.id] = { x: pendingPluginPosition.x, y: pendingPluginPosition.y }
                    pendingPluginPosition = null
                    persistLayout()
                } else {
                    // Try to position near connected peers
                    var peerPos = findConnectedPeerPosition(n.id)
                    if (peerPos) {
                        // Place to the left of a sink/target, or right of a source
                        var offsetX = (n.type === "Sink" || n.type === "StreamInput") ? 250 : -250
                        nodePositions[n.id] = { x: peerPos.x + offsetX, y: peerPos.y }
                    } else {
                        var col = getNodeColumn(n.type)
                        var cursor = layoutCursors[col]
                        nodePositions[n.id] = { x: cursor.x, y: cursor.y }
                        cursor.y += calculateNodeHeight(n) + 20
                    }
                }
                // Resolve overlaps: shift down until no collision
                resolveNodeOverlap(n)
            }
        }

        canvas.requestPaint()
        repaintTimer.restart()
    }

    function findConnectedPeerPosition(nodeId) {
        // Look through current links to find any node already positioned
        // that is connected to this node
        for (var li = 0; li < links.length; li++) {
            var link = links[li]
            var peerId = -1
            if (link.outputNodeId === nodeId && link.inputNodeId in nodePositions) {
                peerId = link.inputNodeId
            } else if (link.inputNodeId === nodeId && link.outputNodeId in nodePositions) {
                peerId = link.outputNodeId
            }
            if (peerId >= 0) {
                return nodePositions[peerId]
            }
        }
        return null
    }

    function resolveNodeOverlap(node, skipIds) {
        var pos = nodePositions[node.id]
        if (!pos) return
        var h = calculateNodeHeight(node)
        var w = getNodeWidth(node.id) || minNodeWidth
        var maxAttempts = 50
        var spacing = 10
        for (var attempt = 0; attempt < maxAttempts; attempt++) {
            var overlaps = false
            for (var oi = 0; oi < nodes.length; oi++) {
                var other = nodes[oi]
                if (other.id === node.id) continue
                if (skipIds && skipIds[other.id]) continue
                var opos = nodePositions[other.id]
                if (!opos) continue
                var oh = calculateNodeHeight(other)
                var ow = getNodeWidth(other.id) || minNodeWidth

                // Check AABB overlap
                if (pos.x < opos.x + ow && pos.x + w > opos.x &&
                    pos.y < opos.y + oh && pos.y + h > opos.y) {
                    // Calculate vertical overlap amounts
                    var overlapTop = opos.y + oh - pos.y    // how far into the other from the top
                    var overlapBot = pos.y + h - opos.y     // how far into the other from the bottom
                    // Snap above unless the node is mostly past the other (>70% overlap from top)
                    var minH = Math.min(h, oh)
                    if (overlapTop < minH * 0.7) {
                        pos = { x: pos.x, y: opos.y - h - spacing }
                    } else {
                        pos = { x: pos.x, y: opos.y + oh + spacing }
                    }
                    nodePositions[node.id] = pos
                    overlaps = true
                    break
                }
            }
            if (!overlaps) break
        }
    }

    // Snap a node's position to nearby nodes.
    // Snaps left-edge to left-edge, and top/bottom edges within snapThreshold.
    // Returns { x, y, lines } where lines are the snap guide descriptors.
    function computeSnap(nodeId, rawX, rawY, skipIds) {
        var w = getNodeWidth(nodeId) || minNodeWidth
        var node = findNodeData(nodeId)
        var h = node ? calculateNodeHeight(node) : 80
        var snappedX = rawX
        var snappedY = rawY
        var lines = []
        var bestDx = snapThreshold + 1
        var bestDy = snapThreshold + 1

        for (var i = 0; i < nodes.length; i++) {
            var other = nodes[i]
            if (other.id === nodeId) continue
            if (skipIds && skipIds[other.id]) continue
            if (other.layoutKey && hiddenNodes[other.layoutKey]) continue
            var opos = nodePositions[other.id]
            if (!opos) continue
            var ow = getNodeWidth(other.id) || minNodeWidth
            var oh = calculateNodeHeight(other)

            // --- X axis: left-edge to left-edge ---
            var dxLeft = Math.abs(rawX - opos.x)
            if (dxLeft < bestDx) {
                bestDx = dxLeft
                snappedX = opos.x
            }

            // --- Y axis: top-to-top, top-to-bottom, bottom-to-top, bottom-to-bottom ---
            // dragged top vs other top
            var dyTT = Math.abs(rawY - opos.y)
            if (dyTT < bestDy) {
                bestDy = dyTT
                snappedY = opos.y
            }
            // dragged top vs other bottom
            var dyTB = Math.abs(rawY - (opos.y + oh))
            if (dyTB < bestDy) {
                bestDy = dyTB
                snappedY = opos.y + oh
            }
            // dragged bottom vs other top
            var dyBT = Math.abs((rawY + h) - opos.y)
            if (dyBT < bestDy) {
                bestDy = dyBT
                snappedY = opos.y - h
            }
            // dragged bottom vs other bottom
            var dyBB = Math.abs((rawY + h) - (opos.y + oh))
            if (dyBB < bestDy) {
                bestDy = dyBB
                snappedY = opos.y + oh - h
            }
        }

        // Only apply snap if within threshold
        if (bestDx > snapThreshold) snappedX = rawX
        if (bestDy > snapThreshold) snappedY = rawY

        // Build guide lines for active snaps
        if (snappedX !== rawX) {
            lines.push({ axis: "x", pos: snappedX })
        }
        if (snappedY !== rawY) {
            // Determine which y edge snapped
            var snapYEdge = snappedY  // top edge
            // Check if bottom edge was closer
            if (Math.abs((rawY + h) - (snappedY + h)) > 0.01) {
                // It could be a bottom-edge snap; figure out the guide line y
                // The snappedY was set such that the matching edge aligns.
                // We need to find which other-node edge we matched:
                for (var j = 0; j < nodes.length; j++) {
                    var o2 = nodes[j]
                    if (o2.id === nodeId) continue
                    if (skipIds && skipIds[o2.id]) continue
                    if (o2.layoutKey && hiddenNodes[o2.layoutKey]) continue
                    var op2 = nodePositions[o2.id]
                    if (!op2) continue
                    var oh2 = calculateNodeHeight(o2)
                    // Check which edge the snap aligned to
                    if (Math.abs(snappedY - op2.y) < 0.5) {
                        lines.push({ axis: "y", pos: op2.y }); break
                    }
                    if (Math.abs(snappedY - (op2.y + oh2)) < 0.5) {
                        lines.push({ axis: "y", pos: op2.y + oh2 }); break
                    }
                    if (Math.abs((snappedY + h) - op2.y) < 0.5) {
                        lines.push({ axis: "y", pos: op2.y }); break
                    }
                    if (Math.abs((snappedY + h) - (op2.y + oh2)) < 0.5) {
                        lines.push({ axis: "y", pos: op2.y + oh2 }); break
                    }
                }
            } else {
                lines.push({ axis: "y", pos: snappedY })
            }
        }

        return { x: snappedX, y: snappedY, lines: lines }
    }

    function persistLayout() {
        var layoutObj = {}
        for (var existingKey in savedLayout) {
            layoutObj[existingKey] = savedLayout[existingKey]
        }
        for (var ni = 0; ni < nodes.length; ni++) {
            var n = nodes[ni]
            var key = n.layoutKey || ""
            var pos = nodePositions[n.id]
            if (key && pos) {
                layoutObj[key] = [pos.x, pos.y]
            }
        }
        savedLayout = layoutObj
        controller.save_layout(JSON.stringify(layoutObj))
    }

    Timer {
        id: repaintTimer
        interval: 16
        repeat: false
        onTriggered: canvas.requestPaint()
    }

    Timer {
        id: viewportSaveTimer
        interval: 500
        repeat: false
        onTriggered: {
            var vp = JSON.stringify({ panX: panX, panY: panY, zoom: zoom })
            controller.save_viewport(vp)
        }
    }

    onZoomChanged: if (viewportLoaded) viewportSaveTimer.restart()
    onPanXChanged: if (viewportLoaded) viewportSaveTimer.restart()
    onPanYChanged: if (viewportLoaded) viewportSaveTimer.restart()

    Menu {
        id: nodeContextMenu

        MenuItem {
            text: "Hide"
            onTriggered: {
                if (contextNode && contextNode.layoutKey) {
                    hiddenNodes[contextNode.layoutKey] = true
                    hiddenNodes = hiddenNodes
                    persistHidden()
                    canvas.requestPaint()
                }
            }
        }

        MenuSeparator {}

        MenuItem {
            text: contextNode && contextNode.layoutKey === defaultNodeKey && defaultNodeKey !== ""
                  ? "Clear Default" : "Set as Default"
            visible: contextNode !== null && (contextNode.type === "Sink" || contextNode.type === "Duplex" || contextNode.type === "Plugin")
            height: visible ? implicitHeight : 0
            onTriggered: {
                if (contextNode) {
                    var key = contextNode.layoutKey || ""
                    if (key === defaultNodeKey && defaultNodeKey !== "") {
                        defaultNodeKey = ""
                        controller.set_default_node("")
                    } else if (key !== "") {
                        defaultNodeKey = key
                        controller.set_default_node(key)
                    }
                    canvas.requestPaint()
                }
            }
        }

        MenuSeparator {
            visible: contextNode && contextNode.type === "Plugin"
        }

        MenuItem {
            text: "Rename..."
            visible: contextNode !== null && contextNode.type === "Plugin"
            height: visible ? implicitHeight : 0
            onTriggered: {
                renameField.text = contextNode ? contextNode.name : ""
                renameDialog.open()
            }
        }

        MenuItem {
            text: "Open UI..."
            visible: contextNode !== null && contextNode.type === "Plugin" && contextNode.pluginHasUi !== false
            height: visible ? implicitHeight : 0
            onTriggered: {
                if (contextNodeId >= 0)
                    controller.open_plugin_ui(contextNodeId)
            }
        }

        MenuSeparator {
            visible: contextNode !== null && contextNode.type === "Plugin"
        }

        MenuItem {
            text: "Remove Plugin"
            visible: contextNode !== null && contextNode.type === "Plugin"
            height: visible ? implicitHeight : 0
            onTriggered: {
                if (contextNodeId >= 0)
                    controller.remove_plugin(contextNodeId)
            }
        }
    }

    Menu {
        id: canvasContextMenu

        MenuItem {
            text: "Delete Selected Links"
            enabled: {
                for (var k in selectedLinks) return true
                return false
            }
            onTriggered: deleteSelectedLinks()
        }

        MenuSeparator {}

        MenuItem {
            text: "Add Plugin..."
            onTriggered: graphView.openPluginBrowser()
        }

        MenuSeparator {}

        MenuItem {
            text: "Unhide All"
            enabled: {
                for (var k in hiddenNodes) return true
                return false
            }
            onTriggered: {
                hiddenNodes = {}
                persistHidden()
                canvas.requestPaint()
            }
        }

        MenuItem {
            text: "Reset Zoom"
            onTriggered: {
                zoom = 1.0
                panX = 0
                panY = 0
                canvas.requestPaint()
            }
        }

        MenuItem {
            text: "Auto Layout"
            onTriggered: {
                nodePositions = {}
                savedLayout = {}
                layoutCursors = { "source": {x: 50, y: 50}, "stream": {x: 350, y: 50}, "sink": {x: 650, y: 50} }
                refreshData()
                persistLayout()
            }
        }
    }

    Dialog {
        id: renameDialog
        title: "Rename Plugin"
        standardButtons: Dialog.Ok | Dialog.Cancel
        anchors.centerIn: parent
        modal: true
        width: 320

        contentItem: TextField {
            id: renameField
            placeholderText: "New name"
            selectByMouse: true
            onAccepted: renameDialog.accept()
        }

        onAccepted: {
            var newName = renameField.text.trim()
            if (newName.length > 0 && contextNodeId >= 0) {
                controller.rename_plugin(contextNodeId, newName)
            }
        }
    }

    function persistHidden() {
        var arr = []
        for (var k in hiddenNodes) {
            if (hiddenNodes[k]) arr.push(k)
        }
        controller.save_hidden(JSON.stringify(arr))
    }

    function findNodeData(nodeId) {
        for (var i = 0; i < nodes.length; i++) {
            if (nodes[i].id === nodeId) return nodes[i]
        }
        return null
    }

    function getNodeColumn(type) {
        if (!type) return "stream"
        if (type === "Source") return "source"
        if (type === "Sink" || type === "Duplex") return "sink"
        return "stream"
    }

    function getNodeColor(node) {
        if (node.mediaType === "Midi") return colMidi
        if (node.isJack) return colJack
        var type = node.type
        if (!type) return colDefault
        if (type === "Sink") return node.isVirtual ? colVirtualSink : colSink
        if (type === "Source") return node.isVirtual ? colVirtualSource : colSource
        if (type === "StreamOutput") return colStreamOut
        if (type === "StreamInput") return colStreamIn
        if (type === "Duplex") return colDuplex
        if (type === "Plugin") return colLv2
        return colDefault
    }

    function calculateNodeHeight(node) {
        var ports = portsByNode[node.id] || []
        var inputs = ports.filter(function(p) { return p.direction === "Input" }).length
        var outputs = ports.filter(function(p) { return p.direction === "Output" }).length
        var rows = Math.max(inputs, outputs, 1)
        var h = headerHeight + nodePadding * 2 + rows * (portHeight + portSpacing)
        if (node.type === "Plugin")
            h += buttonRowHeight + nodePadding
        return h
    }

    function calculateNodeWidths(ctx) {
        var newWidths = {}
        ctx.save()
        for (var ni = 0; ni < nodes.length; ni++) {
            var node = nodes[ni]
            var ports = portsByNode[node.id] || []

            ctx.font = "bold 11px sans-serif"
            var titleW = ctx.measureText(node.name || "").width + nodePadding * 2

            ctx.font = "10px sans-serif"
            var maxInputW = 0
            var maxOutputW = 0
            for (var pi = 0; pi < ports.length; pi++) {
                var pw = ctx.measureText(ports[pi].name || "").width
                if (ports[pi].direction === "Input") {
                    if (pw > maxInputW) maxInputW = pw
                } else {
                    if (pw > maxOutputW) maxOutputW = pw
                }
            }

            var portW = portRadius + 4 + maxInputW + nodePadding * 2 + maxOutputW + 4 + portRadius

            var w = Math.max(titleW, portW, minNodeWidth)
            w = Math.min(w, maxNodeWidth)
            newWidths[node.id] = Math.ceil(w)
        }
        ctx.restore()
        nodeWidths = newWidths
    }

    function getNodeWidth(nodeId) {
        return nodeWidths[nodeId] || minNodeWidth
    }

    function toCanvas(sx, sy) {
        return { x: (sx - panX) / zoom, y: (sy - panY) / zoom }
    }

    function toScreen(cx, cy) {
        return { x: cx * zoom + panX, y: cy * zoom + panY }
    }

    function findPortAt(sx, sy) {
        var hitRadius = portRadius * zoom * 2.5
        var bestId = -1
        var bestDist = hitRadius + 1
        for (var pid in portPositions) {
            var pp = portPositions[pid]
            var screenX = pp.cx * zoom + panX
            var screenY = pp.cy * zoom + panY
            var dx = sx - screenX
            var dy = sy - screenY
            var dist = Math.sqrt(dx*dx + dy*dy)
            if (dist < bestDist) {
                bestDist = dist
                bestId = parseInt(pid)
            }
        }
        return bestDist <= hitRadius ? bestId : -1
    }

    function getPortDirection(portId) {
        for (var nid in portsByNode) {
            var ports = portsByNode[nid]
            for (var i = 0; i < ports.length; i++) {
                if (ports[i].id === portId) return ports[i].direction
            }
        }
        return ""
    }

    function getPortNodeId(portId) {
        for (var nid in portsByNode) {
            var ports = portsByNode[nid]
            for (var i = 0; i < ports.length; i++) {
                if (ports[i].id === portId) return parseInt(nid)
            }
        }
        return -1
    }

    function findNodeAt(sx, sy) {
        var c = toCanvas(sx, sy)
        for (var i = nodes.length - 1; i >= 0; i--) {
            var n = nodes[i]
            if (n.layoutKey && hiddenNodes[n.layoutKey]) continue
            var pos = nodePositions[n.id]
            if (!pos) continue
            var h = calculateNodeHeight(n)
            if (c.x >= pos.x && c.x <= pos.x + getNodeWidth(n.id) &&
                c.y >= pos.y && c.y <= pos.y + h) {
                return n.id
            }
        }
        return -1
    }

    function findButtonAt(sx, sy) {
        var c = toCanvas(sx, sy)
        for (var i = nodes.length - 1; i >= 0; i--) {
            var n = nodes[i]
            if (n.type !== "Plugin") continue
            if (n.layoutKey && hiddenNodes[n.layoutKey]) continue
            var pos = nodePositions[n.id]
            if (!pos) continue
            var h = calculateNodeHeight(n)
            var nw = getNodeWidth(n.id)
            var btnY = pos.y + h - buttonRowHeight - nodePadding
            var btnH = buttonRowHeight
            var btnW = (nw - nodePadding * 3) / 2
            if (c.x >= pos.x + nodePadding && c.x <= pos.x + nodePadding + btnW &&
                c.y >= btnY && c.y <= btnY + btnH) {
                return { button: "ui", nodeId: n.id, hasUi: n.pluginHasUi !== false }
            }
            if (c.x >= pos.x + nodePadding * 2 + btnW && c.x <= pos.x + nodePadding * 2 + btnW * 2 &&
                c.y >= btnY && c.y <= btnY + btnH) {
                return { button: "params", nodeId: n.id }
            }
        }
        return null
    }

    Canvas {
        id: canvas
        anchors.fill: parent

        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()

            calculateNodeWidths(ctx)

            ctx.fillStyle = "#1e1e1e"
            ctx.fillRect(0, 0, width, height)

            ctx.save()
            ctx.translate(panX, panY)
            ctx.scale(zoom, zoom)

            var newPortPositions = {}

            // Draw nodes first so port positions are computed,
            // then draw links on top using the fresh positions.
            for (var ni = 0; ni < nodes.length; ni++) {
                var node = nodes[ni]
                if (node.layoutKey && hiddenNodes[node.layoutKey]) continue
                var pos = nodePositions[node.id]
                if (!pos) continue

                var x = pos.x
                var y = pos.y
                var ports = portsByNode[node.id] || []
                var inputs = ports.filter(function(p) { return p.direction === "Input" })
                    .sort(function(a, b) { return a.name.localeCompare(b.name) })
                var outputs = ports.filter(function(p) { return p.direction === "Output" })
                    .sort(function(a, b) { return a.name.localeCompare(b.name) })
                var rows = Math.max(inputs.length, outputs.length, 1)
                var h = calculateNodeHeight(node)
                var nw = getNodeWidth(node.id)

                var isNodeSelected = selectedNodes[node.id] === true
                var isDefaultNode = defaultNodeKey !== "" && node.layoutKey === defaultNodeKey
                ctx.fillStyle = "" + colNodeBg
                if (isNodeSelected) {
                    ctx.strokeStyle = "#FFFF00"
                    ctx.lineWidth = 2.5
                } else if (isDefaultNode) {
                    ctx.strokeStyle = "" + colDefaultOutline
                    ctx.lineWidth = 2.5
                } else {
                    ctx.strokeStyle = "" + colNodeBorder
                    ctx.lineWidth = 1.5
                }
                roundRect(ctx, x, y, nw, h, 5)

                // Draw "DEFAULT" badge for the default node
                if (isDefaultNode) {
                    ctx.font = "bold 8px sans-serif"
                    var defBadgeText = "DEFAULT"
                    var defBadgeW = ctx.measureText(defBadgeText).width + 6
                    var defBadgeH = 12
                    var defBadgeX = x + 4
                    var defBadgeY = y + 3
                    ctx.fillStyle = "#004422"
                    ctx.strokeStyle = "" + colDefaultOutline
                    ctx.lineWidth = 1
                    roundRect(ctx, defBadgeX, defBadgeY, defBadgeW, defBadgeH, 2)
                    ctx.fillStyle = "" + colDefaultOutline
                    ctx.textAlign = "center"
                    ctx.textBaseline = "middle"
                    ctx.fillText(defBadgeText, defBadgeX + defBadgeW / 2, defBadgeY + defBadgeH / 2)
                }

                ctx.fillStyle = "" + getNodeColor(node)
                roundRectTop(ctx, x, y, nw, headerHeight, 5)

                ctx.fillStyle = "#ffffff"
                ctx.font = "bold 11px sans-serif"
                ctx.textAlign = "center"
                ctx.textBaseline = "middle"
                ctx.fillText(truncate(node.name, 30), x + nw / 2, y + headerHeight / 2)

                // Draw format badge (LV2/CLAP/VST3) for plugin nodes
                if (node.type === "Plugin" && node.pluginFormat) {
                    var fmt = node.pluginFormat
                    var badgeColor = fmt === "CLAP" ? "#1a3a2a" : fmt === "VST3" ? "#3a2a1a" : "#1a3a5a"
                    var badgeTextCol = fmt === "CLAP" ? "#60e0a0" : fmt === "VST3" ? "#e0a060" : "#60a0e0"
                    ctx.font = "bold 8px sans-serif"
                    var badgeW = ctx.measureText(fmt).width + 6
                    var badgeH = 12
                    var badgeX = x + nw - badgeW - 4
                    var badgeY = y + 3
                    var br = 2
                    ctx.fillStyle = badgeColor
                    ctx.strokeStyle = badgeColor
                    ctx.lineWidth = 1
                    roundRect(ctx, badgeX, badgeY, badgeW, badgeH, br)
                    ctx.fillStyle = badgeTextCol
                    ctx.textAlign = "center"
                    ctx.textBaseline = "middle"
                    ctx.fillText(fmt, badgeX + badgeW / 2, badgeY + badgeH / 2)
                }

                var portBaseY = y + headerHeight + nodePadding
                for (var pi = 0; pi < inputs.length; pi++) {
                    var py = portBaseY + pi * (portHeight + portSpacing) + portHeight / 2
                    var px = x

                    ctx.fillStyle = inputs[pi].mediaType === "Midi" ? ("" + colMidiPort) : ("" + colPortIn)
                    ctx.beginPath()
                    ctx.arc(px, py, portRadius, 0, Math.PI * 2)
                    ctx.fill()

                    if (connectFromPortId >= 0 && connectFromDir === "Output") {
                        var sxIn = px * zoom + panX
                        var syIn = py * zoom + panY
                        var dIn = Math.sqrt(Math.pow(connectMouseX - sxIn, 2) + Math.pow(connectMouseY - syIn, 2))
                        if (dIn < portRadius * zoom * 3) {
                            ctx.strokeStyle = "" + colLinkConnecting
                            ctx.lineWidth = 2
                            ctx.beginPath()
                            ctx.arc(px, py, portRadius + 2, 0, Math.PI * 2)
                            ctx.stroke()
                        }
                    }

                    ctx.fillStyle = "#bbbbbb"
                    ctx.font = "10px sans-serif"
                    ctx.textAlign = "left"
                    ctx.textBaseline = "middle"
                    ctx.fillText(truncate(inputs[pi].name, 24), px + portRadius + 4, py)

                    newPortPositions[inputs[pi].id] = { cx: px, cy: py }
                }

                for (var po = 0; po < outputs.length; po++) {
                    var pyo = portBaseY + po * (portHeight + portSpacing) + portHeight / 2
                    var pxo = x + nw

                    ctx.fillStyle = outputs[po].mediaType === "Midi" ? ("" + colMidiPort) : ("" + colPortOut)
                    ctx.beginPath()
                    ctx.arc(pxo, pyo, portRadius, 0, Math.PI * 2)
                    ctx.fill()

                    if (connectFromPortId >= 0 && connectFromDir === "Input") {
                        var sxOut = pxo * zoom + panX
                        var syOut = pyo * zoom + panY
                        var dOut = Math.sqrt(Math.pow(connectMouseX - sxOut, 2) + Math.pow(connectMouseY - syOut, 2))
                        if (dOut < portRadius * zoom * 3) {
                            ctx.strokeStyle = "" + colLinkConnecting
                            ctx.lineWidth = 2
                            ctx.beginPath()
                            ctx.arc(pxo, pyo, portRadius + 2, 0, Math.PI * 2)
                            ctx.stroke()
                        }
                    }

                    ctx.fillStyle = "#bbbbbb"
                    ctx.font = "10px sans-serif"
                    ctx.textAlign = "right"
                    ctx.textBaseline = "middle"
                    ctx.fillText(truncate(outputs[po].name, 24), pxo - portRadius - 4, pyo)

                    newPortPositions[outputs[po].id] = { cx: pxo, cy: pyo }
                }

                if (node.type === "Plugin") {
                    var btnY = y + h - buttonRowHeight - nodePadding
                    var btnW = (nw - nodePadding * 3) / 2
                    var btnH = buttonRowHeight
                    var hasUi = node.pluginHasUi !== false

                    // UI button — disabled (dimmed) when plugin has no UI
                    ctx.fillStyle = hasUi ? "#373737" : "#2a2a2a"
                    ctx.strokeStyle = hasUi ? "#5a5a5a" : "#3a3a3a"
                    ctx.lineWidth = 1
                    roundRect(ctx, x + nodePadding, btnY, btnW, btnH, 3)

                    ctx.fillStyle = hasUi ? "#ffffff" : "#555555"
                    ctx.font = "10px sans-serif"
                    ctx.textAlign = "center"
                    ctx.textBaseline = "middle"
                    ctx.fillText("UI", x + nodePadding + btnW / 2, btnY + btnH / 2)

                    // Params button — always active
                    ctx.fillStyle = "#373737"
                    ctx.strokeStyle = "#5a5a5a"
                    ctx.lineWidth = 1
                    roundRect(ctx, x + nodePadding * 2 + btnW, btnY, btnW, btnH, 3)

                    ctx.fillStyle = "#ffffff"
                    ctx.font = "10px sans-serif"
                    ctx.textAlign = "center"
                    ctx.textBaseline = "middle"
                    ctx.fillText("Params", x + nodePadding * 2 + btnW + btnW / 2, btnY + btnH / 2)
                }
            }

            // Draw links on top of nodes using freshly computed port positions
            for (var li = 0; li < links.length; li++) {
                var link = links[li]
                var fromPos = newPortPositions[link.outputPortId]
                var toPos = newPortPositions[link.inputPortId]
                if (fromPos && toPos) {
                    var isSelected = selectedLinks[link.id] === true
                    var isMidiLink = portMediaTypes[link.outputPortId] === "Midi"
                                  || portMediaTypes[link.inputPortId] === "Midi"
                    var linkColor = isSelected ? "#FF4444"
                                  : isMidiLink ? colLinkMidi
                                  : (link.active ? colLinkActive : colLinkInactive)
                    var linkWidth = isSelected ? 3 : 2
                    drawBezier(ctx, fromPos.cx, fromPos.cy, toPos.cx, toPos.cy,
                        linkColor, linkWidth)
                }
            }

            if (connectFromPortId >= 0) {
                var dragFrom = newPortPositions[connectFromPortId] || portPositions[connectFromPortId]
                if (dragFrom) {
                    var dragToC = graphView.toCanvas(connectMouseX, connectMouseY)
                    if (connectFromDir === "Input") {
                        drawBezier(ctx, dragToC.x, dragToC.y, dragFrom.cx, dragFrom.cy, colLinkConnecting, 2)
                    } else {
                        drawBezier(ctx, dragFrom.cx, dragFrom.cy, dragToC.x, dragToC.y, colLinkConnecting, 2)
                    }
                }
            }

            if (selectDragging) {
                var sc1 = graphView.toCanvas(selectStartX, selectStartY)
                var sc2 = graphView.toCanvas(selectEndX, selectEndY)
                var selX = Math.min(sc1.x, sc2.x)
                var selY = Math.min(sc1.y, sc2.y)
                var selW = Math.abs(sc2.x - sc1.x)
                var selH = Math.abs(sc2.y - sc1.y)
                ctx.strokeStyle = "#FFFF00"
                ctx.lineWidth = 1 / zoom
                ctx.fillStyle = "rgba(255, 255, 0, 0.08)"
                ctx.beginPath()
                ctx.rect(selX, selY, selW, selH)
                ctx.fill()
                ctx.stroke()
            }

            // Draw snap guide lines
            if (activeSnapLines.length > 0) {
                ctx.save()
                ctx.setLineDash([4 / zoom, 4 / zoom])
                ctx.strokeStyle = "#00AAFF"
                ctx.lineWidth = 1 / zoom
                // Compute visible canvas bounds
                var visLeft = -panX / zoom
                var visTop = -panY / zoom
                var visRight = (canvas.width - panX) / zoom
                var visBottom = (canvas.height - panY) / zoom
                for (var si = 0; si < activeSnapLines.length; si++) {
                    var sl = activeSnapLines[si]
                    ctx.beginPath()
                    if (sl.axis === "x") {
                        ctx.moveTo(sl.pos, visTop)
                        ctx.lineTo(sl.pos, visBottom)
                    } else {
                        ctx.moveTo(visLeft, sl.pos)
                        ctx.lineTo(visRight, sl.pos)
                    }
                    ctx.stroke()
                }
                ctx.restore()
            }

            ctx.restore()
            portPositions = newPortPositions
        }
    }

    focus: true

    Keys.onPressed: (event) => {
        if (event.key === Qt.Key_Delete || event.key === Qt.Key_Backspace) {
            deleteSelectedLinks()
            event.accepted = true
        }
        if (event.key === Qt.Key_Escape) {
            clearSelection()
            event.accepted = true
        }
    }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        hoverEnabled: true

        property real lastX: 0
        property real lastY: 0

        onPressed: (mouse) => {
            graphView.forceActiveFocus()
            lastX = mouse.x
            lastY = mouse.y

            if (mouse.button === Qt.MiddleButton) {
                return
            }

            if (mouse.button === Qt.RightButton) {
                var nodeId = findNodeAt(mouse.x, mouse.y)
                if (nodeId >= 0) {
                    contextNodeId = nodeId
                    contextNode = findNodeData(nodeId)
                    nodeContextMenu.popup()
                } else {
                    contextNodeId = -1
                    contextNode = null
                    var cPos = toCanvas(mouse.x, mouse.y)
                    pendingPluginPosition = { x: cPos.x, y: cPos.y }
                    canvasContextMenu.popup()
                }
                return
            }

            if (mouse.button === Qt.LeftButton) {
                var btnHit = findButtonAt(mouse.x, mouse.y)
                if (btnHit) {
                    if (btnHit.button === "ui" && btnHit.hasUi) {
                        controller.open_plugin_ui(btnHit.nodeId)
                    } else if (btnHit.button === "params") {
                        graphView.openPluginParams(btnHit.nodeId)
                    }
                    return
                }

                var portId = findPortAt(mouse.x, mouse.y)
                if (portId >= 0) {
                    connectFromPortId = portId
                    connectFromDir = getPortDirection(portId)
                    connectMouseX = mouse.x
                    connectMouseY = mouse.y
                    return
                }

                var nodeIdDrag = findNodeAt(mouse.x, mouse.y)
                if (nodeIdDrag >= 0) {
                    var ctrlHeld = (mouse.modifiers & Qt.ControlModifier)
                    var nodeIsSelected = selectedNodes[nodeIdDrag] === true

                    if (ctrlHeld) {
                        var newSel = Object.assign({}, selectedNodes)
                        if (nodeIsSelected) {
                            delete newSel[nodeIdDrag]
                        } else {
                            newSel[nodeIdDrag] = true
                        }
                        selectedNodes = newSel
                        selectedLinks = {}
                        canvas.requestPaint()
                        return
                    }

                    if (nodeIsSelected) {
                        groupDragging = true
                        var cg = toCanvas(mouse.x, mouse.y)
                        groupDragLastX = cg.x
                        groupDragLastY = cg.y
                    } else {
                        selectedNodes = {}
                        selectedLinks = {}
                        dragNodeId = nodeIdDrag
                        var c = toCanvas(mouse.x, mouse.y)
                        var pos = nodePositions[nodeIdDrag]
                        dragOffsetX = c.x - pos.x
                        dragOffsetY = c.y - pos.y
                    }
                    canvas.requestPaint()
                    return
                }

                var clickedLinkId = findLinkAt(mouse.x, mouse.y)
                if (clickedLinkId >= 0) {
                    var ctrlHeldLink = (mouse.modifiers & Qt.ControlModifier)
                    if (ctrlHeldLink) {
                        var newSelLinks = Object.assign({}, selectedLinks)
                        if (newSelLinks[clickedLinkId]) {
                            delete newSelLinks[clickedLinkId]
                        } else {
                            newSelLinks[clickedLinkId] = true
                        }
                        selectedLinks = newSelLinks
                    } else {
                        var freshSel = {}
                        freshSel[clickedLinkId] = true
                        selectedLinks = freshSel
                        selectedNodes = {}
                    }
                    canvas.requestPaint()
                    return
                }

                clearSelection()
                selectDragging = true
                selectStartX = mouse.x
                selectStartY = mouse.y
                selectEndX = mouse.x
                selectEndY = mouse.y
            }
        }

        onPositionChanged: (mouse) => {
            if (mouse.buttons & Qt.MiddleButton) {
                panX += mouse.x - lastX
                panY += mouse.y - lastY
                lastX = mouse.x
                lastY = mouse.y
                canvas.requestPaint()
                return
            }

            if (groupDragging && (mouse.buttons & Qt.LeftButton)) {
                var cg = toCanvas(mouse.x, mouse.y)
                var deltaX = cg.x - groupDragLastX
                var deltaY = cg.y - groupDragLastY
                groupDragLastX = cg.x
                groupDragLastY = cg.y

                // Move all selected nodes by delta
                for (var nid in selectedNodes) {
                    if (selectedNodes[nid] && nodePositions[nid]) {
                        nodePositions[nid] = {
                            x: nodePositions[nid].x + deltaX,
                            y: nodePositions[nid].y + deltaY
                        }
                    }
                }

                // Snap only while Shift is held
                if (mouse.modifiers & Qt.ShiftModifier) {
                    var refId = -1
                    for (var rid in selectedNodes) {
                        if (selectedNodes[rid]) { refId = parseInt(rid); break }
                    }
                    if (refId >= 0 && nodePositions[refId]) {
                        var rawGX = nodePositions[refId].x
                        var rawGY = nodePositions[refId].y
                        var gsnap = computeSnap(refId, rawGX, rawGY, selectedNodes)
                        var sdx = gsnap.x - rawGX
                        var sdy = gsnap.y - rawGY
                        if (sdx !== 0 || sdy !== 0) {
                            for (var sid in selectedNodes) {
                                if (selectedNodes[sid] && nodePositions[sid]) {
                                    nodePositions[sid] = {
                                        x: nodePositions[sid].x + sdx,
                                        y: nodePositions[sid].y + sdy
                                    }
                                }
                            }
                        }
                        activeSnapLines = gsnap.lines
                    } else {
                        activeSnapLines = []
                    }
                } else {
                    activeSnapLines = []
                }

                canvas.requestPaint()
                return
            }

            if (dragNodeId >= 0 && (mouse.buttons & Qt.LeftButton)) {
                var c = toCanvas(mouse.x, mouse.y)
                var rawX = c.x - dragOffsetX
                var rawY = c.y - dragOffsetY
                if (mouse.modifiers & Qt.ShiftModifier) {
                    var snap = computeSnap(dragNodeId, rawX, rawY, null)
                    nodePositions[dragNodeId] = { x: snap.x, y: snap.y }
                    activeSnapLines = snap.lines
                } else {
                    nodePositions[dragNodeId] = { x: rawX, y: rawY }
                    activeSnapLines = []
                }
                canvas.requestPaint()
                return
            }

            if (connectFromPortId >= 0) {
                connectMouseX = mouse.x
                connectMouseY = mouse.y
                canvas.requestPaint()
                return
            }

            if (selectDragging) {
                selectEndX = mouse.x
                selectEndY = mouse.y
                canvas.requestPaint()
            }
        }

        onReleased: (mouse) => {
            activeSnapLines = []
            if (mouse.button === Qt.LeftButton) {
                if (connectFromPortId >= 0) {
                    var targetId = findPortAt(mouse.x, mouse.y)
                    if (targetId >= 0 && targetId !== connectFromPortId) {
                        var targetDir = getPortDirection(targetId)
                        var fromNodeId = getPortNodeId(connectFromPortId)
                        var toNodeId = getPortNodeId(targetId)
                        if (targetDir !== connectFromDir && fromNodeId !== toNodeId) {
                            if (connectFromDir === "Output") {
                                controller.connect_ports(connectFromPortId, targetId)
                            } else {
                                controller.connect_ports(targetId, connectFromPortId)
                            }
                        }
                    }
                    connectFromPortId = -1
                    connectFromDir = ""
                    canvas.requestPaint()
                }

                if (groupDragging) {
                    groupDragging = false
                    // Resolve overlaps for each node in the group against
                    // nodes outside the group (skip fellow group members)
                    for (var gid in selectedNodes) {
                        if (selectedNodes[gid]) {
                            var gNode = findNodeData(parseInt(gid))
                            if (gNode) {
                                resolveNodeOverlap(gNode, selectedNodes)
                            }
                        }
                    }
                    persistLayout()
                    canvas.requestPaint()
                }

                if (dragNodeId >= 0) {
                    var linkUnder = findLinkUnderNode(dragNodeId)
                    if (linkUnder >= 0) {
                        var draggedNode = findNodeData(dragNodeId)
                        if (draggedNode && draggedNode.type === "Plugin") {
                            controller.insert_node_on_link(linkUnder, dragNodeId)
                        }
                    }
                    // Resolve overlap after dropping a single node
                    var droppedNode = findNodeData(dragNodeId)
                    if (droppedNode) {
                        resolveNodeOverlap(droppedNode)
                    }
                    persistLayout()
                    canvas.requestPaint()
                }
                dragNodeId = -1

                if (selectDragging) {
                    selectDragging = false
                    var dx = Math.abs(selectEndX - selectStartX)
                    var dy = Math.abs(selectEndY - selectStartY)
                    if (dx > 5 || dy > 5) {
                        selectedLinks = findLinksInRect(selectStartX, selectStartY, selectEndX, selectEndY)
                        selectedNodes = findNodesInRect(selectStartX, selectStartY, selectEndX, selectEndY)
                    }
                    canvas.requestPaint()
                }
            }
        }

        onWheel: (wheel) => {
            var oldZoom = zoom
            var factor = wheel.angleDelta.y > 0 ? 1.1 : 0.9
            var newZoom = Math.max(0.25, Math.min(3.0, oldZoom * factor))

            var mx = wheel.x
            var my = wheel.y
            var canvasX = (mx - panX) / oldZoom
            var canvasY = (my - panY) / oldZoom

            panX += canvasX * (oldZoom - newZoom)
            panY += canvasY * (oldZoom - newZoom)
            zoom = newZoom
            canvas.requestPaint()
        }
    }

    function drawBezier(ctx, x1, y1, x2, y2, color, lineWidth) {
        var ctrlDist = Math.max(Math.abs(x2 - x1) / 2, 50)
        ctx.strokeStyle = "" + color
        ctx.lineWidth = lineWidth
        ctx.beginPath()
        ctx.moveTo(x1, y1)
        ctx.bezierCurveTo(x1 + ctrlDist, y1, x2 - ctrlDist, y2, x2, y2)
        ctx.stroke()
    }

    function roundRect(ctx, x, y, w, h, r) {
        ctx.beginPath()
        ctx.moveTo(x + r, y)
        ctx.lineTo(x + w - r, y)
        ctx.arcTo(x + w, y, x + w, y + r, r)
        ctx.lineTo(x + w, y + h - r)
        ctx.arcTo(x + w, y + h, x + w - r, y + h, r)
        ctx.lineTo(x + r, y + h)
        ctx.arcTo(x, y + h, x, y + h - r, r)
        ctx.lineTo(x, y + r)
        ctx.arcTo(x, y, x + r, y, r)
        ctx.closePath()
        ctx.fill()
        ctx.stroke()
    }

    function roundRectTop(ctx, x, y, w, h, r) {
        ctx.beginPath()
        ctx.moveTo(x + r, y)
        ctx.lineTo(x + w - r, y)
        ctx.arcTo(x + w, y, x + w, y + r, r)
        ctx.lineTo(x + w, y + h)
        ctx.lineTo(x, y + h)
        ctx.lineTo(x, y + r)
        ctx.arcTo(x, y, x + r, y, r)
        ctx.closePath()
        ctx.fill()
    }

    function truncate(str, maxLen) {
        if (!str) return ""
        return str.length > maxLen ? str.substring(0, maxLen - 1) + "\u2026" : str
    }

    function findLinkAt(sx, sy) {
        var c = toCanvas(sx, sy)
        var threshold = 6 / zoom
        var bestId = -1
        var bestDist = threshold + 1

        for (var li = 0; li < links.length; li++) {
            var link = links[li]
            var fromPos = portPositions[link.outputPortId]
            var toPos = portPositions[link.inputPortId]
            if (!fromPos || !toPos) continue

            var dist = distToBezier(c.x, c.y, fromPos.cx, fromPos.cy, toPos.cx, toPos.cy)
            if (dist < bestDist) {
                bestDist = dist
                bestId = link.id
            }
        }
        return bestDist <= threshold ? bestId : -1
    }

    function findLinkUnderNode(nodeId) {
        var pos = nodePositions[nodeId]
        if (!pos) return -1
        var node = findNodeData(nodeId)
        if (!node) return -1
        var nw = getNodeWidth(nodeId)
        var nh = calculateNodeHeight(node)
        var nx = pos.x
        var ny = pos.y

        var bestId = -1
        var bestDist = Infinity

        for (var li = 0; li < links.length; li++) {
            var link = links[li]
            if (link.outputPortId === undefined) continue
            // Skip links connected to the node itself to prevent self-loops
            if (link.outputNodeId === nodeId || link.inputNodeId === nodeId) continue
            var fromPos = portPositions[link.outputPortId]
            var toPos = portPositions[link.inputPortId]
            if (!fromPos || !toPos) continue

            var ctrlDist = Math.max(Math.abs(toPos.cx - fromPos.cx) / 2, 50)
            var cx1 = fromPos.cx + ctrlDist
            var cy1 = fromPos.cy
            var cx2 = toPos.cx - ctrlDist
            var cy2 = toPos.cy
            var steps = 30
            for (var i = 0; i <= steps; i++) {
                var t = i / steps
                var u = 1 - t
                var bx = u*u*u*fromPos.cx + 3*u*u*t*cx1 + 3*u*t*t*cx2 + t*t*t*toPos.cx
                var by = u*u*u*fromPos.cy + 3*u*u*t*cy1 + 3*u*t*t*cy2 + t*t*t*toPos.cy
                if (bx >= nx && bx <= nx + nw && by >= ny && by <= ny + nh) {
                    var cx = nx + nw / 2
                    var cy = ny + nh / 2
                    var dx = bx - cx
                    var dy = by - cy
                    var d = dx*dx + dy*dy
                    if (d < bestDist) {
                        bestDist = d
                        bestId = link.id
                    }
                    break
                }
            }
        }
        return bestId
    }

    function distToBezier(px, py, x1, y1, x2, y2) {
        var ctrlDist = Math.max(Math.abs(x2 - x1) / 2, 50)
        var cx1 = x1 + ctrlDist
        var cy1 = y1
        var cx2 = x2 - ctrlDist
        var cy2 = y2
        var steps = 30
        var minDist = Infinity
        for (var i = 0; i <= steps; i++) {
            var t = i / steps
            var u = 1 - t
            var bx = u*u*u*x1 + 3*u*u*t*cx1 + 3*u*t*t*cx2 + t*t*t*x2
            var by = u*u*u*y1 + 3*u*u*t*cy1 + 3*u*t*t*cy2 + t*t*t*y2
            var dx = px - bx
            var dy = py - by
            var d = Math.sqrt(dx*dx + dy*dy)
            if (d < minDist) minDist = d
        }
        return minDist
    }

    function bezierIntersectsRect(x1, y1, x2, y2, rx, ry, rw, rh) {
        var ctrlDist = Math.max(Math.abs(x2 - x1) / 2, 50)
        var cx1 = x1 + ctrlDist
        var cy1 = y1
        var cx2 = x2 - ctrlDist
        var cy2 = y2
        var steps = 20
        for (var i = 0; i <= steps; i++) {
            var t = i / steps
            var u = 1 - t
            var px = u*u*u*x1 + 3*u*u*t*cx1 + 3*u*t*t*cx2 + t*t*t*x2
            var py = u*u*u*y1 + 3*u*u*t*cy1 + 3*u*t*t*cy2 + t*t*t*y2
            if (px >= rx && px <= rx + rw && py >= ry && py <= ry + rh) {
                return true
            }
        }
        return false
    }

    function findLinksInRect(sx1, sy1, sx2, sy2) {
        var minSx = Math.min(sx1, sx2)
        var minSy = Math.min(sy1, sy2)
        var maxSx = Math.max(sx1, sx2)
        var maxSy = Math.max(sy1, sy2)
        var c1 = toCanvas(minSx, minSy)
        var c2 = toCanvas(maxSx, maxSy)
        var rx = c1.x
        var ry = c1.y
        var rw = c2.x - c1.x
        var rh = c2.y - c1.y

        var result = {}
        for (var li = 0; li < links.length; li++) {
            var link = links[li]
            var fromPos = portPositions[link.outputPortId]
            var toPos = portPositions[link.inputPortId]
            if (fromPos && toPos) {
                if (bezierIntersectsRect(fromPos.cx, fromPos.cy, toPos.cx, toPos.cy,
                                         rx, ry, rw, rh)) {
                    result[link.id] = true
                }
            }
        }
        return result
    }

    function findNodesInRect(sx1, sy1, sx2, sy2) {
        var c1 = toCanvas(Math.min(sx1, sx2), Math.min(sy1, sy2))
        var c2 = toCanvas(Math.max(sx1, sx2), Math.max(sy1, sy2))
        var rx = c1.x
        var ry = c1.y
        var rr = c2.x
        var rb = c2.y

        var result = {}
        for (var i = 0; i < nodes.length; i++) {
            var n = nodes[i]
            if (n.layoutKey && hiddenNodes[n.layoutKey]) continue
            var pos = nodePositions[n.id]
            if (!pos) continue
            var h = calculateNodeHeight(n)
            var nx = pos.x
            var ny = pos.y
            var nr = nx + getNodeWidth(n.id)
            var nb = ny + h
            if (nx < rr && nr > rx && ny < rb && nb > ry) {
                result[n.id] = true
            }
        }
        return result
    }

    function deleteSelectedLinks() {
        var count = 0
        for (var linkId in selectedLinks) {
            if (selectedLinks[linkId]) {
                controller.disconnect_link(parseInt(linkId))
                count++
            }
        }
        if (count > 0) {
            selectedLinks = {}
            canvas.requestPaint()
        }
    }

    function clearSelection() {
        var hadSelection = false
        for (var k in selectedLinks) { hadSelection = true; break }
        if (!hadSelection) {
            for (var k2 in selectedNodes) { hadSelection = true; break }
        }
        if (hadSelection) {
            selectedLinks = {}
            selectedNodes = {}
            canvas.requestPaint()
        }
    }
}
