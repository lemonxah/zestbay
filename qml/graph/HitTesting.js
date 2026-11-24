function refreshBlocked(graph) {
    try {
        graph.blockedPairs = JSON.parse(graph.controller.get_blocked_pairs_json());
    } catch (e) {
        graph.blockedPairs = [];
    }
    graph.pruneSelectedBlocks();
    graph.elements.canvas.requestPaint();
}

function blockKey(graph, b) {
    return b.ruleId + "|" + b.source + "|" + b.target;
}

function pruneSelectedBlocks(graph) {
    var live = {};
    for (var i = 0; i < graph.blockedPairs.length; i++)
        live[graph.blockKey(graph.blockedPairs[i])] = true;
    var kept = {};
    var dropped = false;
    for (var k in graph.selectedBlocks) {
        if (live[k])
            kept[k] = graph.selectedBlocks[k];
        else
            dropped = true;
    }
    if (dropped)
        graph.selectedBlocks = kept;
    if (graph.hoveredBlockKey !== "" && !live[graph.hoveredBlockKey])
        graph.hoveredBlockKey = "";
}

function blockHit(graph, sx, sy) {
    var c = graph.toCanvas(sx, sy);
    var threshold = graph.linkHitPx / graph.zoom;
    var best = null;
    var bestDist = threshold + 1;
    for (var i = 0; i < graph.blockGeom.length; i++) {
        var g = graph.blockGeom[i];
        var dm = Math.sqrt((c.x - g.mx) * (c.x - g.mx) + (c.y - g.my) * (c.y - g.my));
        var d = dm <= 10 / graph.zoom ? 0 : graph.distToWire(c.x, c.y, g.x1, g.y1, g.x2, g.y2, "b|" + graph.blockKey(g));
        if (d < bestDist) {
            bestDist = d;
            best = g;
        }
    }
    return best && bestDist <= threshold ? {
        g: best,
        dist: bestDist
    } : null;
}

function findBlockAt(graph, sx, sy) {
    var h = graph.blockHit(sx, sy);
    return h ? h.g : null;
}

function wireAt(graph, sx, sy) {
    var lh = graph.linkHit(sx, sy);
    var bh = graph.blockHit(sx, sy);
    if (bh && (!lh || bh.dist <= lh.dist))
        return {
            link: -1,
            block: bh.g
        };
    return {
        link: lh ? lh.id : -1,
        block: null
    };
}

function updateInsertPreview(graph, nodeId) {
    var linkId = -1;
    var node = nodeId >= 0 ? graph.findNodeData(nodeId) : null;
    if (node && node.type === "Plugin")
        linkId = graph.findLinkUnderNode(nodeId);
    if (linkId < 0) {
        graph.insertPreview = null;
        return;
    }
    if (graph.insertPreview && graph.insertPreview.linkId === linkId && graph.insertPreview.nodeId === nodeId)
        return;
    var plan = null;
    try {
        var json = graph.controller.insert_on_link_preview_json(linkId, nodeId);
        plan = json ? JSON.parse(json) : null;
    } catch (e) {
        plan = null;
    }
    if (!plan) {
        graph.insertPreview = null;
        return;
    }
    var remove = {};
    for (var i = 0; i < plan.remove.length; i++)
        remove[plan.remove[i]] = true;
    graph.insertPreview = {
        linkId: linkId,
        nodeId: nodeId,
        remove: remove,
        add: plan.add
    };
}

function coveredAbove(graph, cx, cy, z) {
    for (var i = graph.nodes.length - 1; i > z; i--) {
        var n = graph.nodes[i];
        if (n.layoutKey && graph.hiddenNodes[n.layoutKey])
            continue;
        var pos = graph.nodePositions[n.id];
        if (!pos)
            continue;
        if (cx >= pos.x && cx <= pos.x + graph.getNodeWidth(n.id) && cy >= pos.y && cy <= pos.y + graph.calculateNodeHeight(n)) {
            return true;
        }
    }
    return false;
}

