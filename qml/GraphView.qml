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

    // In-hand placement: when >= 0, this node follows the cursor until the
    // user left-clicks to drop it (or Escape to drop in place). Used when a
    // newly added plugin can't find a free spot near the add-point.
    property int placingNodeId: -1

    // Nodes that appeared without any links yet, placed provisionally near the
    // cluster. Once a patchbay rule connects them, they get repositioned next to
    // the node that picked them up. Map of nodeId -> true.
    property var awaitingLinkPlacement: ({})

    // Nodes positioned automatically (not by the user, not from a saved
    // layout). Only these may be moved by the backwards-link correction pass;
    // a user drag removes the node from this set. Map of nodeId -> true.
    property var autoPlaced: ({})

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

    property var pinnedNodes: ({})
    property bool pinnedLoaded: false

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
    // Canvas point of the last right-click on empty canvas (for "Add Plugin...").
    property var contextCanvasPos: null
    // Where the next user-added plugin should appear; consumed by the plugin
    // node named pendingPluginName (or any new plugin when the name is empty).
    property var pendingPluginPosition: null
    property string pendingPluginName: ""
    property string defaultNodeKey: ""

    // Local bypass overrides for instant visual feedback
    // Maps nodeId -> bool (true = bypassed). An entry is dropped as soon as a
    // refresh reports the backend's bypass state, or when the node goes away.
    property var localBypassState: ({})

    // Per-node caches, rebuilt by refreshData(): sorted port lists, node
    // heights, and the unique layout key of each node.
    property var sortedPortsByNode: ({})
    property var nodeHeights: ({})
    property var uniqueKeyById: ({})
    // Node widths need text measurement (a canvas context); recomputed on the
    // next paint after the data or theme changed.
    property bool widthsDirty: true

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

    readonly property color colSink: Theme.colSink
    readonly property color colSource: Theme.colSource
    readonly property color colVirtualSink: Theme.colVirtualSink
    readonly property color colVirtualSource: Theme.colVirtualSource
    readonly property color colStreamOut: Theme.colStreamOut
    readonly property color colStreamIn: Theme.colStreamIn
    readonly property color colDuplex: Theme.colDuplex
    readonly property color colJack: Theme.colJack
    readonly property color colLv2: Theme.colLv2
    readonly property color colDefault: Theme.colDefault
    readonly property color colNodeBg: Theme.nodeBg
    readonly property color colNodeBorder: Theme.nodeBorder
    readonly property color colPortIn: Theme.colPortIn
    readonly property color colPortOut: Theme.colPortOut
    readonly property color colMidi: Theme.colMidi
    readonly property color colMidiPort: Theme.colMidiPort
    readonly property color colLinkActive: Theme.colLinkActive
    readonly property color colLinkInactive: Theme.colLinkInactive
    readonly property color colLinkMidi: Theme.colLinkMidi
    readonly property color colLinkConnecting: Theme.colLinkConnecting
    readonly property color colDefaultOutline: Theme.colDefaultOutline

    // Redraw graph when system theme changes
    Connections {
        target: Theme
        function onDarkChanged() {
            widthsDirty = true
            canvas.requestPaint()
        }
    }

    function portSortCmp(a, b) {
        var am = a.mediaType === "Midi" ? 0 : 1
        var bm = b.mediaType === "Midi" ? 0 : 1
        return am !== bm ? am - bm : naturalCmp(a.name, b.name)
    }

    function getSortedPorts(nodeId) {
        return sortedPortsByNode[nodeId] || { inputs: [], outputs: [] }
    }

    // Selection entries remember what they referred to, so a recycled
    // PipeWire id can never make a selection point at an unrelated object.
    function linkSig(link) {
        return { o: link.outputPortId, i: link.inputPortId,
                 on: link.outputNodeId, inn: link.inputNodeId }
    }

    function linkMatchesSig(link, sig) {
        return !!link && !!sig && link.outputPortId === sig.o && link.inputPortId === sig.i
               && link.outputNodeId === sig.on && link.inputNodeId === sig.inn
    }

    function nodeSig(node) {
        return { key: (node && node.layoutKey) || "" }
    }

    function findLinkData(linkId) {
        for (var i = 0; i < links.length; i++) {
            if (links[i].id === linkId) return links[i]
        }
        return null
    }

    function naturalCmp(a, b) {
        var ax = a.split(/(\d+)/), bx = b.split(/(\d+)/)
        for (var i = 0; i < Math.min(ax.length, bx.length); i++) {
            var an = parseInt(ax[i]), bn = parseInt(bx[i])
            if (!isNaN(an) && !isNaN(bn)) {
                if (an !== bn) return an - bn
            } else {
                if (ax[i] < bx[i]) return -1
                if (ax[i] > bx[i]) return 1
            }
        }
        return ax.length - bx.length
    }

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
        if (!pinnedLoaded) {
            try {
                var pinnedArr = JSON.parse(controller.get_pinned_json())
                var pSet = {}
                for (var pi = 0; pi < pinnedArr.length; pi++) {
                    pSet[pinnedArr[pi]] = true
                }
                pinnedNodes = pSet
            } catch(e) {
                pinnedNodes = {}
            }
            pinnedLoaded = true
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

        var newSorted = {}
        var newHeights = {}
        for (var si = 0; si < nodes.length; si++) {
            var sn = nodes[si]
            var sp = newPorts[sn.id] || []
            var sorted = {
                inputs: sp.filter(function(p) { return p.direction === "Input" }).sort(function(a, b) { return portSortCmp(a, b) }),
                outputs: sp.filter(function(p) { return p.direction === "Output" }).sort(function(a, b) { return portSortCmp(a, b) })
            }
            newSorted[sn.id] = sorted
            var rows = Math.max(sorted.inputs.length, sorted.outputs.length, 1)
            var hgt = headerHeight + nodePadding * 2 + rows * (portHeight + portSpacing)
            if (sn.type === "Plugin")
                hgt += buttonRowHeight + nodePadding
            newHeights[sn.id] = hgt
        }
        sortedPortsByNode = newSorted
        nodeHeights = newHeights
        uniqueKeyById = buildUniqueLayoutKeys()
        widthsDirty = true

        var newPortMedia = {}
        for (var nid in newPorts) {
            var pp = newPorts[nid]
            for (var pi2 = 0; pi2 < pp.length; pi2++) {
                newPortMedia[pp[pi2].id] = pp[pi2].mediaType || "Unknown"
            }
        }
        portMediaTypes = newPortMedia

        // Drop per-id state for nodes that no longer exist. PipeWire recycles
        // object ids (and bridge sub-node ids can change), so a stale entry
        // would hand a brand-new node some old node's position and bypass the
        // savedLayout restore below — the pin/saved position would be ignored
        // and then overwritten by the next persistLayout().
        var liveIds = {}
        for (var lv = 0; lv < nodes.length; lv++) liveIds[nodes[lv].id] = true
        for (var stale in nodePositions) {
            if (!liveIds[stale]) delete nodePositions[stale]
        }
        for (var staleA in awaitingLinkPlacement) {
            if (!liveIds[staleA]) delete awaitingLinkPlacement[staleA]
        }
        for (var staleP in autoPlaced) {
            if (!liveIds[staleP]) delete autoPlaced[staleP]
        }

        // Selections must refer to the same objects they did when made: drop
        // entries whose id vanished or now belongs to something else.
        var liveNodeById = {}
        for (var ln = 0; ln < nodes.length; ln++) liveNodeById[nodes[ln].id] = nodes[ln]
        var keptNodes = {}
        var nodesPruned = false
        for (var selN in selectedNodes) {
            var selNode = liveNodeById[selN]
            if (selNode && selectedNodes[selN] && selectedNodes[selN].key === (selNode.layoutKey || "")) {
                keptNodes[selN] = selectedNodes[selN]
            } else {
                nodesPruned = true
            }
        }
        if (nodesPruned) selectedNodes = keptNodes

        var liveLinkById = {}
        for (var lk = 0; lk < links.length; lk++) liveLinkById[links[lk].id] = links[lk]
        var keptLinks = {}
        var linksPruned = false
        for (var selL in selectedLinks) {
            if (linkMatchesSig(liveLinkById[selL], selectedLinks[selL])) {
                keptLinks[selL] = selectedLinks[selL]
            } else {
                linksPruned = true
            }
        }
        if (linksPruned) selectedLinks = keptLinks

        // The backend's bypass state wins once it is reported.
        var bypassPruned = false
        for (var bpId in localBypassState) {
            var bpNode = liveNodeById[bpId]
            if (!bpNode || bpNode.pluginBypassed !== undefined) {
                delete localBypassState[bpId]
                bypassPruned = true
            }
        }
        if (bypassPruned) localBypassState = localBypassState

        for (var ni = 0; ni < nodes.length; ni++) {
            var n = nodes[ni]
            if (n.id in nodePositions) continue
            var key = uniqueKeyById[n.id] || ""

            if (key && savedLayout[key]) {
                // Known position from a previous session/placement. Restore it
                // exactly and never auto-move it — a device toggling off/on (new
                // PipeWire id, same layoutKey) must reappear where the user left it.
                var saved = savedLayout[key]
                nodePositions[n.id] = { x: saved[0], y: saved[1] }
            } else if (n.type === "Plugin" && pendingPluginPosition
                       && (pendingPluginName === "" || n.name === pendingPluginName)) {
                // User-added plugin: try to drop it on a free spot near the
                // add-point. If there's no room nearby, place it "in hand" so the
                // user can position it themselves.
                var anchor = pendingPluginPosition
                pendingPluginPosition = null
                pendingPluginName = ""
                nodePositions[n.id] = { x: anchor.x, y: anchor.y }
                var spot = findFreeSpot(n, anchor.x, anchor.y, true)
                if (spot) {
                    nodePositions[n.id] = spot
                    persistLayout()
                } else {
                    startPlacing(n.id)
                }
            } else {
                // Auto-appearing node (hardware device, stream). If a patchbay
                // rule has already linked it, drop it next to the node that
                // picked it up. Otherwise place it provisionally — near the
                // default node if one is set (that's where a ruleless node will
                // connect), else near the cluster — and remember it so the
                // second pass can reposition it once its links show up.
                autoPlaced[n.id] = true
                if (!placeNearPeer(n)) {
                    var pa = provisionalAnchor(n)
                    nodePositions[n.id] = pa
                    var spot2 = findFreeSpot(n, pa.x, pa.y, false)
                    if (spot2) nodePositions[n.id] = spot2
                    persistLayout()
                    if (!nodeIsPinned(n)) {
                        awaitingLinkPlacement[n.id] = true
                        awaitingLinkPlacement = awaitingLinkPlacement
                    }
                }
            }

            // Auto-pin plugins so their manual placement sticks across restarts.
            if (n.type === "Plugin" && n.layoutKey && !pinnedNodes[n.layoutKey]) {
                pinnedNodes[n.layoutKey] = true
                pinnedNodes = pinnedNodes
                persistPinned()
            }
        }

        // Second placement pass: reposition nodes that were waiting for a rule
        // to connect them, now that their links may have arrived.
        var stillAwaiting = {}
        for (var aid in awaitingLinkPlacement) {
            var anode = findNodeData(parseInt(aid))
            if (!anode) continue            // node vanished — forget it
            if (nodeIsPinned(anode)) continue  // user pinned it — honor the pin
            if (!placeNearPeer(anode)) {
                stillAwaiting[aid] = true   // no peer yet, keep waiting
            }
        }
        awaitingLinkPlacement = stillAwaiting

        // Third pass: fix auto-placed nodes whose links flow backwards (the
        // line's input end sits left of its output end, i.e. the connection
        // angle points right-to-left). One-shot per node, and only for nodes
        // the user never touched — pinned or hand-placed nodes are sacred.
        for (var bid in autoPlaced) {
            if (awaitingLinkPlacement[bid]) continue
            var bnode = findNodeData(parseInt(bid))
            if (!bnode || nodeIsPinned(bnode)) continue
            if (nodeHasBackwardLink(parseInt(bid))) {
                delete autoPlaced[bid]
                placeNearPeer(bnode)
            }
        }

        canvas.requestPaint()
        repaintTimer.restart()
    }

    // True if any link touching `nodeId` runs right-to-left: the input end
    // (peer's left edge) is left of the output end (source's right edge).
    // Equivalent to the link line's angle atan2(dy, dx) being outside ±90°.
    // A small tolerance ignores near-vertical links between stacked nodes.
    function nodeHasBackwardLink(nodeId) {
        for (var li = 0; li < links.length; li++) {
            var l = links[li]
            if (l.outputNodeId !== nodeId && l.inputNodeId !== nodeId) continue
            if (l.outputNodeId === l.inputNodeId) continue
            var fromPos = nodePositions[l.outputNodeId]
            var toPos = nodePositions[l.inputNodeId]
            if (!fromPos || !toPos) continue
            var outX = fromPos.x + (getNodeWidth(l.outputNodeId) || minNodeWidth)
            if (toPos.x < outX - 20) return true
        }
        return false
    }

    // A node is pinned if its layout key is in the pinned set. Pinned nodes are
    // never auto-moved — pins are honored above all else.
    function nodeIsPinned(node) {
        return !!(node && node.layoutKey && pinnedNodes[node.layoutKey])
    }

    // If `node` is linked to an already-positioned peer, place it immediately
    // next to that peer (downstream → right of peer, upstream → left) on the
    // nearest free spot. Returns true if placed. Pinned nodes are never moved.
    function placeNearPeer(node) {
        if (nodeIsPinned(node)) return false
        var nodeId = node.id
        for (var li = 0; li < links.length; li++) {
            var link = links[li]
            var peerId = -1
            var nodeFeedsPeer = false  // node output → peer input (node sits left)
            if (link.outputNodeId === nodeId && (link.inputNodeId in nodePositions) && link.inputNodeId !== nodeId) {
                peerId = link.inputNodeId
                nodeFeedsPeer = true
            } else if (link.inputNodeId === nodeId && (link.outputNodeId in nodePositions) && link.outputNodeId !== nodeId) {
                peerId = link.outputNodeId
                nodeFeedsPeer = false
            }
            if (peerId < 0) continue
            var peerNode = findNodeData(peerId)
            if (!peerNode || !nodePositions[peerId]) continue

            var anchor = adjacentAnchor(node, peerNode, nodeFeedsPeer)
            var spot = findFreeSpot(node, anchor.x, anchor.y, false)
            nodePositions[nodeId] = spot || anchor
            persistLayout()
            return true
        }
        return false
    }

    // Top-left anchor placing `node` directly beside `peer`: to the peer's left
    // when the node feeds the peer (signal flows node → peer), otherwise to its
    // right. Same vertical level as the peer.
    function adjacentAnchor(node, peer, nodeFeedsPeer) {
        var peerPos = nodePositions[peer.id]
        var w = getNodeWidth(node.id) || minNodeWidth
        var peerW = getNodeWidth(peer.id) || minNodeWidth
        var spacing = 40
        var anchorX = nodeFeedsPeer ? (peerPos.x - w - spacing)
                                    : (peerPos.x + peerW + spacing)
        return { x: anchorX, y: peerPos.y }
    }

    function findNodeByLayoutKey(key) {
        if (!key) return null
        for (var i = 0; i < nodes.length; i++) {
            if (nodes[i].layoutKey === key) return nodes[i]
        }
        return null
    }

    // Position in the signal flow, used to guess which side of a peer a node
    // belongs on before any links exist (sources left → sinks right).
    function flowRank(type) {
        if (type === "Source") return 0
        if (type === "StreamOutput") return 1
        if (type === "StreamInput") return 3
        if (type === "Sink" || type === "Duplex") return 4
        return 2  // Plugin / Unknown — middle
    }

    // Where to drop a freshly-appeared node before its links exist: beside the
    // default node if one is set (a ruleless node connects there), otherwise
    // near the existing cluster. The second pass corrects this once real links
    // arrive; if the node never connects, it simply stays here.
    function provisionalAnchor(node) {
        var defNode = findNodeByLayoutKey(defaultNodeKey)
        if (defNode && defNode.id !== node.id && nodePositions[defNode.id]) {
            var nodeFeedsDefault = flowRank(node.type) <= flowRank(defNode.type)
            return adjacentAnchor(node, defNode, nodeFeedsDefault)
        }
        return clusterAnchor(node)
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
            var key = uniqueKeyById[n.id] || ""
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

        MenuItem {
            text: contextNode && contextNode.layoutKey && pinnedNodes[contextNode.layoutKey]
                  ? "Unpin" : "Pin"
            onTriggered: {
                if (contextNode && contextNode.layoutKey) {
                    var key = contextNode.layoutKey
                    if (pinnedNodes[key]) {
                        delete pinnedNodes[key]
                    } else {
                        pinnedNodes[key] = true
                    }
                    pinnedNodes = pinnedNodes
                    persistPinned()
                    canvas.requestPaint()
                }
            }
        }

        MenuSeparator {
            visible: contextNode !== null && (contextNode.type === "Sink" || contextNode.type === "Duplex" || contextNode.type === "Plugin")
            height: visible ? implicitHeight : 0
        }

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
            height: visible ? implicitHeight : 0
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
            height: visible ? implicitHeight : 0
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
            onTriggered: {
                if (contextCanvasPos) {
                    pendingPluginPosition = { x: contextCanvasPos.x, y: contextCanvasPos.y }
                    pendingPluginName = ""
                }
                graphView.openPluginBrowser()
            }
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
            onTriggered: runAutoLayout()
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

    // Confirmation dialog for inserting a plugin node onto a link
    property int pendingInsertLinkId: -1
    property int pendingInsertNodeId: -1

    Dialog {
        id: insertConfirmDialog
        title: "Insert Plugin on Link"
        standardButtons: Dialog.Yes | Dialog.No
        anchors.centerIn: parent
        modal: true
        width: 360

        contentItem: Text {
            text: "Insert this plugin between the connected nodes?"
            wrapMode: Text.WordWrap
            color: Theme.textPrimary
            padding: 12
        }

        onAccepted: {
            if (pendingInsertLinkId >= 0 && pendingInsertNodeId >= 0) {
                controller.insert_node_on_link(pendingInsertLinkId, pendingInsertNodeId)
            }
            pendingInsertLinkId = -1
            pendingInsertNodeId = -1
        }

        onRejected: {
            pendingInsertLinkId = -1
            pendingInsertNodeId = -1
        }
    }

    function persistHidden() {
        var arr = []
        for (var k in hiddenNodes) {
            if (hiddenNodes[k]) arr.push(k)
        }
        controller.save_hidden(JSON.stringify(arr))
    }

    function persistPinned() {
        var arr = []
        for (var k in pinnedNodes) {
            if (pinnedNodes[k]) arr.push(k)
        }
        controller.save_pinned(JSON.stringify(arr))
    }

    // Build a mapping from node.id → unique layout key, disambiguating nodes
    // that share a layoutKey by appending #1, #2, ... in node-id (creation)
    // order. Used for both saving and restoring layout.json, so duplicates get
    // distinct, stable keys and never persist ephemeral ids.
    function buildUniqueLayoutKeys() {
        var sorted = []
        for (var i = 0; i < nodes.length; i++) {
            var n = nodes[i]
            var key = n.layoutKey || ""
            if (key) sorted.push({ id: n.id, key: key })
        }
        sorted.sort(function(a, b) { return a.id - b.id })
        var counts = {}
        var result = {}
        for (var j = 0; j < sorted.length; j++) {
            var base = sorted[j].key
            var c = counts[base] || 0
            result[sorted[j].id] = c === 0 ? base : base + "#" + c
            counts[base] = c + 1
        }
        return result
    }

    // Sizes of the visible nodes, keyed by node id. Hidden nodes are left out
    // so the backend doesn't lay them out.
    function collectNodeSizes() {
        var sizesObj = {}
        for (var ni = 0; ni < nodes.length; ni++) {
            var n = nodes[ni]
            if (n.layoutKey && hiddenNodes[n.layoutKey]) continue
            sizesObj[n.id] = [getNodeWidth(n.id) || minNodeWidth, calculateNodeHeight(n)]
        }
        return sizesObj
    }

    function collectPinnedPositions() {
        var pinned = {}
        for (var ni = 0; ni < nodes.length; ni++) {
            var n = nodes[ni]
            if (n.layoutKey && hiddenNodes[n.layoutKey]) continue
            if (nodeIsPinned(n) && nodePositions[n.id]) {
                var pos = nodePositions[n.id]
                pinned[n.id] = [pos.x, pos.y]
            }
        }
        return pinned
    }

    function runAutoLayout() {
        var sizesJson = JSON.stringify(collectNodeSizes())
        var pinnedJson = JSON.stringify(collectPinnedPositions())
        var resultJson = controller.auto_layout(sizesJson, pinnedJson)
        try {
            // Result is keyed by node id. Only the laid-out nodes move; hidden
            // nodes keep their positions and saved entries for absent devices
            // and plugins stay in savedLayout (persistLayout merges into it).
            var positions = JSON.parse(resultJson)
            for (var ni = 0; ni < nodes.length; ni++) {
                var n = nodes[ni]
                var pos = positions[n.id]
                if (pos && pos.length >= 2) {
                    nodePositions[n.id] = { x: pos[0], y: pos[1] }
                    delete autoPlaced[n.id]
                    delete awaitingLinkPlacement[n.id]
                }
            }
            canvas.requestPaint()
            persistLayout()
        } catch(e) {
            console.log("Auto layout failed:", e)
        }
    }

    // Does a node of size w×h placed at (x, y) clear every obstacle rect,
    // keeping at least `spacing` px of gap?
    function spotIsFree(x, y, w, h, obstacles, spacing) {
        for (var oi = 0; oi < obstacles.length; oi++) {
            var o = obstacles[oi]
            if (x < o.x + o.w + spacing && x + w + spacing > o.x &&
                y < o.y + o.h + spacing && y + h + spacing > o.y) {
                return false
            }
        }
        return true
    }

    // Find the nearest free spot for `node` starting at (anchorX, anchorY),
    // searching outward in a grid spiral. When `bounded` is true the search
    // gives up after a few rings (caller falls back to in-hand placement);
    // when false it always returns a spot (falling back to the right of
    // everything). Work is capped: obstacles are gathered once and only those
    // near the search area are tested.
    function findFreeSpot(node, anchorX, anchorY, bounded) {
        var w = getNodeWidth(node.id) || minNodeWidth
        var h = calculateNodeHeight(node)
        var spacing = 20
        var stepX = w + spacing
        var stepY = h + spacing
        var maxRings = bounded ? 4 : 24

        var reachX = (maxRings + 1) * stepX + maxNodeWidth
        var reachY = (maxRings + 1) * stepY + h
        var obstacles = []
        var maxRight = -Infinity
        for (var oi = 0; oi < nodes.length; oi++) {
            var other = nodes[oi]
            if (other.id === node.id) continue
            if (other.layoutKey && hiddenNodes[other.layoutKey]) continue
            var opos = nodePositions[other.id]
            if (!opos) continue
            var ow = getNodeWidth(other.id) || minNodeWidth
            maxRight = Math.max(maxRight, opos.x + ow)
            if (Math.abs(opos.x - anchorX) > reachX || Math.abs(opos.y - anchorY) > reachY) continue
            obstacles.push({ x: opos.x, y: opos.y, w: ow, h: calculateNodeHeight(other) })
        }

        if (spotIsFree(anchorX, anchorY, w, h, obstacles, spacing)) {
            return { x: anchorX, y: anchorY }
        }

        for (var ring = 1; ring <= maxRings; ring++) {
            for (var dx = -ring; dx <= ring; dx++) {
                for (var dy = -ring; dy <= ring; dy++) {
                    // Only test the perimeter of the current ring.
                    if (Math.abs(dx) !== ring && Math.abs(dy) !== ring) continue
                    var cx = anchorX + dx * stepX
                    var cy = anchorY + dy * stepY
                    if (spotIsFree(cx, cy, w, h, obstacles, spacing)) {
                        return { x: cx, y: cy }
                    }
                }
            }
        }
        if (bounded) return null
        // Nothing nearby: to the right of every node is always free.
        return { x: (maxRight > -Infinity ? maxRight : anchorX) + spacing * 2, y: anchorY }
    }

    // Anchor point for auto-appearing nodes: the centre of the existing
    // visible cluster (or a default origin if the canvas is empty).
    function clusterAnchor(node) {
        var minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
        var count = 0
        for (var i = 0; i < nodes.length; i++) {
            var o = nodes[i]
            if (o.id === node.id) continue
            if (o.layoutKey && hiddenNodes[o.layoutKey]) continue
            var p = nodePositions[o.id]
            if (!p) continue
            minX = Math.min(minX, p.x); minY = Math.min(minY, p.y)
            maxX = Math.max(maxX, p.x); maxY = Math.max(maxY, p.y)
            count++
        }
        if (count === 0) return { x: 50, y: 50 }
        return { x: (minX + maxX) / 2, y: (minY + maxY) / 2 }
    }

    // Begin in-hand placement: the node tracks the cursor until dropped.
    function startPlacing(nodeId) {
        placingNodeId = nodeId
        activeSnapLines = []
        canvas.requestPaint()
    }

    // Finish in-hand placement, leaving the node wherever it currently sits.
    function finishPlacing() {
        if (placingNodeId < 0) return
        delete autoPlaced[placingNodeId]
        placingNodeId = -1
        persistLayout()
        canvas.requestPaint()
    }

    function isNodeBypassed(node) {
        if (node.id in localBypassState) return localBypassState[node.id]
        return node.pluginBypassed === true
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
        var cached = nodeHeights[node.id]
        if (cached !== undefined) return cached
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

    // Is canvas point (cx, cy) covered by a visible node drawn above z-index `z`?
    function coveredAbove(cx, cy, z) {
        for (var i = nodes.length - 1; i > z; i--) {
            var n = nodes[i]
            if (n.layoutKey && hiddenNodes[n.layoutKey]) continue
            var pos = nodePositions[n.id]
            if (!pos) continue
            if (cx >= pos.x && cx <= pos.x + getNodeWidth(n.id) &&
                cy >= pos.y && cy <= pos.y + calculateNodeHeight(n)) {
                return true
            }
        }
        return false
    }

    // Nearest port under the cursor whose node isn't covered there by a node
    // drawn on top of it.
    function findPortAt(sx, sy) {
        var hitRadius = portRadius * zoom * 2.5
        var c = toCanvas(sx, sy)
        var bestId = -1
        var bestDist = hitRadius + 1
        for (var pid in portPositions) {
            var pp = portPositions[pid]
            var screenX = pp.cx * zoom + panX
            var screenY = pp.cy * zoom + panY
            var dx = sx - screenX
            var dy = sy - screenY
            var dist = Math.sqrt(dx*dx + dy*dy)
            if (dist < bestDist && !coveredAbove(c.x, c.y, pp.z !== undefined ? pp.z : -1)) {
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

    // Pin glyph hit box (canvas coords relative to the node's top-right).
    readonly property real pinAreaWidth: 20

    // Only the topmost node under the cursor can have its pin/buttons hit.
    function findPinButtonAt(sx, sy) {
        var topId = findNodeAt(sx, sy)
        if (topId < 0) return null
        var n = findNodeData(topId)
        var pos = n ? nodePositions[n.id] : null
        if (!pos) return null
        var c = toCanvas(sx, sy)
        var nw = getNodeWidth(n.id)
        var btnX = pos.x + nw - 18
        var btnY = pos.y + 2
        var btnSize = 16
        if (c.x >= btnX && c.x <= btnX + btnSize &&
            c.y >= btnY && c.y <= btnY + btnSize) {
            return n
        }
        return null
    }

    function findButtonAt(sx, sy) {
        var topId = findNodeAt(sx, sy)
        if (topId < 0) return null
        var n = findNodeData(topId)
        if (!n || n.type !== "Plugin") return null
        var pos = nodePositions[n.id]
        if (!pos) return null
        var c = toCanvas(sx, sy)
        var h = calculateNodeHeight(n)
        var nw = getNodeWidth(n.id)
        var btnY = pos.y + h - buttonRowHeight - nodePadding
        var btnH = buttonRowHeight
        var btnW = (nw - nodePadding * 4) / 3
        // On/Off button
        if (c.x >= pos.x + nodePadding && c.x <= pos.x + nodePadding + btnW &&
            c.y >= btnY && c.y <= btnY + btnH) {
            return { button: "onoff", nodeId: n.id, bypassed: isNodeBypassed(n) }
        }
        // UI button
        if (c.x >= pos.x + nodePadding * 2 + btnW && c.x <= pos.x + nodePadding * 2 + btnW * 2 &&
            c.y >= btnY && c.y <= btnY + btnH) {
            return { button: "ui", nodeId: n.id, hasUi: n.pluginHasUi !== false }
        }
        // Params button
        if (c.x >= pos.x + nodePadding * 3 + btnW * 2 && c.x <= pos.x + nodePadding * 3 + btnW * 3 &&
            c.y >= btnY && c.y <= btnY + btnH) {
            return { button: "params", nodeId: n.id }
        }
        return null
    }

    Canvas {
        id: canvas
        anchors.fill: parent

        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()

            if (widthsDirty) {
                calculateNodeWidths(ctx)
                widthsDirty = false
            }

            ctx.fillStyle = "" + Theme.windowBg
            ctx.fillRect(0, 0, width, height)

            ctx.save()
            ctx.translate(panX, panY)
            ctx.scale(zoom, zoom)

            var newPortPositions = {}

            // Pass 1: compute port positions (no drawing yet)
            for (var ni = 0; ni < nodes.length; ni++) {
                var node = nodes[ni]
                if (node.layoutKey && hiddenNodes[node.layoutKey]) continue
                var pos = nodePositions[node.id]
                if (!pos) continue

                var x = pos.x
                var y = pos.y
                var sortedPorts = getSortedPorts(node.id)
                var inputs = sortedPorts.inputs
                var outputs = sortedPorts.outputs
                var nw = getNodeWidth(node.id)

                var portBaseY = y + headerHeight + nodePadding
                for (var pi = 0; pi < inputs.length; pi++) {
                    var py = portBaseY + pi * (portHeight + portSpacing) + portHeight / 2
                    newPortPositions[inputs[pi].id] = { cx: x, cy: py, nodeId: node.id, z: ni }
                }
                for (var po = 0; po < outputs.length; po++) {
                    var pyo = portBaseY + po * (portHeight + portSpacing) + portHeight / 2
                    newPortPositions[outputs[po].id] = { cx: x + nw, cy: pyo, nodeId: node.id, z: ni }
                }
            }

            // Pass 2: draw links BEHIND nodes using freshly computed port positions
            for (var li = 0; li < links.length; li++) {
                var link = links[li]
                var fromPos = newPortPositions[link.outputPortId]
                var toPos = newPortPositions[link.inputPortId]
                if (fromPos && toPos) {
                    var isSelected = !!selectedLinks[link.id]
                    var isMidiLink = portMediaTypes[link.outputPortId] === "Midi"
                                  || portMediaTypes[link.inputPortId] === "Midi"
                    var linkColor = isSelected ? ("" + Theme.colLinkSelected)
                                  : isMidiLink ? colLinkMidi
                                  : (link.active ? colLinkActive : colLinkInactive)
                    var linkWidth = isSelected ? 3 : 2
                    drawBezier(ctx, fromPos.cx, fromPos.cy, toPos.cx, toPos.cy,
                        linkColor, linkWidth)
                }
            }

            // Pass 3: draw nodes ON TOP of links
            for (var ni2 = 0; ni2 < nodes.length; ni2++) {
                var node2 = nodes[ni2]
                if (node2.layoutKey && hiddenNodes[node2.layoutKey]) continue
                var pos2 = nodePositions[node2.id]
                if (!pos2) continue

                var nx = pos2.x
                var ny = pos2.y
                var nSorted = getSortedPorts(node2.id)
                var nInputs = nSorted.inputs
                var nOutputs = nSorted.outputs
                var nh = calculateNodeHeight(node2)
                var nnw = getNodeWidth(node2.id)

                var isNodeSelected = !!selectedNodes[node2.id]
                var isDefaultNode = defaultNodeKey !== "" && node2.layoutKey === defaultNodeKey
                ctx.fillStyle = "" + colNodeBg
                if (isNodeSelected) {
                    ctx.strokeStyle = "" + Theme.selectionOutline
                    ctx.lineWidth = 2.5
                } else if (isDefaultNode) {
                    ctx.strokeStyle = "" + colDefaultOutline
                    ctx.lineWidth = 2.5
                } else {
                    ctx.strokeStyle = "" + colNodeBorder
                    ctx.lineWidth = 1.5
                }
                roundRect(ctx, nx, ny, nnw, nh, 5)

                // Draw "DEFAULT" badge for the default node
                if (isDefaultNode) {
                    ctx.font = "bold 8px sans-serif"
                    var defBadgeText = "DEFAULT"
                    var defBadgeW = ctx.measureText(defBadgeText).width + 6
                    var defBadgeH = 12
                    var defBadgeX = nx + 4
                    var defBadgeY = ny + 3
                    ctx.fillStyle = "" + Theme.defaultBadgeBg
                    ctx.strokeStyle = "" + colDefaultOutline
                    ctx.lineWidth = 1
                    roundRect(ctx, defBadgeX, defBadgeY, defBadgeW, defBadgeH, 2)
                    ctx.fillStyle = "" + colDefaultOutline
                    ctx.textAlign = "center"
                    ctx.textBaseline = "middle"
                    ctx.fillText(defBadgeText, defBadgeX + defBadgeW / 2, defBadgeY + defBadgeH / 2)
                }

                ctx.fillStyle = "" + getNodeColor(node2)
                roundRectTop(ctx, nx, ny, nnw, headerHeight, 5)

                ctx.fillStyle = "" + Theme.textPrimary
                ctx.font = "bold 11px sans-serif"
                ctx.textAlign = "center"
                ctx.textBaseline = "middle"
                ctx.fillText(truncate(node2.name, 30), nx + nnw / 2, ny + headerHeight / 2)

                var isPinned = !!(node2.layoutKey && pinnedNodes[node2.layoutKey])
                var pinX = nx + nnw - 16
                var pinY = ny + 3
                var pinS = 12
                var pinAlpha = isPinned ? 1.0 : 0.25
                ctx.save()
                ctx.globalAlpha = pinAlpha
                ctx.fillStyle = "" + Theme.textPrimary
                ctx.beginPath()
                ctx.arc(pinX + pinS / 2, pinY + pinS * 0.3, pinS * 0.25, 0, Math.PI * 2)
                ctx.fill()
                ctx.strokeStyle = "" + Theme.textPrimary
                ctx.lineWidth = 1.5
                ctx.beginPath()
                ctx.moveTo(pinX + pinS / 2, pinY + pinS * 0.5)
                ctx.lineTo(pinX + pinS / 2, pinY + pinS)
                ctx.stroke()
                ctx.restore()

                // Draw format badge (LV2/CLAP/VST3) for plugin nodes
                if (node2.type === "Plugin" && node2.pluginFormat) {
                    var fmt = node2.pluginFormat
                    var badgeColor = fmt === "CLAP" ? ("" + Theme.badgeClapBg) : fmt === "VST3" ? ("" + Theme.badgeVst3Bg) : ("" + Theme.badgeLv2Bg)
                    var badgeTextCol = fmt === "CLAP" ? ("" + Theme.badgeClapText) : fmt === "VST3" ? ("" + Theme.badgeVst3Text) : ("" + Theme.badgeLv2Text)
                    ctx.font = "bold 8px sans-serif"
                    var badgeW = ctx.measureText(fmt).width + 6
                    var badgeH = 12
                    var badgeX = nx + nnw - badgeW - pinAreaWidth
                    var badgeY = ny + 3
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

                var nPortBaseY = ny + headerHeight + nodePadding
                for (var npi = 0; npi < nInputs.length; npi++) {
                    var npy = nPortBaseY + npi * (portHeight + portSpacing) + portHeight / 2
                    var npx = nx

                    ctx.fillStyle = nInputs[npi].mediaType === "Midi" ? ("" + colMidiPort) : ("" + colPortIn)
                    ctx.beginPath()
                    ctx.arc(npx, npy, portRadius, 0, Math.PI * 2)
                    ctx.fill()

                    if (connectFromPortId >= 0 && connectFromDir === "Output") {
                        var sxIn = npx * zoom + panX
                        var syIn = npy * zoom + panY
                        var dIn = Math.sqrt(Math.pow(connectMouseX - sxIn, 2) + Math.pow(connectMouseY - syIn, 2))
                        if (dIn < portRadius * zoom * 3) {
                            ctx.strokeStyle = "" + colLinkConnecting
                            ctx.lineWidth = 2
                            ctx.beginPath()
                            ctx.arc(npx, npy, portRadius + 2, 0, Math.PI * 2)
                            ctx.stroke()
                        }
                    }

                    ctx.fillStyle = "" + Theme.textSecondary
                    ctx.font = "10px sans-serif"
                    ctx.textAlign = "left"
                    ctx.textBaseline = "middle"
                    ctx.fillText(truncate(nInputs[npi].name, 24), npx + portRadius + 4, npy)
                }

                for (var npo = 0; npo < nOutputs.length; npo++) {
                    var npyo = nPortBaseY + npo * (portHeight + portSpacing) + portHeight / 2
                    var npxo = nx + nnw

                    ctx.fillStyle = nOutputs[npo].mediaType === "Midi" ? ("" + colMidiPort) : ("" + colPortOut)
                    ctx.beginPath()
                    ctx.arc(npxo, npyo, portRadius, 0, Math.PI * 2)
                    ctx.fill()

                    if (connectFromPortId >= 0 && connectFromDir === "Input") {
                        var sxOut = npxo * zoom + panX
                        var syOut = npyo * zoom + panY
                        var dOut = Math.sqrt(Math.pow(connectMouseX - sxOut, 2) + Math.pow(connectMouseY - syOut, 2))
                        if (dOut < portRadius * zoom * 3) {
                            ctx.strokeStyle = "" + colLinkConnecting
                            ctx.lineWidth = 2
                            ctx.beginPath()
                            ctx.arc(npxo, npyo, portRadius + 2, 0, Math.PI * 2)
                            ctx.stroke()
                        }
                    }

                    ctx.fillStyle = "" + Theme.textSecondary
                    ctx.font = "10px sans-serif"
                    ctx.textAlign = "right"
                    ctx.textBaseline = "middle"
                    ctx.fillText(truncate(nOutputs[npo].name, 24), npxo - portRadius - 4, npyo)
                }

                if (node2.type === "Plugin") {
                    var btnY2 = ny + nh - buttonRowHeight - nodePadding
                    var btnW3 = (nnw - nodePadding * 4) / 3
                    var btnH2 = buttonRowHeight
                    var hasUi = node2.pluginHasUi !== false
                    var isBypassed = isNodeBypassed(node2)

                    // On/Off button
                    ctx.fillStyle = isBypassed ? ("" + Theme.buttonOffBg) : ("" + Theme.buttonActiveBg)
                    ctx.strokeStyle = isBypassed ? ("" + Theme.buttonOffBorder) : ("" + Theme.buttonActiveBorder)
                    ctx.lineWidth = 1
                    roundRect(ctx, nx + nodePadding, btnY2, btnW3, btnH2, 3)

                    ctx.fillStyle = isBypassed ? ("" + Theme.buttonOffText) : ("" + Theme.buttonActiveText)
                    ctx.font = "bold 10px sans-serif"
                    ctx.textAlign = "center"
                    ctx.textBaseline = "middle"
                    ctx.fillText(isBypassed ? "OFF" : "ON", nx + nodePadding + btnW3 / 2, btnY2 + btnH2 / 2)

                    // UI button — disabled (dimmed) when plugin has no UI
                    ctx.fillStyle = hasUi ? ("" + Theme.buttonBg) : ("" + Theme.buttonDisabledBg)
                    ctx.strokeStyle = hasUi ? ("" + Theme.buttonBorder) : ("" + Theme.buttonDisabledBorder)
                    ctx.lineWidth = 1
                    roundRect(ctx, nx + nodePadding * 2 + btnW3, btnY2, btnW3, btnH2, 3)

                    ctx.fillStyle = hasUi ? ("" + Theme.textPrimary) : ("" + Theme.textDisabled)
                    ctx.font = "10px sans-serif"
                    ctx.textAlign = "center"
                    ctx.textBaseline = "middle"
                    ctx.fillText("UI", nx + nodePadding * 2 + btnW3 + btnW3 / 2, btnY2 + btnH2 / 2)

                    // Params button — always active
                    ctx.fillStyle = "" + Theme.buttonBg
                    ctx.strokeStyle = "" + Theme.buttonBorder
                    ctx.lineWidth = 1
                    roundRect(ctx, nx + nodePadding * 3 + btnW3 * 2, btnY2, btnW3, btnH2, 3)

                    ctx.fillStyle = "" + Theme.textPrimary
                    ctx.font = "10px sans-serif"
                    ctx.textAlign = "center"
                    ctx.textBaseline = "middle"
                    ctx.fillText("Params", nx + nodePadding * 3 + btnW3 * 2 + btnW3 / 2, btnY2 + btnH2 / 2)
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
                ctx.strokeStyle = "" + Theme.selectionOutline
                ctx.lineWidth = 1 / zoom
                ctx.fillStyle = "rgba(255, 255, 0, " + Theme.selectionFillAlpha + ")"
                ctx.beginPath()
                ctx.rect(selX, selY, selW, selH)
                ctx.fill()
                ctx.stroke()
            }

            // Draw snap guide lines
            if (activeSnapLines.length > 0) {
                ctx.save()
                ctx.setLineDash([4 / zoom, 4 / zoom])
                ctx.strokeStyle = "" + Theme.snapGuide
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
            if (placingNodeId >= 0) {
                finishPlacing()
            } else {
                clearSelection()
            }
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

            // In-hand placement: a click drops the node where it currently sits.
            if (placingNodeId >= 0) {
                finishPlacing()
                return
            }

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
                    contextCanvasPos = { x: cPos.x, y: cPos.y }
                    canvasContextMenu.popup()
                }
                return
            }

            if (mouse.button === Qt.LeftButton) {
                var pinHit = findPinButtonAt(mouse.x, mouse.y)
                if (pinHit && pinHit.layoutKey) {
                    var pk = pinHit.layoutKey
                    if (pinnedNodes[pk]) {
                        delete pinnedNodes[pk]
                    } else {
                        pinnedNodes[pk] = true
                    }
                    pinnedNodes = pinnedNodes
                    persistPinned()
                    canvas.requestPaint()
                    return
                }

                var btnHit = findButtonAt(mouse.x, mouse.y)
                if (btnHit) {
                    if (btnHit.button === "onoff") {
                        var newBypassed = !btnHit.bypassed
                        // Immediate local feedback
                        localBypassState[btnHit.nodeId] = newBypassed
                        localBypassState = localBypassState  // trigger change signal
                        controller.set_plugin_bypass(btnHit.nodeId, newBypassed)
                        canvas.requestPaint()
                    } else if (btnHit.button === "ui" && btnHit.hasUi) {
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
                    var nodeIsSelected = !!selectedNodes[nodeIdDrag]

                    if (ctrlHeld) {
                        var newSel = Object.assign({}, selectedNodes)
                        if (nodeIsSelected) {
                            delete newSel[nodeIdDrag]
                        } else {
                            newSel[nodeIdDrag] = nodeSig(findNodeData(nodeIdDrag))
                        }
                        selectedNodes = newSel
                        selectedLinks = {}
                        canvas.requestPaint()
                        return
                    }

                    if (nodeIsSelected) {
                        groupDragging = true
                        for (var agid in selectedNodes) {
                            if (selectedNodes[agid]) delete autoPlaced[agid]
                        }
                        var cg = toCanvas(mouse.x, mouse.y)
                        groupDragLastX = cg.x
                        groupDragLastY = cg.y
                    } else {
                        selectedNodes = {}
                        selectedLinks = {}
                        dragNodeId = nodeIdDrag
                        delete autoPlaced[nodeIdDrag]
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
                    var clickedLink = findLinkData(clickedLinkId)
                    if (ctrlHeldLink) {
                        var newSelLinks = Object.assign({}, selectedLinks)
                        if (newSelLinks[clickedLinkId]) {
                            delete newSelLinks[clickedLinkId]
                        } else if (clickedLink) {
                            newSelLinks[clickedLinkId] = linkSig(clickedLink)
                        }
                        selectedLinks = newSelLinks
                    } else {
                        var freshSel = {}
                        if (clickedLink) freshSel[clickedLinkId] = linkSig(clickedLink)
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
            // In-hand placement: keep the node centred under the cursor.
            if (placingNodeId >= 0) {
                var pc = toCanvas(mouse.x, mouse.y)
                var pn = findNodeData(placingNodeId)
                var pw = (getNodeWidth(placingNodeId) || minNodeWidth)
                var ph = pn ? calculateNodeHeight(pn) : 80
                nodePositions[placingNodeId] = { x: pc.x - pw / 2, y: pc.y - ph / 2 }
                canvas.requestPaint()
                return
            }

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
                            pendingInsertLinkId = linkUnder
                            pendingInsertNodeId = dragNodeId
                            insertConfirmDialog.open()
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
                    result[link.id] = linkSig(link)
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
                result[n.id] = nodeSig(n)
            }
        }
        return result
    }

    function deleteSelectedLinks() {
        var count = 0
        for (var linkId in selectedLinks) {
            var lid = parseInt(linkId)
            // Only disconnect a link that is still the one that was selected.
            if (linkMatchesSig(findLinkData(lid), selectedLinks[linkId])) {
                controller.disconnect_link(lid)
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
