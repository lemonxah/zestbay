function applyDensity(graph) {
    graph.densityMoved = 0;
    if (!graph.layoutLoaded)
        return;
    graph.refreshData();
    var before = {};
    for (var i = 0; i < graph.nodes.length; i++) {
        if (graph.nodePositions[graph.nodes[i].id])
            before[graph.nodes[i].id] = {
                x: graph.nodePositions[graph.nodes[i].id].x,
                y: graph.nodePositions[graph.nodes[i].id].y
            };
    }
    graph.separateOverlaps();
    var entry = graph.recordMove("re-space", before);
    if (entry) {
        graph.densityMoved = entry.items.length;
        graph.persistLayout();
    }
    graph.elements.canvas.requestPaint();
}

function separateOverlaps(graph) {
    var gap = graph.nodeSpacing;
    for (var pass = 0; pass < 40; pass++) {
        var vis = [];
        for (var i = 0; i < graph.nodes.length; i++) {
            var n = graph.nodes[i];
            if ((n.layoutKey && graph.hiddenNodes[n.layoutKey]) || !graph.nodePositions[n.id])
                continue;
            vis.push({
                node: n,
                x: graph.nodePositions[n.id].x,
                y: graph.nodePositions[n.id].y,
                w: graph.getNodeWidth(n.id),
                h: graph.calculateNodeHeight(n)
            });
        }
        vis.sort((a, b) => a.y - b.y || a.x - b.x);
        var fixed = false;
        for (var ai = 0; ai < vis.length && !fixed; ai++) {
            for (var bi = ai + 1; bi < vis.length; bi++) {
                var a = vis[ai], b = vis[bi];
                if (!(a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y))
                    continue;
                if (graph.nodeIsPinned(b.node) && !graph.nodeIsPinned(a.node)) {
                    var up = b.y - a.h - gap;
                    if (graph.snapToGrid)
                        up = Math.floor(up / graph.gridSize) * graph.gridSize;
                    graph.nodePositions[a.node.id] = {
                        x: a.x,
                        y: up
                    };
                } else {
                    var down = a.y + a.h + gap;
                    if (graph.snapToGrid)
                        down = Math.ceil(down / graph.gridSize) * graph.gridSize;
                    graph.nodePositions[b.node.id] = {
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

function nodeIdIsHidden(graph, nodeId) {
    var n = graph.findNodeData(nodeId);
    return !!(n && n.layoutKey && graph.hiddenNodes[n.layoutKey]);
}

function nodeHasBackwardLink(graph, nodeId) {
    for (var li = 0; li < graph.links.length; li++) {
        var l = graph.links[li];
        if (l.outputNodeId !== nodeId && l.inputNodeId !== nodeId)
            continue;
        if (l.outputNodeId === l.inputNodeId)
            continue;
        if (graph.nodeIdIsHidden(l.outputNodeId) || graph.nodeIdIsHidden(l.inputNodeId))
            continue;
        var fromPos = graph.nodePositions[l.outputNodeId];
        var toPos = graph.nodePositions[l.inputNodeId];
        if (!fromPos || !toPos)
            continue;
        var outX = fromPos.x + (graph.getNodeWidth(l.outputNodeId) || graph.minNodeWidth);
        if (toPos.x < outX - 20)
            return true;
    }
    return false;
}

function nodeIsPinned(graph, node) {
    return !!(node && node.layoutKey && graph.pinnedNodes[node.layoutKey]);
}

function isFresh(graph, nodeId) {
    var a = graph.appearedAt[nodeId];
    return !!a && Date.now() - a.t <= graph.settleWindowMs;
}

function nodeHasLinks(graph, nodeId) {
    for (var li = 0; li < graph.links.length; li++) {
        var l = graph.links[li];
        if (l.outputNodeId === nodeId && !graph.nodeIdIsHidden(l.inputNodeId))
            return true;
        if (l.inputNodeId === nodeId && !graph.nodeIdIsHidden(l.outputNodeId))
            return true;
    }
    return false;
}

function placementMobility(graph, node) {
    if (node.type === "StreamOutput" || node.type === "StreamInput")
        return 2;
    if (node.type === "Source" || node.type === "Sink" || node.type === "Duplex" || node.type === "Plugin" || node.type === "Subgraph")
        return 0;
    return 1;
}

function peerAnchors(graph, node, peer) {
    if (!graph.awaitingLinkPlacement[peer.id] || !graph.isFresh(peer.id))
        return true;
    var mn = graph.placementMobility(node), mp = graph.placementMobility(peer);
    if (mn !== mp)
        return mn > mp;
    var an = graph.appearedAt[node.id], ap = graph.appearedAt[peer.id];
    var sn = an ? an.seq : 0, sp = ap ? ap.seq : 0;
    if (sn !== sp)
        return sn > sp;
    return node.id > peer.id;
}

function placementPeer(graph, node) {
    var nodeId = node.id;
    for (var li = 0; li < graph.links.length; li++) {
        var link = graph.links[li];
        var peerId = -1;
        var nodeFeedsPeer = false;
        if (link.outputNodeId === nodeId && link.inputNodeId !== nodeId) {
            peerId = link.inputNodeId;
            nodeFeedsPeer = true;
        } else if (link.inputNodeId === nodeId && link.outputNodeId !== nodeId) {
            peerId = link.outputNodeId;
        }
        if (peerId < 0 || !graph.nodePositions[peerId])
            continue;
        var peerNode = graph.findNodeData(peerId);
        if (!peerNode || (peerNode.layoutKey && graph.hiddenNodes[peerNode.layoutKey]))
            continue;
        if (!graph.peerAnchors(node, peerNode))
            continue;
        return {
            peer: peerNode,
            nodeFeedsPeer: nodeFeedsPeer
        };
    }
    return null;
}

function placeNearPeer(graph, node) {
    if (graph.nodeIsPinned(node))
        return false;
    var pp = graph.placementPeer(node);
    if (!pp)
        return false;
    var anchor = graph.adjacentAnchor(node, pp.peer, pp.nodeFeedsPeer);
    graph.nodePositions[node.id] = graph.findFreeSpot(node, anchor.x, anchor.y, anchor.dir, false);
    delete graph.awaitingLinkPlacement[node.id];
    graph.persistLayout();
    return true;
}

function adjacentAnchor(graph, node, peer, nodeFeedsPeer) {
    var peerPos = graph.nodePositions[peer.id];
    var w = graph.getNodeWidth(node.id);
    var peerW = graph.getNodeWidth(peer.id);
    var x = nodeFeedsPeer ? (peerPos.x - w - graph.placeColumnGap) : (peerPos.x + peerW + graph.placeColumnGap);
    if (graph.snapToGrid)
        x = nodeFeedsPeer ? Math.floor(x / graph.gridSize) * graph.gridSize : Math.ceil(x / graph.gridSize) * graph.gridSize;
    return {
        x: x,
        y: peerPos.y,
        dir: nodeFeedsPeer ? -1 : 1
    };
}

function findNodeByLayoutKey(graph, key) {
    if (!key)
        return null;
    for (var i = 0; i < graph.nodes.length; i++) {
        if (graph.nodes[i].layoutKey === key)
            return graph.nodes[i];
    }
    return null;
}

function flowRank(graph, type) {
    if (type === "Source")
        return 0;
    if (type === "StreamOutput")
        return 1;
    if (type === "StreamInput")
        return 3;
    if (type === "Sink" || type === "Duplex")
        return 4;
    return 2;
}

function provisionalAnchor(graph, node) {
    var defNode = graph.findNodeByLayoutKey(graph.defaultNodeKey);
    if (defNode && defNode.id !== node.id && graph.nodePositions[defNode.id] && !(defNode.layoutKey && graph.hiddenNodes[defNode.layoutKey])) {
        var nodeFeedsDefault = graph.flowRank(node.type) <= graph.flowRank(defNode.type);
        return graph.adjacentAnchor(node, defNode, nodeFeedsDefault);
    }
    return graph.kindAnchor(node);
}

function resolveNodeOverlap(graph, node, skipIds) {
    var pos = graph.nodePositions[node.id];
    if (!pos || graph.nodeIdIsHidden(node.id))
        return;
    var h = graph.calculateNodeHeight(node);
    var w = graph.getNodeWidth(node.id) || graph.minNodeWidth;
    var maxAttempts = 50;
    var spacing = graph.snapGap;
    for (var attempt = 0; attempt < maxAttempts; attempt++) {
        var overlaps = false;
        for (var oi = 0; oi < graph.nodes.length; oi++) {
            var other = graph.nodes[oi];
            if (other.id === node.id)
                continue;
            if (skipIds && skipIds[other.id])
                continue;
            if (other.layoutKey && graph.hiddenNodes[other.layoutKey])
                continue;
            var opos = graph.nodePositions[other.id];
            if (!opos)
                continue;
            var oh = graph.calculateNodeHeight(other);
            var ow = graph.getNodeWidth(other.id) || graph.minNodeWidth;

            if (pos.x < opos.x + ow && pos.x + w > opos.x && pos.y < opos.y + oh && pos.y + h > opos.y) {
                var overlapTop = opos.y + oh - pos.y;
                var overlapBot = pos.y + h - opos.y;
                var minH = Math.min(h, oh);
                if (overlapTop < minH * 0.7) {
                    pos = {
                        x: pos.x,
                        y: opos.y - h - spacing
                    };
                    if (graph.snapToGrid)
                        pos.y = Math.floor(pos.y / graph.gridSize) * graph.gridSize;
                } else {
                    pos = {
                        x: pos.x,
                        y: opos.y + oh + spacing
                    };
                    if (graph.snapToGrid)
                        pos.y = Math.ceil(pos.y / graph.gridSize) * graph.gridSize;
                }
                graph.nodePositions[node.id] = pos;
                overlaps = true;
                break;
            }
        }
        if (!overlaps)
            break;
    }
}

function gridPos(graph, p) {
    if (!graph.snapToGrid || !p)
        return p;
    return {
        x: Math.round(p.x / graph.gridSize) * graph.gridSize,
        y: Math.round(p.y / graph.gridSize) * graph.gridSize
    };
}

function alignAllToGrid(graph) {
    var moved = false;
    for (var i = 0; i < graph.nodes.length; i++) {
        var n = graph.nodes[i];
        if (n.layoutKey && graph.hiddenNodes[n.layoutKey])
            continue;
        var p = graph.nodePositions[n.id];
        if (!p)
            continue;
        var g = graph.gridPos(p);
        if (g.x !== p.x || g.y !== p.y) {
            graph.nodePositions[n.id] = g;
            moved = true;
        }
    }
    if (!moved)
        return;
    for (var j = 0; j < graph.nodes.length; j++) {
        if (graph.nodePositions[graph.nodes[j].id])
            graph.resolveNodeOverlap(graph.nodes[j]);
    }
    graph.persistLayout();
    graph.elements.canvas.requestPaint();
}

function computeSnap(graph, nodeId, rawX, rawY, skipIds) {
    var w = graph.getNodeWidth(nodeId) || graph.minNodeWidth;
    var node = graph.findNodeData(nodeId);
    var h = node ? graph.calculateNodeHeight(node) : 80;
    var gap = graph.snapGap;
    var best = {
        x: null,
        dx: graph.snapThreshold + 1,
        gx: 0,
        y: null,
        dy: graph.snapThreshold + 1,
        gy: 0
    };

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

    var visL = -graph.panX / graph.zoom;
    var visT = -graph.panY / graph.zoom;
    var visR = (graph.width - graph.panX) / graph.zoom;
    var visB = (graph.height - graph.panY) / graph.zoom;

    for (var i = 0; i < graph.nodes.length; i++) {
        var other = graph.nodes[i];
        if (other.id === nodeId)
            continue;
        if (skipIds && skipIds[other.id])
            continue;
        if (other.layoutKey && graph.hiddenNodes[other.layoutKey])
            continue;
        var opos = graph.nodePositions[other.id];
        if (!opos)
            continue;
        var ow = graph.getNodeWidth(other.id) || graph.minNodeWidth;
        var oh = graph.calculateNodeHeight(other);
        if (opos.x + ow < visL || opos.x > visR || opos.y + oh < visT || opos.y > visB)
            continue;
        tryX(opos.x, opos.x);
        tryX(opos.x + ow + gap, opos.x + ow);
        tryX(opos.x - gap - w, opos.x);

        tryY(opos.y, opos.y);
        tryY(opos.y + oh - h, opos.y + oh);
        tryY(opos.y + oh + gap, opos.y + oh);
        tryY(opos.y - gap - h, opos.y);
    }

    var lines = [];
    var snappedX = rawX;
    var snappedY = rawY;
    if (best.x !== null && best.dx <= graph.snapThreshold) {
        snappedX = best.x;
        lines.push({
            axis: "x",
            pos: best.gx
        });
    }
    if (best.y !== null && best.dy <= graph.snapThreshold) {
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

function persistLayout(graph) {
    var layoutObj = {};
    for (var existingKey in graph.savedLayout) {
        layoutObj[existingKey] = graph.savedLayout[existingKey];
    }
    for (var ni = 0; ni < graph.nodes.length; ni++) {
        var n = graph.nodes[ni];
        var key = graph.uniqueKeyById[n.id] || "";
        var pos = graph.nodePositions[n.id];
        if (key && pos) {
            layoutObj[key] = [pos.x, pos.y];
        }
    }
    graph.savedLayout = layoutObj;
    var json = JSON.stringify(layoutObj);
    if (graph.deferSaveDepth > 0)
        graph.pendingLayoutJson = json;
    else
        graph.controller.save_layout(json);
}

function buildUniqueLayoutKeys(graph) {
    var sorted = [];
    for (var i = 0; i < graph.nodes.length; i++) {
        var n = graph.nodes[i];
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

function collectNodeSizes(graph) {
    var sizesObj = {};
    for (var ni = 0; ni < graph.nodes.length; ni++) {
        var n = graph.nodes[ni];
        if (n.layoutKey && graph.hiddenNodes[n.layoutKey])
            continue;
        sizesObj[n.id] = [graph.getNodeWidth(n.id) || graph.minNodeWidth, graph.calculateNodeHeight(n)];
    }
    return sizesObj;
}

function collectPinnedPositions(graph) {
    var pinned = {};
    for (var ni = 0; ni < graph.nodes.length; ni++) {
        var n = graph.nodes[ni];
        if (n.layoutKey && graph.hiddenNodes[n.layoutKey])
            continue;
        if (graph.nodeIsPinned(n) && graph.nodePositions[n.id]) {
            var pos = graph.nodePositions[n.id];
            pinned[n.id] = [pos.x, pos.y];
        }
    }
    return pinned;
}

function visibleNodeCount(graph) {
    var c = 0;
    for (var i = 0; i < graph.nodes.length; i++) {
        if (!(graph.nodes[i].layoutKey && graph.hiddenNodes[graph.nodes[i].layoutKey]))
            c++;
    }
    return c;
}

function pinnedVisibleCount(graph) {
    var c = 0;
    for (var i = 0; i < graph.nodes.length; i++) {
        var n = graph.nodes[i];
        if (!(n.layoutKey && graph.hiddenNodes[n.layoutKey]) && graph.nodeIsPinned(n))
            c++;
    }
    return c;
}

function requestAutoLayout(graph, includePinned) {
    var total = graph.visibleNodeCount();
    if (total === 0) {
        graph.showToast("Auto layout: there are no nodes to arrange");
        return;
    }
    if (!includePinned && graph.pinnedVisibleCount() === total) {
        graph.elements.autoLayoutAllDialog.open();
        return;
    }
    graph.runAutoLayout(includePinned);
}

function runAutoLayout(graph, includePinned) {
    includePinned = !!includePinned;
    var kept = includePinned ? 0 : graph.pinnedVisibleCount();
    var sizesJson = JSON.stringify(graph.collectNodeSizes());
    var pinnedJson = includePinned ? "{}" : JSON.stringify(graph.collectPinnedPositions());
    var resultJson = graph.controller.auto_layout(sizesJson, pinnedJson, graph.snapToGrid ? graph.gridSize : 0);
    try {
        var positions = JSON.parse(resultJson);
        var before = {};
        for (var ni = 0; ni < graph.nodes.length; ni++) {
            var n = graph.nodes[ni];
            var pos = positions[n.id];
            if (pos && pos.length >= 2) {
                if (graph.nodePositions[n.id])
                    before[n.id] = graph.nodePositions[n.id];
                graph.nodePositions[n.id] = {
                    x: pos[0],
                    y: pos[1]
                };
                graph.markHandPlaced(n.id);
            }
        }
        var laidOut = graph.recordMove("auto layout of", before);
        if (laidOut)
            laidOut.label = includePinned ? "auto layout (all)" : "auto layout";
        var moved = laidOut ? laidOut.items.length : 0;
        var msg = moved > 0 ? "Auto layout: moved " + graph.counted(moved, "node", "nodes") : "Auto layout: nothing needed to move";
        if (kept > 0)
            msg += "; " + graph.counted(kept, "pinned node", "pinned nodes") + " stayed (Shift+L lays out pinned nodes too)";
        graph.showToast(msg + ".");
        graph.elements.canvas.requestPaint();
        graph.persistLayout();
    } catch (e) {
        console.log("Auto layout failed:", e);
        graph.showToast("Auto layout failed");
    }
}

function placementObstacles(graph, exceptId) {
    var obstacles = [];
    for (var oi = 0; oi < graph.nodes.length; oi++) {
        var other = graph.nodes[oi];
        if (other.id === exceptId)
            continue;
        if (other.layoutKey && graph.hiddenNodes[other.layoutKey])
            continue;
        var opos = graph.nodePositions[other.id];
        if (!opos)
            continue;
        obstacles.push({
            x: opos.x,
            y: opos.y,
            w: graph.getNodeWidth(other.id),
            h: graph.calculateNodeHeight(other)
        });
    }
    return obstacles;
}

function spotIsFree(graph, x, y, w, h, obstacles, gap) {
    var eps = 1e-6;
    for (var oi = 0; oi < obstacles.length; oi++) {
        var o = obstacles[oi];
        if (x < o.x + o.w + gap - eps && x + w + gap - eps > o.x && y < o.y + o.h + gap - eps && y + h + gap - eps > o.y)
            return false;
    }
    return true;
}

function spotIsTaken(graph, node, x, y) {
    var w = graph.getNodeWidth(node.id);
    var h = graph.calculateNodeHeight(node);
    var obstacles = graph.placementObstacles(node.id);
    for (var oi = 0; oi < obstacles.length; oi++) {
        var o = obstacles[oi];
        var ox = Math.min(x + w, o.x + o.w) - Math.max(x, o.x);
        var oy = Math.min(y + h, o.y + o.h) - Math.max(y, o.y);
        if (ox > Math.min(w, o.w) / 3 && oy > Math.min(h, o.h) / 3)
            return true;
    }
    return false;
}

function columnSlot(graph, x, anchorY, w, h, obstacles, gap) {
    if (graph.spotIsFree(x, anchorY, w, h, obstacles, gap))
        return anchorY;
    var best = null;
    var bestD = Infinity;
    for (var oi = 0; oi < obstacles.length; oi++) {
        var o = obstacles[oi];
        if (!(x < o.x + o.w + gap && x + w + gap > o.x))
            continue;
        var below = o.y + o.h + gap;
        var above = o.y - h - gap;
        if (graph.snapToGrid) {
            below = Math.ceil(below / graph.gridSize) * graph.gridSize;
            above = Math.floor(above / graph.gridSize) * graph.gridSize;
        }
        var cands = [below, above];
        for (var ci = 0; ci < 2; ci++) {
            var d = Math.abs(cands[ci] - anchorY);
            if (d < bestD && graph.spotIsFree(x, cands[ci], w, h, obstacles, gap)) {
                bestD = d;
                best = cands[ci];
            }
        }
    }
    return best !== null ? best : anchorY;
}

function findFreeSpot(graph, node, anchorX, anchorY, dir, bounded) {
    var w = graph.getNodeWidth(node.id);
    var h = graph.calculateNodeHeight(node);
    var gap = graph.nodeSpacing;
    var a = graph.gridPos({
        x: anchorX,
        y: anchorY
    });
    var colStep = w + graph.placeColumnGap;
    if (graph.snapToGrid)
        colStep = Math.ceil(colStep / graph.gridSize) * graph.gridSize;
    var obstacles = graph.placementObstacles(node.id);
    var cols = dir !== 0 ? [0, dir, 2 * dir, 3 * dir] : [0, 1, -1, 2, -2];
    var best = null;
    var bestCost = Infinity;
    for (var ci = 0; ci < cols.length; ci++) {
        var k = cols[ci];
        var x = a.x + k * colStep;
        var y = graph.columnSlot(x, a.y, w, h, obstacles, gap);
        var dy = Math.abs(y - a.y);
        if (bounded && (Math.abs(k) > 1 || dy > 2 * (h + gap)))
            continue;
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

function kindAnchor(graph, node) {
    var w = graph.getNodeWidth(node.id);
    var rank = graph.flowRank(node.type);
    if (graph.width <= 0 || graph.height <= 0) {
        var b = graph.contentBounds();
        return {
            x: b ? b.left + (b.right - b.left - w) * rank / 4 : 48,
            y: b ? b.top : 72,
            dir: 0
        };
    }
    var visL = -graph.panX / graph.zoom, visT = -graph.panY / graph.zoom;
    var visR = (graph.width - graph.panX) / graph.zoom, visB = (graph.height - graph.panY) / graph.zoom;
    var cx = (visL + visR) / 2, cy = (visT + visB) / 2;
    var best = null;
    var bestD = Infinity;
    for (var i = 0; i < graph.nodes.length; i++) {
        var o = graph.nodes[i];
        if (o.id === node.id || graph.flowRank(o.type) !== rank)
            continue;
        if (o.layoutKey && graph.hiddenNodes[o.layoutKey])
            continue;
        var p = graph.nodePositions[o.id];
        if (!p)
            continue;
        var ow = graph.getNodeWidth(o.id), oh = graph.calculateNodeHeight(o);
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
    var margin = 48 / graph.zoom;
    var span = Math.max(0, visR - visL - w - 2 * margin);
    return {
        x: visL + margin + span * rank / 4,
        y: visT + 72 / graph.zoom,
        dir: 0
    };
}

function startPlacing(graph, nodeId) {
    graph.placingNodeId = nodeId;
    graph.activeSnapLines = [];
    graph.elements.canvas.requestPaint();
}

function finishPlacing(graph) {
    if (graph.placingNodeId < 0)
        return;
    if (graph.nodePositions[graph.placingNodeId])
        graph.nodePositions[graph.placingNodeId] = graph.gridPos(graph.nodePositions[graph.placingNodeId]);
    delete graph.autoPlaced[graph.placingNodeId];
    delete graph.placedKeyById[graph.placingNodeId];
    graph.placingNodeId = -1;
    graph.persistLayout();
    graph.elements.canvas.requestPaint();
}
