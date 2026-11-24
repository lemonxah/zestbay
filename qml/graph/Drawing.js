function fontSpec(graph, weight, size, mono) {
    return weight + " " + size + "px \"" + (mono ? graph.theme.fontMono : graph.theme.fontSans) + "\"";
}

function elide(graph, ctx, str, maxW) {
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

function drawGlyph(graph, ctx, name, x, y, size, color) {
    var g = graph.glyphs.icons[name];
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

function roundRectPath(graph, ctx, x, y, w, h, r) {
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

function drawTag(graph, ctx, text, right, cy) {
    ctx.font = graph.fontSpec(500, graph.theme.sizeNodeTag, true);
    var tw = ctx.measureText(text).width + 8;
    var th = 14;
    graph.roundRectPath(ctx, right - tw, cy - th / 2, tw, th, graph.theme.radiusTag);
    ctx.strokeStyle = "" + graph.theme.borderStrong;
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.fillStyle = "" + graph.theme.textMuted;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(text, right - tw / 2, cy + 0.5);
    return right - tw;
}

function drawDefaultChip(graph, ctx, right, cy) {
    ctx.font = graph.fontSpec(500, graph.theme.sizeNodeTag, true);
    var label = "default";
    var tw = ctx.measureText(label).width + 8 + 9;
    var th = 16;
    graph.roundRectPath(ctx, right - tw, cy - th / 2, tw, th, graph.theme.radiusTag);
    ctx.fillStyle = "" + graph.theme.accentFaint;
    ctx.fill();
    ctx.beginPath();
    ctx.arc(right - tw + 7, cy, 2.5, 0, Math.PI * 2);
    ctx.fillStyle = "" + graph.theme.accent;
    ctx.fill();
    ctx.textAlign = "left";
    ctx.textBaseline = "middle";
    ctx.fillText(label, right - tw + 12, cy + 0.5);
    return right - tw;
}

function drawNode(graph, ctx, node, nx, ny) {
    var lay = graph.nodeLayout(node);
    var nw = graph.getNodeWidth(node.id);
    var nh = graph.calculateNodeHeight(node);
    var isPlugin = node.type === "Plugin";
    var isMeter = graph.isMeterNode(node);
    var isSubgraph = node.type === "Subgraph";
    var selected = !!graph.selectedNodes[node.id];
    var isDefault = graph.defaultNodeKey !== "" && node.layoutKey === graph.defaultNodeKey;
    var cat = graph.nodeCategory(node);
    var custom = graph.customNodeColor(node);
    if (custom)
        cat = {
            color: custom,
            glyph: cat.glyph
        };
    var hdr = graph.theme.nodeHeader;
    var rowH = graph.theme.nodePortRow;
    var portsTop = hdr + graph.theme.nodePortsTop;

    ctx.save();
    var baseAlpha = graph.nodeMatchesSearch(node) ? 1 : 0.35;
    if (graph.snapPreview && graph.snapPreview.meterId === node.id)
        baseAlpha *= 0.45;

    ctx.fillStyle = "" + graph.theme.shadowNode;
    ctx.globalAlpha = baseAlpha * 0.35;
    for (var sh = 0; sh < 3; sh++) {
        graph.roundRectPath(ctx, nx - sh * 2, ny - sh * 2 + graph.theme.shadowNodeY * 0.5, nw + sh * 4, nh + sh * 4, graph.theme.radiusCard + sh * 2);
        ctx.fill();
    }
    ctx.globalAlpha = baseAlpha;
    if (selected) {
        graph.roundRectPath(ctx, nx - 2.5, ny - 2.5, nw + 5, nh + 5, graph.theme.radiusCard + 2.5);
        ctx.strokeStyle = "" + graph.theme.accentRing;
        ctx.lineWidth = 3;
        ctx.stroke();
    }
    var cue = graph.nodeCues[node.id];
    if (cue) {
        var cueAge = Date.now() - cue.t;
        if (cueAge >= 0 && cueAge < graph.cuePulseMs) {
            var ck = cueAge / graph.cuePulseMs;
            var grow = 3 + 13 * ck;
            ctx.globalAlpha = baseAlpha * (1 - ck);
            graph.roundRectPath(ctx, nx - grow, ny - grow, nw + 2 * grow, nh + 2 * grow, graph.theme.radiusCard + grow);
            ctx.strokeStyle = "" + graph.theme.accent;
            ctx.lineWidth = 2;
            ctx.stroke();
            ctx.globalAlpha = baseAlpha;
        }
    }

    if (isSubgraph) {
        graph.roundRectPath(ctx, nx + 4, ny + 4, nw, nh, graph.theme.radiusCard);
        ctx.fillStyle = "" + graph.theme.bgSurface;
        ctx.fill();
        ctx.strokeStyle = "" + graph.theme.borderStrong;
        ctx.lineWidth = 1;
        ctx.stroke();
    }

    var hovered = graph.hoveredNodeId === node.id && !selected;
    graph.roundRectPath(ctx, nx, ny, nw, nh, graph.theme.radiusCard);
    ctx.fillStyle = "" + graph.theme.bgSurface;
    ctx.fill();
    if (hovered) {
        ctx.fillStyle = "" + graph.theme.accentFaint;
        ctx.fill();
    }
    ctx.strokeStyle = "" + (selected || hovered ? graph.theme.accent : graph.theme.borderStrong);
    if (hovered)
        ctx.globalAlpha = baseAlpha * 0.7;
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.globalAlpha = baseAlpha;
    if (graph.snapPreview && graph.snapPreview.nodeId === node.id) {
        graph.roundRectPath(ctx, nx, ny, nw, nh, graph.theme.radiusCard);
        ctx.fillStyle = "" + graph.theme.accentFaint;
        ctx.fill();
        graph.roundRectPath(ctx, nx - 3, ny - 3, nw + 6, nh + 6, graph.theme.radiusCard + 3);
        ctx.strokeStyle = "" + graph.theme.accent;
        ctx.lineWidth = 2;
        ctx.stroke();
    }

    if (custom) {
        ctx.save();
        graph.roundRectPath(ctx, nx, ny, nw, nh, graph.theme.radiusCard);
        ctx.clip();
        ctx.fillStyle = "" + graph.theme.tint(custom, graph.theme.chipAlpha * 0.6);
        ctx.fillRect(nx, ny, nw, hdr);
        ctx.fillStyle = "" + custom;
        ctx.fillRect(nx, ny, 3, nh);
        ctx.restore();
    }

    var chip = graph.theme.nodeChip;
    var chipX = nx + 10;
    var chipY = ny + (hdr - chip) / 2;
    graph.roundRectPath(ctx, chipX, chipY, chip, chip, graph.theme.radiusChip);
    ctx.fillStyle = "" + graph.theme.tint(cat.color, graph.theme.chipAlpha);
    ctx.fill();
    graph.drawGlyph(ctx, cat.glyph, chipX + (chip - graph.theme.nodeGlyph) / 2, chipY + (chip - graph.theme.nodeGlyph) / 2, graph.theme.nodeGlyph, cat.color);

    var pinned = !!(node.layoutKey && graph.pinnedNodes[node.layoutKey]);
    var pinHover = graph.hoveredButton === node.id + ":pin";
    var pr = graph.pinRect(node, nx, ny);
    var pinCol = pinned || pinHover ? graph.theme.textIcon : graph.theme.textDisabled;
    ctx.save();
    if (!pinned && !pinHover)
        ctx.globalAlpha = baseAlpha * 0.6;
    if (pinned) {
        ctx.save();
        ctx.translate(pr.x + 3, pr.y + 3);
        ctx.beginPath();
        ctx.path = graph.glyphs.icons["pinFill"].d;
        ctx.fillStyle = "" + pinCol;
        ctx.fill();
        ctx.restore();
    }
    graph.drawGlyph(ctx, "pin", pr.x + 3, pr.y + 3, 12, pinCol);
    ctx.restore();

    var tagRight = pr.x - 4;
    if (graph.showPortsButton(node)) {
        var fr = graph.portsRect(node, nx, ny);
        var filtering = graph.isConnectedOnly(node);
        var filterHover = graph.hoveredButton === node.id + ":ports";
        ctx.save();
        if (!filtering && !filterHover)
            ctx.globalAlpha = baseAlpha * 0.6;
        graph.drawGlyph(ctx, "portsFilter", fr.x + 3, fr.y + 3, 12, filtering ? graph.theme.accent : (filterHover ? graph.theme.textIcon : graph.theme.textDisabled));
        ctx.restore();
        tagRight = fr.x - 2;
    }
    if (isPlugin && node.pluginFormat && !isMeter)
        tagRight = graph.drawTag(ctx, node.pluginFormat, tagRight, ny + hdr / 2) - 6;
    if (isSubgraph) {
        var present = (node.memberCount || 0) - (node.missing || 0);
        tagRight = graph.drawTag(ctx, node.missing > 0 ? present + "/" + node.memberCount : "" + (node.memberCount || 0), tagRight, ny + hdr / 2) - 6;
        if (node.asSink || node.asSource)
            tagRight = graph.drawTag(ctx, "DEV", tagRight, ny + hdr / 2) - 6;
    }
    if (isDefault)
        tagRight = graph.drawDefaultChip(ctx, tagRight, ny + hdr / 2) - 6;

    ctx.font = graph.fontSpec(600, graph.theme.sizeNodeTitle, false);
    ctx.fillStyle = "" + graph.theme.textPrimary;
    ctx.textAlign = "left";
    ctx.textBaseline = "middle";
    var titleX = chipX + chip + 8;
    var shownTitle = graph.elide(ctx, node.name, tagRight - titleX - 2);
    graph.nameTruncated[node.id] = shownTitle !== node.name;
    ctx.fillText(shownTitle, titleX, ny + hdr / 2 + 0.5);

    ctx.fillStyle = "" + graph.theme.borderHairline;
    ctx.fillRect(nx + 1, ny + hdr, nw - 2, 1);

    var both = lay.inputs.length > 0 && lay.outputs.length > 0;
    var labelW = both ? (nw - 2 * graph.theme.nodePadX - 8) / 2 : nw - 2 * graph.theme.nodePadX;
    ctx.font = graph.fontSpec(400, graph.theme.sizeMono, true);
    ctx.textBaseline = "middle";
    var meterChannel = 0;
    for (var i = 0; i < lay.inputs.length; i++) {
        var pin = lay.inputs[i];
        var py = ny + portsTop + i * rowH + rowH / 2;
        ctx.fillStyle = "" + graph.theme.textSecondary;
        ctx.textAlign = "left";
        var meterRow = isMeter && graph.isAudioRow(pin);
        if (meterRow)
            meterChannel++;
        ctx.fillText(meterRow ? "" + meterChannel : graph.elide(ctx, pin.name, labelW), nx + graph.theme.nodePadX, py);
        if (meterRow) {
            var mb = graph.meterBarRect(nw, i);
            graph.roundRectPath(ctx, nx + mb.x, ny + mb.y, mb.w, mb.h, mb.h / 2);
            ctx.fillStyle = "" + graph.theme.meterTrack;
            ctx.fill();
        }
        graph.drawPin(ctx, pin, node.id, nx, py, selected);
    }
    for (var o = 0; o < lay.outputs.length; o++) {
        var pout = lay.outputs[o];
        var pyo = ny + portsTop + o * rowH + rowH / 2;
        if (!isMeter) {
            ctx.font = graph.fontSpec(400, graph.theme.sizeMono, true);
            ctx.fillStyle = "" + graph.theme.textSecondary;
            ctx.textAlign = "right";
            ctx.fillText(graph.elide(ctx, pout.name, labelW), nx + nw - graph.theme.nodePadX, pyo);
        }
        graph.drawPin(ctx, pout, node.id, nx + nw, pyo, selected);
    }

    var tr = graph.toggleRowRect(node, lay, nx, ny);
    if (tr) {
        var hidden = lay.hiddenIn + lay.hiddenOut;
        var what = lay.hiddenIn > 0 && lay.hiddenOut > 0 ? "ports" : lay.hiddenOut > 0 ? (hidden === 1 ? "output" : "outputs") : (hidden === 1 ? "input" : "inputs");
        var label = lay.expanded ? "show fewer" : hidden + " more " + what;
        var hoverT = graph.hoveredButton === node.id + ":toggle";
        ctx.font = graph.fontSpec(400, graph.theme.sizeBadge, true);
        var lw = ctx.measureText(label).width + 12;
        var lx = lay.hiddenIn > 0 && lay.hiddenOut === 0 ? nx + graph.theme.nodePadX : lay.hiddenOut > 0 && lay.hiddenIn === 0 ? nx + nw - graph.theme.nodePadX - lw : nx + (nw - lw) / 2;
        var cy = tr.y + tr.h / 2;
        ctx.save();
        if (lay.expanded) {
            ctx.translate(lx + 4, cy);
            ctx.rotate(Math.PI);
            graph.drawGlyph(ctx, "chevronDown", -4, -4, 8, hoverT ? graph.theme.textPrimary : graph.theme.textMuted);
        } else {
            graph.drawGlyph(ctx, "chevronDown", lx, cy - 4, 8, hoverT ? graph.theme.textPrimary : graph.theme.textMuted);
        }
        ctx.restore();
        ctx.font = graph.fontSpec(400, graph.theme.sizeBadge, true);
        ctx.fillStyle = "" + (hoverT ? graph.theme.textPrimary : graph.theme.textMuted);
        ctx.textAlign = "left";
        ctx.textBaseline = "middle";
        ctx.fillText(label, lx + 12, cy);
    }

    if (isSubgraph) {
        ctx.fillStyle = "" + graph.theme.borderHairline;
        ctx.fillRect(nx + 1, ny + nh - graph.theme.nodeFooter, nw - 2, 1);
        var ob = graph.footerButtons(node, nx, ny, nh)[0];
        var hoverO = graph.hoveredButton === node.id + ":open";
        graph.roundRectPath(ctx, ob.x, ob.y, ob.w, ob.h, graph.theme.radiusSmall);
        ctx.fillStyle = "" + (hoverO ? graph.theme.borderStrong : graph.theme.bgSubtle);
        ctx.fill();
        ctx.font = graph.fontSpec(500, 11, false);
        var openTxt = "Open subgraph";
        var openW = 12 + 6 + ctx.measureText(openTxt).width;
        var ox = ob.x + (ob.w - openW) / 2;
        graph.drawGlyph(ctx, "enter", ox, ob.y + (ob.h - 12) / 2, 12, graph.theme.textIcon);
        ctx.fillStyle = "" + graph.theme.textIcon;
        ctx.textAlign = "left";
        ctx.textBaseline = "middle";
        ctx.fillText(openTxt, ox + 18, ob.y + ob.h / 2 + 0.5);
    }

    if (isPlugin && !isMeter) {
        var pinfo = graph.presetInfo[node.id] || {
            name: "",
            isDefault: false,
            modified: false,
            count: 0
        };
        var prr = graph.presetRects(node, nx, ny, nh);
        var hoverName = graph.hoveredButton === node.id + ":preset";
        var hoverSave = graph.hoveredButton === node.id + ":presetsave";
        graph.roundRectPath(ctx, prr.name.x, prr.name.y, prr.name.w, prr.name.h, graph.theme.radiusSmall);
        ctx.fillStyle = "" + (hoverName ? graph.theme.borderStrong : graph.theme.bgSubtle);
        ctx.fill();
        var hasName = pinfo.name !== "";
        var label = hasName ? (pinfo.isDefault ? "\u2605 " : "") + pinfo.name + (pinfo.modified ? " *" : "") : (pinfo.count > 0 ? "Preset\u2026" : "No presets");
        ctx.font = graph.fontSpec(500, 11, false);
        ctx.fillStyle = "" + (hasName ? graph.theme.textPrimary : graph.theme.textMuted);
        ctx.textAlign = "left";
        ctx.textBaseline = "middle";
        ctx.fillText(graph.elide(ctx, label, prr.name.w - 12 - 18), prr.name.x + 8, prr.name.y + prr.name.h / 2 + 0.5);
        var chx = prr.name.x + prr.name.w - 12;
        var chy = prr.name.y + prr.name.h / 2;
        ctx.beginPath();
        ctx.moveTo(chx - 3.5, chy - 1.5);
        ctx.lineTo(chx, chy + 2);
        ctx.lineTo(chx + 3.5, chy - 1.5);
        ctx.strokeStyle = "" + graph.theme.textIcon;
        ctx.lineWidth = 1.5;
        ctx.stroke();
        graph.roundRectPath(ctx, prr.save.x, prr.save.y, prr.save.w, prr.save.h, graph.theme.radiusSmall);
        ctx.fillStyle = "" + (hoverSave ? graph.theme.borderStrong : graph.theme.bgSubtle);
        ctx.fill();
        ctx.font = graph.fontSpec(500, 11, false);
        ctx.fillStyle = "" + graph.theme.textIcon;
        ctx.textAlign = "center";
        ctx.fillText("Save", prr.save.x + prr.save.w / 2, prr.save.y + prr.save.h / 2 + 0.5);
    }

    if (isPlugin && !isMeter) {
        ctx.fillStyle = "" + graph.theme.borderHairline;
        ctx.fillRect(nx + 1, ny + nh - graph.theme.nodeFooter, nw - 2, 1);
        var bypassed = graph.isNodeBypassed(node);
        var enabled = graph.isNodeEnabled(node);
        var hasUi = node.pluginHasUi !== false;
        var btns = graph.footerButtons(node, nx, ny, nh);
        for (var b = 0; b < btns.length; b++) {
            var bt = btns[b];
            var hoverB = graph.hoveredButton === node.id + ":" + bt.key;
            var on = bt.key === "onoff" && enabled;
            var byp = bt.key === "bypass" && bypassed;
            graph.roundRectPath(ctx, bt.x, bt.y, bt.w, bt.h, graph.theme.radiusSmall);
            ctx.fillStyle = "" + (on ? graph.theme.accentFaint : byp ? graph.theme.bypassFaint : (hoverB ? graph.theme.borderStrong : graph.theme.bgSubtle));
            ctx.fill();
            var txt = bt.key === "onoff" ? (enabled ? "On" : "Off") : bt.key === "bypass" ? "Bypass" : bt.key === "ui" ? "UI" : "Params";
            var col = on ? graph.theme.accent : byp ? graph.theme.bypass : bt.key === "ui" && !hasUi ? graph.theme.textDisabled : bt.key === "onoff" || bt.key === "bypass" ? graph.theme.textMuted : graph.theme.textIcon;
            ctx.font = graph.fontSpec(on || byp ? 600 : 500, 11, false);
            ctx.fillStyle = "" + col;
            ctx.textBaseline = "middle";
            if (bt.key === "onoff") {
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

function drawPin(graph, ctx, port, nodeId, cx, cy, nodeSelected) {
    var connected = !!graph.connectedPorts[port.id];
    var col = graph.signalColor(port.mediaType);
    if (nodeSelected && connected)
        col = graph.theme.accent;
    var r = port.id === graph.hoveredPortId ? graph.theme.nodePinHover / 2 : graph.theme.nodePin / 2;

    if (graph.connectFromPortId >= 0 && graph.isValidDropTarget(port, nodeId)) {
        ctx.beginPath();
        ctx.arc(cx, cy, r + 4, 0, Math.PI * 2);
        ctx.fillStyle = "" + graph.theme.accentRing;
        ctx.fill();
    }
    ctx.beginPath();
    ctx.arc(cx, cy, r, 0, Math.PI * 2);
    if (connected) {
        ctx.fillStyle = "" + col;
        ctx.fill();
    } else {
        ctx.fillStyle = "" + graph.theme.bgSurface;
        ctx.fill();
        ctx.beginPath();
        ctx.arc(cx, cy, r - 1, 0, Math.PI * 2);
        ctx.strokeStyle = "" + col;
        ctx.lineWidth = 2;
        ctx.stroke();
    }
    if (graph.portMatchesSearch(port)) {
        ctx.beginPath();
        ctx.arc(cx, cy, r + 3, 0, Math.PI * 2);
        ctx.strokeStyle = "" + graph.theme.accent;
        ctx.lineWidth = 2;
        ctx.stroke();
    }
}
