import "graph/Placement.js" as Placement
import "graph/Input.js" as Input
import "graph/Data.js" as Data
import "graph/HitTesting.js" as HitTesting
import "graph/Nodes.js" as Nodes
import "graph/Meters.js" as Meters
import "graph/Viewport.js" as Viewport
import "graph/Drawing.js" as Drawing
import "graph/Wires.js" as Wires
import "graph/Selection.js" as Selection
import "graph/History.js" as History
import "graph/Subgraphs.js" as Subgraphs
import "graph/Paint.js" as Paint
import QtQuick
import QtQuick.Controls

Item {
    id: graphView

    readonly property QtObject theme: Theme
    readonly property QtObject glyphs: Glyphs
    readonly property var elements: ({
        autoLayoutAllDialog: autoLayoutAllDialog,
        blockContextMenu: graphMenus.elements.blockContextMenu,
        canvas: canvas,
        canvasContextMenu: graphMenus.elements.canvasContextMenu,
        cueQuietTimer: cueQuietTimer,
        cueTimer: cueTimer,
        deleteSelectionDialog: graphMenus.elements.deleteSelectionDialog,
        hScrollBar: graphTools.elements.hScrollBar,
        linkContextMenu: graphMenus.elements.linkContextMenu,
        mouseArea: mouseArea,
        nameMetrics: nameMetrics,
        nameTimer: nameTimer,
        nameTip: nameTip,
        nodeContextMenu: graphMenus.elements.nodeContextMenu,
        nudgeTimer: nudgeTimer,
        orphanTimer: orphanTimer,
        pinTip: pinTip,
        portContextMenu: graphMenus.elements.portContextMenu,
        presetMenu: graphMenus.elements.presetMenu,
        presetSaveDialog: graphMenus.elements.presetSaveDialog,
        repaintTimer: repaintTimer,
        subgraphDialog: graphMenus.elements.subgraphDialog,
        toast: graphTools.elements.toast,
        toastText: graphTools.elements.toastText,
        toastTimer: graphTools.elements.toastTimer,
        vScrollBar: graphTools.elements.vScrollBar,
        vdevDialog: graphMenus.elements.vdevDialog,
        viewportSaveTimer: viewportSaveTimer
    })

    required property var controller

    signal openPluginBrowser
    signal openPluginParams(int nodeId)
    signal showShortcuts

    property real zoom: 1.0
    property real panX: 0
    property real panY: 0
    property bool viewportLoaded: false
    property bool initialViewChecked: false

    readonly property real minZoom: 0.25
    readonly property real maxZoom: 3.0
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

    property int placingNodeId: -1

    property var awaitingLinkPlacement: ({})

    property var autoPlaced: ({})

    property var placedKeyById: ({})
    property var placedBaseById: ({})

    property var appearedAt: ({})
    property int refreshSeq: 0
    property int settleWindowMs: 8000
    property real nodeSpacing: 20
    readonly property real placeColumnGap: Math.round(nodeSpacing * 2.5)
    property int cardRow: 22
    property int densityMoved: 0
    onCardRowChanged: {
        Theme.cardRow = cardRow;
        applyDensity();
    }
    function applyDensity() {
        return Placement.applyDensity(graphView);
    }

    function separateOverlaps() {
        return Placement.separateOverlaps(graphView);
    }

    property int connectFromPortId: -1
    property string connectFromDir: ""
    property bool connectBlocking: false
    property real connectPressX: 0
    property real connectPressY: 0
    property real connectMouseX: 0
    property real connectMouseY: 0

    property var nodes: []
    property var links: []
    property var blockedPairs: []
    property var blockGeom: []
    property var contextBlock: null
    property var selectedBlocks: ({})
    property string hoveredBlockKey: ""
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
    property var nodeStyles: ({})
    property bool nodeStylesLoaded: false

    property bool selectDragging: false
    property real selectStartX: 0
    property real selectStartY: 0
    property real selectEndX: 0
    property real selectEndY: 0
    property var selectedLinks: ({})
    property var selectedNodes: ({})
    property bool groupDragging: false

    property string dragMode: "grab"
    property bool grabPanning: false
    property bool grabMoved: false
    property bool hoverEmpty: false
    onDragModeChanged: updateCursor()

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
    property bool selectAdditive: false
    property var selectBaseNodes: ({})
    property var selectBaseLinks: ({})
    property var dragStartPos: null

    function setDragMode(mode) {
        return Input.setDragMode(graphView, mode);
    }

    function toggleDragMode() {
        return Input.toggleDragMode(graphView);
    }

    property int contextNodeId: -1
    property int contextPortId: -1
    property var contextNode: null
    property var contextCanvasPos: null

    property string currentSubgraph: ""
    property string currentSubgraphName: ""
    property var mainViewport: null
    property bool fitAfterRefresh: false
    readonly property bool contextIsSubgraph: contextNode !== null && contextNode.type === "Subgraph"
    readonly property bool contextIsVdev: contextNode !== null && !!contextNode.vdevId
    property var contextLinkIds: []
    property var pendingPluginPosition: null
    property string pendingPluginName: ""
    property string defaultNodeKey: ""

    property var localBypassState: ({})
    property var localEnabledState: ({})

    property var sortedPortsByNode: ({})
    property var nodeHeights: ({})
    property var uniqueKeyById: ({})
    property bool widthsDirty: true

    readonly property real snapThreshold: 5
    property real snapGap: 10

    property bool snapToGrid: true
    property real gridSize: 24

    property string wireStyle: "bezier"
    onWireStyleChanged: canvas.requestPaint()
    property bool gradientWires: false
    onGradientWiresChanged: canvas.requestPaint()
    property var wireLanes: ({})
    property var wireRoutes: ({})
    property var wireObstacleCache: []
    readonly property real wireClearance: 6
    readonly property real wireBendCost: 30
    readonly property real wireStub: 16
    readonly property real wireCorner: 8
    readonly property real wireLaneGap: 10
    onSnapToGridChanged: if (snapToGrid)
        alignAllToGrid()
    onGridSizeChanged: {
        if (snapToGrid)
            alignAllToGrid();
        canvas.requestPaint();
    }

    property var groupDragStart: ({})
    property real groupDragStartX: 0
    property real groupDragStartY: 0
    property var activeSnapLines: []

    readonly property real minNodeWidth: Theme.nodeWidth
    readonly property real maxNodeWidth: Theme.nodeWidthPlugin
    readonly property real portRadius: Theme.nodePin / 2
    property var nodeWidths: ({})

    property var nodeLayouts: ({})
    property var connectedPorts: ({})
    property var expandedNodes: ({})
    readonly property int collapseMinHidden: 3

    property string searchText: ""
    onSearchTextChanged: canvas.requestPaint()

    property int hoveredPortId: -1
    property int hoveredLinkId: -1
    property int hoveredNodeId: -1
    property var nameTruncated: ({})
    property var presetInfo: ({})
    property int presetMenuNodeId: -1
    property var presetMenuNames: []
    property string presetMenuSelected: ""
    property string presetMenuDefault: ""
    property var presetMenuChanges: []
    property var presetSaveNames: []
    property int presetSaveNodeId: -1
    property string hoveredButton: ""

    Connections {
        target: controller
        function onRules_changed() {
            refreshBlocked();
        }
    }

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
        return Data.reloadSessionData(graphView);
    }

    function refreshBlocked() {
        return HitTesting.refreshBlocked(graphView);
    }

    function blockKey(b) {
        return HitTesting.blockKey(graphView, b);
    }

    function pruneSelectedBlocks() {
        return HitTesting.pruneSelectedBlocks(graphView);
    }

    function blockHit(sx, sy) {
        return HitTesting.blockHit(graphView, sx, sy);
    }

    function findBlockAt(sx, sy) {
        return HitTesting.findBlockAt(graphView, sx, sy);
    }

    function wireAt(sx, sy) {
        return HitTesting.wireAt(graphView, sx, sy);
    }

    function showToast(message) {
        return Data.showToast(graphView, message);
    }

    Connections {
        target: Theme
        function onColorsChanged() {
            widthsDirty = true;
            canvas.requestPaint();
        }
    }

    function portSortCmp(a, b) {
        return Data.portSortCmp(graphView, a, b);
    }

    function getSortedPorts(nodeId) {
        return Data.getSortedPorts(graphView, nodeId);
    }

    function linkSig(link) {
        return Data.linkSig(graphView, link);
    }

    function linkMatchesSig(link, sig) {
        return Data.linkMatchesSig(graphView, link, sig);
    }

    function nodeSig(node) {
        return Data.nodeSig(graphView, node);
    }

    function findLinkData(linkId) {
        return Data.findLinkData(graphView, linkId);
    }

    function naturalCmp(a, b) {
        return Data.naturalCmp(graphView, a, b);
    }

    property int deferSaveDepth: 0
    property var pendingLayoutJson: null
    property var pendingPinnedJson: null

    function refreshData() {
        return Data.refreshData(graphView);
    }

    function refreshDataNow() {
        return Data.refreshDataNow(graphView);
    }

    function nodeIdIsHidden(nodeId) {
        return Placement.nodeIdIsHidden(graphView, nodeId);
    }

    function nodeHasBackwardLink(nodeId) {
        return Placement.nodeHasBackwardLink(graphView, nodeId);
    }

    function nodeIsPinned(node) {
        return Placement.nodeIsPinned(graphView, node);
    }

    function isFresh(nodeId) {
        return Placement.isFresh(graphView, nodeId);
    }

    function nodeHasLinks(nodeId) {
        return Placement.nodeHasLinks(graphView, nodeId);
    }

    function placementMobility(node) {
        return Placement.placementMobility(graphView, node);
    }

    function peerAnchors(node, peer) {
        return Placement.peerAnchors(graphView, node, peer);
    }

    function placementPeer(node) {
        return Placement.placementPeer(graphView, node);
    }

    function placeNearPeer(node) {
        return Placement.placeNearPeer(graphView, node);
    }

    function adjacentAnchor(node, peer, nodeFeedsPeer) {
        return Placement.adjacentAnchor(graphView, node, peer, nodeFeedsPeer);
    }

    function findNodeByLayoutKey(key) {
        return Placement.findNodeByLayoutKey(graphView, key);
    }

    function flowRank(type) {
        return Placement.flowRank(graphView, type);
    }

    function provisionalAnchor(node) {
        return Placement.provisionalAnchor(graphView, node);
    }

    function resolveNodeOverlap(node, skipIds) {
        return Placement.resolveNodeOverlap(graphView, node, skipIds);
    }

    function gridPos(p) {
        return Placement.gridPos(graphView, p);
    }

    function alignAllToGrid() {
        return Placement.alignAllToGrid(graphView);
    }

    function computeSnap(nodeId, rawX, rawY, skipIds) {
        return Placement.computeSnap(graphView, nodeId, rawX, rawY, skipIds);
    }

    function persistLayout() {
        return Placement.persistLayout(graphView);
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
    readonly property bool contextIsMeter: contextNode !== null && isMeterNode(contextNode)
    readonly property bool contextCanBeDefault: contextNode !== null && (contextNode.type === "Sink" || contextNode.type === "Duplex" || contextNode.type === "Plugin")

    property string templateSubgraphId: ""
    property var insertPreview: null

    GraphMenus {
        id: graphMenus
        graph: graphView
    }

    function updateInsertPreview(nodeId) {
        return HitTesting.updateInsertPreview(graphView, nodeId);
    }

    function persistHidden() {
        return Nodes.persistHidden(graphView);
    }

    function nodeStyle(node) {
        return Nodes.nodeStyle(graphView, node);
    }

    function setNodeStyle(node, field, value) {
        return Nodes.setNodeStyle(graphView, node, field, value);
    }

    readonly property var nodeColorTokens: ["catHardware", "catVirtual", "catSubgraph", "accent", "catApp", "catPlugin", "catMidi", "textDestructive"]

    function customNodeColor(node) {
        return Nodes.customNodeColor(graphView, node);
    }

    function nodeColor(node) {
        return Nodes.nodeColor(graphView, node);
    }

    function isConnectedOnly(node) {
        return Nodes.isConnectedOnly(graphView, node);
    }

    function persistPinned() {
        return Nodes.persistPinned(graphView);
    }

    function buildUniqueLayoutKeys() {
        return Placement.buildUniqueLayoutKeys(graphView);
    }

    function collectNodeSizes() {
        return Placement.collectNodeSizes(graphView);
    }

    function collectPinnedPositions() {
        return Placement.collectPinnedPositions(graphView);
    }

    function visibleNodeCount() {
        return Placement.visibleNodeCount(graphView);
    }
    function pinnedVisibleCount() {
        return Placement.pinnedVisibleCount(graphView);
    }

    function requestAutoLayout(includePinned) {
        return Placement.requestAutoLayout(graphView, includePinned);
    }

    function runAutoLayout(includePinned) {
        return Placement.runAutoLayout(graphView, includePinned);
    }

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

    function placementObstacles(exceptId) {
        return Placement.placementObstacles(graphView, exceptId);
    }

    function spotIsFree(x, y, w, h, obstacles, gap) {
        return Placement.spotIsFree(graphView, x, y, w, h, obstacles, gap);
    }

    function spotIsTaken(node, x, y) {
        return Placement.spotIsTaken(graphView, node, x, y);
    }

    function columnSlot(x, anchorY, w, h, obstacles, gap) {
        return Placement.columnSlot(graphView, x, anchorY, w, h, obstacles, gap);
    }

    function findFreeSpot(node, anchorX, anchorY, dir, bounded) {
        return Placement.findFreeSpot(graphView, node, anchorX, anchorY, dir, bounded);
    }

    function kindAnchor(node) {
        return Placement.kindAnchor(graphView, node);
    }

    function startPlacing(nodeId) {
        return Placement.startPlacing(graphView, nodeId);
    }

    function finishPlacing() {
        return Placement.finishPlacing(graphView);
    }

    function isNodeBypassed(node) {
        return Nodes.isNodeBypassed(graphView, node);
    }

    function isNodeEnabled(node) {
        return Nodes.isNodeEnabled(graphView, node);
    }

    function findNodeData(nodeId) {
        return Nodes.findNodeData(graphView, nodeId);
    }

    function getNodeColumn(type) {
        return Nodes.getNodeColumn(graphView, type);
    }

    function nodeCategory(node) {
        return Nodes.nodeCategory(graphView, node);
    }

    function signalColor(mediaType) {
        return Nodes.signalColor(graphView, mediaType);
    }

    function visiblePortCount(ports) {
        return Nodes.visiblePortCount(graphView, ports);
    }

    function isMeterNode(n) {
        return Nodes.isMeterNode(graphView, n);
    }

    property var meterNodeIds: []
    property var meterFrames: ({})
    property var meterLevels: ({})

    readonly property real meterFloorDb: -60
    readonly property real meterHotFrac: (-12 - meterFloorDb) / -meterFloorDb
    readonly property real meterClipFrac: (-3 - meterFloorDb) / -meterFloorDb

    function levelFraction(level) {
        return Meters.levelFraction(graphView, level);
    }

    function isAudioRow(port) {
        return Meters.isAudioRow(graphView, port);
    }

    function meterRows(node) {
        return Meters.meterRows(graphView, node);
    }

    function meterBarRect(nw, row) {
        return Meters.meterBarRect(graphView, nw, row);
    }

    function updateMeterNodeIds() {
        return Meters.updateMeterNodeIds(graphView);
    }

    property var dockedMeters: ({})
    property var dockBarIds: []
    property var dockBarFrames: ({})
    property var snapRedirect: ({})

    function isSnapInternal(fromPos, toPos) {
        return Meters.isSnapInternal(graphView, fromPos, toPos);
    }

    property var snapSeenAnchored: ({})
    property var snapSuspended: ({})
    property var snapUnanchoredSince: ({})
    property var removedMeterKeys: ({})
    readonly property int snapGraceMs: 10000
    readonly property int snapFirstGraceMs: 30000

    function snapAnchorCount(meter, side) {
        return Meters.snapAnchorCount(graphView, meter, side);
    }

    function updateSnapStates() {
        return Meters.updateSnapStates(graphView);
    }

    function suspendOrphanedSnaps() {
        return Meters.suspendOrphanedSnaps(graphView);
    }

    function forgetMeterWhenGone(node) {
        return Meters.forgetMeterWhenGone(graphView, node);
    }

    function forgetStaleMeterKeys(name) {
        return Meters.forgetStaleMeterKeys(graphView, name);
    }

    function snappedMeterCount() {
        return Meters.snappedMeterCount(graphView);
    }

    function unsnapAllMeters() {
        return Meters.unsnapAllMeters(graphView);
    }

    property var snapPreview: null

    function meterSnapCandidate(meter, sx, sy) {
        return Meters.meterSnapCandidate(graphView, meter, sx, sy);
    }

    readonly property real dockBarW: 64
    readonly property real dockBarH: 3
    function dockBarRect(cx, cy, output) {
        return Meters.dockBarRect(graphView, cx, cy, output);
    }

    function connectReplacing(outPort, inPort, oldLink) {
        return Meters.connectReplacing(graphView, outPort, inPort, oldLink);
    }

    property var pendingMeterAttach: []

    function audioPortsOf(nodeId, outputs) {
        return Meters.audioPortsOf(graphView, nodeId, outputs);
    }

    function meterChunks(n) {
        return Meters.meterChunks(graphView, n);
    }

    function canMeterPorts(node, outputs) {
        return Meters.canMeterPorts(graphView, node, outputs);
    }

    function addMetersTo(node, outputs) {
        return Meters.addMetersTo(graphView, node, outputs);
    }

    function processPendingMeterAttach() {
        return Meters.processPendingMeterAttach(graphView);
    }

    function removeMetersOn(nodeId) {
        return Meters.removeMetersOn(graphView, nodeId);
    }

    function snapTargetPort(portId) {
        return Meters.snapTargetPort(graphView, portId);
    }

    function persistDocked() {
        return Meters.persistDocked(graphView);
    }

    function isDockedMeter(n) {
        return Meters.isDockedMeter(graphView, n);
    }

    function meterChannelPorts(node) {
        return Meters.meterChannelPorts(graphView, node);
    }

    function canSnapMeter(node, side) {
        return Meters.canSnapMeter(graphView, node, side);
    }

    property var meterKeyAlias: ({})

    function resolveMeterKey(key) {
        return Meters.resolveMeterKey(graphView, key);
    }

    function isAutoMeterName(name) {
        return Meters.isAutoMeterName(graphView, name);
    }

    function snapMeterName(hostId, side, anchorPorts) {
        return Meters.snapMeterName(graphView, hostId, side, anchorPorts);
    }

    function snapMeterNameFromLinks(meter, side) {
        return Meters.snapMeterNameFromLinks(graphView, meter, side);
    }

    function renameMeter(meter, base) {
        return Meters.renameMeter(graphView, meter, base);
    }

    function setMeterSnapped(key, side) {
        return Meters.setMeterSnapped(graphView, key, side);
    }

    function snapMeter(node, side) {
        return Meters.snapMeter(graphView, node, side);
    }

    function metersSnappedOn(nodeId) {
        return Meters.metersSnappedOn(graphView, nodeId);
    }

    function unsnapMetersOn(nodeId) {
        return Meters.unsnapMetersOn(graphView, nodeId);
    }

    function addMeter(channels) {
        return Meters.addMeter(graphView, channels);
    }

    function computeNodeLayout(node, sorted) {
        return Nodes.computeNodeLayout(graphView, node, sorted);
    }

    function nodeLayout(node) {
        return Nodes.nodeLayout(graphView, node);
    }

    function calculateNodeHeight(node) {
        return Nodes.calculateNodeHeight(graphView, node);
    }

    function calculateNodeWidths() {
        return Nodes.calculateNodeWidths(graphView);
    }

    function getNodeWidth(nodeId) {
        return Nodes.getNodeWidth(graphView, nodeId);
    }

    function toggleExpanded(node) {
        return Nodes.toggleExpanded(graphView, node);
    }

    function toCanvas(sx, sy) {
        return Viewport.toCanvas(graphView, sx, sy);
    }

    function toScreen(cx, cy) {
        return Viewport.toScreen(graphView, cx, cy);
    }

    function contentBounds() {
        return Viewport.contentBounds(graphView);
    }

    function clampPan() {
        return Viewport.clampPan(graphView);
    }

    function zoomAt(sx, sy, newZoom) {
        return Viewport.zoomAt(graphView, sx, sy, newZoom);
    }

    function fitToView() {
        return Viewport.fitToView(graphView);
    }

    function frameSelection() {
        return Viewport.frameSelection(graphView);
    }

    function fitBounds(b) {
        return Viewport.fitBounds(graphView, b);
    }

    function scrollRange(lo, hi, viewLo, viewLen) {
        return Viewport.scrollRange(graphView, lo, hi, viewLo, viewLen);
    }

    function updateScrollBars() {
        return Viewport.updateScrollBars(graphView);
    }

    function coveredAbove(cx, cy, z) {
        return HitTesting.coveredAbove(graphView, cx, cy, z);
    }

    function findPortAt(sx, sy) {
        return HitTesting.findPortAt(graphView, sx, sy);
    }

    function findPortLabelAt(sx, sy) {
        return HitTesting.findPortLabelAt(graphView, sx, sy);
    }

    function linksOfPort(portId) {
        return HitTesting.linksOfPort(graphView, portId);
    }

    function visibleLinksOfPort(portId) {
        return HitTesting.visibleLinksOfPort(graphView, portId);
    }

    function openPortMenu(portId) {
        return HitTesting.openPortMenu(graphView, portId);
    }

    function getPortDirection(portId) {
        return HitTesting.getPortDirection(graphView, portId);
    }

    function getPortNodeId(portId) {
        return HitTesting.getPortNodeId(graphView, portId);
    }

    function findNodeAt(sx, sy) {
        return HitTesting.findNodeAt(graphView, sx, sy);
    }

    function refreshPresetInfo() {
        return HitTesting.refreshPresetInfo(graphView);
    }

    function presetRects(n, nx, ny, nh) {
        return HitTesting.presetRects(graphView, n, nx, ny, nh);
    }

    function inRect(c, r) {
        return HitTesting.inRect(graphView, c, r);
    }

    function openPresetMenu(nodeId) {
        return HitTesting.openPresetMenu(graphView, nodeId);
    }

    function openPresetSave(nodeId) {
        return HitTesting.openPresetSave(graphView, nodeId);
    }

    function presetChanged() {
        return HitTesting.presetChanged(graphView);
    }

    function footerButtons(n, nx, ny, nh) {
        return HitTesting.footerButtons(graphView, n, nx, ny, nh);
    }

    function pinRect(n, nx, ny) {
        return HitTesting.pinRect(graphView, n, nx, ny);
    }

    function portsRect(n, nx, ny) {
        return HitTesting.portsRect(graphView, n, nx, ny);
    }

    function hasUnconnectedPorts(node) {
        return HitTesting.hasUnconnectedPorts(graphView, node);
    }

    function showPortsButton(node) {
        return HitTesting.showPortsButton(graphView, node);
    }

    function togglePinned(node) {
        return HitTesting.togglePinned(graphView, node);
    }

    function toggleRowRect(n, lay, nx, ny) {
        return HitTesting.toggleRowRect(graphView, n, lay, nx, ny);
    }

    function findButtonAt(sx, sy) {
        return HitTesting.findButtonAt(graphView, sx, sy);
    }

    function fontSpec(weight, size, mono) {
        return Drawing.fontSpec(graphView, weight, size, mono);
    }

    function elide(ctx, str, maxW) {
        return Drawing.elide(graphView, ctx, str, maxW);
    }

    function drawGlyph(ctx, name, x, y, size, color) {
        return Drawing.drawGlyph(graphView, ctx, name, x, y, size, color);
    }

    function roundRectPath(ctx, x, y, w, h, r) {
        return Drawing.roundRectPath(graphView, ctx, x, y, w, h, r);
    }

    function drawTag(ctx, text, right, cy) {
        return Drawing.drawTag(graphView, ctx, text, right, cy);
    }

    function drawDefaultChip(ctx, right, cy) {
        return Drawing.drawDefaultChip(graphView, ctx, right, cy);
    }

    function nodeMatchesSearch(node) {
        return Nodes.nodeMatchesSearch(graphView, node);
    }

    function portMatchesSearch(port) {
        return Nodes.portMatchesSearch(graphView, port);
    }

    property int dragSourcePortId: -1
    property int dragSourceNodeId: -1
    function connectFromNodeId() {
        return Nodes.connectFromNodeId(graphView);
    }

    function isValidDropTarget(port, nodeId) {
        return Nodes.isValidDropTarget(graphView, port, nodeId);
    }

    Canvas {
        id: canvas
        anchors.fill: parent

        onPaint: Paint.paint(graphView)
    }

    GraphMeters {
        id: graphMeters
        graph: graphView
    }





    Timer {
        id: orphanTimer
        interval: 1000
        onTriggered: graphView.suspendOrphanedSnaps()
    }

    Timer {
        interval: 50
        repeat: true
        running: graphView.meterNodeIds.length > 0 && graphView.visible && graphView.Window.window !== null && graphView.Window.window.visibility !== Window.Minimized && graphView.Window.window.visibility !== Window.Hidden
        onTriggered: {
            try {
                graphView.meterLevels = JSON.parse(controller.meter_levels_json());
            } catch (e) {
                graphView.meterLevels = {};
            }
        }
    }

    function drawNode(ctx, node, nx, ny) {
        return Drawing.drawNode(graphView, ctx, node, nx, ny);
    }

    function drawPin(ctx, port, nodeId, cx, cy, nodeSelected) {
        return Drawing.drawPin(graphView, ctx, port, nodeId, cx, cy, nodeSelected);
    }

    focus: true

    Keys.onShortcutOverride: event => Input.keyShortcutOverride(graphView, event)

    Keys.onPressed: event => Input.keyPressed(graphView, event)

    Keys.onReleased: event => Input.keyReleased(graphView, event)

    MouseArea {
        id: mouseArea
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
        hoverEnabled: true

        onDoubleClicked: mouse => Input.pointerDoubleClicked(graphView, mouse)

        property real lastX: 0
        property real lastY: 0
        property real pressX: 0
        property real pressY: 0

        onPressed: mouse => Input.pointerPressed(graphView, mouse)

        onPositionChanged: mouse => Input.pointerPositionChanged(graphView, mouse)

        onExited: updateHover(-1, -1)

        onReleased: mouse => Input.pointerReleased(graphView, mouse)

        onWheel: wheel => Input.pointerWheel(graphView, wheel)

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

    GraphTools {
        id: graphTools
        graph: graphView
    }

    function updateHover(sx, sy) {
        return Input.updateHover(graphView, sx, sy);
    }

    function updateCursor() {
        return Input.updateCursor(graphView);
    }

    function portName(portId) {
        return Input.portName(graphView, portId);
    }

    ZbTooltip {
        id: pinTip
        delay: 300
    }

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
        return Input.hideNameTip(graphView);
    }
    function showNameTip() {
        return Input.showNameTip(graphView);
    }
    function placeNameTip(n, pos) {
        return Input.placeNameTip(graphView, n, pos);
    }

    function bezierCtrl(x1, x2) {
        return Wires.bezierCtrl(graphView, x1, x2);
    }

    function isOrthoWire() {
        return Wires.isOrthoWire(graphView);
    }

    function wireLane(wid) {
        return Wires.wireLane(graphView, wid);
    }

    function orthoPoints(x1, y1, x2, y2, lane) {
        return Wires.orthoPoints(graphView, x1, y1, x2, y2, lane);
    }

    function wirePathPoints(x1, y1, x2, y2, wid) {
        return Wires.wirePathPoints(graphView, x1, y1, x2, y2, wid);
    }

    function wireObstacles() {
        return Wires.wireObstacles(graphView);
    }

    function segmentBlocked(ax, ay, bx, by, obs, shrink) {
        return Wires.segmentBlocked(graphView, ax, ay, bx, by, obs, shrink);
    }

    function pathBlocked(pts, x1, y1, x2, y2, obs) {
        return Wires.pathBlocked(graphView, pts, x1, y1, x2, y2, obs);
    }

    function uniqueSorted(vals) {
        return Wires.uniqueSorted(graphView, vals);
    }

    function nearestIndex(arr, v) {
        return Wires.nearestIndex(graphView, arr, v);
    }

    function gridRoute(sx, sy, ex, ey, obs) {
        return Wires.gridRoute(graphView, sx, sy, ex, ey, obs);
    }

    function simplifyOrtho(pts) {
        return Wires.simplifyOrtho(graphView, pts);
    }

    function routeOrtho(x1, y1, x2, y2, obs) {
        return Wires.routeOrtho(graphView, x1, y1, x2, y2, obs);
    }

    function planOrtho(x1, y1, x2, y2, lane, obs, prev) {
        return Wires.planOrtho(graphView, x1, y1, x2, y2, lane, obs, prev);
    }

    function nudgeRoutes(routes, obs) {
        return Wires.nudgeRoutes(graphView, routes, obs);
    }

    function shiftSegment(seg, offset, obs) {
        return Wires.shiftSegment(graphView, seg, offset, obs);
    }

    function updateWireRoutes(positions) {
        return Wires.updateWireRoutes(graphView, positions);
    }

    function routeExtraWire(wid, x1, y1, x2, y2) {
        return Wires.routeExtraWire(graphView, wid, x1, y1, x2, y2);
    }

    function wirePolyline(x1, y1, x2, y2, wid) {
        return Wires.wirePolyline(graphView, x1, y1, x2, y2, wid);
    }

    function wireSamples(x1, y1, x2, y2, wid, step) {
        return Wires.wireSamples(graphView, x1, y1, x2, y2, wid, step);
    }

    function wireBounds(x1, y1, x2, y2, wid) {
        return Wires.wireBounds(graphView, x1, y1, x2, y2, wid);
    }

    function wireMidpoint(x1, y1, x2, y2, wid) {
        return Wires.wireMidpoint(graphView, x1, y1, x2, y2, wid);
    }

    function traceWire(ctx, x1, y1, x2, y2, wid) {
        return Wires.traceWire(graphView, ctx, x1, y1, x2, y2, wid);
    }

    function strokeStyleOf(color) {
        return Wires.strokeStyleOf(graphView, color);
    }

    function drawWire(ctx, x1, y1, x2, y2, color, lineWidth, wid) {
        return Wires.drawWire(graphView, ctx, x1, y1, x2, y2, color, lineWidth, wid);
    }

    function computeWireLanes(positions) {
        return Wires.computeWireLanes(graphView, positions);
    }

    function assignLaneGroup(group, lanes) {
        return Wires.assignLaneGroup(graphView, group, lanes);
    }

    readonly property real linkHitPx: 10
    function linkHit(sx, sy) {
        return HitTesting.linkHit(graphView, sx, sy);
    }

    function findLinkAt(sx, sy) {
        return HitTesting.findLinkAt(graphView, sx, sy);
    }

    function findLinkUnderNode(nodeId) {
        return HitTesting.findLinkUnderNode(graphView, nodeId);
    }

    function distToWire(px, py, x1, y1, x2, y2, wid) {
        return HitTesting.distToWire(graphView, px, py, x1, y1, x2, y2, wid);
    }

    function wireIntersectsRect(x1, y1, x2, y2, wid, rx, ry, rw, rh) {
        return HitTesting.wireIntersectsRect(graphView, x1, y1, x2, y2, wid, rx, ry, rw, rh);
    }

    function findLinksInRect(sx1, sy1, sx2, sy2) {
        return HitTesting.findLinksInRect(graphView, sx1, sy1, sx2, sy2);
    }

    function findNodesInRect(sx1, sy1, sx2, sy2) {
        return HitTesting.findNodesInRect(graphView, sx1, sy1, sx2, sy2);
    }

    function deleteSelectedLinks() {
        return Selection.deleteSelectedLinks(graphView);
    }

    property var nodeCues: ({})
    property bool cueQuiet: true
    readonly property int cuePulseMs: 1600
    readonly property int cueMarkerMs: 12000
    property var edgeMarkers: []

    function quietCues() {
        return Viewport.quietCues(graphView);
    }

    function noteAppeared(ids) {
        return Viewport.noteAppeared(graphView, ids);
    }

    function nodeOnScreen(node) {
        return Viewport.nodeOnScreen(graphView, node);
    }

    function updateCues() {
        return Viewport.updateCues(graphView);
    }

    function revealNode(nodeId) {
        return Viewport.revealNode(graphView, nodeId);
    }

    Timer {
        id: cueTimer
        interval: 40
        repeat: true
        onTriggered: graphView.updateCues()
    }

    Timer {
        id: cueQuietTimer
        interval: 2500
        running: true
        onTriggered: graphView.cueQuiet = false
    }

    property var undoStack: []
    property var redoStack: []
    readonly property int undoLimit: 50
    readonly property bool canUndo: undoStack.length > 0
    readonly property bool canRedo: redoStack.length > 0

    function pushUndo(entry) {
        return History.pushUndo(graphView, entry);
    }

    function clearUndo() {
        return History.clearUndo(graphView);
    }

    function plural(n, one, many) {
        return History.plural(graphView, n, one, many);
    }
    function counted(n, one, many) {
        return History.counted(graphView, n, one, many);
    }

    function nodeIdForKey(key) {
        return History.nodeIdForKey(graphView, key);
    }

    function markHandPlaced(nodeId) {
        return History.markHandPlaced(graphView, nodeId);
    }

    function recordMove(verb, fromById) {
        return History.recordMove(graphView, verb, fromById);
    }

    function moveEntry(verb, fromById) {
        return History.moveEntry(graphView, verb, fromById);
    }

    function portLinkRecord(outPort, inPort) {
        return History.portLinkRecord(graphView, outPort, inPort);
    }

    function insertRewireEntry(linkId, nodeId) {
        return History.insertRewireEntry(graphView, linkId, nodeId);
    }

    function applyLinkRecords(recs, make) {
        return History.applyLinkRecords(graphView, recs, make);
    }

    function linkRecord(link) {
        return History.linkRecord(graphView, link);
    }

    function linkRecordValid(rec) {
        return History.linkRecordValid(graphView, rec);
    }

    function findLinkByPorts(outPort, inPort) {
        return History.findLinkByPorts(graphView, outPort, inPort);
    }

    function connectPorts(outPort, inPort) {
        return History.connectPorts(graphView, outPort, inPort);
    }

    function disconnectLinks(ids) {
        return History.disconnectLinks(graphView, ids);
    }

    function applyHistory(entry, undoing) {
        return History.applyHistory(graphView, entry, undoing);
    }

    function undo() {
        return History.undo(graphView);
    }

    function redo() {
        return History.redo(graphView);
    }

    function selectedNodeIds() {
        return History.selectedNodeIds(graphView);
    }

    function selectAll() {
        return History.selectAll(graphView);
    }

    property var nudgeEntry: null
    function nudgeSelection(dx, dy, big) {
        return Selection.nudgeSelection(graphView, dx, dy, big);
    }

    Timer {
        id: nudgeTimer
        interval: 700
        onTriggered: {
            graphView.nudgeEntry = null;
            graphView.persistLayout();
        }
    }

    property var deletePlan: null
    function deleteSelection() {
        return Selection.deleteSelection(graphView);
    }

    function deletePlanLines() {
        return Selection.deletePlanLines(graphView);
    }

    function performDelete() {
        return Selection.performDelete(graphView);
    }

    function blockedReasonFor(fromPort, fromDir, toPort) {
        return Selection.blockedReasonFor(graphView, fromPort, fromDir, toPort);
    }

    function blockLinks(linkIds) {
        return Selection.blockLinks(graphView, linkIds);
    }

    function blockSelectedLinks() {
        return Selection.blockSelectedLinks(graphView);
    }

    function hasSelection() {
        return Subgraphs.hasSelection(graphView);
    }

    function selectedPluginIds() {
        return Subgraphs.selectedPluginIds(graphView);
    }

    function subgraphInfo(id) {
        return Subgraphs.subgraphInfo(graphView, id);
    }

    property var groupAnchor: null
    function groupIntoSubgraph(nodeId) {
        return Subgraphs.groupIntoSubgraph(graphView, nodeId);
    }

    function openSubgraphEditor(id) {
        return Subgraphs.openSubgraphEditor(graphView, id);
    }

    function openVdevEditor(id) {
        return Subgraphs.openVdevEditor(graphView, id);
    }

    function applyInsertedLayout(resultJson) {
        return Subgraphs.applyInsertedLayout(graphView, resultJson);
    }

    function duplicateSubgraph(node) {
        return Subgraphs.duplicateSubgraph(graphView, node);
    }

    function insertTemplate(file) {
        return Subgraphs.insertTemplate(graphView, file);
    }

    function enterSubgraph(id) {
        return Subgraphs.enterSubgraph(graphView, id);
    }

    function exitSubgraph() {
        return Subgraphs.exitSubgraph(graphView);
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
        return Selection.clearSelection(graphView);
    }
}