function findPortAt(graph, sx, sy) {
    var hitRadius = graph.portRadius * graph.zoom * 2.5;
    var c = graph.toCanvas(sx, sy);
    var bestId = -1;
    var bestDist = hitRadius + 1;
    for (var pid in graph.portPositions) {
        var pp = graph.portPositions[pid];
        if (pp.alias)
            continue;
        var screenX = pp.cx * graph.zoom + graph.panX;
        var screenY = pp.cy * graph.zoom + graph.panY;
        var dx = sx - screenX;
        var dy = sy - screenY;
        var dist = Math.sqrt(dx * dx + dy * dy);
        if (dist < bestDist && !graph.coveredAbove(c.x, c.y, pp.z !== undefined ? pp.z : -1)) {
            bestDist = dist;
            bestId = parseInt(pid);
        }
    }
    return bestDist <= hitRadius ? bestId : -1;
}

function findPortLabelAt(graph, sx, sy) {
    var c = graph.toCanvas(sx, sy);
    var half = graph.theme.nodePortRow / 2;
    for (var pid in graph.portPositions) {
        var pp = graph.portPositions[pid];
        if (pp.labelX0 === undefined)
            continue;
        if (c.y < pp.cy - half || c.y > pp.cy + half || c.x < pp.labelX0 || c.x > pp.labelX1)
            continue;
        if (!graph.coveredAbove(c.x, c.y, pp.z !== undefined ? pp.z : -1))
            return parseInt(pid);
    }
    return -1;
}

function linksOfPort(graph, portId) {
    var out = [];
    for (var i = 0; i < graph.links.length; i++) {
        if (graph.links[i].outputPortId === portId || graph.links[i].inputPortId === portId)
            out.push(graph.links[i].id);
    }
    return out;
}

function visibleLinksOfPort(graph, portId) {
    var meterPort = graph.snapRedirect[portId];
    if (meterPort === undefined)
        return graph.linksOfPort(portId);
    var meterNode = graph.getPortNodeId(meterPort);
    var out = [];
    var own = graph.linksOfPort(portId);
    for (var i = 0; i < own.length; i++) {
        var l = graph.findLinkData(own[i]);
        if (l && l.outputNodeId !== meterNode && l.inputNodeId !== meterNode)
            out.push(own[i]);
    }
    var viaMeter = graph.linksOfPort(meterPort);
    for (var j = 0; j < viaMeter.length; j++) {
        var m = graph.findLinkData(viaMeter[j]);
        if (m && m.outputPortId !== portId && m.inputPortId !== portId && out.indexOf(viaMeter[j]) < 0)
            out.push(viaMeter[j]);
    }
    return out;
}

function openPortMenu(graph, portId) {
    var nodeId = graph.getPortNodeId(portId);
    if (nodeId < 0)
        return;
    graph.contextNodeId = nodeId;
    graph.contextNode = graph.findNodeData(nodeId);
    graph.contextPortId = portId;
    graph.elements.portContextMenu.popup();
}

function getPortDirection(graph, portId) {
    for (var nid in graph.portsByNode) {
        var ports = graph.portsByNode[nid];
        for (var i = 0; i < ports.length; i++) {
            if (ports[i].id === portId)
                return ports[i].direction;
        }
    }
    return "";
}

function getPortNodeId(graph, portId) {
    for (var nid in graph.portsByNode) {
        var ports = graph.portsByNode[nid];
        for (var i = 0; i < ports.length; i++) {
            if (ports[i].id === portId)
                return parseInt(nid);
        }
    }
    return -1;
}

function findNodeAt(graph, sx, sy) {
    var c = graph.toCanvas(sx, sy);
    for (var i = graph.nodes.length - 1; i >= 0; i--) {
        var n = graph.nodes[i];
        if (n.layoutKey && graph.hiddenNodes[n.layoutKey])
            continue;
        var pos = graph.nodePositions[n.id];
        if (!pos)
            continue;
        var h = graph.calculateNodeHeight(n);
        if (c.x >= pos.x && c.x <= pos.x + graph.getNodeWidth(n.id) && c.y >= pos.y && c.y <= pos.y + h) {
            return n.id;
        }
    }
    return -1;
}

function refreshPresetInfo(graph) {
    try {
        graph.presetInfo = JSON.parse(graph.controller.get_plugin_presets_all_json());
    } catch (e) {
        graph.presetInfo = {};
    }
}

