function paint(graph) {
    var ctx = graph.elements.canvas.getContext("2d");
    ctx.reset();

    if (graph.widthsDirty) {
        graph.calculateNodeWidths();
        graph.widthsDirty = false;
    }

    ctx.fillStyle = "" + graph.theme.bgCanvas;
    ctx.fillRect(0, 0, graph.width, graph.height);

    ctx.save();
    ctx.translate(graph.panX, graph.panY);
    ctx.scale(graph.zoom, graph.zoom);

    var visLeft = -graph.panX / graph.zoom;
    var visTop = -graph.panY / graph.zoom;
    var visRight = (graph.width - graph.panX) / graph.zoom;
    var visBottom = (graph.height - graph.panY) / graph.zoom;

    var pitch = graph.gridSize;
    while (pitch * graph.zoom < 10)
        pitch *= 2;
    var dot = 1.5 / graph.zoom;
    ctx.fillStyle = "" + graph.theme.gridDot;
    var gx0 = Math.floor(visLeft / pitch) * pitch;
    var gy0 = Math.floor(visTop / pitch) * pitch;
    for (var gx = gx0; gx <= visRight; gx += pitch) {
        for (var gy = gy0; gy <= visBottom; gy += pitch) {
            ctx.fillRect(gx, gy, dot, dot);
        }
    }

    var newPortPositions = {};
    var hdr = graph.theme.nodeHeader;
    var rowH = graph.theme.nodePortRow;
    var portsTop = hdr + graph.theme.nodePortsTop;

    for (var ni = 0; ni < graph.nodes.length; ni++) {
        var node = graph.nodes[ni];
        if (node.layoutKey && graph.hiddenNodes[node.layoutKey])
            continue;
        var pos = graph.nodePositions[node.id];
        if (!pos)
            continue;
        var lay = graph.nodeLayout(node);
        var nw = graph.getNodeWidth(node.id);
        var labelBoth = lay.inputs.length > 0 && lay.outputs.length > 0;
        var labelWidth = labelBoth ? (nw - 2 * graph.theme.nodePadX - 8) / 2 : nw - 2 * graph.theme.nodePadX;
        for (var pi = 0; pi < lay.inputs.length; pi++) {
            newPortPositions[lay.inputs[pi].id] = {
                cx: pos.x,
                cy: pos.y + portsTop + pi * rowH + rowH / 2,
                nodeId: node.id,
                z: ni,
                labelX0: pos.x + graph.theme.nodePadX,
                labelX1: pos.x + graph.theme.nodePadX + labelWidth
            };
        }
        for (var po = 0; po < lay.outputs.length; po++) {
            newPortPositions[lay.outputs[po].id] = {
                cx: pos.x + nw,
                cy: pos.y + portsTop + po * rowH + rowH / 2,
                nodeId: node.id,
                z: ni,
                labelX0: pos.x + nw - graph.theme.nodePadX - labelWidth,
                labelX1: pos.x + nw - graph.theme.nodePadX
            };
        }
    }

    var newDockFrames = {};
    var newDockIds = [];
    var newRedirect = {};
    var bypassLinks = {};
    for (var dmi = 0; dmi < graph.nodes.length; dmi++) {
        var dm = graph.nodes[dmi];
        if (!graph.isDockedMeter(dm))
            continue;
        var toSource = graph.dockedMeters[dm.layoutKey] === "source";
        var dchans = graph.meterChannelPorts(dm);
        for (var dk = 0; dk < dchans.length; dk++) {
            var dc = dchans[dk];
            var anchorId = toSource ? dc.src : (dc.dests.length > 0 ? dc.dests[0] : -1);
            var meterPort = toSource ? dc.outPort : dc.inPort;
            var anchor = newPortPositions[anchorId];
            if (!anchor || anchor.alias || meterPort < 0)
                continue;
            newPortPositions[meterPort] = {
                cx: anchor.cx,
                cy: anchor.cy,
                nodeId: anchor.nodeId,
                z: anchor.z,
                alias: true
            };
            var otherPort = toSource ? dc.inPort : dc.outPort;
            if (otherPort >= 0 && (toSource ? dc.srcs.length : dc.dests.length) > 1)
                newPortPositions[otherPort] = newPortPositions[meterPort];
            if (newRedirect[anchorId] === undefined)
                newRedirect[anchorId] = meterPort;
            for (var bl = 0; bl < graph.links.length; bl++) {
                var blk = graph.links[bl];
                if (toSource ? (blk.outputPortId === anchorId && blk.inputPortId !== dc.inPort) : (blk.inputPortId === anchorId && blk.outputPortId !== dc.outPort))
                    bypassLinks[blk.id] = true;
            }
            var dkey = dm.id + ":" + dk;
            newDockIds.push(dkey);
            newDockFrames[dkey] = {
                meterId: dm.id,
                meterKey: dm.layoutKey,
                channel: dk,
                nodeId: anchor.nodeId,
                output: toSource,
                cx: anchor.cx,
                cy: anchor.cy
            };
        }
    }

    ctx.lineCap = "round";
    graph.wireLanes = graph.computeWireLanes(newPortPositions);
    graph.updateWireRoutes(newPortPositions);
    var wireNodeColors = {};
    if (graph.gradientWires) {
        for (var wn = 0; wn < graph.nodes.length; wn++)
            wireNodeColors[graph.nodes[wn].id] = graph.nodeColor(graph.nodes[wn]);
    }
    for (var li = 0; li < graph.links.length; li++) {
        var link = graph.links[li];
        var fromPos = newPortPositions[link.outputPortId];
        var toPos = newPortPositions[link.inputPortId];
        if (!fromPos || !toPos)
            continue;
        if (graph.isSnapInternal(fromPos, toPos))
            continue;
        var accented = !!graph.selectedLinks[link.id] || !!graph.selectedNodes[fromPos.nodeId] || !!graph.selectedNodes[toPos.nodeId];
        var hovered = link.id === graph.hoveredLinkId;
        var mt = graph.portMediaTypes[link.outputPortId];
        if (!mt || mt === "Unknown")
            mt = graph.portMediaTypes[link.inputPortId];
        if (graph.insertPreview && graph.insertPreview.remove[link.id]) {
            ctx.save();
            ctx.globalAlpha = 0.3;
            ctx.setLineDash([3, 5]);
            graph.drawWire(ctx, fromPos.cx, fromPos.cy, toPos.cx, toPos.cy, graph.signalColor(mt), graph.theme.wireWidth, link.id);
            ctx.restore();
            continue;
        }
        ctx.globalAlpha = accented || hovered ? 1 : (link.active === false ? 0.35 : graph.theme.wireOpacity);
        if (bypassLinks[link.id])
            ctx.setLineDash([5, 4]);
        var wireCol = accented ? graph.theme.accent : graph.signalColor(mt);
        if (graph.gradientWires && !accented && mt === "Audio" && wireNodeColors[fromPos.nodeId] && wireNodeColors[toPos.nodeId]) {
            wireCol = ctx.createLinearGradient(fromPos.cx, fromPos.cy, toPos.cx, toPos.cy);
            wireCol.addColorStop(0, "" + wireNodeColors[fromPos.nodeId]);
            wireCol.addColorStop(1, "" + wireNodeColors[toPos.nodeId]);
        }
        graph.drawWire(ctx, fromPos.cx, fromPos.cy, toPos.cx, toPos.cy, wireCol, accented || hovered ? graph.theme.wireWidthActive : graph.theme.wireWidth, link.id);
        if (bypassLinks[link.id])
            ctx.setLineDash([]);
    }
    ctx.globalAlpha = 1;

    var newBlockGeom = [];
    for (var bi = 0; bi < graph.blockedPairs.length; bi++) {
        var bp = graph.blockedPairs[bi];
        var bsPos = graph.nodePositions[bp.source];
        var btPos = graph.nodePositions[bp.target];
        if (!bsPos || !btPos)
            continue;
        var bsNode = graph.findNodeData(bp.source);
        var btNode = graph.findNodeData(bp.target);
        if (!bsNode || !btNode)
            continue;
        if ((bsNode.layoutKey && graph.hiddenNodes[bsNode.layoutKey]) || (btNode.layoutKey && graph.hiddenNodes[btNode.layoutKey]))
            continue;
        var bx1 = bsPos.x + (graph.getNodeWidth(bp.source) || graph.minNodeWidth);
        var by1 = bsPos.y + 17;
        var bx2 = btPos.x;
        var by2 = btPos.y + 17;
        var bWid = "b|" + graph.blockKey(bp);
        graph.routeExtraWire(bWid, bx1, by1, bx2, by2);
        var bMid = graph.wireMidpoint(bx1, by1, bx2, by2, bWid);
        var bmx = bMid.x;
        var bmy = bMid.y;
        var bKey = graph.blockKey(bp);
        var bSel = !!graph.selectedBlocks[bKey];
        var bHot = bSel || graph.hoveredBlockKey === bKey || (graph.contextBlock && graph.contextBlock.ruleId === bp.ruleId && graph.contextBlock.source === bp.source && graph.contextBlock.target === bp.target);
        ctx.save();
        if (bSel) {
            ctx.globalAlpha = 1;
            ctx.setLineDash([]);
            graph.drawWire(ctx, bx1, by1, bx2, by2, graph.theme.tint(graph.theme.textDestructive, 0.28), 8, bWid);
        }
        ctx.globalAlpha = bHot ? 1 : 0.8;
        ctx.setLineDash([6, 4]);
        graph.drawWire(ctx, bx1, by1, bx2, by2, graph.theme.textDestructive, bHot ? graph.theme.wireWidthActive + (bSel ? 0.5 : 0) : graph.theme.wireWidth, bWid);
        ctx.setLineDash([]);
        ctx.globalAlpha = 1;
        var bR = bHot ? 8.5 : 7;
        ctx.beginPath();
        ctx.arc(bmx, bmy, bR, 0, Math.PI * 2);
        ctx.fillStyle = "" + graph.theme.bgCanvas;
        ctx.fill();
        if (bHot) {
            ctx.fillStyle = "" + graph.theme.tint(graph.theme.textDestructive, bSel ? 0.35 : 0.18);
            ctx.fill();
        }
        ctx.strokeStyle = "" + graph.theme.textDestructive;
        ctx.lineWidth = bHot ? 2 : 1.5;
        ctx.stroke();
        var bK = bHot ? 5.2 : 4.5;
        ctx.beginPath();
        ctx.moveTo(bmx - bK, bmy + bK);
        ctx.lineTo(bmx + bK, bmy - bK);
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
    graph.blockGeom = newBlockGeom;

    var newMeterFrames = {};
    for (var ni2 = 0; ni2 < graph.nodes.length; ni2++) {
        var n2 = graph.nodes[ni2];
        if (n2.layoutKey && graph.hiddenNodes[n2.layoutKey])
            continue;
        var p2 = graph.nodePositions[n2.id];
        if (!p2)
            continue;
        graph.drawNode(ctx, n2, p2.x, p2.y);
        if (graph.isMeterNode(n2))
            newMeterFrames[n2.id] = {
                sx: graph.panX + p2.x * graph.zoom,
                sy: graph.panY + p2.y * graph.zoom,
                w: graph.getNodeWidth(n2.id),
                h: graph.calculateNodeHeight(n2),
                zoom: graph.zoom,
                alpha: (graph.nodeMatchesSearch(n2) ? 1 : 0.35) * (graph.snapPreview && graph.snapPreview.meterId === n2.id ? 0.45 : 1),
                channels: n2.meterChannels,
                rows: graph.meterRows(n2)
            };
    }

    if (graph.snapPreview) {
        ctx.save();
        ctx.fillStyle = "" + graph.theme.accent;
        for (var spi = 0; spi < graph.snapPreview.ports.length; spi++) {
            var spp = newPortPositions[graph.snapPreview.ports[spi]];
            if (!spp)
                continue;
            var spr = graph.dockBarRect(spp.cx, spp.cy, graph.snapPreview.side === "source");
            graph.roundRectPath(ctx, spr.x, spr.y, spr.w, spr.h, spr.h / 2);
            ctx.fill();
        }
        ctx.restore();
    }

    if (graph.insertPreview) {
        ctx.save();
        ctx.setLineDash([6, 4]);
        for (var gi = 0; gi < graph.insertPreview.add.length; gi++) {
            var gOut = graph.insertPreview.add[gi][0];
            var gIn = graph.insertPreview.add[gi][1];
            var gFrom = newPortPositions[gOut];
            var gTo = newPortPositions[gIn];
            if (!gFrom || !gTo)
                continue;
            var gmt = graph.portMediaTypes[gOut];
            if (!gmt || gmt === "Unknown")
                gmt = graph.portMediaTypes[gIn];
            ctx.globalAlpha = 0.85;
            graph.drawWire(ctx, gFrom.cx, gFrom.cy, gTo.cx, gTo.cy, graph.signalColor(gmt), graph.theme.wireWidthActive);
            ctx.globalAlpha = 1;
            ctx.fillStyle = "" + graph.signalColor(gmt);
            ctx.beginPath();
            ctx.arc(gFrom.cx, gFrom.cy, 3, 0, Math.PI * 2);
            ctx.moveTo(gTo.cx + 3, gTo.cy);
            ctx.arc(gTo.cx, gTo.cy, 3, 0, Math.PI * 2);
            ctx.fill();
        }
        ctx.restore();
    }

    if (graph.connectFromPortId >= 0) {
        var dragFrom = newPortPositions[graph.connectFromPortId] || graph.portPositions[graph.connectFromPortId];
        if (dragFrom) {
            var dragToC = graph.toCanvas(graph.connectMouseX, graph.connectMouseY);
            var dragColor = graph.connectBlocking || graph.connectTargetBlocked ? graph.theme.textDestructive : graph.signalColor(graph.portMediaTypes[graph.connectFromPortId]);
            ctx.save();
            ctx.setLineDash([4, 4]);
            if (graph.connectFromDir === "Input") {
                graph.routeExtraWire("drag", dragToC.x, dragToC.y, dragFrom.cx, dragFrom.cy);
                graph.drawWire(ctx, dragToC.x, dragToC.y, dragFrom.cx, dragFrom.cy, dragColor, graph.theme.wireWidthActive, "drag");
            } else {
                graph.routeExtraWire("drag", dragFrom.cx, dragFrom.cy, dragToC.x, dragToC.y);
                graph.drawWire(ctx, dragFrom.cx, dragFrom.cy, dragToC.x, dragToC.y, dragColor, graph.theme.wireWidthActive, "drag");
            }
            if (graph.connectBlocking || graph.connectTargetBlocked) {
                ctx.setLineDash([]);
                ctx.strokeStyle = "" + graph.theme.textDestructive;
                ctx.lineWidth = 1.5 / graph.zoom;
                var br = 6 / graph.zoom;
                ctx.beginPath();
                ctx.arc(dragToC.x, dragToC.y, br, 0, Math.PI * 2);
                ctx.moveTo(dragToC.x - br * 0.7, dragToC.y + br * 0.7);
                ctx.lineTo(dragToC.x + br * 0.7, dragToC.y - br * 0.7);
                ctx.stroke();
            }
            ctx.restore();
        }
    }

    if (graph.selectDragging) {
        var sc1 = graph.toCanvas(graph.selectStartX, graph.selectStartY);
        var sc2 = graph.toCanvas(graph.selectEndX, graph.selectEndY);
        ctx.beginPath();
        ctx.rect(Math.min(sc1.x, sc2.x), Math.min(sc1.y, sc2.y), Math.abs(sc2.x - sc1.x), Math.abs(sc2.y - sc1.y));
        ctx.fillStyle = "" + graph.theme.accentFaint;
        ctx.fill();
        ctx.strokeStyle = "" + graph.theme.accent;
        ctx.lineWidth = 1 / graph.zoom;
        ctx.stroke();
    }

    if (graph.activeSnapLines.length > 0) {
        ctx.save();
        ctx.setLineDash([4 / graph.zoom, 4 / graph.zoom]);
        ctx.strokeStyle = "" + graph.theme.textMuted;
        ctx.lineWidth = 1 / graph.zoom;
        for (var si = 0; si < graph.activeSnapLines.length; si++) {
            var sl = graph.activeSnapLines[si];
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
    graph.portPositions = newPortPositions;
    graph.meterFrames = newMeterFrames;
    graph.dockBarFrames = newDockFrames;
    graph.snapRedirect = newRedirect;
    if (newDockIds.join(",") !== graph.dockBarIds.join(","))
        graph.dockBarIds = newDockIds;
    graph.updateScrollBars();
}
