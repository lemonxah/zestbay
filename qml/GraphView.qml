import QtQuick
import QtQuick.Controls

// Graph visualization canvas — renders PipeWire nodes, ports, and links
// with zoom, pan, node dragging, and drag-to-connect support.
Item {
    id: graphView

    // Reference to the Rust AppController
    required property var controller

    // Signals to parent
    signal openPluginBrowser()
    signal openPluginParams(int nodeId)

    // ── State ─────────────────────────────────────────────────────
    property real zoom: 1.0
    property real panX: 0
    property real panY: 0
    property bool viewportLoaded: false

    // Node positions: { nodeId: {x, y} }
    property var nodePositions: ({})
    // Layout cursor for auto-placement
    property var layoutCursors: ({ "source": {x: 50, y: 50}, "stream": {x: 350, y: 50}, "sink": {x: 650, y: 50} })

    // Drag state
    property int dragNodeId: -1
    property real dragOffsetX: 0
    property real dragOffsetY: 0

    // Connection drag state
    property int connectFromPortId: -1
    property string connectFromDir: ""
    property real connectMouseX: 0
    property real connectMouseY: 0

    // Cached graph data
    property var nodes: []
    property var links: []
    property var portsByNode: ({})  // nodeId -> [ports]
    property var portPositions: ({})  // portId -> {cx, cy} in canvas coords
    property int refreshCount: 0

    // Layout persistence: layoutKey -> [x, y]
    property var savedLayout: ({})
    property bool layoutLoaded: false

    // Hidden nodes: set of layoutKey strings
    property var hiddenNodes: ({})
    property bool hiddenLoaded: false

    // Selection box state (screen coords)
    property bool selectDragging: false
    property real selectStartX: 0
    property real selectStartY: 0
    property real selectEndX: 0
    property real selectEndY: 0
    // Set of selected link IDs: { linkId: true }
    property var selectedLinks: ({})
    // Set of selected node IDs: { nodeId: true }
    property var selectedNodes: ({})
    // Group drag state
    property bool groupDragging: false
    property real groupDragLastX: 0
    property real groupDragLastY: 0

    // Context menu state
    property int contextNodeId: -1
    property var contextNode: null

    // ── Constants ─────────────────────────────────────────────────
    readonly property real nodeWidth: 240
    readonly property real headerHeight: 26
    readonly property real portHeight: 18
    readonly property real portSpacing: 3
    readonly property real portRadius: 5
    readonly property real nodePadding: 8
    readonly property real buttonRowHeight: 22

    // ── Colors ────────────────────────────────────────────────────
    readonly property color colSink: "#4682B4"
    readonly property color colSource: "#3CB371"
    readonly property color colStreamOut: "#FFA500"
    readonly property color colStreamIn: "#BA55D3"
    readonly property color colDuplex: "#FFD700"
    readonly property color colLv2: "#00BFFF"
    readonly property color colDefault: "#808080"
    readonly property color colNodeBg: "#282828"
    readonly property color colNodeBorder: "#3c3c3c"
    readonly property color colPortIn: "#6495ED"
    readonly property color colPortOut: "#90EE90"
    readonly property color colLinkActive: "#32CD32"
    readonly property color colLinkInactive: "#555555"
    readonly property color colLinkConnecting: "#FFFF00"

    // ── Data refresh ──────────────────────────────────────────────
    function refreshData() {
        // Restore saved viewport (pan/zoom) on first refresh
        if (!viewportLoaded) {
            try {
                var vp = JSON.parse(controller.get_viewport_json())
                if (vp.panX !== undefined) panX = vp.panX
                if (vp.panY !== undefined) panY = vp.panY
                if (vp.zoom !== undefined) zoom = vp.zoom
            } catch(e) {}
            viewportLoaded = true
        }

        // Load saved layout and hidden nodes on first refresh
        if (!layoutLoaded) {
            try {
                savedLayout = JSON.parse(controller.get_layout_json())
            } catch(e) {
                savedLayout = {}
            }
            layoutLoaded = true
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

        // Fetch ports for each node
        var newPorts = {}
        for (var i = 0; i < nodes.length; i++) {
            try {
                newPorts[nodes[i].id] = JSON.parse(controller.get_ports_json(nodes[i].id))
            } catch(e) {
                newPorts[nodes[i].id] = []
            }
        }
        portsByNode = newPorts

        // Auto-layout nodes without positions, using saved layout if available
        for (var ni = 0; ni < nodes.length; ni++) {
            var n = nodes[ni]
            if (!(n.id in nodePositions)) {
                // Check if we have a saved position for this node's layoutKey
                var key = n.layoutKey || ""
                if (key && savedLayout[key]) {
                    var saved = savedLayout[key]
                    nodePositions[n.id] = { x: saved[0], y: saved[1] }
                } else {
                    var col = getNodeColumn(n.type)
                    var cursor = layoutCursors[col]
                    nodePositions[n.id] = { x: cursor.x, y: cursor.y }
                    cursor.y += calculateNodeHeight(n) + 20
                }
            }
        }

        // First paint populates portPositions, second paint draws links
        // using the updated positions.
        canvas.requestPaint()
        repaintTimer.restart()
    }

    /// Save current node positions to disk via the controller.
    /// Merges current positions into savedLayout so that positions of
    /// nodes that are temporarily offline (e.g. a game that restarted)
    /// are preserved rather than discarded.
    function persistLayout() {
        // Start from the existing saved layout to keep positions of
        // nodes that aren't currently in the graph.
        var layoutObj = {}
        for (var existingKey in savedLayout) {
            layoutObj[existingKey] = savedLayout[existingKey]
        }
        // Update / add positions for all currently visible nodes
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

    // Deferred second repaint so links render with correct port positions
    Timer {
        id: repaintTimer
        interval: 16  // ~1 frame at 60fps
        repeat: false
        onTriggered: canvas.requestPaint()
    }

    // Debounced viewport (pan/zoom) persistence
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

    // ── Context Menus ──────────────────────────────────────────────

    // Node context menu (right-click on a node)
    Menu {
        id: nodeContextMenu

        MenuItem {
            text: "Hide"
            onTriggered: {
                if (contextNode && contextNode.layoutKey) {
                    hiddenNodes[contextNode.layoutKey] = true
                    // Trigger reactive update
                    hiddenNodes = hiddenNodes
                    persistHidden()
                    canvas.requestPaint()
                }
            }
        }

        MenuSeparator {
            visible: contextNode && contextNode.type === "Lv2Plugin"
        }

        MenuItem {
            text: "Rename..."
            visible: contextNode !== null && contextNode.type === "Lv2Plugin"
            height: visible ? implicitHeight : 0
            onTriggered: {
                renameField.text = contextNode ? contextNode.name : ""
                renameDialog.open()
            }
        }

        MenuItem {
            text: "Open UI..."
            visible: contextNode !== null && contextNode.type === "Lv2Plugin"
            height: visible ? implicitHeight : 0
            onTriggered: {
                if (contextNodeId >= 0)
                    controller.open_plugin_ui(contextNodeId)
            }
        }

        MenuSeparator {
            visible: contextNode !== null && contextNode.type === "Lv2Plugin"
        }

        MenuItem {
            text: "Remove Plugin"
            visible: contextNode !== null && contextNode.type === "Lv2Plugin"
            height: visible ? implicitHeight : 0
            onTriggered: {
                if (contextNodeId >= 0)
                    controller.remove_plugin(contextNodeId)
            }
        }
    }

    // Canvas context menu (right-click on empty space)
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

    // Rename dialog
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

    // ── Hidden persistence helper ──────────────────────────────────
    function persistHidden() {
        var arr = []
        for (var k in hiddenNodes) {
            if (hiddenNodes[k]) arr.push(k)
        }
        controller.save_hidden(JSON.stringify(arr))
    }

    // ── Node lookup by ID ──────────────────────────────────────────
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
        // StreamOutput, StreamInput, Lv2Plugin, Unknown → middle column
        return "stream"
    }

    function getNodeColor(type) {
        if (!type) return colDefault
        if (type === "Sink") return colSink
        if (type === "Source") return colSource
        if (type === "StreamOutput") return colStreamOut
        if (type === "StreamInput") return colStreamIn
        if (type === "Duplex") return colDuplex
        if (type === "Lv2Plugin") return colLv2
        return colDefault
    }

    function calculateNodeHeight(node) {
        var ports = portsByNode[node.id] || []
        var inputs = ports.filter(function(p) { return p.direction === "Input" }).length
        var outputs = ports.filter(function(p) { return p.direction === "Output" }).length
        var rows = Math.max(inputs, outputs, 1)
        var h = headerHeight + nodePadding * 2 + rows * (portHeight + portSpacing)
        // Add button row for LV2 plugins
        if (node.type === "Lv2Plugin")
            h += buttonRowHeight + nodePadding
        return h
    }

    // Screen coords -> canvas coords
    function toCanvas(sx, sy) {
        return { x: (sx - panX) / zoom, y: (sy - panY) / zoom }
    }

    // Canvas coords -> screen coords
    function toScreen(cx, cy) {
        return { x: cx * zoom + panX, y: cy * zoom + panY }
    }

    // Find port at screen position (portPositions stores canvas coords)
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

    // Find node at screen position
    function findNodeAt(sx, sy) {
        var c = toCanvas(sx, sy)
        for (var i = nodes.length - 1; i >= 0; i--) {
            var n = nodes[i]
            // Skip hidden nodes
            if (n.layoutKey && hiddenNodes[n.layoutKey]) continue
            var pos = nodePositions[n.id]
            if (!pos) continue
            var h = calculateNodeHeight(n)
            if (c.x >= pos.x && c.x <= pos.x + nodeWidth &&
                c.y >= pos.y && c.y <= pos.y + h) {
                return n.id
            }
        }
        return -1
    }

    // Check if screen position hits an LV2 button. Returns "ui", "params", or ""
    function findButtonAt(sx, sy) {
        var c = toCanvas(sx, sy)
        for (var i = nodes.length - 1; i >= 0; i--) {
            var n = nodes[i]
            if (n.type !== "Lv2Plugin") continue
            if (n.layoutKey && hiddenNodes[n.layoutKey]) continue
            var pos = nodePositions[n.id]
            if (!pos) continue
            var h = calculateNodeHeight(n)
            var btnY = pos.y + h - buttonRowHeight - nodePadding
            var btnH = buttonRowHeight
            var btnW = (nodeWidth - nodePadding * 3) / 2
            // UI button (left)
            if (c.x >= pos.x + nodePadding && c.x <= pos.x + nodePadding + btnW &&
                c.y >= btnY && c.y <= btnY + btnH) {
                return { button: "ui", nodeId: n.id }
            }
            // Params button (right)
            if (c.x >= pos.x + nodePadding * 2 + btnW && c.x <= pos.x + nodePadding * 2 + btnW * 2 &&
                c.y >= btnY && c.y <= btnY + btnH) {
                return { button: "params", nodeId: n.id }
            }
        }
        return null
    }

    // ── Canvas ────────────────────────────────────────────────────
    Canvas {
        id: canvas
        anchors.fill: parent

        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()

            // Background
            ctx.fillStyle = "#1e1e1e"
            ctx.fillRect(0, 0, width, height)

            ctx.save()
            ctx.translate(panX, panY)
            ctx.scale(zoom, zoom)

            var newPortPositions = {}

            // ── Draw links ────────────────────────────────────────
            for (var li = 0; li < links.length; li++) {
                var link = links[li]
                var fromPos = portPositions[link.outputPortId]
                var toPos = portPositions[link.inputPortId]
                if (fromPos && toPos) {
                    var isSelected = selectedLinks[link.id] === true
                    var linkColor = isSelected ? "#FF4444" : (link.active ? colLinkActive : colLinkInactive)
                    var linkWidth = isSelected ? 3 : 2
                    drawBezier(ctx, fromPos.cx, fromPos.cy, toPos.cx, toPos.cy,
                        linkColor, linkWidth)
                }
            }

            // ── Draw in-progress connection ───────────────────────
            if (connectFromPortId >= 0) {
                var dragFrom = portPositions[connectFromPortId]
                if (dragFrom) {
                    var dragToC = graphView.toCanvas(connectMouseX, connectMouseY)
                    if (connectFromDir === "Input") {
                        drawBezier(ctx, dragToC.x, dragToC.y, dragFrom.cx, dragFrom.cy, colLinkConnecting, 2)
                    } else {
                        drawBezier(ctx, dragFrom.cx, dragFrom.cy, dragToC.x, dragToC.y, colLinkConnecting, 2)
                    }
                }
            }

            // ── Draw selection box ─────────────────────────────────
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

            // ── Draw nodes ────────────────────────────────────────
            for (var ni = 0; ni < nodes.length; ni++) {
                var node = nodes[ni]
                // Skip hidden nodes
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

                // Node body
                var isNodeSelected = selectedNodes[node.id] === true
                ctx.fillStyle = "" + colNodeBg
                ctx.strokeStyle = isNodeSelected ? "#FFFF00" : ("" + colNodeBorder)
                ctx.lineWidth = isNodeSelected ? 2.5 : 1.5
                roundRect(ctx, x, y, nodeWidth, h, 5)

                // Header
                ctx.fillStyle = "" + getNodeColor(node.type)
                roundRectTop(ctx, x, y, nodeWidth, headerHeight, 5)

                // Title
                ctx.fillStyle = "#ffffff"
                ctx.font = "bold 11px sans-serif"
                ctx.textAlign = "center"
                ctx.textBaseline = "middle"
                ctx.fillText(truncate(node.name, 30), x + nodeWidth / 2, y + headerHeight / 2)

                // Input ports (left side)
                var portBaseY = y + headerHeight + nodePadding
                for (var pi = 0; pi < inputs.length; pi++) {
                    var py = portBaseY + pi * (portHeight + portSpacing) + portHeight / 2
                    var px = x

                    // Port dot
                    ctx.fillStyle = "" + colPortIn
                    ctx.beginPath()
                    ctx.arc(px, py, portRadius, 0, Math.PI * 2)
                    ctx.fill()

                    // Hover highlight when dragging from an output port
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

                    // Port label
                    ctx.fillStyle = "#bbbbbb"
                    ctx.font = "10px sans-serif"
                    ctx.textAlign = "left"
                    ctx.textBaseline = "middle"
                    ctx.fillText(truncate(inputs[pi].name, 24), px + portRadius + 4, py)

                    // Store canvas position for link drawing and hit-testing
                    newPortPositions[inputs[pi].id] = { cx: px, cy: py }
                }

                // Output ports (right side)
                for (var po = 0; po < outputs.length; po++) {
                    var pyo = portBaseY + po * (portHeight + portSpacing) + portHeight / 2
                    var pxo = x + nodeWidth

                    // Port dot
                    ctx.fillStyle = "" + colPortOut
                    ctx.beginPath()
                    ctx.arc(pxo, pyo, portRadius, 0, Math.PI * 2)
                    ctx.fill()

                    // Hover highlight when dragging from an input port
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

                    // Port label
                    ctx.fillStyle = "#bbbbbb"
                    ctx.font = "10px sans-serif"
                    ctx.textAlign = "right"
                    ctx.textBaseline = "middle"
                    ctx.fillText(truncate(outputs[po].name, 24), pxo - portRadius - 4, pyo)

                    newPortPositions[outputs[po].id] = { cx: pxo, cy: pyo }
                }

                // ── LV2 button row ─────────────────────────────────
                if (node.type === "Lv2Plugin") {
                    var btnY = y + h - buttonRowHeight - nodePadding
                    var btnW = (nodeWidth - nodePadding * 3) / 2
                    var btnH = buttonRowHeight

                    // "UI" button (left)
                    ctx.fillStyle = "#373737"
                    ctx.strokeStyle = "#5a5a5a"
                    ctx.lineWidth = 1
                    roundRect(ctx, x + nodePadding, btnY, btnW, btnH, 3)

                    ctx.fillStyle = "#ffffff"
                    ctx.font = "10px sans-serif"
                    ctx.textAlign = "center"
                    ctx.textBaseline = "middle"
                    ctx.fillText("UI", x + nodePadding + btnW / 2, btnY + btnH / 2)

                    // "Params" button (right)
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

            ctx.restore()
            portPositions = newPortPositions
        }
    }

    // ── Keyboard handling ──────────────────────────────────────────
    // focus must be on the graphView Item so Keys work
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

    // ── Mouse interaction ─────────────────────────────────────────
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
                // Middle mouse: start pan
                return
            }

            if (mouse.button === Qt.RightButton) {
                // Right-click: context menu
                var nodeId = findNodeAt(mouse.x, mouse.y)
                if (nodeId >= 0) {
                    contextNodeId = nodeId
                    contextNode = findNodeData(nodeId)
                    nodeContextMenu.popup()
                } else {
                    contextNodeId = -1
                    contextNode = null
                    canvasContextMenu.popup()
                }
                return
            }

            if (mouse.button === Qt.LeftButton) {
                // Check if clicking on an LV2 button
                var btnHit = findButtonAt(mouse.x, mouse.y)
                if (btnHit) {
                    if (btnHit.button === "ui") {
                        controller.open_plugin_ui(btnHit.nodeId)
                    } else if (btnHit.button === "params") {
                        graphView.openPluginParams(btnHit.nodeId)
                    }
                    return
                }

                // Check if clicking on a port (start connection)
                var portId = findPortAt(mouse.x, mouse.y)
                if (portId >= 0) {
                    connectFromPortId = portId
                    connectFromDir = getPortDirection(portId)
                    connectMouseX = mouse.x
                    connectMouseY = mouse.y
                    return
                }

                // Check if clicking on a node (start drag)
                var nodeIdDrag = findNodeAt(mouse.x, mouse.y)
                if (nodeIdDrag >= 0) {
                    var ctrlHeld = (mouse.modifiers & Qt.ControlModifier)
                    var nodeIsSelected = selectedNodes[nodeIdDrag] === true

                    if (ctrlHeld) {
                        // Ctrl+click: toggle this node in selection
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
                        // Click on an already-selected node: start group drag
                        groupDragging = true
                        var cg = toCanvas(mouse.x, mouse.y)
                        groupDragLastX = cg.x
                        groupDragLastY = cg.y
                    } else {
                        // Click on unselected node without Ctrl: clear selection, single drag
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

                // Check if clicking on a link
                var clickedLinkId = findLinkAt(mouse.x, mouse.y)
                if (clickedLinkId >= 0) {
                    var ctrlHeldLink = (mouse.modifiers & Qt.ControlModifier)
                    if (ctrlHeldLink) {
                        // Ctrl+click: toggle this link in selection
                        var newSelLinks = Object.assign({}, selectedLinks)
                        if (newSelLinks[clickedLinkId]) {
                            delete newSelLinks[clickedLinkId]
                        } else {
                            newSelLinks[clickedLinkId] = true
                        }
                        selectedLinks = newSelLinks
                    } else {
                        // Single click: select only this link
                        var freshSel = {}
                        freshSel[clickedLinkId] = true
                        selectedLinks = freshSel
                        selectedNodes = {}
                    }
                    canvas.requestPaint()
                    return
                }

                // Click on empty space — start selection box
                clearSelection()
                selectDragging = true
                selectStartX = mouse.x
                selectStartY = mouse.y
                selectEndX = mouse.x
                selectEndY = mouse.y
            }
        }

        onPositionChanged: (mouse) => {
            // Pan with middle mouse
            if (mouse.buttons & Qt.MiddleButton) {
                panX += mouse.x - lastX
                panY += mouse.y - lastY
                lastX = mouse.x
                lastY = mouse.y
                canvas.requestPaint()
                return
            }

            // Group dragging (multiple selected nodes)
            if (groupDragging && (mouse.buttons & Qt.LeftButton)) {
                var cg = toCanvas(mouse.x, mouse.y)
                var deltaX = cg.x - groupDragLastX
                var deltaY = cg.y - groupDragLastY
                groupDragLastX = cg.x
                groupDragLastY = cg.y
                for (var nid in selectedNodes) {
                    if (selectedNodes[nid] && nodePositions[nid]) {
                        nodePositions[nid] = {
                            x: nodePositions[nid].x + deltaX,
                            y: nodePositions[nid].y + deltaY
                        }
                    }
                }
                canvas.requestPaint()
                return
            }

            // Single node dragging
            if (dragNodeId >= 0 && (mouse.buttons & Qt.LeftButton)) {
                var c = toCanvas(mouse.x, mouse.y)
                nodePositions[dragNodeId] = {
                    x: c.x - dragOffsetX,
                    y: c.y - dragOffsetY
                }
                canvas.requestPaint()
                return
            }

            // Connection dragging
            if (connectFromPortId >= 0) {
                connectMouseX = mouse.x
                connectMouseY = mouse.y
                canvas.requestPaint()
                return
            }

            // Selection box dragging
            if (selectDragging) {
                selectEndX = mouse.x
                selectEndY = mouse.y
                canvas.requestPaint()
            }
        }

        onReleased: (mouse) => {
            if (mouse.button === Qt.LeftButton) {
                // Finish connection
                if (connectFromPortId >= 0) {
                    var targetId = findPortAt(mouse.x, mouse.y)
                    if (targetId >= 0 && targetId !== connectFromPortId) {
                        var targetDir = getPortDirection(targetId)
                        if (targetDir !== connectFromDir) {
                            // Connect!
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

                // Persist layout if we were group-dragging
                if (groupDragging) {
                    groupDragging = false
                    persistLayout()
                }

                // Persist layout if we were dragging a single node
                if (dragNodeId >= 0) {
                    persistLayout()
                }
                dragNodeId = -1

                // Finish selection box
                if (selectDragging) {
                    selectDragging = false
                    // Only select if the box is bigger than a small threshold (avoid click-select)
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

            // Zoom centered on mouse position
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

    // Zoom with mouse wheel — handled via MouseArea.onWheel which is
    // more reliable across Qt 6 versions than WheelHandler.

    // ── Helper functions ──────────────────────────────────────────
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

    // ── Link hit-testing ─────────────────────────────────────────

    /// Test if a screen point is close to a bezier link curve.
    /// Returns the link id of the closest link within threshold, or -1.
    function findLinkAt(sx, sy) {
        var c = toCanvas(sx, sy)
        var threshold = 6 / zoom  // pixel tolerance in canvas coords
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

    /// Compute the minimum distance from point (px,py) to a cubic bezier
    /// with the same control-point logic as drawBezier.
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

    // ── Selection box helpers ─────────────────────────────────────

    /// Test if a cubic bezier (same control-point logic as drawBezier)
    /// passes through a rectangle given in canvas coords.
    /// We sample N points along the curve and check if any fall inside.
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

    /// Find all links that intersect a selection rectangle (in screen coords).
    function findLinksInRect(sx1, sy1, sx2, sy2) {
        // Normalize rect
        var minSx = Math.min(sx1, sx2)
        var minSy = Math.min(sy1, sy2)
        var maxSx = Math.max(sx1, sx2)
        var maxSy = Math.max(sy1, sy2)
        // Convert to canvas coords
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

    /// Find all nodes whose bounding box intersects a selection rectangle (in screen coords).
    function findNodesInRect(sx1, sy1, sx2, sy2) {
        // Convert to canvas coords
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
            // Check if node rect overlaps selection rect
            var nx = pos.x
            var ny = pos.y
            var nr = nx + nodeWidth
            var nb = ny + h
            if (nx < rr && nr > rx && ny < rb && nb > ry) {
                result[n.id] = true
            }
        }
        return result
    }

    /// Delete all currently selected links.
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

    /// Clear all selection (links and nodes).
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
