function toCanvas(graph, sx, sy) {
    return {
        x: (sx - graph.panX) / graph.zoom,
        y: (sy - graph.panY) / graph.zoom
    };
}

function toScreen(graph, cx, cy) {
    return {
        x: cx * graph.zoom + graph.panX,
        y: cy * graph.zoom + graph.panY
    };
}

function contentBounds(graph) {
    var b = null;
    for (var i = 0; i < graph.nodes.length; i++) {
        var node = graph.nodes[i];
        if (node.layoutKey && graph.hiddenNodes[node.layoutKey])
            continue;
        var pos = graph.nodePositions[node.id];
        if (!pos)
            continue;
        var r = pos.x + graph.getNodeWidth(node.id);
        var btm = pos.y + graph.calculateNodeHeight(node);
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

function clampPan(graph) {
    var b = graph.contentBounds();
    if (!b || graph.width <= 0 || graph.height <= 0)
        return;
    var keepX = Math.min(graph.minVisibleContent, (b.right - b.left) * graph.zoom, graph.width);
    var keepY = Math.min(graph.minVisibleContent, (b.bottom - b.top) * graph.zoom, graph.height);
    var l = b.left * graph.zoom + graph.panX, r = b.right * graph.zoom + graph.panX;
    var t = b.top * graph.zoom + graph.panY, btm = b.bottom * graph.zoom + graph.panY;
    if (r < keepX)
        graph.panX += keepX - r;
    else if (l > graph.width - keepX)
        graph.panX -= l - (graph.width - keepX);
    if (btm < keepY)
        graph.panY += keepY - btm;
    else if (t > graph.height - keepY)
        graph.panY -= t - (graph.height - keepY);
}

function zoomAt(graph, sx, sy, newZoom) {
    graph.hideNameTip();
    newZoom = Math.max(graph.minZoom, Math.min(graph.maxZoom, newZoom));
    var c = graph.toCanvas(sx, sy);
    graph.panX += c.x * (graph.zoom - newZoom);
    graph.panY += c.y * (graph.zoom - newZoom);
    graph.zoom = newZoom;
    graph.clampPan();
    graph.elements.canvas.requestPaint();
}

function fitToView(graph) {
    graph.fitBounds(graph.contentBounds());
}

function frameSelection(graph) {
    var b = null;
    for (var k in graph.selectedNodes) {
        var node = graph.findNodeData(parseInt(k));
        var pos = node ? graph.nodePositions[node.id] : null;
        if (!pos || (node.layoutKey && graph.hiddenNodes[node.layoutKey]))
            continue;
        var r = pos.x + graph.getNodeWidth(node.id);
        var btm = pos.y + graph.calculateNodeHeight(node);
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
    graph.fitBounds(b || graph.contentBounds());
}

function fitBounds(graph, b) {
    if (!b || graph.width <= 0 || graph.height <= 0) {
        graph.zoom = 1.0;
        graph.panX = 0;
        graph.panY = 0;
        graph.elements.canvas.requestPaint();
        return;
    }
    var margin = 40;
    var top = 58, bottom = 52;
    var bw = Math.max(b.right - b.left, 1);
    var bh = Math.max(b.bottom - b.top, 1);
    var availH = Math.max(graph.height - top - bottom, 1);
    var z = Math.min((graph.width - 2 * margin) / bw, availH / bh, 1.0);
    graph.zoom = Math.max(graph.minZoom, z);
    graph.panX = graph.width / 2 - (b.left + bw / 2) * graph.zoom;
    graph.panY = top + availH / 2 - (b.top + bh / 2) * graph.zoom;
    graph.elements.canvas.requestPaint();
}

function scrollRange(graph, lo, hi, viewLo, viewLen) {
    var pad = 40 / graph.zoom;
    var a = Math.min(lo - pad, viewLo);
    var e = Math.max(hi + pad, viewLo + viewLen);
    return {
        lo: a,
        len: e - a
    };
}

function updateScrollBars(graph) {
    var b = graph.contentBounds();
    var viewW = graph.width / graph.zoom, viewH = graph.height / graph.zoom;
    var viewL = -graph.panX / graph.zoom, viewT = -graph.panY / graph.zoom;
    if (!b) {
        graph.elements.hScrollBar.size = 1;
        graph.elements.vScrollBar.size = 1;
        return;
    }
    if (!graph.elements.hScrollBar.pressed) {
        var hr = graph.scrollRange(b.left, b.right, viewL, viewW);
        graph.elements.hScrollBar.rangeLo = hr.lo;
        graph.elements.hScrollBar.rangeLen = hr.len;
        graph.elements.hScrollBar.size = Math.min(1, viewW / hr.len);
        graph.elements.hScrollBar.position = (viewL - hr.lo) / hr.len;
    }
    if (!graph.elements.vScrollBar.pressed) {
        var vr = graph.scrollRange(b.top, b.bottom, viewT, viewH);
        graph.elements.vScrollBar.rangeLo = vr.lo;
        graph.elements.vScrollBar.rangeLen = vr.len;
        graph.elements.vScrollBar.size = Math.min(1, viewH / vr.len);
        graph.elements.vScrollBar.position = (viewT - vr.lo) / vr.len;
    }
}

function quietCues(graph) {
    graph.cueQuiet = true;
    graph.nodeCues = {};
    graph.edgeMarkers = [];
    graph.elements.cueQuietTimer.restart();
}

function noteAppeared(graph, ids) {
    if (ids.length === 0)
        return;
    if (graph.cueQuiet) {
        graph.elements.cueQuietTimer.restart();
        return;
    }
    if (ids.length > 4)
        return;
    var now = Date.now();
    var added = false;
    for (var i = 0; i < ids.length; i++) {
        var n = graph.findNodeData(ids[i]);
        if (!n || ids[i] === graph.placingNodeId || (n.layoutKey && graph.hiddenNodes[n.layoutKey]))
            continue;
        graph.nodeCues[ids[i]] = {
            born: now,
            t: now
        };
        added = true;
    }
    if (added) {
        graph.nodeCues = graph.nodeCues;
        graph.elements.cueTimer.start();
    }
}

function nodeOnScreen(graph, node) {
    var p = graph.nodePositions[node.id];
    if (!p)
        return false;
    var a = graph.toScreen(p.x, p.y);
    var b = graph.toScreen(p.x + graph.getNodeWidth(node.id), p.y + graph.calculateNodeHeight(node));
    return b.x > 0 && a.x < graph.width && b.y > 0 && a.y < graph.height;
}

function updateCues(graph) {
    var now = Date.now();
    var markers = [];
    var pulsing = false;
    var left = 0;
    for (var id in graph.nodeCues) {
        var cue = graph.nodeCues[id];
        var n = graph.findNodeData(parseInt(id));
        if (!n || (n.layoutKey && graph.hiddenNodes[n.layoutKey])) {
            delete graph.nodeCues[id];
            continue;
        }
        var onScreen = graph.nodeOnScreen(n);
        if (onScreen ? now - cue.t > graph.cuePulseMs : now - cue.born > graph.cueMarkerMs) {
            delete graph.nodeCues[id];
            continue;
        }
        left++;
        if (onScreen) {
            pulsing = true;
            continue;
        }
        cue.t = now;
        var p = graph.nodePositions[n.id];
        var c = graph.toScreen(p.x + graph.getNodeWidth(n.id) / 2, p.y + graph.calculateNodeHeight(n) / 2);
        var mx = Math.max(90, Math.min(graph.width - 90, c.x));
        var my = Math.max(70, Math.min(graph.height - 64, c.y));
        markers.push({
            id: n.id,
            name: n.name || "",
            x: Math.round(mx),
            y: Math.round(my),
            angle: Math.round(Math.atan2(c.y - my, c.x - mx) * 180 / Math.PI)
        });
    }
    if (JSON.stringify(markers) !== JSON.stringify(graph.edgeMarkers))
        graph.edgeMarkers = markers;
    if (pulsing)
        graph.elements.canvas.requestPaint();
    if (left === 0)
        graph.elements.cueTimer.stop();
}

function revealNode(graph, nodeId) {
    var n = graph.findNodeData(nodeId);
    var p = n ? graph.nodePositions[nodeId] : null;
    if (!p)
        return;
    graph.panX = graph.width / 2 - (p.x + graph.getNodeWidth(nodeId) / 2) * graph.zoom;
    graph.panY = graph.height / 2 - (p.y + graph.calculateNodeHeight(n) / 2) * graph.zoom;
    graph.clampPan();
    graph.nodeCues[nodeId] = {
        born: Date.now(),
        t: Date.now()
    };
    graph.nodeCues = graph.nodeCues;
    graph.elements.cueTimer.start();
    graph.elements.canvas.requestPaint();
}
