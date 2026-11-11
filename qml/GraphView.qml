import QtQuick
import QtQuick.Controls

Item {
    id: graphView

    required property var controller

    signal openPluginBrowser
    signal openPluginParams(int nodeId)

    property real zoom: 1.0
    property real panX: 0
    property real panY: 0
    property bool viewportLoaded: false
    // Set once the first refresh with nodes has checked that the restored
    // viewport actually shows some of them.
    property bool initialViewChecked: false

    readonly property real minZoom: 0.25
    readonly property real maxZoom: 3.0
    // Screen pixels of the graph that panning/zooming must leave in view.
    readonly property real minVisibleContent: 80

    property var nodePositions: ({})
    property var layoutCursors: ({
            "source": {
                x: 50,
                y: 50
            },
            "stream": {
                x: 350,
                y: 50
            },
            "sink": {
                x: 650,
                y: 50
            }
        })

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

    // Layout key each node's position was assigned under (saved, added or
    // auto-placed); dropped once the user moves the node. A node can change
    // key while keeping its id (a plugin's filter node first appears untyped
    // as "Unknown:<name>"), and then its saved position must still be applied.
    // placedBaseById holds the node's own layoutKey at that time: when only
    // the "#n" suffix of the unique key changes (another node of the same
    // name went away) the node must stay where it is.
    property var placedKeyById: ({})
    property var placedBaseById: ({})

    // When each node first appeared: nodeId -> { t: ms, seq: refresh number }.
    // Automatic repositioning (waiting for a rule to link the node, fixing a
    // backward wire) only ever touches a node inside settleWindowMs of its
    // appearance; after that it stays where it is until the user moves it.
    property var appearedAt: ({})
    property int refreshSeq: 0
    // Longer than it takes a patchbay rule to connect a new node (main.qml
    // widens it when the rule settle time is raised).
    property int settleWindowMs: 8000
    // Graph density (preference node_spacing_px; 8 compact, 20 normal, 40
    // spacious): the gap automatic placement leaves between stacked nodes,
    // in graph units. A column beside a peer is 2.5x as far away, leaving
    // room for the wires. Auto layout uses the same two gaps. (How big the
    // cards are is the separate cardRow below.)
    property real nodeSpacing: 20
    readonly property real placeColumnGap: Math.round(nodeSpacing * 2.5)
    // Node card density (preference card_row_px; 18 compact, 22 normal, 26
    // spacious): the height of a port row. Everything on the card scales
    // with it (Theme.nodeScale). Cards are re-measured at once and cards that
    // now overlap are pushed apart (one undo step); the rest of the layout
    // stays until Auto layout.
    property int cardRow: 22
    property int densityMoved: 0   // nodes moved by the last card size change
    onCardRowChanged: {
        Theme.cardRow = cardRow;
        applyDensity();
    }
    function applyDensity() {
        densityMoved = 0;
        if (!layoutLoaded)
            return;   // nothing measured or placed yet
        refreshData();
        var before = {};
        for (var i = 0; i < nodes.length; i++) {
            if (nodePositions[nodes[i].id])
                before[nodes[i].id] = {
                    x: nodePositions[nodes[i].id].x,
                    y: nodePositions[nodes[i].id].y
                };
        }
        separateOverlaps();
        var entry = recordMove("re-space", before);
        if (entry) {
            densityMoved = entry.items.length;
            persistLayout();
        }
        canvas.requestPaint();
    }

    // Push overlapping cards apart: the lower one moves down just clear of the
    // upper one, keeping the node spacing (on the grid); when only the lower
    // one is pinned, the upper one moves up instead.
    function separateOverlaps() {
        var gap = nodeSpacing;
        for (var pass = 0; pass < 40; pass++) {
            var vis = [];
            for (var i = 0; i < nodes.length; i++) {
                var n = nodes[i];
                if ((n.layoutKey && hiddenNodes[n.layoutKey]) || !nodePositions[n.id])
                    continue;
                vis.push({
                    node: n,
                    x: nodePositions[n.id].x,
                    y: nodePositions[n.id].y,
                    w: getNodeWidth(n.id),
                    h: calculateNodeHeight(n)
                });
            }
            vis.sort((a, b) => a.y - b.y || a.x - b.x);
            var fixed = false;
            for (var ai = 0; ai < vis.length && !fixed; ai++) {
                for (var bi = ai + 1; bi < vis.length; bi++) {
                    var a = vis[ai], b = vis[bi];
                    if (!(a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y))
                        continue;
                    if (nodeIsPinned(b.node) && !nodeIsPinned(a.node)) {
                        var up = b.y - a.h - gap;
                        if (snapToGrid)
                            up = Math.floor(up / gridSize) * gridSize;
                        nodePositions[a.node.id] = {
                            x: a.x,
                            y: up
                        };
                    } else {
                        var down = a.y + a.h + gap;
                        if (snapToGrid)
                            down = Math.ceil(down / gridSize) * gridSize;
                        nodePositions[b.node.id] = {
                            x: b.x,
                            y: down
                        };
                    }
                    fixed = true;
                    break;
                }
            }
            if (!fixed)
                break;
        }
    }

    property int connectFromPortId: -1
    property string connectFromDir: ""
    // Right-drag from a port: the drag wire is red and dropping it on a port
    // blocks that node pair instead of connecting it.
    property bool connectBlocking: false
    property real connectPressX: 0
    property real connectPressY: 0
    property real connectMouseX: 0
    property real connectMouseY: 0

    property var nodes: []
    property var links: []
    // Node pairs under an enabled block rule: [{source, target, ruleId, label}],
    // drawn as red ⊘ lines. blockGeom holds their last drawn curves for
    // hit-testing.
    property var blockedPairs: []
    property var blockGeom: []
    property var contextBlock: null
    // Left-drag over a port that a block rule forbids: the wire turns red.
    property int connectHoverPortId: -1
    property bool connectTargetBlocked: false
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

    // What a left-drag on empty canvas does: "grab" pans the view (the same
    // as a middle-drag), "select" draws a selection box. Switched with the
    // buttons at the canvas's top-left or with W; saved as the drag_mode
    // preference.
    property string dragMode: "grab"
    // Left-drag pan in grab mode. grabMoved tells a drag from a click: a
    // click on empty canvas clears the selection, a pan keeps it.
    property bool grabPanning: false
    property bool grabMoved: false
    // The cursor is over empty canvas (no node, pin, button or wire).
    property bool hoverEmpty: false
    onDragModeChanged: updateCursor()

    // Temporary overrides of the drag mode while a key is held: Space makes
    // a left-drag pan from anywhere (even over a node); Shift makes a
    // left-drag on empty canvas draw a selection box in grab mode. The hint
    // at the bottom of the canvas names the override and shows when it is on.
    property bool spaceHeld: false
    property bool shiftHeld: false
    property bool spacePanning: false
    property bool middlePanning: false
    readonly property string overrideState: spaceHeld ? "pan" : (shiftHeld && dragMode === "grab" && dragNodeId < 0 && !groupDragging && connectFromPortId < 0 && !grabPanning) ? "select" : ""
    onOverrideStateChanged: updateCursor()
    onActiveFocusChanged: if (!activeFocus) {
        spaceHeld = false;
        shiftHeld = false;
    }
    // A selection box that adds to the selection it started with (Shift or
    // Ctrl held when it began).
    property bool selectAdditive: false
    property var selectBaseNodes: ({})
    property var selectBaseLinks: ({})
    // Where the node(s) being dragged started, for undo.
    property var dragStartPos: null

    function setDragMode(mode) {
        if ((mode !== "grab" && mode !== "select") || mode === dragMode)
            return;
        dragMode = mode;
        controller.set_preference("drag_mode", mode);
    }

    function toggleDragMode() {
        // Not in the middle of a left-drag: it would change meaning under the hand.
        if (grabPanning || spacePanning || selectDragging)
            return;
        setDragMode(dragMode === "grab" ? "select" : "grab");
    }

    property int contextNodeId: -1
    // Plugin port under the right-click that opened the node menu (-1: none).
    property int contextPortId: -1
    property var contextNode: null
    // Canvas point of the last right-click on empty canvas (for "Add Plugin...").
    property var contextCanvasPos: null

    // Subgraphs (design/GROUPS.md): the subgraph this view is inside ("" =
    // main graph), the main graph's pan/zoom to return to, and a fit to run
    // once a freshly opened subgraph has been laid out.
    property string currentSubgraph: ""
    property string currentSubgraphName: ""
    property var mainViewport: null
    property bool fitAfterRefresh: false
    readonly property bool contextIsSubgraph: contextNode !== null && contextNode.type === "Subgraph"
    readonly property bool contextIsVdev: contextNode !== null && !!contextNode.vdevId
    // Links the wire context menu acts on: the right-clicked link, or the
    // whole selection when the right-clicked link is part of it.
    property var contextLinkIds: []
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
    // Gap between nodes when snapping beside/above/below one, and when a
    // dropped node is pushed off another (graph units = px at 100 % zoom).
    // Set from preferences (snap_gap_px).
    property real snapGap: 10

    // Snap to grid (preferences snap_to_grid / grid_size_px): node positions
    // land on multiples of gridSize (graph units); the dot grid uses the
    // same pitch. Auto layout follows it too.
    property bool snapToGrid: true
    property real gridSize: 24
    onSnapToGridChanged: if (snapToGrid)
        alignAllToGrid()
    onGridSizeChanged: {
        if (snapToGrid)
            alignAllToGrid();
        canvas.requestPaint();
    }

    // Group drag: positions of the dragged nodes and the cursor at the start,
    // so snapping never accumulates across mouse moves.
    property var groupDragStart: ({})
    property real groupDragStartX: 0
    property real groupDragStartY: 0
    property var activeSnapLines: []  // [{axis:"x"|"y", pos: number}]

    // Node card geometry comes from the Theme (design bible §6.5).
    readonly property real minNodeWidth: Theme.nodeWidth
    readonly property real maxNodeWidth: Theme.nodeWidthPlugin
    readonly property real portRadius: Theme.nodePin / 2
    property var nodeWidths: ({})

    // Per-node layout (visible ports, collapse state, height), rebuilt by
    // refreshData(); see computeNodeLayout().
    property var nodeLayouts: ({})
    // Ports with at least one link: portId -> true.
    property var connectedPorts: ({})
    // Plugins whose collapsed ports the user expanded (session only):
    // layoutKey -> true.
    property var expandedNodes: ({})
    // Collapse a side's trailing free ports only when at least this many.
    readonly property int collapseMinHidden: 3

    // Toolbar search: non-matching nodes dim, matching ports get a ring.
    property string searchText: ""
    onSearchTextChanged: canvas.requestPaint()

    // Hover state (canvas repaint + pin tooltip).
    property int hoveredPortId: -1
    property int hoveredLinkId: -1
    // The node under the cursor (its pins count): highlighted on the canvas.
    // nameTruncated: nodeId -> true while drawNode had to cut its title short
    // (a popup then shows the full name); not bound to anything.
    property int hoveredNodeId: -1
    property var nameTruncated: ({})
    // Preset selector on plugin cards: nodeId -> {name, isDefault, modified, count}
    property var presetInfo: ({})
    property int presetMenuNodeId: -1
    property var presetMenuNames: []
    property string presetMenuSelected: ""
    property string presetMenuDefault: ""
    property var presetMenuChanges: []
    property var presetSaveNames: []
    property int presetSaveNodeId: -1
    property string hoveredButton: ""   // "<nodeId>:<key>" for footer buttons / toggle row

    // Redraw graph when system theme changes
    Connections {
        target: controller
        function onRules_changed() {
            refreshBlocked();
        }
    }

    // A session switch: save the viewport into the old session, then forget
    // its layout and load the new one's.
    Connections {
        target: controller
        function onSession_changing() {
            if (viewportSaveTimer.running) {
                viewportSaveTimer.stop();
                viewportSaveTimer.triggered();
            }
        }
        function onSession_changed() {
            reloadSessionData();
        }
    }

    function reloadSessionData() {
        viewportSaveTimer.stop();
        viewportLoaded = false;
        layoutLoaded = false;
        hiddenLoaded = false;
        pinnedLoaded = false;
        savedLayout = {};
        hiddenNodes = {};
        pinnedNodes = {};
        nodePositions = {};
        placedKeyById = {};
        placedBaseById = {};
        appearedAt = {};
        autoPlaced = {};
        awaitingLinkPlacement = {};
        selectedNodes = {};
        selectedLinks = {};
        clearUndo();
        quietCues();
        defaultNodeKey = "";
        currentSubgraph = "";
        currentSubgraphName = "";
        mainViewport = null;
        zoom = 1.0;
        panX = 0;
        panY = 0;
        refreshData();
    }

    function refreshBlocked() {
        try {
            blockedPairs = JSON.parse(controller.get_blocked_pairs_json());
        } catch (e) {
            blockedPairs = [];
        }
        canvas.requestPaint();
    }

    function findBlockAt(sx, sy) {
        var c = toCanvas(sx, sy);
        var threshold = 5 / zoom;
        var best = null;
        var bestDist = threshold + 1;
        for (var i = 0; i < blockGeom.length; i++) {
            var g = blockGeom[i];
            // The ⊘ marker is a bigger target than the thin line.
            var dm = Math.sqrt((c.x - g.mx) * (c.x - g.mx) + (c.y - g.my) * (c.y - g.my));
            var d = dm <= 9 / zoom ? 0 : distToBezier(c.x, c.y, g.x1, g.y1, g.x2, g.y2);
            if (d < bestDist) {
                bestDist = d;
                best = g;
            }
        }
        return bestDist <= threshold ? best : null;
    }

    function showToast(message) {
        toastText.text = message;
        toast.opacity = 1;
        toastTimer.restart();
    }

    Connections {
        target: Theme
        function onDarkChanged() {
            widthsDirty = true;
            canvas.requestPaint();
        }
    }

    function portSortCmp(a, b) {
        var am = a.mediaType === "Midi" ? 0 : 1;
        var bm = b.mediaType === "Midi" ? 0 : 1;
        return am !== bm ? am - bm : naturalCmp(a.sortName || a.name, b.sortName || b.name);
    }

    function getSortedPorts(nodeId) {
        return sortedPortsByNode[nodeId] || {
            inputs: [],
            outputs: []
        };
    }

    // Selection entries remember what they referred to, so a recycled
    // PipeWire id can never make a selection point at an unrelated object.
    function linkSig(link) {
        return {
            o: link.outputPortId,
            i: link.inputPortId,
            on: link.outputNodeId,
            inn: link.inputNodeId
        };
    }

    function linkMatchesSig(link, sig) {
        return !!link && !!sig && link.outputPortId === sig.o && link.inputPortId === sig.i && link.outputNodeId === sig.on && link.inputNodeId === sig.inn;
    }

    function nodeSig(node) {
        return {
            key: (node && node.layoutKey) || ""
        };
    }

    function findLinkData(linkId) {
        for (var i = 0; i < links.length; i++) {
            if (links[i].id === linkId)
                return links[i];
        }
        return null;
    }

    function naturalCmp(a, b) {
        var ax = a.split(/(\d+)/), bx = b.split(/(\d+)/);
        for (var i = 0; i < Math.min(ax.length, bx.length); i++) {
            var an = parseInt(ax[i]), bn = parseInt(bx[i]);
            if (!isNaN(an) && !isNaN(bn)) {
                if (an !== bn)
                    return an - bn;
            } else {
                if (ax[i] < bx[i])
                    return -1;
                if (ax[i] > bx[i])
                    return 1;
            }
        }
        return ax.length - bx.length;
    }

    // While a refresh runs, layout and pin saves are collected and written
    // once at the end: every write is an fsync on the UI thread, and a
    // refresh can place many new nodes.
    property int deferSaveDepth: 0
    property var pendingLayoutJson: null
    property var pendingPinnedJson: null

    function refreshData() {
        deferSaveDepth++;
        try {
            refreshDataNow();
        } finally {
            deferSaveDepth--;
            if (deferSaveDepth === 0) {
                if (pendingLayoutJson !== null) {
                    var layoutJson = pendingLayoutJson;
                    pendingLayoutJson = null;
                    controller.save_layout(layoutJson);
                }
                if (pendingPinnedJson !== null) {
                    var pinnedJson = pendingPinnedJson;
                    pendingPinnedJson = null;
                    controller.save_pinned(pinnedJson);
                }
            }
        }
    }

    function refreshDataNow() {
        // Follow the controller's view (it leaves a subgraph by itself when
        // the subgraph is ungrouped or deleted).
        var sgView = controller.get_subgraph_view ? controller.get_subgraph_view() : "";
        if (sgView !== currentSubgraph) {
            if (sgView === "" && mainViewport) {
                zoom = mainViewport.zoom;
                panX = mainViewport.panX;
                panY = mainViewport.panY;
            }
            currentSubgraph = sgView;
            var sgInfo = sgView !== "" ? subgraphInfo(sgView) : null;
            currentSubgraphName = sgInfo ? sgInfo.name : "";
        }
        if (!viewportLoaded) {
            try {
                var vp = JSON.parse(controller.get_viewport_json());
                if (vp.panX !== undefined)
                    panX = vp.panX;
                if (vp.panY !== undefined)
                    panY = vp.panY;
                if (vp.zoom !== undefined)
                    zoom = vp.zoom;
            } catch (e) {}
            viewportLoaded = true;
        }

        if (!layoutLoaded) {
            try {
                savedLayout = JSON.parse(controller.get_layout_json());
            } catch (e) {
                savedLayout = {};
            }
            layoutLoaded = true;
            try {
                var defNode = controller.get_default_node();
                if (defNode)
                    defaultNodeKey = defNode;
            } catch (e) {}
        }
        if (!hiddenLoaded) {
            try {
                var hiddenArr = JSON.parse(controller.get_hidden_json());
                var hSet = {};
                for (var hi = 0; hi < hiddenArr.length; hi++) {
                    hSet[hiddenArr[hi]] = true;
                }
                hiddenNodes = hSet;
            } catch (e) {
                hiddenNodes = {};
            }
            hiddenLoaded = true;
        }
        if (!pinnedLoaded) {
            try {
                var pinnedArr = JSON.parse(controller.get_pinned_json());
                var pSet = {};
                for (var pi = 0; pi < pinnedArr.length; pi++) {
                    pSet[pinnedArr[pi]] = true;
                }
                pinnedNodes = pSet;
            } catch (e) {
                pinnedNodes = {};
            }
            pinnedLoaded = true;
        }

        try {
            var nodesJson = controller.get_nodes_json();
            var linksJson = controller.get_links_json();
            nodes = JSON.parse(nodesJson);
            links = JSON.parse(linksJson);
        } catch (e) {
            nodes = [];
            links = [];
        }
        refreshPresetInfo();
        try {
            blockedPairs = JSON.parse(controller.get_blocked_pairs_json());
        } catch (e) {
            blockedPairs = [];
        }

        var newPorts = {};
        for (var i = 0; i < nodes.length; i++) {
            try {
                newPorts[nodes[i].id] = JSON.parse(controller.get_ports_json(nodes[i].id));
            } catch (e) {
                newPorts[nodes[i].id] = [];
            }
        }
        portsByNode = newPorts;

        var newConnected = {};
        for (var ci = 0; ci < links.length; ci++) {
            newConnected[links[ci].outputPortId] = true;
            newConnected[links[ci].inputPortId] = true;
        }
        connectedPorts = newConnected;

        var newSorted = {};
        var newHeights = {};
        var newLayouts = {};
        for (var si = 0; si < nodes.length; si++) {
            var sn = nodes[si];
            var sp = newPorts[sn.id] || [];
            var sorted = {
                inputs: sp.filter(function (p) {
                    return p.direction === "Input";
                }).sort(function (a, b) {
                    return portSortCmp(a, b);
                }),
                outputs: sp.filter(function (p) {
                    return p.direction === "Output";
                }).sort(function (a, b) {
                    return portSortCmp(a, b);
                })
            };
            newSorted[sn.id] = sorted;
            var lay = computeNodeLayout(sn, sorted);
            newLayouts[sn.id] = lay;
            newHeights[sn.id] = lay.height;
        }
        sortedPortsByNode = newSorted;
        nodeLayouts = newLayouts;
        nodeHeights = newHeights;
        uniqueKeyById = buildUniqueLayoutKeys();
        // Placement below needs the real widths, not the first paint's.
        calculateNodeWidths();
        widthsDirty = true;

        var newPortMedia = {};
        for (var nid in newPorts) {
            var pp = newPorts[nid];
            for (var pi2 = 0; pi2 < pp.length; pi2++) {
                newPortMedia[pp[pi2].id] = pp[pi2].mediaType || "Unknown";
            }
        }
        portMediaTypes = newPortMedia;
        // Port ids can be recycled across refreshes.
        dragSourcePortId = -1;

        // Drop per-id state for nodes that no longer exist. PipeWire recycles
        // object ids (and bridge sub-node ids can change), so a stale entry
        // would hand a brand-new node some old node's position and bypass the
        // savedLayout restore below — the pin/saved position would be ignored
        // and then overwritten by the next persistLayout().
        var liveIds = {};
        for (var lv = 0; lv < nodes.length; lv++)
            liveIds[nodes[lv].id] = true;
        for (var stale in nodePositions) {
            if (!liveIds[stale])
                delete nodePositions[stale];
        }
        for (var staleA in awaitingLinkPlacement) {
            if (!liveIds[staleA])
                delete awaitingLinkPlacement[staleA];
        }
        for (var staleP in autoPlaced) {
            if (!liveIds[staleP])
                delete autoPlaced[staleP];
        }
        for (var staleK in placedKeyById) {
            if (!liveIds[staleK])
                delete placedKeyById[staleK];
        }
        for (var staleB in placedBaseById) {
            if (!liveIds[staleB])
                delete placedBaseById[staleB];
        }
        for (var staleT in appearedAt) {
            if (!liveIds[staleT])
                delete appearedAt[staleT];
        }

        // Selections must refer to the same objects they did when made: drop
        // entries whose id vanished or now belongs to something else.
        var liveNodeById = {};
        for (var ln = 0; ln < nodes.length; ln++)
            liveNodeById[nodes[ln].id] = nodes[ln];
        var keptNodes = {};
        var nodesPruned = false;
        for (var selN in selectedNodes) {
            var selNode = liveNodeById[selN];
            if (selNode && selectedNodes[selN] && selectedNodes[selN].key === (selNode.layoutKey || "")) {
                keptNodes[selN] = selectedNodes[selN];
            } else {
                nodesPruned = true;
            }
        }
        if (nodesPruned)
            selectedNodes = keptNodes;

        var liveLinkById = {};
        for (var lk = 0; lk < links.length; lk++)
            liveLinkById[links[lk].id] = links[lk];
        var keptLinks = {};
        var linksPruned = false;
        for (var selL in selectedLinks) {
            if (linkMatchesSig(liveLinkById[selL], selectedLinks[selL])) {
                keptLinks[selL] = selectedLinks[selL];
            } else {
                linksPruned = true;
            }
        }
        if (linksPruned)
            selectedLinks = keptLinks;

        // The backend's bypass state wins once it is reported.
        var bypassPruned = false;
        for (var bpId in localBypassState) {
            var bpNode = liveNodeById[bpId];
            if (!bpNode || bpNode.pluginBypassed !== undefined) {
                delete localBypassState[bpId];
                bypassPruned = true;
            }
        }
        if (bypassPruned)
            localBypassState = localBypassState;

        refreshSeq++;
        var nowMs = Date.now();
        var appeared = [];
        for (var ni = 0; ni < nodes.length; ni++) {
            var n = nodes[ni];
            var key = uniqueKeyById[n.id] || "";
            var baseKey = n.layoutKey || "";
            if (n.id in nodePositions) {
                var placedKey = placedKeyById[n.id];
                if (placedKey !== undefined && placedKey !== key) {
                    if (placedBaseById[n.id] !== baseKey && key && savedLayout[key]) {
                        // Same node, new key (a plugin's filter node gets its
                        // type), and the user hasn't moved it since it was
                        // placed: apply the position saved for the new key, or
                        // the next persistLayout() would overwrite it.
                        var resaved = savedLayout[key];
                        nodePositions[n.id] = {
                            x: resaved[0],
                            y: resaved[1]
                        };
                        delete autoPlaced[n.id];
                        delete awaitingLinkPlacement[n.id];
                    } else {
                        // Only the "#n" suffix moved because another node of
                        // the same name went away: the node stays put, and its
                        // position is saved under the key it has now.
                        persistLayout();
                    }
                    placedKeyById[n.id] = key;
                    placedBaseById[n.id] = baseKey;
                }
                continue;
            }
            placedKeyById[n.id] = key;
            placedBaseById[n.id] = baseKey;
            appearedAt[n.id] = {
                t: nowMs,
                seq: refreshSeq
            };
            appeared.push(n.id);

            if (key && savedLayout[key]) {
                // Known position from a previous session/placement. Restore it
                // and never auto-move it — a device toggling off/on (new
                // PipeWire id, same layoutKey) must reappear where the user
                // left it. Only if something else has taken the spot in the
                // meantime does it move to the nearest free slot (pins are
                // restored exactly, always).
                var saved = savedLayout[key];
                var restored = {
                    x: saved[0],
                    y: saved[1]
                };
                nodePositions[n.id] = restored;
                if (!nodeIsPinned(n) && !(n.layoutKey && hiddenNodes[n.layoutKey]) && spotIsTaken(n, restored.x, restored.y)) {
                    nodePositions[n.id] = findFreeSpot(n, restored.x, restored.y, 0, false);
                    persistLayout();
                }
            } else if (n.type === "Plugin" && pendingPluginPosition && (pendingPluginName === "" || n.name === pendingPluginName)) {
                // User-added plugin: try to drop it on a free spot near the
                // add-point. If there's no room nearby, place it "in hand" so the
                // user can position it themselves.
                var anchor = pendingPluginPosition;
                pendingPluginPosition = null;
                pendingPluginName = "";
                nodePositions[n.id] = {
                    x: anchor.x,
                    y: anchor.y
                };
                var spot = findFreeSpot(n, anchor.x, anchor.y, 0, true);
                if (spot) {
                    nodePositions[n.id] = spot;
                    persistLayout();
                } else {
                    startPlacing(n.id);
                }
            } else {
                // Auto-appearing node (hardware device, stream). If a patchbay
                // rule has already linked it, drop it next to the node that
                // picked it up. Otherwise place it provisionally — beside the
                // default node if one is set (that's where a ruleless node will
                // connect), else with its own kind inside the view — and let
                // the second pass reposition it if its links show up shortly.
                autoPlaced[n.id] = true;
                if (!placeNearPeer(n)) {
                    var pa = provisionalAnchor(n);
                    nodePositions[n.id] = findFreeSpot(n, pa.x, pa.y, pa.dir, false);
                    persistLayout();
                    if (!nodeIsPinned(n)) {
                        awaitingLinkPlacement[n.id] = true;
                        awaitingLinkPlacement = awaitingLinkPlacement;
                    }
                }
            }

            // Auto-pin plugins so their manual placement sticks across restarts.
            if (n.type === "Plugin" && n.layoutKey && !pinnedNodes[n.layoutKey]) {
                pinnedNodes[n.layoutKey] = true;
                pinnedNodes = pinnedNodes;
                persistPinned();
            }
        }

        // Second placement pass: a node that appeared a moment ago without
        // links moves beside its peer once a rule has connected it. Only the
        // newcomer moves (see placementPeer); a node that has been sitting
        // there longer than settleWindowMs is settled and stays.
        var stillAwaiting = {};
        for (var aid in awaitingLinkPlacement) {
            var anode = findNodeData(parseInt(aid));
            if (!anode)
                // node vanished — forget it
                continue;
            if (nodeIsPinned(anode) || !isFresh(anode.id))
                // pinned or settled — it stays where it is
                continue;
            if (placeNearPeer(anode))
                continue;
            if (!nodeHasLinks(anode.id))
                stillAwaiting[aid] = true;   // no peer yet, keep waiting
            // else: linked, but it is the anchor its newer peers move to.
        }
        awaitingLinkPlacement = stillAwaiting;

        // Third pass: fix freshly auto-placed nodes whose links flow backwards
        // (the line's input end sits left of its output end). One-shot per
        // node, and only for nodes that just appeared and the user never
        // touched — settled, pinned or hand-placed nodes are sacred.
        for (var bid in autoPlaced) {
            if (awaitingLinkPlacement[bid])
                continue;
            var bnode = findNodeData(parseInt(bid));
            if (!bnode || nodeIsPinned(bnode))
                continue;
            if (!isFresh(bnode.id)) {
                delete autoPlaced[bid];
                continue;
            }
            if (nodeHasBackwardLink(parseInt(bid))) {
                delete autoPlaced[bid];
                placeNearPeer(bnode);
            }
        }

        noteAppeared(appeared);

        // A restored viewport (or one saved on a bigger screen) may show none of
        // the graph; bring it back into view once, on the first populated refresh.
        if (!initialViewChecked && nodes.length > 0 && width > 0 && height > 0) {
            initialViewChecked = true;
            var b = contentBounds();
            if (b && (b.right * zoom + panX < 0 || b.left * zoom + panX > width || b.bottom * zoom + panY < 0 || b.top * zoom + panY > height))
                fitToView();
        }

        // A subgraph opened for the first time: frame its nodes.
        if (fitAfterRefresh && nodes.length > 0 && width > 0 && height > 0) {
            fitAfterRefresh = false;
            Qt.callLater(fitToView);
        }

        canvas.requestPaint();
        repaintTimer.restart();
    }

    // True if any link touching `nodeId` runs right-to-left: the input end
    // (peer's left edge) is left of the output end (source's right edge).
    // Equivalent to the link line's angle atan2(dy, dx) being outside ±90°.
    // A small tolerance ignores near-vertical links between stacked nodes.
    function nodeHasBackwardLink(nodeId) {
        for (var li = 0; li < links.length; li++) {
            var l = links[li];
            if (l.outputNodeId !== nodeId && l.inputNodeId !== nodeId)
                continue;
            if (l.outputNodeId === l.inputNodeId)
                continue;
            var fromPos = nodePositions[l.outputNodeId];
            var toPos = nodePositions[l.inputNodeId];
            if (!fromPos || !toPos)
                continue;
            var outX = fromPos.x + (getNodeWidth(l.outputNodeId) || minNodeWidth);
            if (toPos.x < outX - 20)
                return true;
        }
        return false;
    }

    // A node is pinned if its layout key is in the pinned set. Pinned nodes are
    // never auto-moved — pins are honored above all else.
    function nodeIsPinned(node) {
        return !!(node && node.layoutKey && pinnedNodes[node.layoutKey]);
    }

    // Still inside the settle window after it first appeared?
    function isFresh(nodeId) {
        var a = appearedAt[nodeId];
        return !!a && Date.now() - a.t <= settleWindowMs;
    }

    function nodeHasLinks(nodeId) {
        for (var li = 0; li < links.length; li++) {
            if (links[li].outputNodeId === nodeId || links[li].inputNodeId === nodeId)
                return true;
        }
        return false;
    }

    // How readily a node gives way when two unsettled nodes get linked:
    // streams come to devices and plugins, never the other way round.
    function placementMobility(node) {
        if (node.type === "StreamOutput" || node.type === "StreamInput")
            return 2;
        if (node.type === "Source" || node.type === "Sink" || node.type === "Duplex" || node.type === "Plugin" || node.type === "Subgraph")
            return 0;
        return 1;
    }

    // May `node` be placed beside `peer`? A settled peer is always an anchor.
    // When the peer is itself still waiting for placement, only one of the
    // two moves: the more mobile kind, else the one that appeared later.
    function peerAnchors(node, peer) {
        if (!awaitingLinkPlacement[peer.id] || !isFresh(peer.id))
            return true;
        var mn = placementMobility(node), mp = placementMobility(peer);
        if (mn !== mp)
            return mn > mp;
        var an = appearedAt[node.id], ap = appearedAt[peer.id];
        var sn = an ? an.seq : 0, sp = ap ? ap.seq : 0;
        if (sn !== sp)
            return sn > sp;
        return node.id > peer.id;
    }

    // The linked, positioned node that `node` should sit beside, as
    // { peer, nodeFeedsPeer } (nodeFeedsPeer: node output → peer input, so
    // the node belongs on the peer's left), or null.
    function placementPeer(node) {
        var nodeId = node.id;
        for (var li = 0; li < links.length; li++) {
            var link = links[li];
            var peerId = -1;
            var nodeFeedsPeer = false;
            if (link.outputNodeId === nodeId && link.inputNodeId !== nodeId) {
                peerId = link.inputNodeId;
                nodeFeedsPeer = true;
            } else if (link.inputNodeId === nodeId && link.outputNodeId !== nodeId) {
                peerId = link.outputNodeId;
            }
            if (peerId < 0 || !nodePositions[peerId])
                continue;
            var peerNode = findNodeData(peerId);
            if (!peerNode || (peerNode.layoutKey && hiddenNodes[peerNode.layoutKey]))
                continue;
            if (!peerAnchors(node, peerNode))
                continue;
            return {
                peer: peerNode,
                nodeFeedsPeer: nodeFeedsPeer
            };
        }
        return null;
    }

    // If `node` is linked to an already-positioned peer, place it beside that
    // peer (downstream → the column to the peer's right, upstream → to its
    // left) in the nearest free slot. Returns true if placed. Pinned nodes
    // are never moved.
    function placeNearPeer(node) {
        if (nodeIsPinned(node))
            return false;
        var pp = placementPeer(node);
        if (!pp)
            return false;
        var anchor = adjacentAnchor(node, pp.peer, pp.nodeFeedsPeer);
        nodePositions[node.id] = findFreeSpot(node, anchor.x, anchor.y, anchor.dir, false);
        delete awaitingLinkPlacement[node.id];
        persistLayout();
        return true;
    }

    // Top-left anchor placing `node` directly beside `peer`: to the peer's left
    // when the node feeds the peer (signal flows node → peer), otherwise to its
    // right, placeColumnGap away, at the peer's level. `dir` is the side the
    // search may spill to (-1 further left, 1 further right).
    function adjacentAnchor(node, peer, nodeFeedsPeer) {
        var peerPos = nodePositions[peer.id];
        var w = getNodeWidth(node.id);
        var peerW = getNodeWidth(peer.id);
        var x = nodeFeedsPeer ? (peerPos.x - w - placeColumnGap) : (peerPos.x + peerW + placeColumnGap);
        if (snapToGrid)
            x = nodeFeedsPeer ? Math.floor(x / gridSize) * gridSize : Math.ceil(x / gridSize) * gridSize;
        return {
            x: x,
            y: peerPos.y,
            dir: nodeFeedsPeer ? -1 : 1
        };
    }

    function findNodeByLayoutKey(key) {
        if (!key)
            return null;
        for (var i = 0; i < nodes.length; i++) {
            if (nodes[i].layoutKey === key)
                return nodes[i];
        }
        return null;
    }

    // Position in the signal flow, used to guess which side of a peer a node
    // belongs on before any links exist (sources left → sinks right).
    function flowRank(type) {
        if (type === "Source")
            return 0;
        if (type === "StreamOutput")
            return 1;
        if (type === "StreamInput")
            return 3;
        if (type === "Sink" || type === "Duplex")
            return 4;
        return 2;  // Plugin / Unknown — middle
    }

    // Where to drop a freshly-appeared node before its links exist: beside the
    // default node if one is set (a ruleless node connects there), otherwise
    // with its own kind inside the view. The second pass corrects this if
    // real links arrive shortly; if the node never connects, it stays here.
    // Returns { x, y, dir } for findFreeSpot.
    function provisionalAnchor(node) {
        var defNode = findNodeByLayoutKey(defaultNodeKey);
        if (defNode && defNode.id !== node.id && nodePositions[defNode.id] && !(defNode.layoutKey && hiddenNodes[defNode.layoutKey])) {
            var nodeFeedsDefault = flowRank(node.type) <= flowRank(defNode.type);
            return adjacentAnchor(node, defNode, nodeFeedsDefault);
        }
        return kindAnchor(node);
    }

    function findConnectedPeerPosition(nodeId) {
        // Look through current links to find any node already positioned
        // that is connected to this node
        for (var li = 0; li < links.length; li++) {
            var link = links[li];
            var peerId = -1;
            if (link.outputNodeId === nodeId && link.inputNodeId in nodePositions) {
                peerId = link.inputNodeId;
            } else if (link.inputNodeId === nodeId && link.outputNodeId in nodePositions) {
                peerId = link.outputNodeId;
            }
            if (peerId >= 0) {
                return nodePositions[peerId];
            }
        }
        return null;
    }

    function resolveNodeOverlap(node, skipIds) {
        var pos = nodePositions[node.id];
        if (!pos)
            return;
        var h = calculateNodeHeight(node);
        var w = getNodeWidth(node.id) || minNodeWidth;
        var maxAttempts = 50;
        var spacing = snapGap;
        for (var attempt = 0; attempt < maxAttempts; attempt++) {
            var overlaps = false;
            for (var oi = 0; oi < nodes.length; oi++) {
                var other = nodes[oi];
                if (other.id === node.id)
                    continue;
                if (skipIds && skipIds[other.id])
                    continue;
                var opos = nodePositions[other.id];
                if (!opos)
                    continue;
                var oh = calculateNodeHeight(other);
                var ow = getNodeWidth(other.id) || minNodeWidth;

                // Check AABB overlap
                if (pos.x < opos.x + ow && pos.x + w > opos.x && pos.y < opos.y + oh && pos.y + h > opos.y) {
                    // Calculate vertical overlap amounts
                    var overlapTop = opos.y + oh - pos.y;    // how far into the other from the top
                    var overlapBot = pos.y + h - opos.y;     // how far into the other from the bottom
                    // Snap above unless the node is mostly past the other (>70% overlap from top)
                    var minH = Math.min(h, oh);
                    if (overlapTop < minH * 0.7) {
                        pos = {
                            x: pos.x,
                            y: opos.y - h - spacing
                        };
                        if (snapToGrid)
                            pos.y = Math.floor(pos.y / gridSize) * gridSize;
                    } else {
                        pos = {
                            x: pos.x,
                            y: opos.y + oh + spacing
                        };
                        if (snapToGrid)
                            pos.y = Math.ceil(pos.y / gridSize) * gridSize;
                    }
                    nodePositions[node.id] = pos;
                    overlaps = true;
                    break;
                }
            }
            if (!overlaps)
                break;
        }
    }

    // Snap a node's position to nearby nodes.
    // Snaps left-edge to left-edge, and top/bottom edges within snapThreshold.
    // Returns { x, y, lines } where lines are the snap guide descriptors.
    // ---- Grid --------------------------------------------------------------

    // Nearest grid point to p (unchanged when snap to grid is off).
    function gridPos(p) {
        if (!snapToGrid || !p)
            return p;
        return {
            x: Math.round(p.x / gridSize) * gridSize,
            y: Math.round(p.y / gridSize) * gridSize
        };
    }

    // Put every visible node on the grid (when snapping is switched on or
    // the grid size changes), then separate any that now overlap.
    function alignAllToGrid() {
        var moved = false;
        for (var i = 0; i < nodes.length; i++) {
            var n = nodes[i];
            if (n.layoutKey && hiddenNodes[n.layoutKey])
                continue;
            var p = nodePositions[n.id];
            if (!p)
                continue;
            var g = gridPos(p);
            if (g.x !== p.x || g.y !== p.y) {
                nodePositions[n.id] = g;
                moved = true;
            }
        }
        if (!moved)
            return;
        for (var j = 0; j < nodes.length; j++) {
            if (nodePositions[nodes[j].id])
                resolveNodeOverlap(nodes[j]);
        }
        persistLayout();
        canvas.requestPaint();
    }

    // Shift-drag snapping against the other visible nodes (canvas units).
    // Aligning edges (left/left, top/top, bottom/bottom) snaps flush;
    // stacking above/below or placing beside another node keeps `snapGap`
    // between them. Returns { x, y, lines } with guide lines to draw.
    function computeSnap(nodeId, rawX, rawY, skipIds) {
        var w = getNodeWidth(nodeId) || minNodeWidth;
        var node = findNodeData(nodeId);
        var h = node ? calculateNodeHeight(node) : 80;
        var gap = snapGap;
        var best = {
            x: null,
            dx: snapThreshold + 1,
            gx: 0,
            y: null,
            dy: snapThreshold + 1,
            gy: 0
        };

        // Candidate position `value` for this axis; `guide` is the edge of
        // the other node that the guide line marks.
        function tryX(value, guide) {
            var d = Math.abs(rawX - value);
            if (d < best.dx) {
                best.dx = d;
                best.x = value;
                best.gx = guide;
            }
        }
        function tryY(value, guide) {
            var d = Math.abs(rawY - value);
            if (d < best.dy) {
                best.dy = d;
                best.y = value;
                best.gy = guide;
            }
        }

        // Only nodes at least partly on screen: when zoomed in, a drag must
        // not snap to something the user can't see.
        var visL = -panX / zoom;
        var visT = -panY / zoom;
        var visR = (width - panX) / zoom;
        var visB = (height - panY) / zoom;

        for (var i = 0; i < nodes.length; i++) {
            var other = nodes[i];
            if (other.id === nodeId)
                continue;
            if (skipIds && skipIds[other.id])
                continue;
            if (other.layoutKey && hiddenNodes[other.layoutKey])
                continue;
            var opos = nodePositions[other.id];
            if (!opos)
                continue;
            var ow = getNodeWidth(other.id) || minNodeWidth;
            var oh = calculateNodeHeight(other);
            if (opos.x + ow < visL || opos.x > visR || opos.y + oh < visT || opos.y > visB)
                continue;
            tryX(opos.x, opos.x);                          // left edges aligned
            tryX(opos.x + ow + gap, opos.x + ow);          // to the right of it
            tryX(opos.x - gap - w, opos.x);                // to the left of it

            tryY(opos.y, opos.y);                          // tops aligned
            tryY(opos.y + oh - h, opos.y + oh);            // bottoms aligned
            tryY(opos.y + oh + gap, opos.y + oh);          // stacked below it
            tryY(opos.y - gap - h, opos.y);                // stacked above it
        }

        var lines = [];
        var snappedX = rawX;
        var snappedY = rawY;
        if (best.x !== null && best.dx <= snapThreshold) {
            snappedX = best.x;
            lines.push({
                axis: "x",
                pos: best.gx
            });
        }
        if (best.y !== null && best.dy <= snapThreshold) {
            snappedY = best.y;
            lines.push({
                axis: "y",
                pos: best.gy
            });
        }
        return {
            x: snappedX,
            y: snappedY,
            lines: lines
        };
    }

    function persistLayout() {
        var layoutObj = {};
        for (var existingKey in savedLayout) {
            layoutObj[existingKey] = savedLayout[existingKey];
        }
        for (var ni = 0; ni < nodes.length; ni++) {
            var n = nodes[ni];
            var key = uniqueKeyById[n.id] || "";
            var pos = nodePositions[n.id];
            if (key && pos) {
                layoutObj[key] = [pos.x, pos.y];
            }
        }
        savedLayout = layoutObj;
        var json = JSON.stringify(layoutObj);
        if (deferSaveDepth > 0)
            pendingLayoutJson = json;
        else
            controller.save_layout(json);
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
            var vp = JSON.stringify({
                panX: panX,
                panY: panY,
                zoom: zoom
            });
            if (currentSubgraph !== "")
                controller.save_subgraph_view_state(currentSubgraph, vp);
            else
                controller.save_viewport(vp);
        }
    }

    onZoomChanged: if (viewportLoaded)
        viewportSaveTimer.restart()
    onPanXChanged: if (viewportLoaded)
        viewportSaveTimer.restart()
    onPanYChanged: if (viewportLoaded)
        viewportSaveTimer.restart()

    readonly property bool contextIsPlugin: contextNode !== null && contextNode.type === "Plugin"
    readonly property bool contextCanBeDefault: contextNode !== null && (contextNode.type === "Sink" || contextNode.type === "Duplex" || contextNode.type === "Plugin")

    // Device / sink: Hide · Pin · — · Set as default.
    // Plugin: Hide · Pin · — · Set as default · — · Group into subgraph… · Rename… · Open UI… · — · Remove plugin.
    // Subgraph: Open · Rename / edit… · Duplicate · Save as template… · — · Hide · Pin · — · Ungroup · Delete subgraph.
    // Virtual device: … · — · Edit virtual device… · Remove virtual device.
    ZbMenu {
        id: nodeContextMenu

        ZbMenuItem {
            text: "Open subgraph"
            visible: contextIsSubgraph
            height: visible ? implicitHeight : 0
            onTriggered: enterSubgraph(contextNode.subgraphId)
        }
        ZbMenuItem {
            text: "Edit subgraph\u2026"
            visible: contextIsSubgraph
            height: visible ? implicitHeight : 0
            onTriggered: openSubgraphEditor(contextNode.subgraphId)
        }
        ZbMenuItem {
            text: "Duplicate"
            visible: contextIsSubgraph
            height: visible ? implicitHeight : 0
            onTriggered: duplicateSubgraph(contextNode)
        }
        ZbMenuItem {
            text: "Save as template\u2026"
            visible: contextIsSubgraph
            height: visible ? implicitHeight : 0
            onTriggered: {
                templateSubgraphId = contextNode.subgraphId;
                templateNameField.text = contextNode.name;
                templateNameDialog.open();
            }
        }
        ZbMenuSeparator {
            visible: contextIsSubgraph
            height: visible ? implicitHeight : 0
        }

        ZbMenuItem {
            text: "Hide"
            onTriggered: {
                if (contextNode && contextNode.layoutKey) {
                    hiddenNodes[contextNode.layoutKey] = true;
                    hiddenNodes = hiddenNodes;
                    persistHidden();
                    pushUndo({
                        kind: "hide",
                        label: "hide 1 node",
                        keys: [contextNode.layoutKey]
                    });
                    canvas.requestPaint();
                }
            }
        }

        ZbMenuItem {
            text: contextNode && contextNode.layoutKey && pinnedNodes[contextNode.layoutKey] ? "Unpin" : "Pin"
            onTriggered: togglePinned(contextNode)
        }

        ZbMenuSeparator {
            visible: contextCanBeDefault
            height: visible ? implicitHeight : 0
        }

        ZbMenuItem {
            text: contextNode && contextNode.layoutKey === defaultNodeKey && defaultNodeKey !== "" ? "Clear default" : "Set as default"
            visible: contextCanBeDefault
            height: visible ? implicitHeight : 0
            onTriggered: {
                if (contextNode) {
                    var key = contextNode.layoutKey || "";
                    if (key === defaultNodeKey && defaultNodeKey !== "") {
                        defaultNodeKey = "";
                        controller.set_default_node("");
                    } else if (key !== "") {
                        defaultNodeKey = key;
                        controller.set_default_node(key);
                    }
                    canvas.requestPaint();
                }
            }
        }

        ZbMenuSeparator {
            visible: contextIsPlugin
            height: visible ? implicitHeight : 0
        }

        ZbMenuItem {
            text: "Group into subgraph\u2026"
            visible: contextIsPlugin && currentSubgraph === ""
            height: visible ? implicitHeight : 0
            onTriggered: groupIntoSubgraph(contextNodeId)
        }

        ZbMenuItem {
            text: "Rename\u2026"
            visible: contextIsPlugin
            height: visible ? implicitHeight : 0
            onTriggered: {
                renameField.text = contextNode ? contextNode.name : "";
                renameDialog.open();
            }
        }

        ZbMenuItem {
            text: "Open UI\u2026"
            visible: contextIsPlugin
            enabled: contextIsPlugin && contextNode.pluginHasUi !== false
            height: visible ? implicitHeight : 0
            onTriggered: {
                if (contextNodeId >= 0)
                    controller.open_plugin_ui(contextNodeId);
            }
        }

        ZbMenuSeparator {
            visible: contextIsPlugin
            height: visible ? implicitHeight : 0
        }

        ZbMenuItem {
            text: "Remove plugin"
            destructive: true
            visible: contextIsPlugin
            height: visible ? implicitHeight : 0
            onTriggered: {
                if (contextNodeId >= 0)
                    controller.remove_plugin(contextNodeId);
            }
        }

        ZbMenuSeparator {
            visible: contextIsSubgraph || contextIsVdev
            height: visible ? implicitHeight : 0
        }
        ZbMenuItem {
            text: "Ungroup"
            visible: contextIsSubgraph
            height: visible ? implicitHeight : 0
            onTriggered: controller.ungroup_subgraph(contextNode.subgraphId)
        }
        ZbMenuItem {
            text: "Delete subgraph"
            destructive: true
            visible: contextIsSubgraph
            height: visible ? implicitHeight : 0
            onTriggered: {
                deleteSubgraphDialog.subgraphId = contextNode.subgraphId;
                deleteSubgraphDialog.subgraphName = contextNode.name;
                deleteSubgraphDialog.open();
            }
        }
        ZbMenuItem {
            text: "Edit virtual device\u2026"
            visible: contextIsVdev
            height: visible ? implicitHeight : 0
            onTriggered: openVdevEditor(contextNode.vdevId)
        }
        ZbMenuItem {
            text: "Remove virtual device"
            destructive: true
            visible: contextIsVdev
            height: visible ? implicitHeight : 0
            onTriggered: controller.remove_virtual_device(contextNode.vdevId)
        }
    }

    // Right-clicked block line: the rule · Remove block
    ZbMenu {
        id: blockContextMenu
        onClosed: {
            contextBlock = null;
            canvas.requestPaint();
        }

        ZbMenuItem {
            text: contextBlock ? "Blocked: " + contextBlock.label : ""
            enabled: false
        }

        ZbMenuSeparator {}

        ZbMenuItem {
            text: "Remove block"
            onTriggered: {
                if (contextBlock)
                    controller.remove_rule(contextBlock.ruleId);
            }
        }
    }

    // Right-clicked wire: Disconnect · Block this connection
    ZbMenu {
        id: linkContextMenu

        ZbMenuItem {
            text: contextLinkIds.length > 1 ? "Disconnect " + contextLinkIds.length + " links" : "Disconnect"
            shortcutHint: "Del"
            onTriggered: {
                disconnectLinks(contextLinkIds);
                selectedLinks = {};
                canvas.requestPaint();
            }
        }

        ZbMenuItem {
            text: contextLinkIds.length > 1 ? "Block " + contextLinkIds.length + " connections" : "Block this connection"
            destructive: true
            onTriggered: blockLinks(contextLinkIds)
        }
    }

    // Right-click on a port or its name: Rename port… (plugins) · Disconnect.
    ZbMenu {
        id: portContextMenu

        ZbMenuItem {
            text: "Rename port\u2026"
            visible: contextIsPlugin
            height: visible ? implicitHeight : 0
            onTriggered: {
                renamePortField.text = portName(contextPortId);
                renamePortDialog.portId = contextPortId;
                renamePortDialog.open();
            }
        }

        ZbMenuItem {
            readonly property var attached: contextPortId >= 0 ? linksOfPort(contextPortId) : []
            text: attached.length > 1 ? "Disconnect " + attached.length + " links" : "Disconnect"
            enabled: attached.length > 0
            onTriggered: {
                disconnectLinks(attached);
                canvas.requestPaint();
            }
        }
    }

    // Delete selected links · Block selected links · — · Add plugin… · — · Unhide all · Fit to view · Reset zoom · Auto layout
    ZbMenu {
        id: canvasContextMenu

        ZbMenuItem {
            text: graphView.canUndo ? "Undo " + graphView.undoStack[graphView.undoStack.length - 1].label : "Undo"
            shortcutHint: "Ctrl+Z"
            enabled: graphView.canUndo
            onTriggered: undo()
        }

        ZbMenuItem {
            text: graphView.canRedo ? "Redo " + graphView.redoStack[graphView.redoStack.length - 1].label : "Redo"
            shortcutHint: "Ctrl+Shift+Z"
            enabled: graphView.canRedo
            onTriggered: redo()
        }

        ZbMenuSeparator {}

        ZbMenuItem {
            text: "Select all"
            shortcutHint: "Ctrl+A"
            onTriggered: selectAll()
        }

        ZbMenuItem {
            text: "Delete selected links"
            enabled: {
                for (var k in selectedLinks)
                    return true;
                return false;
            }
            onTriggered: deleteSelectedLinks()
        }

        ZbMenuItem {
            text: "Block selected links"
            destructive: true
            enabled: {
                for (var k in selectedLinks)
                    return true;
                return false;
            }
            onTriggered: blockSelectedLinks()
        }

        ZbMenuSeparator {}

        ZbMenuItem {
            text: "Add plugin\u2026"
            onTriggered: {
                if (contextCanvasPos) {
                    pendingPluginPosition = {
                        x: contextCanvasPos.x,
                        y: contextCanvasPos.y
                    };
                    pendingPluginName = "";
                }
                graphView.openPluginBrowser();
            }
        }

        ZbMenuItem {
            text: "Group into subgraph\u2026"
            visible: currentSubgraph === ""
            height: visible ? implicitHeight : 0
            enabled: selectedPluginIds().length > 0
            onTriggered: groupIntoSubgraph(-1)
        }

        ZbMenuItem {
            text: "Add virtual device\u2026"
            visible: currentSubgraph === ""
            height: visible ? implicitHeight : 0
            onTriggered: openVdevEditor("")
        }

        ZbMenuSeparator {}

        ZbMenuItem {
            text: "Unhide all"
            enabled: {
                for (var k in hiddenNodes)
                    return true;
                return false;
            }
            onTriggered: {
                hiddenNodes = {};
                persistHidden();
                canvas.requestPaint();
            }
        }

        ZbMenuItem {
            text: "Fit to view"
            shortcutHint: "Home"
            onTriggered: fitToView()
        }

        ZbMenuItem {
            text: "Reset zoom"
            onTriggered: zoomAt(width / 2, height / 2, 1.0)
        }

        ZbMenuItem {
            text: "Auto layout"
            shortcutHint: "L"
            onTriggered: requestAutoLayout(false)
        }

        ZbMenuItem {
            text: "Auto layout, including pinned"
            shortcutHint: "Shift+L"
            onTriggered: requestAutoLayout(true)
        }
    }

    SubgraphDialog {
        id: subgraphDialog
        onSubmitted: cfg => {
            if (mode === "create") {
                var newId = controller.create_subgraph(JSON.stringify(nodeIds), JSON.stringify(cfg));
                if (newId && groupAnchor) {
                    // The card takes the place of the plugins it replaces.
                    savedLayout["Subgraph:" + newId] = [groupAnchor.x, groupAnchor.y];
                    controller.save_layout(JSON.stringify(savedLayout));
                }
                clearSelection();
            } else {
                controller.update_subgraph(subgraphId, JSON.stringify(cfg));
            }
        }
    }

    VirtualDeviceDialog {
        id: vdevDialog
        onSubmitted: (name, kind, channels) => {
            if (vdevId === "")
                controller.add_virtual_device(name, kind, channels);
            else
                controller.update_virtual_device(vdevId, name, channels);
        }
    }

    // Preset selector: pick a preset, make one the default, or delete it.
    ZbMenu {
        id: presetMenu

        Instantiator {
            model: graphView.presetMenuNames
            delegate: ZbMenuItem {
                required property var modelData
                text: (modelData === graphView.presetMenuDefault ? "\u2605 " : "") + modelData
                selected: modelData === graphView.presetMenuSelected
                onTriggered: {
                    controller.apply_plugin_preset(graphView.presetMenuNodeId, modelData);
                    graphView.presetChanged();
                }
            }
            onObjectAdded: (index, object) => presetMenu.insertItem(index, object)
            onObjectRemoved: (index, object) => presetMenu.removeItem(object)
        }

        ZbMenuItem {
            text: "No presets saved yet"
            enabled: false
            visible: graphView.presetMenuNames.length === 0
            height: visible ? implicitHeight : 0
        }

        ZbMenuSeparator {}

        ZbMenuItem {
            text: "New preset (reset to defaults)"
            onTriggered: {
                controller.start_new_plugin_preset(graphView.presetMenuNodeId);
                graphView.presetChanged();
            }
        }

        ZbMenuSeparator {}

        // Why the name shows a "*": the first difference from the saved preset.
        ZbMenuItem {
            // Just the parameter name (the values are in the log): the menu is narrow.
            text: "Changed: " + (graphView.presetMenuChanges.length > 0 ? graphView.presetMenuChanges[0].split(": ")[0] : "") + (graphView.presetMenuChanges.length > 1 ? "  (+" + (graphView.presetMenuChanges.length - 1) + " more)" : "")
            enabled: false
            visible: graphView.presetMenuChanges.length > 0
            height: visible ? implicitHeight : 0
        }

        ZbMenuItem {
            text: "Set \u201c" + graphView.presetMenuSelected + "\u201d as default"
            enabled: graphView.presetMenuSelected !== "" && graphView.presetMenuSelected !== graphView.presetMenuDefault
            onTriggered: {
                controller.set_default_plugin_preset(graphView.presetMenuNodeId, graphView.presetMenuSelected);
                graphView.presetChanged();
            }
        }

        ZbMenuItem {
            text: "Clear default"
            enabled: graphView.presetMenuDefault !== ""
            onTriggered: {
                controller.set_default_plugin_preset(graphView.presetMenuNodeId, "");
                graphView.presetChanged();
            }
        }

        ZbMenuItem {
            text: "Delete \u201c" + graphView.presetMenuSelected + "\u201d"
            destructive: true
            enabled: graphView.presetMenuSelected !== ""
            onTriggered: {
                controller.delete_plugin_preset(graphView.presetMenuNodeId, graphView.presetMenuSelected);
                graphView.presetChanged();
            }
        }
    }

    // Save the plugin's current settings as a new preset, or over an existing one.
    ZbDialog {
        id: presetSaveDialog
        // The plugin's currently selected preset ("" for none), offered as an update target.
        property string selectedName: ""
        readonly property string trimmed: presetNameField.text.trim()
        readonly property bool exists: graphView.presetSaveNames.indexOf(trimmed) >= 0
        title: "Save preset"
        primaryText: exists ? "Replace preset" : "Save as new preset"
        secondaryText: "Cancel"
        width: 380
        primaryEnabled: trimmed.length > 0

        Column {
            width: parent ? parent.width : 320
            spacing: 8
            ZbTextField {
                id: presetNameField
                width: parent.width
                placeholderText: "New preset name"
                onAccepted: if (presetSaveDialog.primaryEnabled)
                    presetSaveDialog.accept()
            }
            Text {
                width: parent.width
                wrapMode: Text.WordWrap
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeCaption
                color: Theme.textMuted
                text: presetSaveDialog.exists ? "A preset with this name exists; saving replaces its values with the current settings." : "Saves the current parameter values as a new preset for this plugin."
            }
            ZbButton {
                visible: presetSaveDialog.selectedName !== ""
                text: "Update \u201c" + presetSaveDialog.selectedName + "\u201d with the current settings"
                onClicked: {
                    controller.save_plugin_preset(graphView.presetSaveNodeId, presetSaveDialog.selectedName);
                    graphView.presetChanged();
                    presetSaveDialog.close();
                }
            }
        }
        onAboutToShow: presetNameField.text = ""
        onOpened: presetNameField.forceActiveFocus()
        onAccepted: {
            controller.save_plugin_preset(graphView.presetSaveNodeId, trimmed);
            graphView.presetChanged();
        }
    }

    property string templateSubgraphId: ""
    ZbDialog {
        id: templateNameDialog
        title: "Save as template"
        primaryText: "Save template"
        secondaryText: "Cancel"
        width: 380
        primaryEnabled: templateNameField.text.trim().length > 0

        Column {
            width: parent ? parent.width : 320
            spacing: 8
            ZbTextField {
                id: templateNameField
                width: parent.width
                placeholderText: "Template name"
                onAccepted: if (templateNameDialog.primaryEnabled)
                    templateNameDialog.accept()
            }
            Text {
                width: parent.width
                wrapMode: Text.WordWrap
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeCaption
                color: Theme.textMuted
                text: "Saves the plugins with their settings and wiring. Insert it again from Add plugin \u203a Subgraphs."
            }
        }
        onOpened: {
            templateNameField.forceActiveFocus();
            templateNameField.selectAll();
        }
        onAccepted: controller.save_subgraph_template(templateSubgraphId, templateNameField.text.trim())
    }

    ZbDialog {
        id: deleteSubgraphDialog
        property string subgraphId: ""
        property string subgraphName: ""
        title: "Delete subgraph?"
        primaryText: "Delete"
        secondaryText: "Cancel"
        width: 400
        Text {
            width: parent ? parent.width : 340
            wrapMode: Text.WordWrap
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            color: Theme.textPrimary
            text: "\u201c" + deleteSubgraphDialog.subgraphName + "\u201d and the plugins inside it will be removed. To keep the plugins, use Ungroup instead."
        }
        onAccepted: controller.delete_subgraph(subgraphId)
    }

    // Delete key with plugins, subgraphs or virtual devices selected.
    ZbDialog {
        id: deleteSelectionDialog
        title: "Delete selection?"
        primaryText: "Delete"
        secondaryText: "Cancel"
        width: 420
        focus: true
        onOpened: deleteLines.forceActiveFocus()
        Column {
            id: deleteLines
            width: parent ? parent.width : 360
            spacing: 6
            // It was opened from the keyboard: Enter confirms, Esc cancels.
            Keys.onReturnPressed: deleteSelectionDialog.accept()
            Keys.onEnterPressed: deleteSelectionDialog.accept()
            Repeater {
                model: deleteSelectionDialog.visible ? graphView.deletePlanLines() : []
                Text {
                    required property string modelData
                    width: parent.width
                    wrapMode: Text.WordWrap
                    font.family: Theme.fontSans
                    font.pixelSize: Theme.sizeBody
                    color: Theme.textPrimary
                    text: "\u2022  " + modelData
                }
            }
            Text {
                width: parent.width
                topPadding: 6
                wrapMode: Text.WordWrap
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeBody
                color: Theme.textMuted
                text: "Removing can\u2019t be undone."
            }
        }
        onAccepted: graphView.performDelete()
        onRejected: graphView.deletePlan = null
    }

    ZbDialog {
        id: renameDialog
        title: "Rename plugin"
        primaryText: "Rename"
        secondaryText: "Cancel"
        width: 360

        ZbTextField {
            id: renameField
            width: parent ? parent.width : 300
            placeholderText: "New name"
            onAccepted: renameDialog.accept()
        }

        onOpened: {
            renameField.forceActiveFocus();
            renameField.selectAll();
        }
        onAccepted: {
            var newName = renameField.text.trim();
            if (newName.length > 0 && contextNodeId >= 0) {
                controller.rename_plugin(contextNodeId, newName);
            }
        }
    }

    ZbDialog {
        id: renamePortDialog
        property int portId: -1
        title: "Rename port"
        primaryText: "Rename"
        secondaryText: "Cancel"
        width: 360

        ZbTextField {
            id: renamePortField
            width: parent ? parent.width : 300
            placeholderText: "Port name (empty restores the original)"
            onAccepted: renamePortDialog.accept()
        }

        onOpened: {
            renamePortField.forceActiveFocus();
            renamePortField.selectAll();
        }
        onAccepted: {
            if (portId >= 0) {
                controller.rename_port(portId, renamePortField.text.trim());
                graphView.presetChanged();
            }
        }
    }

    // Confirmation dialog for inserting a plugin node onto a link
    property int pendingInsertLinkId: -1
    property int pendingInsertNodeId: -1

    ZbDialog {
        id: insertConfirmDialog
        title: "Insert plugin on link"
        primaryText: "Insert"
        secondaryText: "Cancel"
        width: 380

        Text {
            width: parent ? parent.width : 320
            text: "Insert this plugin between the connected nodes?"
            wrapMode: Text.WordWrap
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            color: Theme.textPrimary
        }

        onAccepted: {
            if (pendingInsertLinkId >= 0 && pendingInsertNodeId >= 0) {
                controller.insert_node_on_link(pendingInsertLinkId, pendingInsertNodeId);
            }
            pendingInsertLinkId = -1;
            pendingInsertNodeId = -1;
        }

        onRejected: {
            pendingInsertLinkId = -1;
            pendingInsertNodeId = -1;
        }
    }

    function persistHidden() {
        var arr = [];
        for (var k in hiddenNodes) {
            if (hiddenNodes[k])
                arr.push(k);
        }
        controller.save_hidden(JSON.stringify(arr));
    }

    function persistPinned() {
        var arr = [];
        for (var k in pinnedNodes) {
            if (pinnedNodes[k])
                arr.push(k);
        }
        var json = JSON.stringify(arr);
        if (deferSaveDepth > 0)
            pendingPinnedJson = json;
        else
            controller.save_pinned(json);
    }

    // Build a mapping from node.id → unique layout key, disambiguating nodes
    // that share a layoutKey by appending #1, #2, ... in node-id (creation)
    // order. Used for both saving and restoring layout.json, so duplicates get
    // distinct, stable keys and never persist ephemeral ids.
    function buildUniqueLayoutKeys() {
        var sorted = [];
        for (var i = 0; i < nodes.length; i++) {
            var n = nodes[i];
            var key = n.layoutKey || "";
            if (key)
                sorted.push({
                    id: n.id,
                    key: key
                });
        }
        sorted.sort(function (a, b) {
            return a.id - b.id;
        });
        var counts = {};
        var result = {};
        for (var j = 0; j < sorted.length; j++) {
            var base = sorted[j].key;
            var c = counts[base] || 0;
            result[sorted[j].id] = c === 0 ? base : base + "#" + c;
            counts[base] = c + 1;
        }
        return result;
    }

    // Sizes of the visible nodes, keyed by node id. Hidden nodes are left out
    // so the backend doesn't lay them out.
    function collectNodeSizes() {
        var sizesObj = {};
        for (var ni = 0; ni < nodes.length; ni++) {
            var n = nodes[ni];
            if (n.layoutKey && hiddenNodes[n.layoutKey])
                continue;
            sizesObj[n.id] = [getNodeWidth(n.id) || minNodeWidth, calculateNodeHeight(n)];
        }
        return sizesObj;
    }

    function collectPinnedPositions() {
        var pinned = {};
        for (var ni = 0; ni < nodes.length; ni++) {
            var n = nodes[ni];
            if (n.layoutKey && hiddenNodes[n.layoutKey])
                continue;
            if (nodeIsPinned(n) && nodePositions[n.id]) {
                var pos = nodePositions[n.id];
                pinned[n.id] = [pos.x, pos.y];
            }
        }
        return pinned;
    }

    // Visible nodes (not hidden) and how many of them are pinned.
    function visibleNodeCount() {
        var c = 0;
        for (var i = 0; i < nodes.length; i++) {
            if (!(nodes[i].layoutKey && hiddenNodes[nodes[i].layoutKey]))
                c++;
        }
        return c;
    }
    function pinnedVisibleCount() {
        var c = 0;
        for (var i = 0; i < nodes.length; i++) {
            var n = nodes[i];
            if (!(n.layoutKey && hiddenNodes[n.layoutKey]) && nodeIsPinned(n))
                c++;
        }
        return c;
    }

    // Auto layout, as the button, the L key and the menus ask for it. Pinned
    // nodes stay where they are, so when every node is pinned there is
    // nothing it may move: ask whether to lay out the pinned ones too.
    // includePinned (Shift+L, Shift-click, the menu item) lays out everything;
    // pinned nodes keep their pin, at their new place.
    function requestAutoLayout(includePinned) {
        var total = visibleNodeCount();
        if (total === 0) {
            showToast("Auto layout: there are no nodes to arrange");
            return;
        }
        if (!includePinned && pinnedVisibleCount() === total) {
            autoLayoutAllDialog.open();
            return;
        }
        runAutoLayout(includePinned);
    }

    function runAutoLayout(includePinned) {
        includePinned = !!includePinned;
        var kept = includePinned ? 0 : pinnedVisibleCount();
        var sizesJson = JSON.stringify(collectNodeSizes());
        var pinnedJson = includePinned ? "{}" : JSON.stringify(collectPinnedPositions());
        var resultJson = controller.auto_layout(sizesJson, pinnedJson, snapToGrid ? gridSize : 0);
        try {
            // Result is keyed by node id. Only the laid-out nodes move; hidden
            // nodes keep their positions and saved entries for absent devices
            // and plugins stay in savedLayout (persistLayout merges into it).
            var positions = JSON.parse(resultJson);
            var before = {};
            for (var ni = 0; ni < nodes.length; ni++) {
                var n = nodes[ni];
                var pos = positions[n.id];
                if (pos && pos.length >= 2) {
                    if (nodePositions[n.id])
                        before[n.id] = nodePositions[n.id];
                    nodePositions[n.id] = {
                        x: pos[0],
                        y: pos[1]
                    };
                    markHandPlaced(n.id);
                }
            }
            var laidOut = recordMove("auto layout of", before);
            if (laidOut)
                laidOut.label = includePinned ? "auto layout (all)" : "auto layout";
            var moved = laidOut ? laidOut.items.length : 0;
            var msg = moved > 0 ? "Auto layout: moved " + counted(moved, "node", "nodes") : "Auto layout: nothing needed to move";
            if (kept > 0)
                msg += "; " + counted(kept, "pinned node", "pinned nodes") + " stayed (Shift+L lays out pinned nodes too)";
            showToast(msg + ".");
            canvas.requestPaint();
            persistLayout();
        } catch (e) {
            console.log("Auto layout failed:", e);
            showToast("Auto layout failed");
        }
    }

    // Every node is pinned: offer to lay out the pinned ones as well.
    ZbDialog {
        id: autoLayoutAllDialog
        title: "Everything is pinned"
        primaryText: "Lay out all"
        secondaryText: "Cancel"
        width: 420
        Text {
            width: parent ? parent.width : 360
            wrapMode: Text.WordWrap
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            color: Theme.textPrimary
            text: "All " + graphView.visibleNodeCount() + " nodes are pinned, and Auto layout never moves pinned nodes. Lay them out anyway? They keep their pin, at their new place. Ctrl+Z undoes it."
        }
        onAccepted: graphView.runAutoLayout(true)
    }

    // Rects of the visible, positioned nodes other than `exceptId`.
    function placementObstacles(exceptId) {
        var obstacles = [];
        for (var oi = 0; oi < nodes.length; oi++) {
            var other = nodes[oi];
            if (other.id === exceptId)
                continue;
            if (other.layoutKey && hiddenNodes[other.layoutKey])
                continue;
            var opos = nodePositions[other.id];
            if (!opos)
                continue;
            obstacles.push({
                x: opos.x,
                y: opos.y,
                w: getNodeWidth(other.id),
                h: calculateNodeHeight(other)
            });
        }
        return obstacles;
    }

    // Does a node of size w×h placed at (x, y) clear every obstacle rect,
    // keeping at least `gap` between them?
    function spotIsFree(x, y, w, h, obstacles, gap) {
        var eps = 1e-6;
        for (var oi = 0; oi < obstacles.length; oi++) {
            var o = obstacles[oi];
            if (x < o.x + o.w + gap - eps && x + w + gap - eps > o.x && y < o.y + o.h + gap - eps && y + h + gap - eps > o.y)
                return false;
        }
        return true;
    }

    // Would `node` at (x, y) sit on top of another visible node? Only a real
    // overlap counts (a third of the smaller card each way), so cards that
    // merely touch after one of them grew are left alone.
    function spotIsTaken(node, x, y) {
        var w = getNodeWidth(node.id);
        var h = calculateNodeHeight(node);
        var obstacles = placementObstacles(node.id);
        for (var oi = 0; oi < obstacles.length; oi++) {
            var o = obstacles[oi];
            var ox = Math.min(x + w, o.x + o.w) - Math.max(x, o.x);
            var oy = Math.min(y + h, o.y + o.h) - Math.max(y, o.y);
            if (ox > Math.min(w, o.w) / 3 && oy > Math.min(h, o.h) / 3)
                return true;
        }
        return false;
    }

    // The free y nearest to `anchorY` for a w×h node in the column at `x`:
    // the anchor itself, or just below / above one of the nodes in the way
    // (on the grid when snapping). There always is one. Ties go below.
    function columnSlot(x, anchorY, w, h, obstacles, gap) {
        if (spotIsFree(x, anchorY, w, h, obstacles, gap))
            return anchorY;
        var best = null;
        var bestD = Infinity;
        for (var oi = 0; oi < obstacles.length; oi++) {
            var o = obstacles[oi];
            if (!(x < o.x + o.w + gap && x + w + gap > o.x))
                continue;
            var below = o.y + o.h + gap;
            var above = o.y - h - gap;
            if (snapToGrid) {
                below = Math.ceil(below / gridSize) * gridSize;
                above = Math.floor(above / gridSize) * gridSize;
            }
            var cands = [below, above];
            for (var ci = 0; ci < 2; ci++) {
                var d = Math.abs(cands[ci] - anchorY);
                if (d < bestD && spotIsFree(x, cands[ci], w, h, obstacles, gap)) {
                    bestD = d;
                    best = cands[ci];
                }
            }
        }
        return best !== null ? best : anchorY;
    }

    // The free spot for `node` nearest to (anchorX, anchorY): stacked in the
    // anchor's column (the nearest slot below or above), or, once that column
    // would put it too far away, in the next column over. `dir` is the side
    // further columns may be on: -1 left, 1 right (so a node placed beside a
    // peer never ends up behind it), 0 either. When `bounded`, a spot far
    // from the anchor is refused and null returned (the caller falls back to
    // in-hand placement); otherwise there always is a spot.
    function findFreeSpot(node, anchorX, anchorY, dir, bounded) {
        var w = getNodeWidth(node.id);
        var h = calculateNodeHeight(node);
        var gap = nodeSpacing;
        var a = gridPos({
            x: anchorX,
            y: anchorY
        });
        var colStep = w + placeColumnGap;
        if (snapToGrid)
            colStep = Math.ceil(colStep / gridSize) * gridSize;
        var obstacles = placementObstacles(node.id);
        var cols = dir !== 0 ? [0, dir, 2 * dir, 3 * dir] : [0, 1, -1, 2, -2];
        var best = null;
        var bestCost = Infinity;
        for (var ci = 0; ci < cols.length; ci++) {
            var k = cols[ci];
            var x = a.x + k * colStep;
            var y = columnSlot(x, a.y, w, h, obstacles, gap);
            var dy = Math.abs(y - a.y);
            if (bounded && (Math.abs(k) > 1 || dy > 2 * (h + gap)))
                continue;
            // A column further out costs as much as a tall stack in this one.
            var cost = dy + Math.abs(k) * colStep * 1.5;
            if (cost < bestCost) {
                bestCost = cost;
                best = {
                    x: x,
                    y: y
                };
            }
        }
        return best;
    }

    // Where an unlinked newcomer goes: into the column of a node of its own
    // kind that is on screen, else into the view at the place its kind has
    // in the signal flow (sources left … sinks right), below the canvas's
    // top-left controls. Returns { x, y, dir } for findFreeSpot.
    function kindAnchor(node) {
        var w = getNodeWidth(node.id);
        var rank = flowRank(node.type);
        if (width <= 0 || height <= 0) {
            var b = contentBounds();
            return {
                x: b ? b.left + (b.right - b.left - w) * rank / 4 : 48,
                y: b ? b.top : 72,
                dir: 0
            };
        }
        var visL = -panX / zoom, visT = -panY / zoom;
        var visR = (width - panX) / zoom, visB = (height - panY) / zoom;
        var cx = (visL + visR) / 2, cy = (visT + visB) / 2;
        var best = null;
        var bestD = Infinity;
        for (var i = 0; i < nodes.length; i++) {
            var o = nodes[i];
            if (o.id === node.id || flowRank(o.type) !== rank)
                continue;
            if (o.layoutKey && hiddenNodes[o.layoutKey])
                continue;
            var p = nodePositions[o.id];
            if (!p)
                continue;
            var ow = getNodeWidth(o.id), oh = calculateNodeHeight(o);
            if (p.x + ow < visL || p.x > visR || p.y + oh < visT || p.y > visB)
                continue;
            var d = Math.abs(p.x + ow / 2 - cx) + Math.abs(p.y + oh / 2 - cy);
            if (d < bestD) {
                bestD = d;
                best = p;
            }
        }
        if (best)
            return {
                x: best.x,
                y: best.y,
                dir: 0
            };
        var margin = 48 / zoom;
        var span = Math.max(0, visR - visL - w - 2 * margin);
        return {
            x: visL + margin + span * rank / 4,
            y: visT + 72 / zoom,
            dir: 0
        };
    }

    // Begin in-hand placement: the node tracks the cursor until dropped.
    function startPlacing(nodeId) {
        placingNodeId = nodeId;
        activeSnapLines = [];
        canvas.requestPaint();
    }

    // Finish in-hand placement, leaving the node wherever it currently sits.
    function finishPlacing() {
        if (placingNodeId < 0)
            return;
        if (nodePositions[placingNodeId])
            nodePositions[placingNodeId] = gridPos(nodePositions[placingNodeId]);
        delete autoPlaced[placingNodeId];
        delete placedKeyById[placingNodeId];
        placingNodeId = -1;
        persistLayout();
        canvas.requestPaint();
    }

    function isNodeBypassed(node) {
        if (node.id in localBypassState)
            return localBypassState[node.id];
        return node.pluginBypassed === true;
    }

    function findNodeData(nodeId) {
        for (var i = 0; i < nodes.length; i++) {
            if (nodes[i].id === nodeId)
                return nodes[i];
        }
        return null;
    }

    function getNodeColumn(type) {
        if (!type)
            return "stream";
        if (type === "Source")
            return "source";
        if (type === "Sink" || type === "Duplex")
            return "sink";
        return "stream";
    }

    // Category of a node for its header chip: { color, glyph }.
    function nodeCategory(node) {
        var type = node.type;
        if (node.mediaType === "Midi" || (node.layoutKey || "").indexOf("MidiBridge:") === 0)
            return {
                color: Theme.catMidi,
                glyph: "din"
            };
        if (type === "Plugin")
            return {
                color: Theme.catPlugin,
                glyph: "plug"
            };
        if (type === "Subgraph")
            return {
                color: Theme.catSubgraph,
                glyph: "subgraph"
            };
        if (type === "SubgraphInput" || type === "SubgraphOutput")
            return {
                color: Theme.catSubgraph,
                glyph: "arrow"
            };
        if (type === "StreamOutput" || type === "StreamInput")
            return {
                color: Theme.catApp,
                glyph: "wave"
            };
        if (node.mediaType === "Video")
            return {
                color: Theme.catHardware,
                glyph: "camera"
            };
        if (node.isVirtual)
            return {
                color: Theme.catVirtual,
                glyph: type === "Source" ? "mic" : "speaker"
            };
        if (type === "Source")
            return {
                color: Theme.catHardware,
                glyph: "mic"
            };
        if (type === "Sink" || type === "Duplex")
            return {
                color: Theme.catHardware,
                glyph: "headphones"
            };
        return {
            color: Theme.catHardware,
            glyph: "speaker"
        };
    }

    function signalColor(mediaType) {
        if (mediaType === "Midi")
            return Theme.sigMidi;
        if (mediaType === "Video")
            return Theme.sigVideo;
        return Theme.sigAudio;
    }

    // How many of a side's ports stay visible when collapsed: everything up
    // to the last connected port, and at least two.
    function visiblePortCount(ports) {
        var last = -1;
        for (var i = 0; i < ports.length; i++) {
            if (connectedPorts[ports[i].id])
                last = i;
        }
        return Math.min(ports.length, Math.max(last + 1, 3)); // 3 cause the top one is the midi out
    }

    // Visible ports, collapse state and height of a node card. Plugins with
    // many trailing unconnected ports fold them into one "n more" row.
    function computeNodeLayout(node, sorted) {
        var ins = sorted.inputs;
        var outs = sorted.outputs;
        var hiddenIn = 0;
        var hiddenOut = 0;
        if (node.type === "Plugin") {
            hiddenIn = ins.length - visiblePortCount(ins);
            hiddenOut = outs.length - visiblePortCount(outs);
            if (hiddenIn < collapseMinHidden)
                hiddenIn = 0;
            if (hiddenOut < collapseMinHidden)
                hiddenOut = 0;
        }
        var expandable = hiddenIn + hiddenOut > 0;
        var expanded = expandable && !!(node.layoutKey && expandedNodes[node.layoutKey]);
        var visIn = expandable && !expanded ? ins.slice(0, ins.length - hiddenIn) : ins;
        var visOut = expandable && !expanded ? outs.slice(0, outs.length - hiddenOut) : outs;
        var rows = Math.max(visIn.length, visOut.length, 1);
        var h = Theme.nodeHeader + Theme.nodePortsTop + Theme.nodePortRow * (rows + (expandable ? 1 : 0)) + Theme.nodePortsBottom + 2;
        if (node.type === "Plugin" || node.type === "Subgraph")
            h += Theme.nodeFooter;
        if (node.type === "Plugin")
            h += Theme.nodePresetRow;
        return {
            inputs: visIn,
            outputs: visOut,
            rows: rows,
            expandable: expandable,
            expanded: expanded,
            hiddenIn: hiddenIn,
            hiddenOut: hiddenOut,
            height: h
        };
    }

    function nodeLayout(node) {
        return nodeLayouts[node.id] || computeNodeLayout(node, getSortedPorts(node.id));
    }

    function calculateNodeHeight(node) {
        var cached = nodeHeights[node.id];
        if (cached !== undefined)
            return cached;
        return nodeLayout(node).height;
    }

    // Fixed card widths: devices, sinks and streams 184; plugins 208.
    function calculateNodeWidths() {
        var newWidths = {};
        for (var ni = 0; ni < nodes.length; ni++) {
            var node = nodes[ni];
            newWidths[node.id] = node.type === "Plugin" || node.type === "Subgraph" ? Theme.nodeWidthPlugin : Theme.nodeWidth;
        }
        nodeWidths = newWidths;
    }

    function getNodeWidth(nodeId) {
        return nodeWidths[nodeId] || minNodeWidth;
    }

    function toggleExpanded(node) {
        if (!node.layoutKey)
            return;
        if (expandedNodes[node.layoutKey])
            delete expandedNodes[node.layoutKey];
        else
            expandedNodes[node.layoutKey] = true;
        expandedNodes = expandedNodes;
        var lay = computeNodeLayout(node, getSortedPorts(node.id));
        nodeLayouts[node.id] = lay;
        nodeHeights[node.id] = lay.height;
        canvas.requestPaint();
    }

    function toCanvas(sx, sy) {
        return {
            x: (sx - panX) / zoom,
            y: (sy - panY) / zoom
        };
    }

    function toScreen(cx, cy) {
        return {
            x: cx * zoom + panX,
            y: cy * zoom + panY
        };
    }

    // Canvas-space bounding box of all visible (non-hidden) nodes, or null.
    function contentBounds() {
        var b = null;
        for (var i = 0; i < nodes.length; i++) {
            var node = nodes[i];
            if (node.layoutKey && hiddenNodes[node.layoutKey])
                continue;
            var pos = nodePositions[node.id];
            if (!pos)
                continue;
            var r = pos.x + getNodeWidth(node.id);
            var btm = pos.y + calculateNodeHeight(node);
            if (!b) {
                b = {
                    left: pos.x,
                    top: pos.y,
                    right: r,
                    bottom: btm
                };
            } else {
                b.left = Math.min(b.left, pos.x);
                b.top = Math.min(b.top, pos.y);
                b.right = Math.max(b.right, r);
                b.bottom = Math.max(b.bottom, btm);
            }
        }
        return b;
    }

    // Keep at least minVisibleContent screen px of the graph inside the
    // viewport so it can't be panned or zoomed out of reach.
    function clampPan() {
        var b = contentBounds();
        if (!b || width <= 0 || height <= 0)
            return;
        var keepX = Math.min(minVisibleContent, (b.right - b.left) * zoom, width);
        var keepY = Math.min(minVisibleContent, (b.bottom - b.top) * zoom, height);
        var l = b.left * zoom + panX, r = b.right * zoom + panX;
        var t = b.top * zoom + panY, btm = b.bottom * zoom + panY;
        if (r < keepX)
            panX += keepX - r;
        else if (l > width - keepX)
            panX -= l - (width - keepX);
        if (btm < keepY)
            panY += keepY - btm;
        else if (t > height - keepY)
            panY -= t - (height - keepY);
    }

    // Zoom to `newZoom` keeping the canvas point under screen (sx, sy) fixed.
    function zoomAt(sx, sy, newZoom) {
        hideNameTip();
        newZoom = Math.max(minZoom, Math.min(maxZoom, newZoom));
        var c = toCanvas(sx, sy);
        panX += c.x * (zoom - newZoom);
        panY += c.y * (zoom - newZoom);
        zoom = newZoom;
        clampPan();
        canvas.requestPaint();
    }

    // Center the view on all visible nodes, zoomed to fit (never above 100%).
    function fitToView() {
        fitBounds(contentBounds());
    }

    // Center the view on the selected nodes (all of them when none are).
    function frameSelection() {
        var b = null;
        for (var k in selectedNodes) {
            var node = findNodeData(parseInt(k));
            var pos = node ? nodePositions[node.id] : null;
            if (!pos || (node.layoutKey && hiddenNodes[node.layoutKey]))
                continue;
            var r = pos.x + getNodeWidth(node.id);
            var btm = pos.y + calculateNodeHeight(node);
            if (!b)
                b = {
                    left: pos.x,
                    top: pos.y,
                    right: r,
                    bottom: btm
                };
            else {
                b.left = Math.min(b.left, pos.x);
                b.top = Math.min(b.top, pos.y);
                b.right = Math.max(b.right, r);
                b.bottom = Math.max(b.bottom, btm);
            }
        }
        fitBounds(b || contentBounds());
    }

    // Zoom (never above 100%) and pan so the canvas box `b` is centred in
    // the part of the view that the top-left controls and the bottom hint
    // leave free.
    function fitBounds(b) {
        if (!b || width <= 0 || height <= 0) {
            zoom = 1.0;
            panX = 0;
            panY = 0;
            canvas.requestPaint();
            return;
        }
        var margin = 40;
        var top = 58, bottom = 52;
        var bw = Math.max(b.right - b.left, 1);
        var bh = Math.max(b.bottom - b.top, 1);
        var availH = Math.max(height - top - bottom, 1);
        var z = Math.min((width - 2 * margin) / bw, availH / bh, 1.0);
        zoom = Math.max(minZoom, z);
        panX = width / 2 - (b.left + bw / 2) * zoom;
        panY = top + availH / 2 - (b.top + bh / 2) * zoom;
        canvas.requestPaint();
    }

    // Scrollable range (canvas space) for one axis: the padded graph extent
    // united with the current view, so the bar always covers where you are.
    function scrollRange(lo, hi, viewLo, viewLen) {
        var pad = 40 / zoom;
        var a = Math.min(lo - pad, viewLo);
        var e = Math.max(hi + pad, viewLo + viewLen);
        return {
            lo: a,
            len: e - a
        };
    }

    // Sync the scroll bars with the current view. Called after every paint;
    // a bar being dragged keeps the range captured when the drag started.
    function updateScrollBars() {
        var b = contentBounds();
        var viewW = width / zoom, viewH = height / zoom;
        var viewL = -panX / zoom, viewT = -panY / zoom;
        if (!b) {
            hScrollBar.size = 1;
            vScrollBar.size = 1;
            return;
        }
        if (!hScrollBar.pressed) {
            var hr = scrollRange(b.left, b.right, viewL, viewW);
            hScrollBar.rangeLo = hr.lo;
            hScrollBar.rangeLen = hr.len;
            hScrollBar.size = Math.min(1, viewW / hr.len);
            hScrollBar.position = (viewL - hr.lo) / hr.len;
        }
        if (!vScrollBar.pressed) {
            var vr = scrollRange(b.top, b.bottom, viewT, viewH);
            vScrollBar.rangeLo = vr.lo;
            vScrollBar.rangeLen = vr.len;
            vScrollBar.size = Math.min(1, viewH / vr.len);
            vScrollBar.position = (viewT - vr.lo) / vr.len;
        }
    }

    // Is canvas point (cx, cy) covered by a visible node drawn above z-index `z`?
    function coveredAbove(cx, cy, z) {
        for (var i = nodes.length - 1; i > z; i--) {
            var n = nodes[i];
            if (n.layoutKey && hiddenNodes[n.layoutKey])
                continue;
            var pos = nodePositions[n.id];
            if (!pos)
                continue;
            if (cx >= pos.x && cx <= pos.x + getNodeWidth(n.id) && cy >= pos.y && cy <= pos.y + calculateNodeHeight(n)) {
                return true;
            }
        }
        return false;
    }

    // Nearest port under the cursor whose node isn't covered there by a node
    // drawn on top of it.
    function findPortAt(sx, sy) {
        var hitRadius = portRadius * zoom * 2.5;
        var c = toCanvas(sx, sy);
        var bestId = -1;
        var bestDist = hitRadius + 1;
        for (var pid in portPositions) {
            var pp = portPositions[pid];
            var screenX = pp.cx * zoom + panX;
            var screenY = pp.cy * zoom + panY;
            var dx = sx - screenX;
            var dy = sy - screenY;
            var dist = Math.sqrt(dx * dx + dy * dy);
            if (dist < bestDist && !coveredAbove(c.x, c.y, pp.z !== undefined ? pp.z : -1)) {
                bestDist = dist;
                bestId = parseInt(pid);
            }
        }
        return bestDist <= hitRadius ? bestId : -1;
    }

    // The port whose name is under the cursor (right-click target), or -1.
    function findPortLabelAt(sx, sy) {
        var c = toCanvas(sx, sy);
        var half = Theme.nodePortRow / 2;
        for (var pid in portPositions) {
            var pp = portPositions[pid];
            if (pp.labelX0 === undefined)
                continue;
            if (c.y < pp.cy - half || c.y > pp.cy + half || c.x < pp.labelX0 || c.x > pp.labelX1)
                continue;
            if (!coveredAbove(c.x, c.y, pp.z !== undefined ? pp.z : -1))
                return parseInt(pid);
        }
        return -1;
    }

    // Links attached to a port.
    function linksOfPort(portId) {
        var out = [];
        for (var i = 0; i < links.length; i++) {
            if (links[i].outputPortId === portId || links[i].inputPortId === portId)
                out.push(links[i].id);
        }
        return out;
    }

    function openPortMenu(portId) {
        var nodeId = getPortNodeId(portId);
        if (nodeId < 0)
            return;
        contextNodeId = nodeId;
        contextNode = findNodeData(nodeId);
        contextPortId = portId;
        portContextMenu.popup();
    }

    function getPortDirection(portId) {
        for (var nid in portsByNode) {
            var ports = portsByNode[nid];
            for (var i = 0; i < ports.length; i++) {
                if (ports[i].id === portId)
                    return ports[i].direction;
            }
        }
        return "";
    }

    function getPortNodeId(portId) {
        for (var nid in portsByNode) {
            var ports = portsByNode[nid];
            for (var i = 0; i < ports.length; i++) {
                if (ports[i].id === portId)
                    return parseInt(nid);
            }
        }
        return -1;
    }

    function findNodeAt(sx, sy) {
        var c = toCanvas(sx, sy);
        for (var i = nodes.length - 1; i >= 0; i--) {
            var n = nodes[i];
            if (n.layoutKey && hiddenNodes[n.layoutKey])
                continue;
            var pos = nodePositions[n.id];
            if (!pos)
                continue;
            var h = calculateNodeHeight(n);
            if (c.x >= pos.x && c.x <= pos.x + getNodeWidth(n.id) && c.y >= pos.y && c.y <= pos.y + h) {
                return n.id;
            }
        }
        return -1;
    }

    // Plugin footer buttons (On / UI / Params): canvas rects for node n.
    // The three buttons share the card width (10 px margins, 6 px gaps),
    // in proportion to their content so "Params" stays the widest.
    function refreshPresetInfo() {
        try {
            presetInfo = JSON.parse(controller.get_plugin_presets_all_json());
        } catch (e) {
            presetInfo = {};
        }
    }

    // The preset selector row of a plugin card, between the ports and the
    // footer: a name button (opens the preset menu) and a Save button.
    function presetRects(n, nx, ny, nh) {
        var margin = 10;
        var gap = 6;
        var saveW = 44;
        var h = Theme.nodeFooterBtn;
        var w = getNodeWidth(n.id);
        var y = ny + nh - Theme.nodeFooter - Theme.nodePresetRow + (Theme.nodePresetRow - h) / 2;
        return {
            name: {
                x: nx + margin,
                y: y,
                w: w - 2 * margin - gap - saveW,
                h: h
            },
            save: {
                x: nx + w - margin - saveW,
                y: y,
                w: saveW,
                h: h
            }
        };
    }

    function inRect(c, r) {
        return c.x >= r.x && c.x <= r.x + r.w && c.y >= r.y && c.y <= r.y + r.h;
    }

    function openPresetMenu(nodeId) {
        var info = {};
        try {
            info = JSON.parse(controller.get_plugin_presets_json(nodeId));
        } catch (e) {}
        presetMenuNodeId = nodeId;
        presetMenuNames = info.presets || [];
        presetMenuSelected = info.selected || "";
        presetMenuDefault = info["default"] || "";
        presetMenuChanges = info.changes || [];
        var pos = nodePositions[nodeId];
        var n = findNodeData(nodeId);
        if (!pos || !n)
            return;
        var r = presetRects(n, pos.x, pos.y, calculateNodeHeight(n)).name;
        var sp = toScreen(r.x, r.y + r.h);
        presetMenu.popup(sp.x, sp.y + 2);
    }

    function openPresetSave(nodeId) {
        var info = {};
        try {
            info = JSON.parse(controller.get_plugin_presets_json(nodeId));
        } catch (e) {}
        presetSaveNodeId = nodeId;
        presetSaveNames = info.presets || [];
        presetSaveDialog.selectedName = info.selected || "";
        presetSaveDialog.open();
    }

    function presetChanged() {
        refreshPresetInfo();
        canvas.requestPaint();
    }

    function footerButtons(n, nx, ny, nh) {
        var y = ny + nh - Theme.nodeFooter + (Theme.nodeFooter - Theme.nodeFooterBtn) / 2;
        var base = n.type === "Subgraph" ? [
            {
                key: "open",
                w: 1
            }
        ] : [
            {
                key: "onoff",
                w: 48
            },
            {
                key: "ui",
                w: 32
            },
            {
                key: "params",
                w: 56
            }
        ];
        var margin = 10;
        var gap = 6;
        var avail = getNodeWidth(n.id) - 2 * margin - gap * (base.length - 1);
        var total = 0;
        for (var t = 0; t < base.length; t++)
            total += base[t].w;
        var x = nx + margin;
        var out = [];
        for (var i = 0; i < base.length; i++) {
            var w = i === base.length - 1 ? nx + getNodeWidth(n.id) - margin - x : Math.round(avail * base[i].w / total);
            out.push({
                key: base[i].key,
                x: x,
                y: y,
                w: w,
                h: Theme.nodeFooterBtn
            });
            x += w + gap;
        }
        return out;
    }

    // Pin toggle in the header's right corner (canvas rect, 18 px hit box).
    function pinRect(n, nx, ny) {
        var s = 18;
        return {
            x: nx + getNodeWidth(n.id) - 8 - s,
            y: ny + (Theme.nodeHeader - s) / 2,
            w: s,
            h: s
        };
    }

    function togglePinned(node) {
        if (!node || !node.layoutKey)
            return;
        if (pinnedNodes[node.layoutKey])
            delete pinnedNodes[node.layoutKey];
        else
            pinnedNodes[node.layoutKey] = true;
        pinnedNodes = pinnedNodes;
        persistPinned();
        canvas.requestPaint();
    }

    // Canvas rect of a plugin's "n more" / "show fewer" row, or null.
    function toggleRowRect(n, lay, nx, ny) {
        if (!lay.expandable)
            return null;
        return {
            x: nx,
            y: ny + Theme.nodeHeader + Theme.nodePortsTop + lay.rows * Theme.nodePortRow,
            w: getNodeWidth(n.id),
            h: Theme.nodePortRow
        };
    }

    // Only the topmost node under the cursor can have its buttons hit.
    // Returns { button: "onoff" | "ui" | "params" | "toggle", nodeId, ... }.
    function findButtonAt(sx, sy) {
        var topId = findNodeAt(sx, sy);
        if (topId < 0)
            return null;
        var n = findNodeData(topId);
        if (!n)
            return null;
        var pos = nodePositions[n.id];
        if (!pos)
            return null;
        var c = toCanvas(sx, sy);
        var pb = pinRect(n, pos.x, pos.y);
        if (c.x >= pb.x && c.x <= pb.x + pb.w && c.y >= pb.y && c.y <= pb.y + pb.h)
            return {
                button: "pin",
                nodeId: n.id
            };
        var lay = nodeLayout(n);
        var tr = toggleRowRect(n, lay, pos.x, pos.y);
        if (tr && c.x >= tr.x && c.x <= tr.x + tr.w && c.y >= tr.y && c.y <= tr.y + tr.h)
            return {
                button: "toggle",
                nodeId: n.id
            };
        if (n.type !== "Plugin" && n.type !== "Subgraph")
            return null;
        if (n.type === "Plugin") {
            var pr = presetRects(n, pos.x, pos.y, calculateNodeHeight(n));
            if (inRect(c, pr.name))
                return {
                    button: "preset",
                    nodeId: n.id
                };
            if (inRect(c, pr.save))
                return {
                    button: "presetsave",
                    nodeId: n.id
                };
        }
        var btns = footerButtons(n, pos.x, pos.y, calculateNodeHeight(n));
        for (var i = 0; i < btns.length; i++) {
            var b = btns[i];
            if (c.x >= b.x && c.x <= b.x + b.w && c.y >= b.y && c.y <= b.y + b.h) {
                if (b.key === "open")
                    return {
                        button: "open",
                        nodeId: n.id,
                        subgraphId: n.subgraphId
                    };
                if (b.key === "onoff")
                    return {
                        button: "onoff",
                        nodeId: n.id,
                        bypassed: isNodeBypassed(n)
                    };
                if (b.key === "ui")
                    return {
                        button: "ui",
                        nodeId: n.id,
                        hasUi: n.pluginHasUi !== false
                    };
                return {
                    button: "params",
                    nodeId: n.id
                };
            }
        }
        return null;
    }

    // ---- Canvas drawing helpers -------------------------------------------

    function fontSpec(weight, size, mono) {
        return weight + " " + size + "px \"" + (mono ? Theme.fontMono : Theme.fontSans) + "\"";
    }

    // Elide `str` with an ellipsis to fit `maxW` in the current ctx.font.
    function elide(ctx, str, maxW) {
        if (!str)
            return "";
        if (ctx.measureText(str).width <= maxW)
            return str;
        var lo = 0;
        var hi = str.length;
        while (lo < hi) {
            var mid = Math.ceil((lo + hi) / 2);
            if (ctx.measureText(str.substring(0, mid) + "\u2026").width <= maxW)
                lo = mid;
            else
                hi = mid - 1;
        }
        return str.substring(0, lo) + "\u2026";
    }

    function drawGlyph(ctx, name, x, y, size, color) {
        var g = Glyphs.icons[name];
        if (!g)
            return;
        ctx.save();
        ctx.translate(x, y);
        ctx.scale(size / g.vb, size / g.vb);
        ctx.strokeStyle = "" + color;
        ctx.lineWidth = g.sw;
        ctx.lineCap = "round";
        ctx.lineJoin = "round";
        ctx.beginPath();
        ctx.path = g.d;
        ctx.stroke();
        ctx.restore();
    }

    function roundRectPath(ctx, x, y, w, h, r) {
        ctx.beginPath();
        ctx.moveTo(x + r, y);
        ctx.lineTo(x + w - r, y);
        ctx.arcTo(x + w, y, x + w, y + r, r);
        ctx.lineTo(x + w, y + h - r);
        ctx.arcTo(x + w, y + h, x + w - r, y + h, r);
        ctx.lineTo(x + r, y + h);
        ctx.arcTo(x, y + h, x, y + h - r, r);
        ctx.lineTo(x, y + r);
        ctx.arcTo(x, y, x + r, y, r);
        ctx.closePath();
    }

    // Small neutral tag (format "LV2", "default"): mono 9, strong outline.
    // Draws right-aligned at `right`; returns its left edge.
    function drawTag(ctx, text, right, cy) {
        ctx.font = fontSpec(500, Theme.sizeNodeTag, true);
        var tw = ctx.measureText(text).width + 8;
        var th = 14;
        roundRectPath(ctx, right - tw, cy - th / 2, tw, th, Theme.radiusTag);
        ctx.strokeStyle = "" + Theme.borderStrong;
        ctx.lineWidth = 1;
        ctx.stroke();
        ctx.fillStyle = "" + Theme.textMuted;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(text, right - tw / 2, cy + 0.5);
        return right - tw;
    }

    // The default target: accent chip with a dot, like "Rules active".
    // Draws right-aligned at `right`; returns its left edge.
    function drawDefaultChip(ctx, right, cy) {
        ctx.font = fontSpec(500, Theme.sizeNodeTag, true);
        var label = "default";
        var tw = ctx.measureText(label).width + 8 + 9;
        var th = 16;
        roundRectPath(ctx, right - tw, cy - th / 2, tw, th, Theme.radiusTag);
        ctx.fillStyle = "" + Theme.accentFaint;
        ctx.fill();
        ctx.beginPath();
        ctx.arc(right - tw + 7, cy, 2.5, 0, Math.PI * 2);
        ctx.fillStyle = "" + Theme.accent;
        ctx.fill();
        ctx.textAlign = "left";
        ctx.textBaseline = "middle";
        ctx.fillText(label, right - tw + 12, cy + 0.5);
        return right - tw;
    }

    function nodeMatchesSearch(node) {
        if (searchText.length === 0)
            return true;
        var q = searchText.toLowerCase();
        if ((node.name || "").toLowerCase().indexOf(q) >= 0)
            return true;
        var ports = portsByNode[node.id] || [];
        for (var i = 0; i < ports.length; i++) {
            if ((ports[i].name || "").toLowerCase().indexOf(q) >= 0)
                return true;
        }
        return false;
    }

    function portMatchesSearch(port) {
        return searchText.length > 0 && (port.name || "").toLowerCase().indexOf(searchText.toLowerCase()) >= 0;
    }

    // Node of the port a wire is being dragged from. Looked up once per
    // drag (and per refresh) instead of once per drawn pin.
    property int dragSourcePortId: -1
    property int dragSourceNodeId: -1
    function connectFromNodeId() {
        if (dragSourcePortId !== connectFromPortId) {
            dragSourceNodeId = getPortNodeId(connectFromPortId);
            dragSourcePortId = connectFromPortId;
        }
        return dragSourceNodeId;
    }

    // Could the wire being dragged end on this port?
    function isValidDropTarget(port, nodeId) {
        if (connectFromPortId < 0 || port.id === connectFromPortId)
            return false;
        if (port.direction === connectFromDir)
            return false;
        if (connectFromNodeId() === nodeId)
            return false;
        var fromMt = portMediaTypes[connectFromPortId];
        return !fromMt || fromMt === "Unknown" || !port.mediaType || port.mediaType === fromMt;
    }

    Canvas {
        id: canvas
        anchors.fill: parent

        onPaint: {
            var ctx = getContext("2d");
            ctx.reset();

            if (widthsDirty) {
                calculateNodeWidths();
                widthsDirty = false;
            }

            ctx.fillStyle = "" + Theme.bgCanvas;
            ctx.fillRect(0, 0, width, height);

            ctx.save();
            ctx.translate(panX, panY);
            ctx.scale(zoom, zoom);

            var visLeft = -panX / zoom;
            var visTop = -panY / zoom;
            var visRight = (width - panX) / zoom;
            var visBottom = (height - panY) / zoom;

            // ---- Dot grid (scene space; 1 screen px dots) -----------------
            var pitch = gridSize;
            while (pitch * zoom < 10)
                pitch *= 2;
            var dot = 1.5 / zoom;    // 1.5 screen px at any zoom
            ctx.fillStyle = "" + Theme.gridDot;
            var gx0 = Math.floor(visLeft / pitch) * pitch;
            var gy0 = Math.floor(visTop / pitch) * pitch;
            for (var gx = gx0; gx <= visRight; gx += pitch) {
                for (var gy = gy0; gy <= visBottom; gy += pitch) {
                    ctx.fillRect(gx, gy, dot, dot);
                }
            }

            var newPortPositions = {};
            var hdr = Theme.nodeHeader;
            var rowH = Theme.nodePortRow;
            var portsTop = hdr + Theme.nodePortsTop;

            // ---- Pass 1: port positions (pins sit on the card's edge) -----
            for (var ni = 0; ni < nodes.length; ni++) {
                var node = nodes[ni];
                if (node.layoutKey && hiddenNodes[node.layoutKey])
                    continue;
                var pos = nodePositions[node.id];
                if (!pos)
                    continue;
                var lay = nodeLayout(node);
                var nw = getNodeWidth(node.id);
                // Same label width as drawNode, for right-click targets.
                var labelBoth = lay.inputs.length > 0 && lay.outputs.length > 0;
                var labelWidth = labelBoth ? (nw - 2 * Theme.nodePadX - 8) / 2 : nw - 2 * Theme.nodePadX;
                for (var pi = 0; pi < lay.inputs.length; pi++) {
                    newPortPositions[lay.inputs[pi].id] = {
                        cx: pos.x,
                        cy: pos.y + portsTop + pi * rowH + rowH / 2,
                        nodeId: node.id,
                        z: ni,
                        labelX0: pos.x + Theme.nodePadX,
                        labelX1: pos.x + Theme.nodePadX + labelWidth
                    };
                }
                for (var po = 0; po < lay.outputs.length; po++) {
                    newPortPositions[lay.outputs[po].id] = {
                        cx: pos.x + nw,
                        cy: pos.y + portsTop + po * rowH + rowH / 2,
                        nodeId: node.id,
                        z: ni,
                        labelX0: pos.x + nw - Theme.nodePadX - labelWidth,
                        labelX1: pos.x + nw - Theme.nodePadX
                    };
                }
            }

            // ---- Pass 2: wires, behind the nodes --------------------------
            ctx.lineCap = "round";
            for (var li = 0; li < links.length; li++) {
                var link = links[li];
                var fromPos = newPortPositions[link.outputPortId];
                var toPos = newPortPositions[link.inputPortId];
                if (!fromPos || !toPos)
                    continue;
                var accented = !!selectedLinks[link.id] || !!selectedNodes[fromPos.nodeId] || !!selectedNodes[toPos.nodeId];
                var hovered = link.id === hoveredLinkId;
                var mt = portMediaTypes[link.outputPortId];
                if (!mt || mt === "Unknown")
                    mt = portMediaTypes[link.inputPortId];
                ctx.globalAlpha = accented || hovered ? 1 : (link.active === false ? 0.35 : Theme.wireOpacity);
                drawBezier(ctx, fromPos.cx, fromPos.cy, toPos.cx, toPos.cy, accented ? Theme.accent : signalColor(mt), accented || hovered ? Theme.wireWidthActive : Theme.wireWidth);
            }
            ctx.globalAlpha = 1;

            // ---- Block lines: red dashed, header to header, ⊘ midway ------
            var newBlockGeom = [];
            for (var bi = 0; bi < blockedPairs.length; bi++) {
                var bp = blockedPairs[bi];
                var bsPos = nodePositions[bp.source];
                var btPos = nodePositions[bp.target];
                if (!bsPos || !btPos)
                    continue;
                var bsNode = findNodeData(bp.source);
                var btNode = findNodeData(bp.target);
                if (!bsNode || !btNode)
                    continue;
                if ((bsNode.layoutKey && hiddenNodes[bsNode.layoutKey]) || (btNode.layoutKey && hiddenNodes[btNode.layoutKey]))
                    continue;
                var bx1 = bsPos.x + (getNodeWidth(bp.source) || minNodeWidth);
                var by1 = bsPos.y + 17;
                var bx2 = btPos.x;
                var by2 = btPos.y + 17;
                var bmx = (bx1 + bx2) / 2;
                var bmy = (by1 + by2) / 2;
                var bHot = contextBlock && contextBlock.ruleId === bp.ruleId && contextBlock.source === bp.source && contextBlock.target === bp.target;
                ctx.save();
                ctx.globalAlpha = bHot ? 1 : 0.8;
                ctx.setLineDash([6, 4]);
                drawBezier(ctx, bx1, by1, bx2, by2, Theme.textDestructive, bHot ? Theme.wireWidthActive : Theme.wireWidth);
                ctx.setLineDash([]);
                ctx.globalAlpha = 1;
                ctx.beginPath();
                ctx.arc(bmx, bmy, 7, 0, Math.PI * 2);
                ctx.fillStyle = "" + Theme.bgCanvas;
                ctx.fill();
                ctx.strokeStyle = "" + Theme.textDestructive;
                ctx.lineWidth = 1.5;
                ctx.stroke();
                ctx.beginPath();
                ctx.moveTo(bmx - 4.5, bmy + 4.5);
                ctx.lineTo(bmx + 4.5, bmy - 4.5);
                ctx.stroke();
                ctx.restore();
                newBlockGeom.push({
                    x1: bx1,
                    y1: by1,
                    x2: bx2,
                    y2: by2,
                    mx: bmx,
                    my: bmy,
                    ruleId: bp.ruleId,
                    label: bp.label,
                    source: bp.source,
                    target: bp.target
                });
            }
            blockGeom = newBlockGeom;

            // ---- Pass 3: node cards ---------------------------------------
            for (var ni2 = 0; ni2 < nodes.length; ni2++) {
                var n2 = nodes[ni2];
                if (n2.layoutKey && hiddenNodes[n2.layoutKey])
                    continue;
                var p2 = nodePositions[n2.id];
                if (!p2)
                    continue;
                drawNode(ctx, n2, p2.x, p2.y);
            }

            // ---- Drag wire: dashed, signal colour (red when blocking) ----
            if (connectFromPortId >= 0) {
                var dragFrom = newPortPositions[connectFromPortId] || portPositions[connectFromPortId];
                if (dragFrom) {
                    var dragToC = graphView.toCanvas(connectMouseX, connectMouseY);
                    var dragColor = connectBlocking || connectTargetBlocked ? Theme.textDestructive : signalColor(portMediaTypes[connectFromPortId]);
                    ctx.save();
                    ctx.setLineDash([4, 4]);
                    if (connectFromDir === "Input")
                        drawBezier(ctx, dragToC.x, dragToC.y, dragFrom.cx, dragFrom.cy, dragColor, Theme.wireWidthActive);
                    else
                        drawBezier(ctx, dragFrom.cx, dragFrom.cy, dragToC.x, dragToC.y, dragColor, Theme.wireWidthActive);
                    if (connectBlocking || connectTargetBlocked) {
                        // A small ⊘ at the cursor end.
                        ctx.setLineDash([]);
                        ctx.strokeStyle = "" + Theme.textDestructive;
                        ctx.lineWidth = 1.5 / zoom;
                        var br = 6 / zoom;
                        ctx.beginPath();
                        ctx.arc(dragToC.x, dragToC.y, br, 0, Math.PI * 2);
                        ctx.moveTo(dragToC.x - br * 0.7, dragToC.y + br * 0.7);
                        ctx.lineTo(dragToC.x + br * 0.7, dragToC.y - br * 0.7);
                        ctx.stroke();
                    }
                    ctx.restore();
                }
            }

            if (selectDragging) {
                var sc1 = graphView.toCanvas(selectStartX, selectStartY);
                var sc2 = graphView.toCanvas(selectEndX, selectEndY);
                ctx.beginPath();
                ctx.rect(Math.min(sc1.x, sc2.x), Math.min(sc1.y, sc2.y), Math.abs(sc2.x - sc1.x), Math.abs(sc2.y - sc1.y));
                ctx.fillStyle = "" + Theme.accentFaint;
                ctx.fill();
                ctx.strokeStyle = "" + Theme.accent;
                ctx.lineWidth = 1 / zoom;
                ctx.stroke();
            }

            // Snap guides while dragging with Shift.
            if (activeSnapLines.length > 0) {
                ctx.save();
                ctx.setLineDash([4 / zoom, 4 / zoom]);
                ctx.strokeStyle = "" + Theme.textMuted;
                ctx.lineWidth = 1 / zoom;
                for (var si = 0; si < activeSnapLines.length; si++) {
                    var sl = activeSnapLines[si];
                    ctx.beginPath();
                    if (sl.axis === "x") {
                        ctx.moveTo(sl.pos, visTop);
                        ctx.lineTo(sl.pos, visBottom);
                    } else {
                        ctx.moveTo(visLeft, sl.pos);
                        ctx.lineTo(visRight, sl.pos);
                    }
                    ctx.stroke();
                }
                ctx.restore();
            }

            ctx.restore();
            portPositions = newPortPositions;
            updateScrollBars();
        }
    }

    // One node card (design bible §6.5). Canvas coordinates, scene space.
    function drawNode(ctx, node, nx, ny) {
        var lay = nodeLayout(node);
        var nw = getNodeWidth(node.id);
        var nh = calculateNodeHeight(node);
        var isPlugin = node.type === "Plugin";
        var isSubgraph = node.type === "Subgraph";
        var selected = !!selectedNodes[node.id];
        var isDefault = defaultNodeKey !== "" && node.layoutKey === defaultNodeKey;
        var cat = nodeCategory(node);
        var hdr = Theme.nodeHeader;
        var rowH = Theme.nodePortRow;
        var portsTop = hdr + Theme.nodePortsTop;

        ctx.save();
        // Search: non-matching nodes dim to 35 %.
        var baseAlpha = nodeMatchesSearch(node) ? 1 : 0.35;

        // Shadow (layered rects, cheap) and selection halo.
        ctx.fillStyle = "" + Theme.shadowNode;
        ctx.globalAlpha = baseAlpha * 0.35;
        for (var sh = 0; sh < 3; sh++) {
            roundRectPath(ctx, nx - sh * 2, ny - sh * 2 + Theme.shadowNodeY * 0.5, nw + sh * 4, nh + sh * 4, Theme.radiusCard + sh * 2);
            ctx.fill();
        }
        ctx.globalAlpha = baseAlpha;
        if (selected) {
            roundRectPath(ctx, nx - 2.5, ny - 2.5, nw + 5, nh + 5, Theme.radiusCard + 2.5);
            ctx.strokeStyle = "" + Theme.accentRing;
            ctx.lineWidth = 3;
            ctx.stroke();
        }
        // A node that just appeared: an accent ring that grows and fades.
        var cue = nodeCues[node.id];
        if (cue) {
            var cueAge = Date.now() - cue.t;
            if (cueAge >= 0 && cueAge < cuePulseMs) {
                var ck = cueAge / cuePulseMs;
                var grow = 3 + 13 * ck;
                ctx.globalAlpha = baseAlpha * (1 - ck);
                roundRectPath(ctx, nx - grow, ny - grow, nw + 2 * grow, nh + 2 * grow, Theme.radiusCard + grow);
                ctx.strokeStyle = "" + Theme.accent;
                ctx.lineWidth = 2;
                ctx.stroke();
                ctx.globalAlpha = baseAlpha;
            }
        }

        // Subgraph: a second card edge behind, offset 4 px (a stack).
        if (isSubgraph) {
            roundRectPath(ctx, nx + 4, ny + 4, nw, nh, Theme.radiusCard);
            ctx.fillStyle = "" + Theme.bgSurface;
            ctx.fill();
            ctx.strokeStyle = "" + Theme.borderStrong;
            ctx.lineWidth = 1;
            ctx.stroke();
        }

        // Body. The node under the cursor gets an accent tint and a softer
        // accent border (selection is the full accent border and halo).
        var hovered = hoveredNodeId === node.id && !selected;
        roundRectPath(ctx, nx, ny, nw, nh, Theme.radiusCard);
        ctx.fillStyle = "" + Theme.bgSurface;
        ctx.fill();
        if (hovered) {
            ctx.fillStyle = "" + Theme.accentFaint;
            ctx.fill();
        }
        ctx.strokeStyle = "" + (selected || hovered ? Theme.accent : Theme.borderStrong);
        if (hovered)
            ctx.globalAlpha = baseAlpha * 0.7;
        ctx.lineWidth = 1;
        ctx.stroke();
        ctx.globalAlpha = baseAlpha;

        // Header: category chip, title, tags; hairline below.
        var chip = Theme.nodeChip;
        var chipX = nx + 10;
        var chipY = ny + (hdr - chip) / 2;
        roundRectPath(ctx, chipX, chipY, chip, chip, Theme.radiusChip);
        ctx.fillStyle = "" + Theme.tint(cat.color, Theme.chipAlpha);
        ctx.fill();
        drawGlyph(ctx, cat.glyph, chipX + (chip - Theme.nodeGlyph) / 2, chipY + (chip - Theme.nodeGlyph) / 2, Theme.nodeGlyph, cat.color);

        // Pin toggle: solid when pinned, faint outline when not.
        var pinned = !!(node.layoutKey && pinnedNodes[node.layoutKey]);
        var pinHover = hoveredButton === node.id + ":pin";
        var pr = pinRect(node, nx, ny);
        var pinCol = pinned || pinHover ? Theme.textIcon : Theme.textDisabled;
        ctx.save();
        if (!pinned && !pinHover)
            ctx.globalAlpha = baseAlpha * 0.6;
        if (pinned) {
            ctx.save();
            ctx.translate(pr.x + 3, pr.y + 3);
            ctx.beginPath();
            ctx.path = Glyphs.icons["pinFill"].d;
            ctx.fillStyle = "" + pinCol;
            ctx.fill();
            ctx.restore();
        }
        drawGlyph(ctx, "pin", pr.x + 3, pr.y + 3, 12, pinCol);
        ctx.restore();

        var tagRight = pr.x - 4;
        if (isPlugin && node.pluginFormat)
            tagRight = drawTag(ctx, node.pluginFormat, tagRight, ny + hdr / 2) - 6;
        if (isSubgraph) {
            var present = (node.memberCount || 0) - (node.missing || 0);
            tagRight = drawTag(ctx, node.missing > 0 ? present + "/" + node.memberCount : "" + (node.memberCount || 0), tagRight, ny + hdr / 2) - 6;
            if (node.asSink || node.asSource)
                tagRight = drawTag(ctx, "DEV", tagRight, ny + hdr / 2) - 6;
        }
        if (isDefault)
            tagRight = drawDefaultChip(ctx, tagRight, ny + hdr / 2) - 6;

        ctx.font = fontSpec(600, Theme.sizeNodeTitle, false);
        ctx.fillStyle = "" + Theme.textPrimary;
        ctx.textAlign = "left";
        ctx.textBaseline = "middle";
        var titleX = chipX + chip + 8;
        var shownTitle = elide(ctx, node.name, tagRight - titleX - 2);
        nameTruncated[node.id] = shownTitle !== node.name;
        ctx.fillText(shownTitle, titleX, ny + hdr / 2 + 0.5);

        ctx.fillStyle = "" + Theme.borderHairline;
        ctx.fillRect(nx + 1, ny + hdr, nw - 2, 1);

        // Port rows.
        var both = lay.inputs.length > 0 && lay.outputs.length > 0;
        var labelW = both ? (nw - 2 * Theme.nodePadX - 8) / 2 : nw - 2 * Theme.nodePadX;
        ctx.font = fontSpec(400, Theme.sizeMono, true);
        ctx.textBaseline = "middle";
        for (var i = 0; i < lay.inputs.length; i++) {
            var pin = lay.inputs[i];
            var py = ny + portsTop + i * rowH + rowH / 2;
            ctx.fillStyle = "" + Theme.textSecondary;
            ctx.textAlign = "left";
            ctx.fillText(elide(ctx, pin.name, labelW), nx + Theme.nodePadX, py);
            drawPin(ctx, pin, node.id, nx, py, selected);
        }
        for (var o = 0; o < lay.outputs.length; o++) {
            var pout = lay.outputs[o];
            var pyo = ny + portsTop + o * rowH + rowH / 2;
            ctx.font = fontSpec(400, Theme.sizeMono, true);
            ctx.fillStyle = "" + Theme.textSecondary;
            ctx.textAlign = "right";
            ctx.fillText(elide(ctx, pout.name, labelW), nx + nw - Theme.nodePadX, pyo);
            drawPin(ctx, pout, node.id, nx + nw, pyo, selected);
        }

        // Collapsed-ports row: "⌄ n more outputs" / "show fewer".
        var tr = toggleRowRect(node, lay, nx, ny);
        if (tr) {
            var hidden = lay.hiddenIn + lay.hiddenOut;
            var what = lay.hiddenIn > 0 && lay.hiddenOut > 0 ? "ports" : lay.hiddenOut > 0 ? (hidden === 1 ? "output" : "outputs") : (hidden === 1 ? "input" : "inputs");
            var label = lay.expanded ? "show fewer" : hidden + " more " + what;
            var hoverT = hoveredButton === node.id + ":toggle";
            ctx.font = fontSpec(400, Theme.sizeBadge, true);
            var lw = ctx.measureText(label).width + 12;
            var lx = lay.hiddenIn > 0 && lay.hiddenOut === 0 ? nx + Theme.nodePadX : lay.hiddenOut > 0 && lay.hiddenIn === 0 ? nx + nw - Theme.nodePadX - lw : nx + (nw - lw) / 2;
            var cy = tr.y + tr.h / 2;
            ctx.save();
            if (lay.expanded) {
                ctx.translate(lx + 4, cy);
                ctx.rotate(Math.PI);
                drawGlyph(ctx, "chevronDown", -4, -4, 8, hoverT ? Theme.textPrimary : Theme.textMuted);
            } else {
                drawGlyph(ctx, "chevronDown", lx, cy - 4, 8, hoverT ? Theme.textPrimary : Theme.textMuted);
            }
            ctx.restore();
            ctx.font = fontSpec(400, Theme.sizeBadge, true);
            ctx.fillStyle = "" + (hoverT ? Theme.textPrimary : Theme.textMuted);
            ctx.textAlign = "left";
            ctx.textBaseline = "middle";
            ctx.fillText(label, lx + 12, cy);
        }

        // Subgraph footer: one Open button; hairline above.
        if (isSubgraph) {
            ctx.fillStyle = "" + Theme.borderHairline;
            ctx.fillRect(nx + 1, ny + nh - Theme.nodeFooter, nw - 2, 1);
            var ob = footerButtons(node, nx, ny, nh)[0];
            var hoverO = hoveredButton === node.id + ":open";
            roundRectPath(ctx, ob.x, ob.y, ob.w, ob.h, Theme.radiusSmall);
            ctx.fillStyle = "" + (hoverO ? Theme.borderStrong : Theme.bgSubtle);
            ctx.fill();
            ctx.font = fontSpec(500, 11, false);
            var openTxt = "Open subgraph";
            var openW = 12 + 6 + ctx.measureText(openTxt).width;
            var ox = ob.x + (ob.w - openW) / 2;
            drawGlyph(ctx, "enter", ox, ob.y + (ob.h - 12) / 2, 12, Theme.textIcon);
            ctx.fillStyle = "" + Theme.textIcon;
            ctx.textAlign = "left";
            ctx.textBaseline = "middle";
            ctx.fillText(openTxt, ox + 18, ob.y + ob.h / 2 + 0.5);
        }

        // Plugin preset row: selected preset (menu) and Save.
        if (isPlugin) {
            var pinfo = presetInfo[node.id] || {
                name: "",
                isDefault: false,
                modified: false,
                count: 0
            };
            var prr = presetRects(node, nx, ny, nh);
            var hoverName = hoveredButton === node.id + ":preset";
            var hoverSave = hoveredButton === node.id + ":presetsave";
            roundRectPath(ctx, prr.name.x, prr.name.y, prr.name.w, prr.name.h, Theme.radiusSmall);
            ctx.fillStyle = "" + (hoverName ? Theme.borderStrong : Theme.bgSubtle);
            ctx.fill();
            var hasName = pinfo.name !== "";
            var label = hasName ? (pinfo.isDefault ? "\u2605 " : "") + pinfo.name + (pinfo.modified ? " *" : "") : (pinfo.count > 0 ? "Preset\u2026" : "No presets");
            ctx.font = fontSpec(500, 11, false);
            ctx.fillStyle = "" + (hasName ? Theme.textPrimary : Theme.textMuted);
            ctx.textAlign = "left";
            ctx.textBaseline = "middle";
            ctx.fillText(elide(ctx, label, prr.name.w - 12 - 18), prr.name.x + 8, prr.name.y + prr.name.h / 2 + 0.5);
            // chevron
            var chx = prr.name.x + prr.name.w - 12;
            var chy = prr.name.y + prr.name.h / 2;
            ctx.beginPath();
            ctx.moveTo(chx - 3.5, chy - 1.5);
            ctx.lineTo(chx, chy + 2);
            ctx.lineTo(chx + 3.5, chy - 1.5);
            ctx.strokeStyle = "" + Theme.textIcon;
            ctx.lineWidth = 1.5;
            ctx.stroke();
            roundRectPath(ctx, prr.save.x, prr.save.y, prr.save.w, prr.save.h, Theme.radiusSmall);
            ctx.fillStyle = "" + (hoverSave ? Theme.borderStrong : Theme.bgSubtle);
            ctx.fill();
            ctx.font = fontSpec(500, 11, false);
            ctx.fillStyle = "" + Theme.textIcon;
            ctx.textAlign = "center";
            ctx.fillText("Save", prr.save.x + prr.save.w / 2, prr.save.y + prr.save.h / 2 + 0.5);
        }

        // Plugin footer: On / UI / Params; hairline above.
        if (isPlugin) {
            ctx.fillStyle = "" + Theme.borderHairline;
            ctx.fillRect(nx + 1, ny + nh - Theme.nodeFooter, nw - 2, 1);
            var bypassed = isNodeBypassed(node);
            var hasUi = node.pluginHasUi !== false;
            var btns = footerButtons(node, nx, ny, nh);
            for (var b = 0; b < btns.length; b++) {
                var bt = btns[b];
                var hoverB = hoveredButton === node.id + ":" + bt.key;
                var on = bt.key === "onoff" && !bypassed;
                roundRectPath(ctx, bt.x, bt.y, bt.w, bt.h, Theme.radiusSmall);
                ctx.fillStyle = "" + (on ? Theme.accentFaint : (hoverB ? Theme.borderStrong : Theme.bgSubtle));
                ctx.fill();
                var txt = bt.key === "onoff" ? (bypassed ? "Off" : "On") : bt.key === "ui" ? "UI" : "Params";
                var col = on ? Theme.accent : bt.key === "ui" && !hasUi ? Theme.textDisabled : bt.key === "onoff" ? Theme.textMuted : Theme.textIcon;
                ctx.font = fontSpec(on ? 600 : 500, 11, false);
                ctx.fillStyle = "" + col;
                ctx.textBaseline = "middle";
                if (bt.key === "onoff") {
                    // Dot + label, centred as a group in the (wider) button.
                    var groupW = 6 + 5 + ctx.measureText(txt).width;
                    var gx = bt.x + (bt.w - groupW) / 2;
                    ctx.beginPath();
                    ctx.arc(gx + 3, bt.y + bt.h / 2, 3, 0, Math.PI * 2);
                    if (on) {
                        ctx.fill();
                    } else {
                        ctx.strokeStyle = "" + col;
                        ctx.lineWidth = 1.25;
                        ctx.stroke();
                    }
                    ctx.textAlign = "left";
                    ctx.fillText(txt, gx + 11, bt.y + bt.h / 2 + 0.5);
                } else {
                    ctx.textAlign = "center";
                    ctx.fillText(txt, bt.x + bt.w / 2, bt.y + bt.h / 2 + 0.5);
                }
            }
        }
        ctx.restore();
    }

    // A 10 px pin centred on the card edge: filled when connected, hollow
    // ring when free; accent on the selected node; halos for hover, valid
    // drop targets and search matches.
    function drawPin(ctx, port, nodeId, cx, cy, nodeSelected) {
        var connected = !!connectedPorts[port.id];
        var col = signalColor(port.mediaType);
        if (nodeSelected && connected)
            col = Theme.accent;
        var r = port.id === hoveredPortId ? Theme.nodePinHover / 2 : Theme.nodePin / 2;

        if (connectFromPortId >= 0 && isValidDropTarget(port, nodeId)) {
            ctx.beginPath();
            ctx.arc(cx, cy, r + 4, 0, Math.PI * 2);
            ctx.fillStyle = "" + Theme.accentRing;
            ctx.fill();
        }
        ctx.beginPath();
        ctx.arc(cx, cy, r, 0, Math.PI * 2);
        if (connected) {
            ctx.fillStyle = "" + col;
            ctx.fill();
        } else {
            ctx.fillStyle = "" + Theme.bgSurface;
            ctx.fill();
            ctx.beginPath();
            ctx.arc(cx, cy, r - 1, 0, Math.PI * 2);
            ctx.strokeStyle = "" + col;
            ctx.lineWidth = 2;
            ctx.stroke();
        }
        if (portMatchesSearch(port)) {
            ctx.beginPath();
            ctx.arc(cx, cy, r + 3, 0, Math.PI * 2);
            ctx.strokeStyle = "" + Theme.accent;
            ctx.lineWidth = 2;
            ctx.stroke();
        }
    }

    focus: true

    // Escape belongs to the graph whenever it has something to cancel, even
    // if a window-level Shortcut elsewhere also listens for it.
    Keys.onShortcutOverride: event => {
        if (event.key === Qt.Key_Escape && (placingNodeId >= 0 || connectFromPortId >= 0 || hasSelection() || currentSubgraph !== ""))
            event.accepted = true;
    }

    Keys.onPressed: event => {
        var ctrl = !!(event.modifiers & Qt.ControlModifier);
        var shift = !!(event.modifiers & Qt.ShiftModifier);
        var plain = !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier));

        // Held keys that override the drag mode (see overrideState).
        if (event.key === Qt.Key_Shift) {
            shiftHeld = true;
            return;
        }
        if (event.key === Qt.Key_Space && plain) {
            if (!event.isAutoRepeat)
                spaceHeld = true;
            event.accepted = true;
            return;
        }

        if (event.key === Qt.Key_Delete || event.key === Qt.Key_Backspace) {
            deleteSelection();
            event.accepted = true;
        } else if (event.key === Qt.Key_Home) {
            fitToView();
            event.accepted = true;
        } else if (event.key === Qt.Key_W && plain && !event.isAutoRepeat) {
            toggleDragMode();
            event.accepted = true;
        } else if (event.key === Qt.Key_F && plain && !event.isAutoRepeat) {
            frameSelection();
            event.accepted = true;
        } else if (event.key === Qt.Key_L && !event.isAutoRepeat && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) {
            requestAutoLayout(shift);
            event.accepted = true;
        } else if (ctrl && event.key === Qt.Key_A) {
            selectAll();
            event.accepted = true;
        } else if (ctrl && event.key === Qt.Key_Z) {
            if (shift)
                redo();
            else
                undo();
            event.accepted = true;
        } else if (ctrl && event.key === Qt.Key_Y) {
            redo();
            event.accepted = true;
        } else if (ctrl && (event.key === Qt.Key_Plus || event.key === Qt.Key_Equal)) {
            zoomAt(width / 2, height / 2, zoom * 1.1);
            event.accepted = true;
        } else if (ctrl && event.key === Qt.Key_Minus) {
            zoomAt(width / 2, height / 2, zoom / 1.1);
            event.accepted = true;
        } else if (ctrl && event.key === Qt.Key_0) {
            zoomAt(width / 2, height / 2, 1.0);
            event.accepted = true;
        } else if (!ctrl && (event.key === Qt.Key_Left || event.key === Qt.Key_Right || event.key === Qt.Key_Up || event.key === Qt.Key_Down)) {
            var ndx = event.key === Qt.Key_Left ? -1 : event.key === Qt.Key_Right ? 1 : 0;
            var ndy = event.key === Qt.Key_Up ? -1 : event.key === Qt.Key_Down ? 1 : 0;
            if (nudgeSelection(ndx, ndy, shift))
                event.accepted = true;
        } else if (event.key === Qt.Key_Escape) {
            if (placingNodeId >= 0) {
                finishPlacing();
            } else if (connectFromPortId >= 0) {
                connectFromPortId = -1;
                connectFromDir = "";
                connectBlocking = false;
                connectHoverPortId = -1;
                connectTargetBlocked = false;
                canvas.requestPaint();
            } else if (hasSelection()) {
                clearSelection();
            } else if (currentSubgraph !== "") {
                exitSubgraph();
            }
            event.accepted = true;
        }
    }

    Keys.onReleased: event => {
        if (event.key === Qt.Key_Shift) {
            shiftHeld = false;
        } else if (event.key === Qt.Key_Space && !event.isAutoRepeat) {
            spaceHeld = false;
            event.accepted = true;
        }
    }

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        hoverEnabled: true

        onDoubleClicked: mouse => {
            if (mouse.button !== Qt.LeftButton)
                return;
            var dn = findNodeData(findNodeAt(mouse.x, mouse.y));
            if (dn && dn.type === "Subgraph")
                enterSubgraph(dn.subgraphId);
        }

        property real lastX: 0
        property real lastY: 0
        property real pressX: 0
        property real pressY: 0

        onPressed: mouse => {
            graphView.forceActiveFocus();
            lastX = mouse.x;
            lastY = mouse.y;
            pressX = mouse.x;
            pressY = mouse.y;
            hideNameTip();
            grabPanning = false;
            spacePanning = false;
            shiftHeld = !!(mouse.modifiers & Qt.ShiftModifier);

            // In-hand placement: a click drops the node where it currently sits.
            if (placingNodeId >= 0) {
                finishPlacing();
                return;
            }

            if (mouse.button === Qt.MiddleButton) {
                middlePanning = true;
                updateCursor();
                return;
            }

            if (mouse.button === Qt.RightButton) {
                // Right-drag from a port draws a block wire; a right-click on
                // a port without dragging opens the node menu on release.
                var blockPort = findPortAt(mouse.x, mouse.y);
                if (blockPort >= 0) {
                    connectFromPortId = blockPort;
                    connectFromDir = getPortDirection(blockPort);
                    connectBlocking = true;
                    connectPressX = mouse.x;
                    connectPressY = mouse.y;
                    connectMouseX = mouse.x;
                    connectMouseY = mouse.y;
                    return;
                }
                var labelPort = findPortLabelAt(mouse.x, mouse.y);
                if (labelPort >= 0) {
                    openPortMenu(labelPort);
                    return;
                }
                var nodeId = findNodeAt(mouse.x, mouse.y);
                var menuLinkId = nodeId < 0 ? findLinkAt(mouse.x, mouse.y) : -1;
                if (menuLinkId >= 0) {
                    var menuLink = findLinkData(menuLinkId);
                    if (selectedLinks[menuLinkId]) {
                        var ids = [];
                        for (var sk in selectedLinks) {
                            var sid = parseInt(sk);
                            if (linkMatchesSig(findLinkData(sid), selectedLinks[sk]))
                                ids.push(sid);
                        }
                        contextLinkIds = ids;
                    } else {
                        var one = {};
                        if (menuLink)
                            one[menuLinkId] = linkSig(menuLink);
                        selectedLinks = one;
                        selectedNodes = {};
                        contextLinkIds = [menuLinkId];
                        canvas.requestPaint();
                    }
                    linkContextMenu.popup();
                } else if (nodeId < 0 && findBlockAt(mouse.x, mouse.y)) {
                    contextBlock = findBlockAt(mouse.x, mouse.y);
                    canvas.requestPaint();
                    blockContextMenu.popup();
                } else if (nodeId >= 0) {
                    contextNodeId = nodeId;
                    contextNode = findNodeData(nodeId);
                    contextPortId = -1;
                    nodeContextMenu.popup();
                } else {
                    contextNodeId = -1;
                    contextNode = null;
                    var cPos = toCanvas(mouse.x, mouse.y);
                    contextCanvasPos = {
                        x: cPos.x,
                        y: cPos.y
                    };
                    canvasContextMenu.popup();
                }
                return;
            }

            if (mouse.button === Qt.LeftButton) {
                // Space held: pan from anywhere, whatever is under the cursor.
                if (spaceHeld) {
                    spacePanning = true;
                    updateCursor();
                    return;
                }

                var btnHit = findButtonAt(mouse.x, mouse.y);
                if (btnHit) {
                    if (btnHit.button === "toggle") {
                        var tn = findNodeData(btnHit.nodeId);
                        if (tn)
                            toggleExpanded(tn);
                    } else if (btnHit.button === "pin") {
                        togglePinned(findNodeData(btnHit.nodeId));
                    } else if (btnHit.button === "onoff") {
                        var newBypassed = !btnHit.bypassed;
                        // Immediate local feedback
                        localBypassState[btnHit.nodeId] = newBypassed;
                        localBypassState = localBypassState;  // trigger change signal
                        controller.set_plugin_bypass(btnHit.nodeId, newBypassed);
                        canvas.requestPaint();
                    } else if (btnHit.button === "ui" && btnHit.hasUi) {
                        controller.open_plugin_ui(btnHit.nodeId);
                    } else if (btnHit.button === "params") {
                        graphView.openPluginParams(btnHit.nodeId);
                    } else if (btnHit.button === "preset") {
                        openPresetMenu(btnHit.nodeId);
                    } else if (btnHit.button === "presetsave") {
                        openPresetSave(btnHit.nodeId);
                    } else if (btnHit.button === "open") {
                        enterSubgraph(btnHit.subgraphId);
                    }
                    return;
                }

                var portId = findPortAt(mouse.x, mouse.y);
                if (portId >= 0) {
                    connectFromPortId = portId;
                    connectFromDir = getPortDirection(portId);
                    connectMouseX = mouse.x;
                    connectMouseY = mouse.y;
                    return;
                }

                var nodeIdDrag = findNodeAt(mouse.x, mouse.y);
                if (nodeIdDrag >= 0) {
                    var ctrlHeld = (mouse.modifiers & Qt.ControlModifier);
                    var nodeIsSelected = !!selectedNodes[nodeIdDrag];

                    if (ctrlHeld) {
                        var newSel = Object.assign({}, selectedNodes);
                        if (nodeIsSelected) {
                            delete newSel[nodeIdDrag];
                        } else {
                            newSel[nodeIdDrag] = nodeSig(findNodeData(nodeIdDrag));
                        }
                        selectedNodes = newSel;
                        selectedLinks = {};
                        canvas.requestPaint();
                        return;
                    }

                    if (nodeIsSelected) {
                        groupDragging = true;
                        for (var agid in selectedNodes) {
                            if (selectedNodes[agid])
                                markHandPlaced(agid);
                        }
                        var cg = toCanvas(mouse.x, mouse.y);
                        groupDragStartX = cg.x;
                        groupDragStartY = cg.y;
                        var starts = {};
                        for (var gsid in selectedNodes) {
                            if (selectedNodes[gsid] && nodePositions[gsid])
                                starts[gsid] = {
                                    x: nodePositions[gsid].x,
                                    y: nodePositions[gsid].y
                                };
                        }
                        groupDragStart = starts;
                    } else {
                        selectedNodes = {};
                        selectedLinks = {};
                        dragNodeId = nodeIdDrag;
                        markHandPlaced(nodeIdDrag);
                        var c = toCanvas(mouse.x, mouse.y);
                        var pos = nodePositions[nodeIdDrag];
                        dragOffsetX = c.x - pos.x;
                        dragOffsetY = c.y - pos.y;
                        dragStartPos = {
                            x: pos.x,
                            y: pos.y
                        };
                    }
                    canvas.requestPaint();
                    return;
                }

                var clickedLinkId = findLinkAt(mouse.x, mouse.y);
                if (clickedLinkId >= 0) {
                    var ctrlHeldLink = (mouse.modifiers & Qt.ControlModifier);
                    var clickedLink = findLinkData(clickedLinkId);
                    if (ctrlHeldLink) {
                        var newSelLinks = Object.assign({}, selectedLinks);
                        if (newSelLinks[clickedLinkId]) {
                            delete newSelLinks[clickedLinkId];
                        } else if (clickedLink) {
                            newSelLinks[clickedLinkId] = linkSig(clickedLink);
                        }
                        selectedLinks = newSelLinks;
                    } else {
                        var freshSel = {};
                        if (clickedLink)
                            freshSel[clickedLinkId] = linkSig(clickedLink);
                        selectedLinks = freshSel;
                        selectedNodes = {};
                    }
                    canvas.requestPaint();
                    return;
                }

                // Empty canvas: pan it (grab mode) or start a selection box
                // (select mode, or Shift held in grab mode). With Shift or
                // Ctrl the box adds to the current selection.
                var boxShift = !!(mouse.modifiers & Qt.ShiftModifier);
                if (dragMode === "grab" && !boxShift) {
                    grabPanning = true;
                    grabMoved = false;
                    updateCursor();
                    return;
                }
                selectAdditive = boxShift || !!(mouse.modifiers & Qt.ControlModifier);
                if (selectAdditive) {
                    selectBaseNodes = Object.assign({}, selectedNodes);
                    selectBaseLinks = Object.assign({}, selectedLinks);
                } else {
                    clearSelection();
                }
                selectDragging = true;
                selectStartX = mouse.x;
                selectStartY = mouse.y;
                selectEndX = mouse.x;
                selectEndY = mouse.y;
            }
        }

        onPositionChanged: mouse => {
            shiftHeld = !!(mouse.modifiers & Qt.ShiftModifier);
            // In-hand placement: keep the node centred under the cursor.
            if (placingNodeId >= 0) {
                var pc = toCanvas(mouse.x, mouse.y);
                var pn = findNodeData(placingNodeId);
                var pw = (getNodeWidth(placingNodeId) || minNodeWidth);
                var ph = pn ? calculateNodeHeight(pn) : 80;
                nodePositions[placingNodeId] = {
                    x: pc.x - pw / 2,
                    y: pc.y - ph / 2
                };
                canvas.requestPaint();
                return;
            }

            if ((mouse.buttons & Qt.MiddleButton) || ((grabPanning || spacePanning) && (mouse.buttons & Qt.LeftButton))) {
                if (grabPanning && !grabMoved && (Math.abs(mouse.x - pressX) > 4 || Math.abs(mouse.y - pressY) > 4))
                    grabMoved = true;
                panX += mouse.x - lastX;
                panY += mouse.y - lastY;
                clampPan();
                lastX = mouse.x;
                lastY = mouse.y;
                canvas.requestPaint();
                return;
            }

            if (groupDragging && (mouse.buttons & Qt.LeftButton)) {
                // Move from the start positions; grid or Shift-snap the
                // reference node and offset the whole group by the same amount.
                var cg = toCanvas(mouse.x, mouse.y);
                var deltaX = cg.x - groupDragStartX;
                var deltaY = cg.y - groupDragStartY;
                var refId = -1;
                for (var rid in groupDragStart) {
                    refId = parseInt(rid);
                    break;
                }
                var adjX = 0;
                var adjY = 0;
                activeSnapLines = [];
                if (refId >= 0) {
                    var rawGX = groupDragStart[refId].x + deltaX;
                    var rawGY = groupDragStart[refId].y + deltaY;
                    if (snapToGrid) {
                        var gp = gridPos({
                            x: rawGX,
                            y: rawGY
                        });
                        adjX = gp.x - rawGX;
                        adjY = gp.y - rawGY;
                    } else if (mouse.modifiers & Qt.ShiftModifier) {
                        var gsnap = computeSnap(refId, rawGX, rawGY, selectedNodes);
                        adjX = gsnap.x - rawGX;
                        adjY = gsnap.y - rawGY;
                        activeSnapLines = gsnap.lines;
                    }
                }
                for (var nid in groupDragStart) {
                    nodePositions[nid] = {
                        x: groupDragStart[nid].x + deltaX + adjX,
                        y: groupDragStart[nid].y + deltaY + adjY
                    };
                }
                canvas.requestPaint();
                return;
            }

            if (dragNodeId >= 0 && (mouse.buttons & Qt.LeftButton)) {
                var c = toCanvas(mouse.x, mouse.y);
                var rawX = c.x - dragOffsetX;
                var rawY = c.y - dragOffsetY;
                if (snapToGrid) {
                    nodePositions[dragNodeId] = gridPos({
                        x: rawX,
                        y: rawY
                    });
                    activeSnapLines = [];
                } else if (mouse.modifiers & Qt.ShiftModifier) {
                    var snap = computeSnap(dragNodeId, rawX, rawY, null);
                    nodePositions[dragNodeId] = {
                        x: snap.x,
                        y: snap.y
                    };
                    activeSnapLines = snap.lines;
                } else {
                    nodePositions[dragNodeId] = {
                        x: rawX,
                        y: rawY
                    };
                    activeSnapLines = [];
                }
                canvas.requestPaint();
                return;
            }

            if (connectFromPortId >= 0) {
                connectMouseX = mouse.x;
                connectMouseY = mouse.y;
                if (!connectBlocking) {
                    var hoverPort = findPortAt(mouse.x, mouse.y);
                    if (hoverPort !== connectHoverPortId) {
                        connectHoverPortId = hoverPort;
                        connectTargetBlocked = hoverPort >= 0 && getPortDirection(hoverPort) !== connectFromDir && blockedReasonFor(connectFromPortId, connectFromDir, hoverPort) !== "";
                    }
                }
                canvas.requestPaint();
                return;
            }

            if (selectDragging) {
                selectEndX = mouse.x;
                selectEndY = mouse.y;
                canvas.requestPaint();
                return;
            }

            if (mouse.buttons === Qt.NoButton)
                updateHover(mouse.x, mouse.y);
        }

        onExited: updateHover(-1, -1)

        onReleased: mouse => {
            activeSnapLines = [];
            if (mouse.button === Qt.MiddleButton) {
                middlePanning = false;
                updateCursor();
            }
            if (mouse.button === Qt.RightButton && connectBlocking) {
                var moved = Math.abs(mouse.x - connectPressX) > 4 || Math.abs(mouse.y - connectPressY) > 4;
                var blockFrom = connectFromPortId;
                var blockDir = connectFromDir;
                connectFromPortId = -1;
                connectFromDir = "";
                connectBlocking = false;
                canvas.requestPaint();
                if (!moved) {
                    openPortMenu(blockFrom);
                    return;
                }
                var blockTo = findPortAt(mouse.x, mouse.y);
                if (blockTo >= 0 && blockTo !== blockFrom && getPortDirection(blockTo) !== blockDir && getPortNodeId(blockTo) !== getPortNodeId(blockFrom)) {
                    if (blockDir === "Output")
                        controller.block_ports(blockFrom, blockTo);
                    else
                        controller.block_ports(blockTo, blockFrom);
                }
                return;
            }
            if (mouse.button === Qt.LeftButton) {
                if (grabPanning) {
                    grabPanning = false;
                    // A click (no drag) on empty canvas clears the selection.
                    if (!grabMoved)
                        clearSelection();
                    updateCursor();
                }
                if (spacePanning) {
                    spacePanning = false;
                    updateCursor();
                }

                if (connectFromPortId >= 0) {
                    var targetId = findPortAt(mouse.x, mouse.y);
                    if (targetId >= 0 && targetId !== connectFromPortId) {
                        var targetDir = getPortDirection(targetId);
                        var fromNodeId = getPortNodeId(connectFromPortId);
                        var toNodeId = getPortNodeId(targetId);
                        if (targetDir !== connectFromDir && fromNodeId !== toNodeId) {
                            var why = blockedReasonFor(connectFromPortId, connectFromDir, targetId);
                            if (why !== "") {
                                showToast("Blocked by rule " + why + ". Remove the block to connect.");
                            } else if (connectFromDir === "Output") {
                                connectPorts(connectFromPortId, targetId);
                            } else {
                                connectPorts(targetId, connectFromPortId);
                            }
                        }
                    }
                    connectFromPortId = -1;
                    connectFromDir = "";
                    connectHoverPortId = -1;
                    connectTargetBlocked = false;
                    canvas.requestPaint();
                }

                if (groupDragging) {
                    groupDragging = false;
                    // Resolve overlaps for each node in the group against
                    // nodes outside the group (skip fellow group members)
                    for (var gid in selectedNodes) {
                        if (selectedNodes[gid]) {
                            var gNode = findNodeData(parseInt(gid));
                            if (gNode) {
                                resolveNodeOverlap(gNode, selectedNodes);
                            }
                        }
                    }
                    recordMove("move", groupDragStart);
                    persistLayout();
                    canvas.requestPaint();
                }

                if (dragNodeId >= 0) {
                    var linkUnder = findLinkUnderNode(dragNodeId);
                    if (linkUnder >= 0) {
                        var draggedNode = findNodeData(dragNodeId);
                        if (draggedNode && draggedNode.type === "Plugin") {
                            pendingInsertLinkId = linkUnder;
                            pendingInsertNodeId = dragNodeId;
                            insertConfirmDialog.open();
                        }
                    }
                    // Resolve overlap after dropping a single node
                    var droppedNode = findNodeData(dragNodeId);
                    if (droppedNode) {
                        resolveNodeOverlap(droppedNode);
                    }
                    if (dragStartPos) {
                        var movedFrom = {};
                        movedFrom[dragNodeId] = dragStartPos;
                        recordMove("move", movedFrom);
                    }
                    persistLayout();
                    canvas.requestPaint();
                }
                dragNodeId = -1;
                dragStartPos = null;

                if (selectDragging) {
                    selectDragging = false;
                    var dx = Math.abs(selectEndX - selectStartX);
                    var dy = Math.abs(selectEndY - selectStartY);
                    if (dx > 5 || dy > 5) {
                        var boxLinks = findLinksInRect(selectStartX, selectStartY, selectEndX, selectEndY);
                        var boxNodes = findNodesInRect(selectStartX, selectStartY, selectEndX, selectEndY);
                        if (selectAdditive) {
                            boxLinks = Object.assign({}, selectBaseLinks, boxLinks);
                            boxNodes = Object.assign({}, selectBaseNodes, boxNodes);
                        }
                        selectedLinks = boxLinks;
                        selectedNodes = boxNodes;
                    }
                    selectAdditive = false;
                    canvas.requestPaint();
                }
            }
        }

        // The wheel zooms around the cursor.
        onWheel: wheel => {
            if (wheel.angleDelta.y === 0)
                return;
            var factor = wheel.angleDelta.y > 0 ? 1.1 : 0.9;
            zoomAt(wheel.x, wheel.y, zoom * factor);
        }

        // Touchpad / touchscreen pinch zooms around the fingers.
        PinchHandler {
            id: pinchZoom
            target: null
            property real startZoom: 1
            onActiveChanged: if (active)
                startZoom = graphView.zoom
            onActiveScaleChanged: if (active)
                graphView.zoomAt(centroid.position.x, centroid.position.y, startZoom * activeScale)
        }
    }

    // Inside a subgraph: "Graph › Mic Chain" at the top-left; "Graph" (or
    // Escape) goes back to the main graph.
    // Drag mode switch at the canvas's top-left: what a left-drag on empty
    // canvas does. The active mode is the accent-tinted cell; W toggles.
    component ModeCell: AbstractButton {
        id: cell
        property string iconName
        property string tip
        property bool active: false
        width: 28
        height: 28
        hoverEnabled: true
        // Keep the keyboard on the canvas (W, Delete, Home, Esc).
        focusPolicy: Qt.NoFocus
        Accessible.name: tip
        background: Rectangle {
            radius: Theme.radiusSmall
            color: cell.active ? Theme.accentFaint : cell.hovered || cell.down ? Theme.bgHover : "transparent"
            border.width: cell.active ? 1 : 0
            border.color: Theme.accentRing
        }
        contentItem: Item {
            ZbIcon {
                anchors.centerIn: parent
                name: cell.iconName
                size: 18
                color: cell.active ? Theme.accent : cell.hovered ? Theme.textIcon : Theme.textMuted
            }
        }
        ZbTooltip {
            visible: cell.hovered
            text: cell.tip
        }
    }

    Rectangle {
        id: modeSwitch
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.margins: 12
        height: 34
        width: modeRow.implicitWidth + 6
        radius: Theme.radiusControl
        color: Theme.bgPanel
        border.width: 1
        border.color: Theme.borderStrong

        // Presses on the pill itself never reach the canvas.
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            hoverEnabled: true
            cursorShape: Qt.ArrowCursor
            onEntered: graphView.updateHover(-1, -1)
        }

        Row {
            id: modeRow
            anchors.centerIn: parent
            spacing: 2
            ModeCell {
                iconName: "grab"
                tip: "Grab: drag the canvas to move around (W)"
                active: graphView.dragMode === "grab"
                onClicked: graphView.setDragMode("grab")
            }
            ModeCell {
                iconName: "select"
                tip: "Select: drag a box around nodes and wires (W)"
                active: graphView.dragMode === "select"
                onClicked: graphView.setDragMode("select")
            }
        }
    }

    // Auto layout at the canvas's top-right (clear of the scroll bar): a
    // plain click arranges everything that is not pinned, Shift+click (or
    // Shift+L) the pinned nodes too.
    Rectangle {
        id: autoLayoutButton
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.rightMargin: 12 + (vScrollBar.visible ? vScrollBar.width : 0)
        anchors.topMargin: 12
        height: 34
        width: 34
        radius: Theme.radiusControl
        color: Theme.bgPanel
        border.width: 1
        border.color: Theme.borderStrong

        AbstractButton {
            id: autoLayoutCell
            anchors.centerIn: parent
            width: 28
            height: 28
            hoverEnabled: true
            focusPolicy: Qt.NoFocus   // the canvas keeps the keyboard
            Accessible.name: "Auto layout"
            onClicked: graphView.requestAutoLayout(graphView.shiftHeld)
            background: Rectangle {
                radius: Theme.radiusSmall
                color: autoLayoutCell.down ? Theme.borderStrong : autoLayoutCell.hovered ? Theme.bgHover : "transparent"
            }
            contentItem: Item {
                ZbIcon {
                    anchors.centerIn: parent
                    name: "autoLayout"
                    size: 18
                    color: autoLayoutCell.hovered ? Theme.textPrimary : Theme.textIcon
                }
            }
            ZbTooltip {
                visible: autoLayoutCell.hovered
                text: "Auto layout (L). Pinned nodes stay put; Shift+click (Shift+L) lays them out too."
            }
        }
    }

    Rectangle {
        id: breadcrumb
        visible: currentSubgraph !== ""
        anchors.left: modeSwitch.right
        anchors.leftMargin: 8
        anchors.top: parent.top
        anchors.topMargin: 12
        height: 34
        width: crumbRow.implicitWidth + 12
        radius: Theme.radiusControl
        color: Theme.bgPanel
        border.width: 1
        border.color: Theme.borderStrong

        Row {
            id: crumbRow
            anchors.verticalCenter: parent.verticalCenter
            anchors.left: parent.left
            anchors.leftMargin: 4
            spacing: 4
            ZbButton {
                id: crumbHome
                kind: "subtle"
                compact: true
                text: "Graph"
                anchors.verticalCenter: parent.verticalCenter
                onClicked: exitSubgraph()
                ZbTooltip {
                    visible: crumbHome.hovered
                    text: "Back to the main graph (Esc)"
                }
            }
            ZbIcon {
                name: "chevronRight"
                size: 10
                color: Theme.textMuted
                anchors.verticalCenter: parent.verticalCenter
            }
            ZbIcon {
                name: "subgraph"
                size: 14
                color: Theme.catSubgraph
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                text: currentSubgraphName
                anchors.verticalCenter: parent.verticalCenter
                rightPadding: 6
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeBody
                font.weight: Font.DemiBold
                color: Theme.textPrimary
            }
        }
    }

    // New nodes that appeared outside the view: a marker at the nearest
    // edge, pointing at the node; click to go there.
    Repeater {
        model: graphView.edgeMarkers
        delegate: Rectangle {
            id: marker
            required property var modelData
            x: Math.max(8, Math.min(graphView.width - width - 8, modelData.x - width / 2))
            y: Math.max(8, Math.min(graphView.height - height - 8, modelData.y - height / 2))
            width: markerRow.implicitWidth + 20
            height: 26
            radius: height / 2
            color: Theme.bgPanel
            border.width: 1
            border.color: Theme.accentRing

            Rectangle {
                anchors.fill: parent
                radius: parent.radius
                color: markerMouse.containsMouse ? Theme.accentRing : Theme.accentFaint
            }
            Row {
                id: markerRow
                anchors.centerIn: parent
                spacing: 6
                ZbIcon {
                    name: "arrow"
                    size: 12
                    color: Theme.accent
                    rotation: marker.modelData.angle
                    anchors.verticalCenter: parent.verticalCenter
                }
                Text {
                    text: "New: " + marker.modelData.name
                    width: Math.min(implicitWidth, 180)
                    elide: Text.ElideRight
                    anchors.verticalCenter: parent.verticalCenter
                    font.family: Theme.fontSans
                    font.pixelSize: 11
                    font.weight: Font.Medium
                    color: Theme.textPrimary
                }
            }
            MouseArea {
                id: markerMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: graphView.revealNode(marker.modelData.id)
            }
        }
    }

    // Temporary drag-mode override, at the bottom centre of the canvas: which
    // key to hold, and — accent-tinted — that the override is on right now.
    Rectangle {
        id: overrideHint
        readonly property bool active: graphView.overrideState !== ""
        // What holding the key does: the other mode's action.
        readonly property string action: graphView.overrideState !== "" ? graphView.overrideState : (graphView.dragMode === "grab" ? "select" : "pan")
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 18
        height: 28
        width: hintRow.implicitWidth + 22
        radius: Theme.radiusControl
        color: Theme.bgPanel
        border.width: 1
        border.color: active ? Theme.accentRing : Theme.borderStrong

        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: Theme.accentFaint
            visible: overrideHint.active
        }

        component HintText: Text {
            property bool on: false
            anchors.verticalCenter: parent.verticalCenter
            font.family: Theme.fontSans
            font.pixelSize: 11
            font.weight: on ? Font.DemiBold : Font.Normal
            color: on ? Theme.accent : Theme.textMuted
        }

        Row {
            id: hintRow
            anchors.centerIn: parent
            spacing: 6
            ZbIcon {
                name: overrideHint.action === "pan" ? "grab" : "select"
                size: 14
                color: overrideHint.active ? Theme.accent : Theme.textMuted
                anchors.verticalCenter: parent.verticalCenter
            }
            HintText {
                on: overrideHint.active
                text: overrideHint.active ? (overrideHint.action === "pan" ? "Panning \u2014 release" : "Selecting \u2014 release") : "Hold"
            }
            // Key cap.
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                height: 18
                width: keyText.implicitWidth + 12
                radius: Theme.radiusTag
                color: "transparent"
                border.width: 1
                border.color: overrideHint.active ? Theme.accent : Theme.borderStrong
                Text {
                    id: keyText
                    anchors.centerIn: parent
                    text: overrideHint.action === "pan" ? "Space" : "Shift"
                    font.family: Theme.fontMono
                    font.pixelSize: 10
                    color: overrideHint.active ? Theme.accent : Theme.textIcon
                }
            }
            HintText {
                on: overrideHint.active
                text: overrideHint.active ? (graphView.dragMode === "grab" ? "to go back to Grab" : "to go back to Select") : (overrideHint.action === "pan" ? "to pan" : "and drag to select")
            }
        }
    }

    // Short notice at the bottom of the canvas (e.g. a refused connection).
    Rectangle {
        id: toast
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: overrideHint.top
        anchors.bottomMargin: 10
        width: Math.min(toastText.implicitWidth + 32, parent.width - 32)
        height: toastText.implicitHeight + 16
        radius: Theme.radiusControl
        color: Theme.bgTooltip
        border.width: 1
        border.color: Theme.borderStrong
        opacity: 0
        visible: opacity > 0
        Behavior on opacity {
            NumberAnimation {
                duration: 150
            }
        }

        Text {
            id: toastText
            anchors.centerIn: parent
            width: Math.min(implicitWidth, toast.parent.width - 64)
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            color: Theme.textPrimary
            wrapMode: Text.WordWrap
        }

        Timer {
            id: toastTimer
            interval: 3500
            onTriggered: toast.opacity = 0
        }
    }

    // The canvas has no content box to sit beside, so its bars overlay the
    // edges (design bible §5.3).
    ZbScrollBar {
        id: hScrollBar
        orientation: Qt.Horizontal
        anchors.left: parent.left
        anchors.right: vScrollBar.visible ? vScrollBar.left : parent.right
        anchors.bottom: parent.bottom
        visible: size < 1
        policy: ScrollBar.AlwaysOn
        // Canvas-space range the bar spans (see updateScrollBars).
        property real rangeLo: 0
        property real rangeLen: 1
        onPositionChanged: {
            if (!pressed)
                return;
            panX = -(rangeLo + position * rangeLen) * zoom;
            canvas.requestPaint();
        }
        onPressedChanged: if (!pressed)
            canvas.requestPaint()
    }

    ZbScrollBar {
        id: vScrollBar
        orientation: Qt.Vertical
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.bottom: hScrollBar.visible ? hScrollBar.top : parent.bottom
        visible: size < 1
        policy: ScrollBar.AlwaysOn
        property real rangeLo: 0
        property real rangeLen: 1
        onPositionChanged: {
            if (!pressed)
                return;
            panY = -(rangeLo + position * rangeLen) * zoom;
            canvas.requestPaint();
        }
        onPressedChanged: if (!pressed)
            canvas.requestPaint()
    }

    // Hover feedback: pin enlarges + tooltip, wire brightens, buttons and
    // the collapse row highlight. (-1, -1) clears it.
    function updateHover(sx, sy) {
        var port = -1;
        var link = -1;
        var btn = "";
        var empty = false;
        var nodeUnder = -1;
        if (sx >= 0 && connectFromPortId < 0 && dragNodeId < 0 && !groupDragging) {
            port = findPortAt(sx, sy);
            nodeUnder = findNodeAt(sx, sy);
            if (nodeUnder < 0 && port >= 0 && portPositions[port])
                nodeUnder = portPositions[port].nodeId;
            if (port < 0) {
                var hit = findButtonAt(sx, sy);
                if (hit) {
                    btn = hit.nodeId + ":" + hit.button;
                } else if (nodeUnder < 0) {
                    link = findLinkAt(sx, sy);
                    empty = link < 0;
                }
            }
        }
        var nodeChanged = nodeUnder !== hoveredNodeId;
        var changed = nodeChanged || port !== hoveredPortId || link !== hoveredLinkId || btn !== hoveredButton;
        hoveredPortId = port;
        hoveredLinkId = link;
        hoveredButton = btn;
        hoveredNodeId = nodeUnder;
        hoverEmpty = empty;
        updateCursor();
        // Full-name popup: only for a node whose title is cut short, and not
        // while a pin or a node button has a tooltip of its own.
        if (nodeChanged || port >= 0 || btn !== "") {
            hideNameTip();
            if (nodeUnder >= 0 && port < 0 && btn === "" && nameTruncated[nodeUnder])
                nameTimer.restart();
        }
        if (changed) {
            if (port >= 0 && portPositions[port]) {
                var pp = portPositions[port];
                var sp = toScreen(pp.cx, pp.cy);
                pinTip.text = portName(port);
                pinTip.x = sp.x + 10;
                pinTip.y = sp.y + 10;
                pinTip.open();
            } else if (btn.indexOf(":pin") > 0) {
                var pn = findNodeData(parseInt(btn));
                var pnPos = pn ? nodePositions[pn.id] : null;
                if (pnPos) {
                    var pinned = !!(pn.layoutKey && pinnedNodes[pn.layoutKey]);
                    var r = pinRect(pn, pnPos.x, pnPos.y);
                    var tp = toScreen(r.x + r.w, r.y + r.h);
                    pinTip.text = pinned ? "Pinned \u2014 click to unpin" : "Pin (keep this position)";
                    pinTip.x = tp.x;
                    pinTip.y = tp.y + 4;
                    pinTip.open();
                }
            } else {
                pinTip.close();
            }
            canvas.requestPaint();
        }
    }

    // Canvas cursor: a closed hand while panning (grab, Space or middle
    // drag) and an open one wherever a left-drag would pan (Space held, or
    // empty canvas in grab mode); a cross where Shift would start a selection
    // box; a pointing hand over pins and node buttons.
    function updateCursor() {
        var shape = Qt.ArrowCursor;
        if (grabPanning || spacePanning || middlePanning)
            shape = Qt.ClosedHandCursor;
        else if (spaceHeld)
            shape = Qt.OpenHandCursor;
        else if (hoveredButton !== "" || hoveredPortId >= 0)
            shape = Qt.PointingHandCursor;
        else if (overrideState === "select" && hoverEmpty)
            shape = Qt.CrossCursor;
        else if (dragMode === "grab" && hoverEmpty && placingNodeId < 0)
            shape = Qt.OpenHandCursor;
        if (mouseArea.cursorShape !== shape)
            mouseArea.cursorShape = shape;
    }

    function portName(portId) {
        for (var nid in portsByNode) {
            var ports = portsByNode[nid];
            for (var i = 0; i < ports.length; i++) {
                if (ports[i].id === portId)
                    return ports[i].name;
            }
        }
        return "";
    }

    // Full port name on pin hover.
    ZbTooltip {
        id: pinTip
        delay: 300
    }

    // Full node name, when the card's title is cut short: shown above the
    // card (below its header when there is no room above) after a short pause.
    ZbTooltip {
        id: nameTip
        delay: 0
    }
    TextMetrics {
        id: nameMetrics
        font: nameTip.font
        text: nameTip.text
    }
    Timer {
        id: nameTimer
        interval: 350
        onTriggered: graphView.showNameTip()
    }
    function hideNameTip() {
        nameTimer.stop();
        nameTip.close();
    }
    function showNameTip() {
        var n = hoveredNodeId >= 0 ? findNodeData(hoveredNodeId) : null;
        var pos = n ? nodePositions[n.id] : null;
        if (!n || !pos || !nameTruncated[n.id] || hoveredPortId >= 0 || hoveredButton !== "" || dragNodeId >= 0 || groupDragging || grabPanning || spacePanning || middlePanning)
            return;
        nameTip.text = n.name;
        nameTip.width = Math.min(nameMetrics.advanceWidth + nameTip.leftPadding + nameTip.rightPadding + 2, Math.min(420, width - 16));
        placeNameTip(n, pos);
        nameTip.open();
        Qt.callLater(placeNameTip, n, pos);   // the height settles once it wraps
    }
    function placeNameTip(n, pos) {
        var s = toScreen(pos.x, pos.y);
        nameTip.x = Math.max(8, Math.min(width - nameTip.width - 8, s.x));
        var above = s.y - nameTip.height - 6;
        nameTip.y = above >= 8 ? above : s.y + Theme.nodeHeader * zoom + 4;
    }

    // Wire control-point offset: clamp(0.5·|dx|, 32, 200), horizontal only,
    // so wires leave and enter pins horizontally. Shared by drawing and
    // every hit test.
    function bezierCtrl(x1, x2) {
        return Math.max(32, Math.min(200, Math.abs(x2 - x1) / 2));
    }

    function drawBezier(ctx, x1, y1, x2, y2, color, lineWidth) {
        var ctrlDist = bezierCtrl(x1, x2);
        ctx.strokeStyle = "" + color;
        ctx.lineWidth = lineWidth;
        ctx.lineCap = "round";
        ctx.beginPath();
        ctx.moveTo(x1, y1);
        ctx.bezierCurveTo(x1 + ctrlDist, y1, x2 - ctrlDist, y2, x2, y2);
        ctx.stroke();
    }

    // The wire nearest to the cursor within linkHitPx screen pixels, so a
    // wire highlights (and can be clicked) from close by, not only when the
    // cursor is exactly on it.
    readonly property real linkHitPx: 10
    function findLinkAt(sx, sy) {
        var c = toCanvas(sx, sy);
        var threshold = linkHitPx / zoom;
        var bestId = -1;
        var bestDist = threshold + 1;

        for (var li = 0; li < links.length; li++) {
            var link = links[li];
            var fromPos = portPositions[link.outputPortId];
            var toPos = portPositions[link.inputPortId];
            if (!fromPos || !toPos)
                continue;
            // The curve stays inside its control polygon's box.
            var ctrl = bezierCtrl(fromPos.cx, toPos.cx);
            if (c.x < Math.min(fromPos.cx, toPos.cx - ctrl) - threshold || c.x > Math.max(fromPos.cx + ctrl, toPos.cx) + threshold)
                continue;
            if (c.y < Math.min(fromPos.cy, toPos.cy) - threshold || c.y > Math.max(fromPos.cy, toPos.cy) + threshold)
                continue;
            var dist = distToBezier(c.x, c.y, fromPos.cx, fromPos.cy, toPos.cx, toPos.cy);
            if (dist < bestDist) {
                bestDist = dist;
                bestId = link.id;
            }
        }
        return bestDist <= threshold ? bestId : -1;
    }

    function findLinkUnderNode(nodeId) {
        var pos = nodePositions[nodeId];
        if (!pos)
            return -1;
        var node = findNodeData(nodeId);
        if (!node)
            return -1;
        var nw = getNodeWidth(nodeId);
        var nh = calculateNodeHeight(node);
        var nx = pos.x;
        var ny = pos.y;

        var bestId = -1;
        var bestDist = Infinity;

        for (var li = 0; li < links.length; li++) {
            var link = links[li];
            if (link.outputPortId === undefined)
                continue;
            // Skip links connected to the node itself to prevent self-loops
            if (link.outputNodeId === nodeId || link.inputNodeId === nodeId)
                continue;
            // Only an audio link can take a plugin.
            if (portMediaTypes[link.outputPortId] !== "Audio" || portMediaTypes[link.inputPortId] !== "Audio")
                continue;
            var fromPos = portPositions[link.outputPortId];
            var toPos = portPositions[link.inputPortId];
            if (!fromPos || !toPos)
                continue;
            var ctrlDist = bezierCtrl(fromPos.cx, toPos.cx);
            var cx1 = fromPos.cx + ctrlDist;
            var cy1 = fromPos.cy;
            var cx2 = toPos.cx - ctrlDist;
            var cy2 = toPos.cy;
            var steps = 30;
            for (var i = 0; i <= steps; i++) {
                var t = i / steps;
                var u = 1 - t;
                var bx = u * u * u * fromPos.cx + 3 * u * u * t * cx1 + 3 * u * t * t * cx2 + t * t * t * toPos.cx;
                var by = u * u * u * fromPos.cy + 3 * u * u * t * cy1 + 3 * u * t * t * cy2 + t * t * t * toPos.cy;
                if (bx >= nx && bx <= nx + nw && by >= ny && by <= ny + nh) {
                    var cx = nx + nw / 2;
                    var cy = ny + nh / 2;
                    var dx = bx - cx;
                    var dy = by - cy;
                    var d = dx * dx + dy * dy;
                    if (d < bestDist) {
                        bestDist = d;
                        bestId = link.id;
                    }
                    break;
                }
            }
        }
        return bestId;
    }

    // Distance from (px, py) to the wire from (x1, y1) to (x2, y2): the
    // curve is walked as a polyline and the distance taken to its segments,
    // so a long wire has no dead spots between sample points.
    function distToBezier(px, py, x1, y1, x2, y2) {
        var ctrlDist = bezierCtrl(x1, x2);
        var cx1 = x1 + ctrlDist;
        var cy1 = y1;
        var cx2 = x2 - ctrlDist;
        var cy2 = y2;
        var steps = 30;
        var minSq = Infinity;
        var ax = x1, ay = y1;
        for (var i = 1; i <= steps; i++) {
            var t = i / steps;
            var u = 1 - t;
            var bx = u * u * u * x1 + 3 * u * u * t * cx1 + 3 * u * t * t * cx2 + t * t * t * x2;
            var by = u * u * u * y1 + 3 * u * u * t * cy1 + 3 * u * t * t * cy2 + t * t * t * y2;
            var sx = bx - ax, sy = by - ay;
            var len = sx * sx + sy * sy;
            var k = len > 0 ? Math.max(0, Math.min(1, ((px - ax) * sx + (py - ay) * sy) / len)) : 0;
            var dx = px - (ax + k * sx);
            var dy = py - (ay + k * sy);
            var d = dx * dx + dy * dy;
            if (d < minSq)
                minSq = d;
            ax = bx;
            ay = by;
        }
        return Math.sqrt(minSq);
    }

    function bezierIntersectsRect(x1, y1, x2, y2, rx, ry, rw, rh) {
        var ctrlDist = bezierCtrl(x1, x2);
        var cx1 = x1 + ctrlDist;
        var cy1 = y1;
        var cx2 = x2 - ctrlDist;
        var cy2 = y2;
        var steps = 20;
        for (var i = 0; i <= steps; i++) {
            var t = i / steps;
            var u = 1 - t;
            var px = u * u * u * x1 + 3 * u * u * t * cx1 + 3 * u * t * t * cx2 + t * t * t * x2;
            var py = u * u * u * y1 + 3 * u * u * t * cy1 + 3 * u * t * t * cy2 + t * t * t * y2;
            if (px >= rx && px <= rx + rw && py >= ry && py <= ry + rh) {
                return true;
            }
        }
        return false;
    }

    function findLinksInRect(sx1, sy1, sx2, sy2) {
        var minSx = Math.min(sx1, sx2);
        var minSy = Math.min(sy1, sy2);
        var maxSx = Math.max(sx1, sx2);
        var maxSy = Math.max(sy1, sy2);
        var c1 = toCanvas(minSx, minSy);
        var c2 = toCanvas(maxSx, maxSy);
        var rx = c1.x;
        var ry = c1.y;
        var rw = c2.x - c1.x;
        var rh = c2.y - c1.y;

        var result = {};
        for (var li = 0; li < links.length; li++) {
            var link = links[li];
            var fromPos = portPositions[link.outputPortId];
            var toPos = portPositions[link.inputPortId];
            if (fromPos && toPos) {
                if (bezierIntersectsRect(fromPos.cx, fromPos.cy, toPos.cx, toPos.cy, rx, ry, rw, rh)) {
                    result[link.id] = linkSig(link);
                }
            }
        }
        return result;
    }

    function findNodesInRect(sx1, sy1, sx2, sy2) {
        var c1 = toCanvas(Math.min(sx1, sx2), Math.min(sy1, sy2));
        var c2 = toCanvas(Math.max(sx1, sx2), Math.max(sy1, sy2));
        var rx = c1.x;
        var ry = c1.y;
        var rr = c2.x;
        var rb = c2.y;

        var result = {};
        for (var i = 0; i < nodes.length; i++) {
            var n = nodes[i];
            if (n.layoutKey && hiddenNodes[n.layoutKey])
                continue;
            var pos = nodePositions[n.id];
            if (!pos)
                continue;
            var h = calculateNodeHeight(n);
            var nx = pos.x;
            var ny = pos.y;
            var nr = nx + getNodeWidth(n.id);
            var nb = ny + h;
            if (nx < rr && nr > rx && ny < rb && nb > ry) {
                result[n.id] = nodeSig(n);
            }
        }
        return result;
    }

    function deleteSelectedLinks() {
        var ids = [];
        for (var linkId in selectedLinks) {
            var lid = parseInt(linkId);
            // Only disconnect a link that is still the one that was selected.
            if (linkMatchesSig(findLinkData(lid), selectedLinks[linkId]))
                ids.push(lid);
        }
        if (disconnectLinks(ids) > 0) {
            selectedLinks = {};
            canvas.requestPaint();
        }
    }

    // ---- New-node cue -----------------------------------------------------
    // A node that appears while you are working pulses once (an accent ring,
    // drawn by drawNode); when it appeared outside the view, a marker at the
    // nearest edge points to it until it has been seen. Nothing is cued
    // while the graph is still filling in: at start-up, after a session
    // switch or when entering / leaving a subgraph (cueQuiet).
    // nodeId -> { born: ms it appeared, t: ms its pulse (re)started }
    property var nodeCues: ({})
    property bool cueQuiet: true
    readonly property int cuePulseMs: 1600
    readonly property int cueMarkerMs: 12000
    // [{ id, name, x, y, angle }] in screen space, for the edge markers.
    property var edgeMarkers: []

    function quietCues() {
        cueQuiet = true;
        nodeCues = {};
        edgeMarkers = [];
        cueQuietTimer.restart();
    }

    // Called by refreshData with the ids that appeared in this refresh.
    function noteAppeared(ids) {
        if (ids.length === 0)
            return;
        if (cueQuiet) {
            cueQuietTimer.restart();
            return;
        }
        // A burst (a device with many nodes, a subgraph being ungrouped) is
        // not news about any one node.
        if (ids.length > 4)
            return;
        var now = Date.now();
        var added = false;
        for (var i = 0; i < ids.length; i++) {
            var n = findNodeData(ids[i]);
            if (!n || ids[i] === placingNodeId || (n.layoutKey && hiddenNodes[n.layoutKey]))
                continue;
            nodeCues[ids[i]] = {
                born: now,
                t: now
            };
            added = true;
        }
        if (added) {
            nodeCues = nodeCues;
            cueTimer.start();
        }
    }

    // Is any part of the node inside the view?
    function nodeOnScreen(node) {
        var p = nodePositions[node.id];
        if (!p)
            return false;
        var a = toScreen(p.x, p.y);
        var b = toScreen(p.x + getNodeWidth(node.id), p.y + calculateNodeHeight(node));
        return b.x > 0 && a.x < width && b.y > 0 && a.y < height;
    }

    // Drop finished cues and place a marker for each cued node out of view.
    function updateCues() {
        var now = Date.now();
        var markers = [];
        var pulsing = false;
        var left = 0;
        for (var id in nodeCues) {
            var cue = nodeCues[id];
            var n = findNodeData(parseInt(id));
            if (!n || (n.layoutKey && hiddenNodes[n.layoutKey])) {
                delete nodeCues[id];
                continue;
            }
            var onScreen = nodeOnScreen(n);
            if (onScreen ? now - cue.t > cuePulseMs : now - cue.born > cueMarkerMs) {
                // Seen (the pulse has played), or the marker has had its time.
                delete nodeCues[id];
                continue;
            }
            left++;
            if (onScreen) {
                pulsing = true;
                continue;
            }
            // Off screen: the pulse waits until the node comes into view.
            cue.t = now;
            var p = nodePositions[n.id];
            var c = toScreen(p.x + getNodeWidth(n.id) / 2, p.y + calculateNodeHeight(n) / 2);
            var mx = Math.max(90, Math.min(width - 90, c.x));
            var my = Math.max(70, Math.min(height - 64, c.y));
            markers.push({
                id: n.id,
                name: n.name || "",
                x: Math.round(mx),
                y: Math.round(my),
                angle: Math.round(Math.atan2(c.y - my, c.x - mx) * 180 / Math.PI)
            });
        }
        if (JSON.stringify(markers) !== JSON.stringify(edgeMarkers))
            edgeMarkers = markers;
        if (pulsing)
            canvas.requestPaint();
        if (left === 0)
            cueTimer.stop();
    }

    // Bring a node to the middle of the view and pulse it again.
    function revealNode(nodeId) {
        var n = findNodeData(nodeId);
        var p = n ? nodePositions[nodeId] : null;
        if (!p)
            return;
        panX = width / 2 - (p.x + getNodeWidth(nodeId) / 2) * zoom;
        panY = height / 2 - (p.y + calculateNodeHeight(n) / 2) * zoom;
        clampPan();
        nodeCues[nodeId] = {
            born: Date.now(),
            t: Date.now()
        };
        nodeCues = nodeCues;
        cueTimer.start();
        canvas.requestPaint();
    }

    Timer {
        id: cueTimer
        interval: 40
        repeat: true
        onTriggered: graphView.updateCues()
    }

    // The graph has stopped filling in: start cueing newcomers.
    Timer {
        id: cueQuietTimer
        interval: 2500
        running: true
        onTriggered: graphView.cueQuiet = false
    }

    // ---- Undo / redo ------------------------------------------------------
    // Session-only history of node moves, wires made and removed, and nodes
    // hidden. Entries name nodes by layout key and wires by their ports, and
    // are checked against the live graph when applied, so a recycled id can
    // never be moved or rewired by mistake. Cleared when the view changes
    // (another session, into or out of a subgraph).
    property var undoStack: []
    property var redoStack: []
    readonly property int undoLimit: 50
    readonly property bool canUndo: undoStack.length > 0
    readonly property bool canRedo: redoStack.length > 0

    function pushUndo(entry) {
        var st = undoStack.slice();
        st.push(entry);
        if (st.length > undoLimit)
            st.shift();
        undoStack = st;
        redoStack = [];
    }

    function clearUndo() {
        undoStack = [];
        redoStack = [];
    }

    // "node" / "3 nodes" (history labels), and "1 node" / "3 nodes".
    function plural(n, one, many) {
        return n === 1 ? one : n + " " + many;
    }
    function counted(n, one, many) {
        return n + " " + (n === 1 ? one : many);
    }

    function nodeIdForKey(key) {
        for (var id in uniqueKeyById) {
            if (uniqueKeyById[id] === key)
                return parseInt(id);
        }
        return -1;
    }

    // The user positioned this node: automatic placement leaves it alone.
    function markHandPlaced(nodeId) {
        delete autoPlaced[nodeId];
        delete placedKeyById[nodeId];
        delete awaitingLinkPlacement[nodeId];
    }

    // Record a move of the nodes in `fromById` (nodeId -> {x, y} before) to
    // where they are now. Nodes that did not move are left out.
    function recordMove(verb, fromById) {
        var items = [];
        for (var id in fromById) {
            var nid = parseInt(id);
            var from = fromById[id];
            var to = nodePositions[nid];
            var key = uniqueKeyById[nid];
            if (!from || !to || !key || (from.x === to.x && from.y === to.y))
                continue;
            items.push({
                key: key,
                from: {
                    x: from.x,
                    y: from.y
                },
                to: {
                    x: to.x,
                    y: to.y
                }
            });
        }
        if (items.length === 0)
            return null;
        var entry = {
            kind: "move",
            label: verb + " " + plural(items.length, "node", "nodes"),
            items: items
        };
        pushUndo(entry);
        return entry;
    }

    function linkRecord(link) {
        return {
            o: link.outputPortId,
            i: link.inputPortId,
            onKey: uniqueKeyById[link.outputNodeId] || "",
            innKey: uniqueKeyById[link.inputNodeId] || ""
        };
    }

    // Do the ports of a recorded wire still belong to the nodes they did?
    function linkRecordValid(rec) {
        var on = getPortNodeId(rec.o);
        var inn = getPortNodeId(rec.i);
        return on >= 0 && inn >= 0 && (uniqueKeyById[on] || "") === rec.onKey && (uniqueKeyById[inn] || "") === rec.innKey;
    }

    function findLinkByPorts(outPort, inPort) {
        for (var i = 0; i < links.length; i++) {
            if (links[i].outputPortId === outPort && links[i].inputPortId === inPort)
                return links[i];
        }
        return null;
    }

    // Connect two ports by hand, as one undoable step. The two nodes stay
    // where they are: the user is wiring them in place.
    function connectPorts(outPort, inPort) {
        var on = getPortNodeId(outPort);
        var inn = getPortNodeId(inPort);
        markHandPlaced(on);
        markHandPlaced(inn);
        controller.connect_ports(outPort, inPort);
        pushUndo({
            kind: "connect",
            label: "connect",
            links: [
                {
                    o: outPort,
                    i: inPort,
                    onKey: uniqueKeyById[on] || "",
                    innKey: uniqueKeyById[inn] || ""
                }
            ]
        });
    }

    // Disconnect the links with these ids, as one undoable step. Returns
    // how many there were.
    function disconnectLinks(ids) {
        var recs = [];
        for (var i = 0; i < ids.length; i++) {
            var l = findLinkData(ids[i]);
            if (!l)
                continue;
            recs.push(linkRecord(l));
            controller.disconnect_link(l.id);
        }
        if (recs.length > 0)
            pushUndo({
                kind: "disconnect",
                label: "disconnect " + plural(recs.length, "wire", "wires"),
                links: recs
            });
        return recs.length;
    }

    // Apply a history entry backwards (undoing) or forwards. Returns how
    // many of its items could still be applied.
    function applyHistory(entry, undoing) {
        var done = 0;
        if (entry.kind === "move") {
            for (var mi = 0; mi < entry.items.length; mi++) {
                var it = entry.items[mi];
                var nid = nodeIdForKey(it.key);
                if (nid < 0)
                    continue;
                var target = undoing ? it.from : it.to;
                nodePositions[nid] = {
                    x: target.x,
                    y: target.y
                };
                markHandPlaced(nid);
                done++;
            }
            if (done > 0)
                persistLayout();
        } else if (entry.kind === "hide") {
            for (var hi = 0; hi < entry.keys.length; hi++) {
                if (undoing)
                    delete hiddenNodes[entry.keys[hi]];
                else
                    hiddenNodes[entry.keys[hi]] = true;
                done++;
            }
            hiddenNodes = hiddenNodes;
            persistHidden();
        } else {
            // "connect" / "disconnect": undoing one is doing the other.
            var make = (entry.kind === "disconnect") === undoing;
            for (var li = 0; li < entry.links.length; li++) {
                var rec = entry.links[li];
                if (!linkRecordValid(rec))
                    continue;
                var live = findLinkByPorts(rec.o, rec.i);
                if (make && !live) {
                    controller.connect_ports(rec.o, rec.i);
                    done++;
                } else if (!make && live) {
                    controller.disconnect_link(live.id);
                    done++;
                }
            }
        }
        canvas.requestPaint();
        return done;
    }

    function undo() {
        if (undoStack.length === 0) {
            showToast("Nothing to undo");
            return;
        }
        var st = undoStack.slice();
        var entry = st.pop();
        undoStack = st;
        if (applyHistory(entry, true) > 0) {
            var rs = redoStack.slice();
            rs.push(entry);
            redoStack = rs;
            showToast("Undo: " + entry.label);
        } else {
            showToast("Can\u2019t undo \u201c" + entry.label + "\u201d: it is no longer there");
        }
    }

    function redo() {
        if (redoStack.length === 0) {
            showToast("Nothing to redo");
            return;
        }
        var rs = redoStack.slice();
        var entry = rs.pop();
        redoStack = rs;
        if (applyHistory(entry, false) > 0) {
            var st = undoStack.slice();
            st.push(entry);
            undoStack = st;
            showToast("Redo: " + entry.label);
        } else {
            showToast("Can\u2019t redo \u201c" + entry.label + "\u201d: it is no longer there");
        }
    }

    // ---- Keyboard edits ---------------------------------------------------

    // Ids of the selected nodes that are still the nodes that were selected.
    function selectedNodeIds() {
        var ids = [];
        for (var k in selectedNodes) {
            var n = findNodeData(parseInt(k));
            if (n && selectedNodes[k] && selectedNodes[k].key === (n.layoutKey || ""))
                ids.push(n.id);
        }
        return ids;
    }

    function selectAll() {
        var allNodes = {};
        for (var i = 0; i < nodes.length; i++) {
            var n = nodes[i];
            if ((n.layoutKey && hiddenNodes[n.layoutKey]) || !nodePositions[n.id])
                continue;
            allNodes[n.id] = nodeSig(n);
        }
        var allLinks = {};
        for (var li = 0; li < links.length; li++) {
            var l = links[li];
            if (portPositions[l.outputPortId] && portPositions[l.inputPortId])
                allLinks[l.id] = linkSig(l);
        }
        selectedNodes = allNodes;
        selectedLinks = allLinks;
        canvas.requestPaint();
    }

    // Arrow keys: move the selected nodes one grid step (1 px without the
    // grid, 10 with Shift). A run of nudges is one undo step.
    property var nudgeEntry: null
    function nudgeSelection(dx, dy, big) {
        var ids = selectedNodeIds();
        if (ids.length === 0)
            return false;
        var step = snapToGrid ? gridSize : (big ? 10 : 1);
        var from = {};
        for (var i = 0; i < ids.length; i++) {
            var p = nodePositions[ids[i]];
            if (!p)
                continue;
            from[ids[i]] = {
                x: p.x,
                y: p.y
            };
            nodePositions[ids[i]] = {
                x: p.x + dx * step,
                y: p.y + dy * step
            };
            markHandPlaced(ids[i]);
        }
        var last = undoStack.length > 0 ? undoStack[undoStack.length - 1] : null;
        if (nudgeTimer.running && nudgeEntry !== null && last === nudgeEntry) {
            // Same run: keep where the nodes started, update where they are.
            for (var ni = 0; ni < nudgeEntry.items.length; ni++) {
                var it = nudgeEntry.items[ni];
                var nid = nodeIdForKey(it.key);
                if (nid >= 0 && nodePositions[nid])
                    it.to = {
                        x: nodePositions[nid].x,
                        y: nodePositions[nid].y
                    };
            }
        } else {
            nudgeEntry = recordMove("move", from);
        }
        nudgeTimer.restart();
        canvas.requestPaint();
        return true;
    }

    // Ends a run of nudges and saves the layout once for the whole run.
    Timer {
        id: nudgeTimer
        interval: 700
        onTriggered: {
            graphView.nudgeEntry = null;
            graphView.persistLayout();
        }
    }

    // Delete: disconnect the selected wires and take the selected nodes off
    // the canvas — plugins, subgraphs and virtual devices are removed (after
    // a confirmation, that can't be undone); devices and app streams, which
    // can't be removed, are hidden.
    property var deletePlan: null
    function deleteSelection() {
        var plan = {
            links: [],
            plugins: [],
            subgraphs: [],
            vdevs: [],
            hide: []
        };
        var removed = {};
        var ids = selectedNodeIds();
        for (var i = 0; i < ids.length; i++) {
            var n = findNodeData(ids[i]);
            if (n.type === "Plugin") {
                plan.plugins.push(n);
                removed[n.id] = true;
            } else if (n.type === "Subgraph") {
                plan.subgraphs.push(n);
                removed[n.id] = true;
            } else if (n.vdevId) {
                plan.vdevs.push(n);
                removed[n.id] = true;
            } else if (n.type !== "SubgraphInput" && n.type !== "SubgraphOutput" && n.layoutKey) {
                plan.hide.push(n);
            }
        }
        for (var linkId in selectedLinks) {
            var l = findLinkData(parseInt(linkId));
            // Wires of a node that is being removed go with it.
            if (linkMatchesSig(l, selectedLinks[linkId]) && !removed[l.outputNodeId] && !removed[l.inputNodeId])
                plan.links.push(l.id);
        }
        var destructive = plan.plugins.length + plan.subgraphs.length + plan.vdevs.length;
        if (destructive + plan.hide.length + plan.links.length === 0)
            return;
        deletePlan = plan;
        if (destructive > 0)
            deleteSelectionDialog.open();
        else
            performDelete();
    }

    function deletePlanLines() {
        var plan = deletePlan;
        if (!plan)
            return [];
        var lines = [];
        if (plan.plugins.length > 0)
            lines.push("Remove " + counted(plan.plugins.length, "plugin", "plugins"));
        if (plan.subgraphs.length > 0)
            lines.push("Delete " + counted(plan.subgraphs.length, "subgraph", "subgraphs") + " and the plugins inside");
        if (plan.vdevs.length > 0)
            lines.push("Remove " + counted(plan.vdevs.length, "virtual device", "virtual devices"));
        if (plan.hide.length > 0)
            lines.push("Hide " + counted(plan.hide.length, "node", "nodes") + " (devices and app streams can\u2019t be removed)");
        if (plan.links.length > 0)
            lines.push("Disconnect " + counted(plan.links.length, "wire", "wires"));
        return lines;
    }

    function performDelete() {
        var plan = deletePlan;
        deletePlan = null;
        if (!plan)
            return;
        disconnectLinks(plan.links);
        if (plan.hide.length > 0) {
            var keys = [];
            for (var hi = 0; hi < plan.hide.length; hi++) {
                hiddenNodes[plan.hide[hi].layoutKey] = true;
                keys.push(plan.hide[hi].layoutKey);
            }
            hiddenNodes = hiddenNodes;
            persistHidden();
            pushUndo({
                kind: "hide",
                label: "hide " + plural(keys.length, "node", "nodes"),
                keys: keys
            });
        }
        for (var pi = 0; pi < plan.plugins.length; pi++)
            controller.remove_plugin(plan.plugins[pi].id);
        for (var si = 0; si < plan.subgraphs.length; si++)
            controller.delete_subgraph(plan.subgraphs[si].subgraphId);
        for (var vi = 0; vi < plan.vdevs.length; vi++)
            controller.remove_virtual_device(plan.vdevs[vi].vdevId);
        selectedNodes = {};
        selectedLinks = {};
        if (plan.hide.length > 0)
            showToast("Hid " + counted(plan.hide.length, "node", "nodes") + ". Ctrl+Z or right-click \u203a Unhide all brings them back.");
        canvas.requestPaint();
    }

    // Block rule label ("A → B") forbidding a drag from `fromPort` (direction
    // `fromDir`) to `toPort`, or "".
    function blockedReasonFor(fromPort, fromDir, toPort) {
        return fromDir === "Output" ? controller.blocked_reason(fromPort, toPort) : controller.blocked_reason(toPort, fromPort);
    }

    // Block the node pairs of `linkIds` (each pair once: block_link removes
    // every link between the pair).
    function blockLinks(linkIds) {
        var pairs = {};
        for (var i = 0; i < linkIds.length; i++) {
            var l = findLinkData(linkIds[i]);
            if (!l)
                continue;
            var key = l.outputNodeId + ">" + l.inputNodeId;
            if (pairs[key])
                continue;
            pairs[key] = true;
            controller.block_link(l.id);
        }
        selectedLinks = {};
        canvas.requestPaint();
    }

    function blockSelectedLinks() {
        var ids = [];
        for (var linkId in selectedLinks) {
            var lid = parseInt(linkId);
            if (linkMatchesSig(findLinkData(lid), selectedLinks[linkId]))
                ids.push(lid);
        }
        if (ids.length > 0)
            blockLinks(ids);
    }

    // ---- Subgraphs ------------------------------------------------------

    function hasSelection() {
        for (var k in selectedLinks)
            return true;
        for (var k2 in selectedNodes)
            return true;
        return false;
    }

    function selectedPluginIds() {
        var ids = [];
        for (var k in selectedNodes) {
            var n = findNodeData(parseInt(k));
            if (n && n.type === "Plugin")
                ids.push(n.id);
        }
        return ids;
    }

    function subgraphInfo(id) {
        try {
            var list = JSON.parse(controller.get_subgraphs_json()).subgraphs;
            for (var i = 0; i < list.length; i++)
                if (list[i].id === id)
                    return list[i];
        } catch (e) {}
        return null;
    }

    // Group the selected plugins (or `nodeId` when it isn't selected).
    property var groupAnchor: null
    function groupIntoSubgraph(nodeId) {
        var ids = selectedPluginIds();
        if (nodeId >= 0 && ids.indexOf(nodeId) < 0)
            ids = [nodeId];
        if (ids.length === 0)
            return;
        var minX = Infinity, minY = Infinity;
        for (var i = 0; i < ids.length; i++) {
            var p = nodePositions[ids[i]];
            if (p) {
                minX = Math.min(minX, p.x);
                minY = Math.min(minY, p.y);
            }
        }
        groupAnchor = isFinite(minX) ? gridPos({
            x: minX,
            y: minY
        }) : null;
        var io = {};
        try {
            io = JSON.parse(controller.infer_subgraph_io(JSON.stringify(ids)));
        } catch (e) {}
        subgraphDialog.mode = "create";
        subgraphDialog.nodeIds = ids;
        subgraphDialog.pluginCount = io.plugins || ids.length;
        subgraphDialog.load(io);
        subgraphDialog.open();
    }

    function openSubgraphEditor(id) {
        var info = subgraphInfo(id);
        if (!info)
            return;
        subgraphDialog.mode = "edit";
        subgraphDialog.subgraphId = id;
        subgraphDialog.load(info);
        subgraphDialog.open();
    }

    function openVdevEditor(id) {
        vdevDialog.vdevId = id;
        vdevDialog.deviceName = "Virtual Sink";
        vdevDialog.channels = 2;
        vdevDialog.kindIndex = 0;
        if (id !== "") {
            try {
                var list = JSON.parse(controller.get_virtual_devices_json());
                for (var i = 0; i < list.length; i++) {
                    if (list[i].id === id) {
                        vdevDialog.deviceName = list[i].name;
                        vdevDialog.channels = list[i].channels;
                        vdevDialog.kindIndex = list[i].kind === "source" ? 1 : 0;
                    }
                }
            } catch (e) {}
        }
        vdevDialog.open();
    }

    // Put the nodes of an inserted/duplicated subgraph where the result says.
    function applyInsertedLayout(resultJson) {
        try {
            var r = JSON.parse(resultJson);
            for (var key in r.layout)
                savedLayout[key] = r.layout[key];
            controller.save_layout(JSON.stringify(savedLayout));
            return r.subgraphId;
        } catch (e) {
            return "";
        }
    }

    function duplicateSubgraph(node) {
        var p = nodePositions[node.id] || {
            x: 0,
            y: 0
        };
        var at = gridPos({
            x: p.x + getNodeWidth(node.id) + 40,
            y: p.y
        });
        applyInsertedLayout(controller.duplicate_subgraph(node.subgraphId, at.x, at.y));
    }

    // Insert a saved template at the pending add position (or the view centre).
    function insertTemplate(file) {
        var at = pendingPluginPosition || toCanvas(width / 2, height / 2);
        pendingPluginPosition = null;
        pendingPluginName = "";
        at = gridPos({
            x: at.x,
            y: at.y
        });
        applyInsertedLayout(controller.insert_subgraph_template(file, at.x, at.y));
    }

    function enterSubgraph(id) {
        if (!id || id === currentSubgraph)
            return;
        var info = subgraphInfo(id);
        if (!info)
            return;
        if (currentSubgraph === "")
            mainViewport = {
                panX: panX,
                panY: panY,
                zoom: zoom
            };
        clearSelection();
        clearUndo();
        quietCues();
        currentSubgraph = id;
        currentSubgraphName = info.name;
        fitAfterRefresh = !(info.view && info.view.zoom);
        if (!fitAfterRefresh) {
            zoom = info.view.zoom;
            panX = info.view.panX;
            panY = info.view.panY;
        }
        controller.set_subgraph_view(id);
    }

    function exitSubgraph() {
        if (currentSubgraph === "")
            return;
        controller.save_subgraph_view_state(currentSubgraph, JSON.stringify({
            panX: panX,
            panY: panY,
            zoom: zoom
        }));
        clearSelection();
        clearUndo();
        quietCues();
        currentSubgraph = "";
        currentSubgraphName = "";
        fitAfterRefresh = false;
        if (mainViewport) {
            zoom = mainViewport.zoom;
            panX = mainViewport.panX;
            panY = mainViewport.panY;
        }
        controller.set_subgraph_view("");
    }

    Connections {
        target: controller
        function onSubgraphs_changed() {
            if (currentSubgraph === "")
                return;
            var info = subgraphInfo(currentSubgraph);
            if (info)
                currentSubgraphName = info.name;
            else {
                currentSubgraph = "";
                currentSubgraphName = "";
            }
        }
    }

    function clearSelection() {
        var hadSelection = false;
        for (var k in selectedLinks) {
            hadSelection = true;
            break;
        }
        if (!hadSelection) {
            for (var k2 in selectedNodes) {
                hadSelection = true;
                break;
            }
        }
        if (hadSelection) {
            selectedLinks = {};
            selectedNodes = {};
            canvas.requestPaint();
        }
    }
}