function presetRects(graph, n, nx, ny, nh) {
    var margin = 10;
    var gap = 6;
    var saveW = 44;
    var h = graph.theme.nodeFooterBtn;
    var w = graph.getNodeWidth(n.id);
    var y = ny + nh - graph.theme.nodeFooter - graph.theme.nodePresetRow + (graph.theme.nodePresetRow - h) / 2;
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

function inRect(graph, c, r) {
    return c.x >= r.x && c.x <= r.x + r.w && c.y >= r.y && c.y <= r.y + r.h;
}

function openPresetMenu(graph, nodeId) {
    var info = {};
    try {
        info = JSON.parse(graph.controller.get_plugin_presets_json(nodeId));
    } catch (e) {}
    graph.presetMenuNodeId = nodeId;
    graph.presetMenuNames = info.presets || [];
    graph.presetMenuSelected = info.selected || "";
    graph.presetMenuDefault = info["default"] || "";
    graph.presetMenuChanges = info.changes || [];
    var pos = graph.nodePositions[nodeId];
    var n = graph.findNodeData(nodeId);
    if (!pos || !n)
        return;
    var r = graph.presetRects(n, pos.x, pos.y, graph.calculateNodeHeight(n)).name;
    var sp = graph.toScreen(r.x, r.y + r.h);
    graph.elements.presetMenu.popup(sp.x, sp.y + 2);
}

function openPresetSave(graph, nodeId) {
    var info = {};
    try {
        info = JSON.parse(graph.controller.get_plugin_presets_json(nodeId));
    } catch (e) {}
    graph.presetSaveNodeId = nodeId;
    graph.presetSaveNames = info.presets || [];
    graph.elements.presetSaveDialog.selectedName = info.selected || "";
    graph.elements.presetSaveDialog.open();
}

function presetChanged(graph) {
    graph.refreshPresetInfo();
    graph.elements.canvas.requestPaint();
}

function footerButtons(graph, n, nx, ny, nh) {
    var y = ny + nh - graph.theme.nodeFooter + (graph.theme.nodeFooter - graph.theme.nodeFooterBtn) / 2;
    var base = n.type === "Subgraph" ? [
        {
            key: "open",
            w: 1
        }
    ] : [
        {
            key: "onoff",
            w: 40
        },
        {
            key: "bypass",
            w: 50
        },
        {
            key: "ui",
            w: 28
        },
        {
            key: "params",
            w: 52
        }
    ];
    var margin = 10;
    var gap = 6;
    var avail = graph.getNodeWidth(n.id) - 2 * margin - gap * (base.length - 1);
    var total = 0;
    for (var t = 0; t < base.length; t++)
        total += base[t].w;
    var x = nx + margin;
    var out = [];
    for (var i = 0; i < base.length; i++) {
        var w = i === base.length - 1 ? nx + graph.getNodeWidth(n.id) - margin - x : Math.round(avail * base[i].w / total);
        out.push({
            key: base[i].key,
            x: x,
            y: y,
            w: w,
            h: graph.theme.nodeFooterBtn
        });
        x += w + gap;
    }
    return out;
}

function pinRect(graph, n, nx, ny) {
    var s = 18;
    return {
        x: nx + graph.getNodeWidth(n.id) - 8 - s,
        y: ny + (graph.theme.nodeHeader - s) / 2,
        w: s,
        h: s
    };
}

function portsRect(graph, n, nx, ny) {
    var r = graph.pinRect(n, nx, ny);
    return {
        x: r.x - r.w,
        y: r.y,
        w: r.w,
        h: r.h
    };
}

function hasUnconnectedPorts(graph, node) {
    var sorted = graph.getSortedPorts(node.id);
    var all = sorted.inputs.concat(sorted.outputs);
    for (var i = 0; i < all.length; i++) {
        if (!graph.connectedPorts[all[i].id])
            return true;
    }
    return false;
}

function showPortsButton(graph, node) {
    if (!node || !node.layoutKey || graph.isMeterNode(node))
        return false;
    if (graph.isConnectedOnly(node))
        return true;
    return graph.hoveredNodeId === node.id && graph.hasUnconnectedPorts(node);
}

function togglePinned(graph, node) {
    if (!node || !node.layoutKey)
        return;
    if (graph.pinnedNodes[node.layoutKey])
        delete graph.pinnedNodes[node.layoutKey];
    else
        graph.pinnedNodes[node.layoutKey] = true;
    graph.pinnedNodes = graph.pinnedNodes;
    graph.persistPinned();
    graph.elements.canvas.requestPaint();
}

function toggleRowRect(graph, n, lay, nx, ny) {
    if (!lay.expandable)
        return null;
    return {
        x: nx,
        y: ny + graph.theme.nodeHeader + graph.theme.nodePortsTop + lay.rows * graph.theme.nodePortRow,
        w: graph.getNodeWidth(n.id),
        h: graph.theme.nodePortRow
    };
}

function findButtonAt(graph, sx, sy) {
    var topId = graph.findNodeAt(sx, sy);
    if (topId < 0)
        return null;
    var n = graph.findNodeData(topId);
    if (!n)
        return null;
    var pos = graph.nodePositions[n.id];
    if (!pos)
        return null;
    var c = graph.toCanvas(sx, sy);
    var pb = graph.pinRect(n, pos.x, pos.y);
    if (c.x >= pb.x && c.x <= pb.x + pb.w && c.y >= pb.y && c.y <= pb.y + pb.h)
        return {
            button: "pin",
            nodeId: n.id
        };
    if (graph.showPortsButton(n)) {
        var fb = graph.portsRect(n, pos.x, pos.y);
        if (c.x >= fb.x && c.x <= fb.x + fb.w && c.y >= fb.y && c.y <= fb.y + fb.h)
            return {
                button: "ports",
                nodeId: n.id
            };
    }
    var lay = graph.nodeLayout(n);
    var tr = graph.toggleRowRect(n, lay, pos.x, pos.y);
    if (tr && c.x >= tr.x && c.x <= tr.x + tr.w && c.y >= tr.y && c.y <= tr.y + tr.h)
        return {
            button: "toggle",
            nodeId: n.id
        };
    if (n.type !== "Plugin" && n.type !== "Subgraph")
        return null;
    if (graph.isMeterNode(n))
        return null;
    if (n.type === "Plugin") {
        var pr = graph.presetRects(n, pos.x, pos.y, graph.calculateNodeHeight(n));
        if (graph.inRect(c, pr.name))
            return {
                button: "preset",
                nodeId: n.id
            };
        if (graph.inRect(c, pr.save))
            return {
                button: "presetsave",
                nodeId: n.id
            };
    }
    var btns = graph.footerButtons(n, pos.x, pos.y, graph.calculateNodeHeight(n));
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
                    enabled: graph.isNodeEnabled(n)
                };
            if (b.key === "bypass")
                return {
                    button: "bypass",
                    nodeId: n.id,
                    bypassed: graph.isNodeBypassed(n)
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

function linkHit(graph, sx, sy) {
    var c = graph.toCanvas(sx, sy);
    var threshold = graph.linkHitPx / graph.zoom;
    var bestId = -1;
    var bestDist = threshold + 1;

    for (var li = 0; li < graph.links.length; li++) {
        var link = graph.links[li];
        var fromPos = graph.portPositions[link.outputPortId];
        var toPos = graph.portPositions[link.inputPortId];
        if (!fromPos || !toPos || graph.isSnapInternal(fromPos, toPos))
            continue;
        var wb = graph.wireBounds(fromPos.cx, fromPos.cy, toPos.cx, toPos.cy, link.id);
        if (c.x < wb.x0 - threshold || c.x > wb.x1 + threshold || c.y < wb.y0 - threshold || c.y > wb.y1 + threshold)
            continue;
        var dist = graph.distToWire(c.x, c.y, fromPos.cx, fromPos.cy, toPos.cx, toPos.cy, link.id);
        if (dist < bestDist) {
            bestDist = dist;
            bestId = link.id;
        }
    }
    return bestId >= 0 && bestDist <= threshold ? {
        id: bestId,
        dist: bestDist
    } : null;
}

function findLinkAt(graph, sx, sy) {
    var h = graph.linkHit(sx, sy);
    return h ? h.id : -1;
}

function findLinkUnderNode(graph, nodeId) {
    var pos = graph.nodePositions[nodeId];
    if (!pos)
        return -1;
    var node = graph.findNodeData(nodeId);
    if (!node)
        return -1;
    var nw = graph.getNodeWidth(nodeId);
    var nh = graph.calculateNodeHeight(node);
    var nx = pos.x;
    var ny = pos.y;

    var bestId = -1;
    var bestDist = Infinity;

    for (var li = 0; li < graph.links.length; li++) {
        var link = graph.links[li];
        if (link.outputPortId === undefined)
            continue;
        if (link.outputNodeId === nodeId || link.inputNodeId === nodeId)
            continue;
        if (graph.portMediaTypes[link.outputPortId] !== "Audio" || graph.portMediaTypes[link.inputPortId] !== "Audio")
            continue;
        var fromPos = graph.portPositions[link.outputPortId];
        var toPos = graph.portPositions[link.inputPortId];
        if (!fromPos || !toPos || graph.isSnapInternal(fromPos, toPos))
            continue;
        if (fromPos.nodeId === nodeId || toPos.nodeId === nodeId)
            continue;
        var samples = graph.wireSamples(fromPos.cx, fromPos.cy, toPos.cx, toPos.cy, link.id, 8);
        for (var i = 0; i < samples.length; i++) {
            var bx = samples[i][0];
            var by = samples[i][1];
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

function distToWire(graph, px, py, x1, y1, x2, y2, wid) {
    var pts = graph.wirePolyline(x1, y1, x2, y2, wid);
    var minSq = Infinity;
    for (var i = 1; i < pts.length; i++) {
        var ax = pts[i - 1][0], ay = pts[i - 1][1];
        var sx = pts[i][0] - ax, sy = pts[i][1] - ay;
        var len = sx * sx + sy * sy;
        var k = len > 0 ? Math.max(0, Math.min(1, ((px - ax) * sx + (py - ay) * sy) / len)) : 0;
        var dx = px - (ax + k * sx);
        var dy = py - (ay + k * sy);
        minSq = Math.min(minSq, dx * dx + dy * dy);
    }
    return Math.sqrt(minSq);
}

function wireIntersectsRect(graph, x1, y1, x2, y2, wid, rx, ry, rw, rh) {
    var samples = graph.wireSamples(x1, y1, x2, y2, wid, 6);
    for (var i = 0; i < samples.length; i++) {
        var px = samples[i][0];
        var py = samples[i][1];
        if (px >= rx && px <= rx + rw && py >= ry && py <= ry + rh)
            return true;
    }
    return false;
}

function findLinksInRect(graph, sx1, sy1, sx2, sy2) {
    var minSx = Math.min(sx1, sx2);
    var minSy = Math.min(sy1, sy2);
    var maxSx = Math.max(sx1, sx2);
    var maxSy = Math.max(sy1, sy2);
    var c1 = graph.toCanvas(minSx, minSy);
    var c2 = graph.toCanvas(maxSx, maxSy);
    var rx = c1.x;
    var ry = c1.y;
    var rw = c2.x - c1.x;
    var rh = c2.y - c1.y;

    var result = {};
    for (var li = 0; li < graph.links.length; li++) {
        var link = graph.links[li];
        var fromPos = graph.portPositions[link.outputPortId];
        var toPos = graph.portPositions[link.inputPortId];
        if (fromPos && toPos && !graph.isSnapInternal(fromPos, toPos)) {
            if (graph.wireIntersectsRect(fromPos.cx, fromPos.cy, toPos.cx, toPos.cy, link.id, rx, ry, rw, rh)) {
                result[link.id] = graph.linkSig(link);
            }
        }
    }
    return result;
}

function findNodesInRect(graph, sx1, sy1, sx2, sy2) {
    var c1 = graph.toCanvas(Math.min(sx1, sx2), Math.min(sy1, sy2));
    var c2 = graph.toCanvas(Math.max(sx1, sx2), Math.max(sy1, sy2));
    var rx = c1.x;
    var ry = c1.y;
    var rr = c2.x;
    var rb = c2.y;

    var result = {};
    for (var i = 0; i < graph.nodes.length; i++) {
        var n = graph.nodes[i];
        if (n.layoutKey && graph.hiddenNodes[n.layoutKey])
            continue;
        var pos = graph.nodePositions[n.id];
        if (!pos)
            continue;
        var h = graph.calculateNodeHeight(n);
        var nx = pos.x;
        var ny = pos.y;
        var nr = nx + graph.getNodeWidth(n.id);
        var nb = ny + h;
        if (nx < rr && nr > rx && ny < rb && nb > ry) {
            result[n.id] = graph.nodeSig(n);
        }
    }
    return result;
}
