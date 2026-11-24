function levelFraction(graph, level) {
    if (!(level > 0))
        return 0;
    var db = 20 * Math.log(level) / Math.LN10;
    return Math.max(0, Math.min(1, (db - graph.meterFloorDb) / -graph.meterFloorDb));
}

function isAudioRow(graph, port) {
    return port.mediaType !== "Midi" && port.mediaType !== "Video";
}

function meterRows(graph, node) {
    var ins = graph.nodeLayout(node).inputs;
    var rows = [];
    for (var i = 0; i < ins.length; i++) {
        if (graph.isAudioRow(ins[i]))
            rows.push(i);
    }
    return rows;
}

function meterBarRect(graph, nw, row) {
    var rowH = graph.theme.nodePortRow;
    var h = Math.max(4, Math.round(rowH * 0.32));
    var x0 = graph.theme.nodePadX + 14;
    var x1 = nw - graph.theme.nodePadX;
    return {
        x: x0,
        y: graph.theme.nodeHeader + graph.theme.nodePortsTop + row * rowH + (rowH - h) / 2,
        w: x1 - x0,
        h: h
    };
}

function updateMeterNodeIds(graph) {
    var ids = [];
    for (var i = 0; i < graph.nodes.length; i++) {
        if (graph.isMeterNode(graph.nodes[i]))
            ids.push(graph.nodes[i].id);
    }
    if (ids.join(",") !== graph.meterNodeIds.join(","))
        graph.meterNodeIds = ids;
    if (ids.length === 0)
        graph.meterLevels = {};
}

function isSnapInternal(graph, fromPos, toPos) {
    return (fromPos.alias || toPos.alias) && fromPos.cx === toPos.cx && fromPos.cy === toPos.cy;
}

function snapAnchorCount(graph, meter, side) {
    var chans = graph.meterChannelPorts(meter);
    var n = 0;
    for (var k = 0; k < chans.length; k++) {
        if ((side === "source" ? chans[k].src : (chans[k].dests.length > 0 ? chans[k].dests[0] : -1)) >= 0)
            n++;
    }
    return n;
}

function updateSnapStates(graph) {
    var now = Date.now();
    var hiddenChanged = false;
    var forgot = false;
    for (var rk in graph.removedMeterKeys) {
        if (!graph.findNodeByLayoutKey(rk)) {
            delete graph.dockedMeters[rk];
            delete graph.hiddenNodes[rk];
            delete graph.snapSuspended[rk];
            delete graph.snapUnanchoredSince[rk];
            delete graph.snapSeenAnchored[rk];
            delete graph.removedMeterKeys[rk];
            forgot = true;
        } else if (now - graph.removedMeterKeys[rk] > 15000) {
            delete graph.removedMeterKeys[rk];
        }
    }
    var waiting = false;
    for (var k in graph.dockedMeters) {
        var meter = graph.findNodeByLayoutKey(k);
        if (!meter || !graph.isMeterNode(meter)) {
            delete graph.snapUnanchoredSince[k];
            continue;
        }
        if (graph.snapAnchorCount(meter, graph.dockedMeters[k]) > 0) {
            delete graph.snapUnanchoredSince[k];
            graph.snapSeenAnchored[k] = true;
            if (graph.snapSuspended[k] || !graph.hiddenNodes[k]) {
                delete graph.snapSuspended[k];
                graph.hiddenNodes[k] = true;
                delete graph.selectedNodes[meter.id];
                hiddenChanged = true;
            }
        } else if (!graph.snapSuspended[k]) {
            if (graph.snapUnanchoredSince[k] === undefined)
                graph.snapUnanchoredSince[k] = now;
            waiting = true;
        }
    }
    if (forgot) {
        graph.dockedMeters = graph.dockedMeters;
        hiddenChanged = true;
        graph.persistDocked();
        graph.persistHidden();
    }
    if (hiddenChanged)
        graph.hiddenNodes = graph.hiddenNodes;
    if (waiting && !graph.elements.orphanTimer.running)
        graph.elements.orphanTimer.start();
}

function suspendOrphanedSnaps(graph) {
    var now = Date.now();
    var shown = 0;
    var waiting = false;
    for (var k in graph.snapUnanchoredSince) {
        if (!graph.dockedMeters[k] || graph.snapSuspended[k]) {
            delete graph.snapUnanchoredSince[k];
            continue;
        }
        var grace = graph.snapSeenAnchored[k] ? graph.snapGraceMs : graph.snapFirstGraceMs;
        if (now - graph.snapUnanchoredSince[k] < grace) {
            waiting = true;
            continue;
        }
        delete graph.snapUnanchoredSince[k];
        graph.snapSuspended[k] = true;
        delete graph.hiddenNodes[k];
        shown++;
    }
    if (shown > 0) {
        graph.hiddenNodes = graph.hiddenNodes;
        graph.showToast(shown > 1 ? shown + " snapped meters shown as cards until the ports they sit on are back" : "Snapped meter shown as a card until the port it sits on is back");
        graph.elements.canvas.requestPaint();
    }
    if (waiting)
        graph.elements.orphanTimer.start();
}

function forgetMeterWhenGone(graph, node) {
    if (!graph.isMeterNode(node) || !node.layoutKey)
        return;
    graph.removedMeterKeys[node.layoutKey] = Date.now();
}

function forgetStaleMeterKeys(graph, name) {
    var changed = false;
    for (var k in graph.dockedMeters) {
        if (k.substring(k.indexOf(":") + 1) === name && !graph.findNodeByLayoutKey(k)) {
            delete graph.dockedMeters[k];
            delete graph.hiddenNodes[k];
            delete graph.snapSuspended[k];
            delete graph.snapUnanchoredSince[k];
            delete graph.snapSeenAnchored[k];
            changed = true;
        }
    }
    if (changed) {
        graph.dockedMeters = graph.dockedMeters;
        graph.hiddenNodes = graph.hiddenNodes;
        graph.persistDocked();
        graph.persistHidden();
    }
}

function snappedMeterCount(graph) {
    var n = 0;
    for (var k in graph.dockedMeters)
        n++;
    return n;
}

function unsnapAllMeters(graph) {
    var parts = [];
    for (var k in graph.dockedMeters) {
        parts.push({
            kind: "snap",
            label: "unsnap meter",
            key: k,
            side: "",
            prevSide: graph.dockedMeters[k]
        });
    }
    for (var i = 0; i < parts.length; i++)
        graph.setMeterSnapped(parts[i].key, "");
    if (parts.length > 0)
        graph.pushUndo({
            kind: "group",
            label: parts.length > 1 ? "unsnap " + parts.length + " meters" : "unsnap meter",
            parts: parts
        });
}

function meterSnapCandidate(graph, meter, sx, sy) {
    if (!graph.isMeterNode(meter))
        return null;
    var c = graph.toCanvas(sx, sy);
    var targetId = -1;
    for (var i = graph.nodes.length - 1; i >= 0; i--) {
        var n = graph.nodes[i];
        if (n.id === meter.id || (n.layoutKey && graph.hiddenNodes[n.layoutKey]))
            continue;
        var pos = graph.nodePositions[n.id];
        if (pos && c.x >= pos.x && c.x <= pos.x + graph.getNodeWidth(n.id) && c.y >= pos.y && c.y <= pos.y + graph.calculateNodeHeight(n)) {
            targetId = n.id;
            break;
        }
    }
    if (targetId < 0)
        return null;
    var chans = graph.meterChannelPorts(meter);
    var sides = ["source", "dest"];
    for (var si = 0; si < sides.length; si++) {
        var side = sides[si];
        if (!graph.canSnapMeter(meter, side))
            continue;
        var ports = [];
        var ok = true;
        for (var k = 0; k < chans.length; k++) {
            var a = side === "source" ? chans[k].src : (chans[k].dests.length > 0 ? chans[k].dests[0] : -1);
            if (a < 0)
                continue;
            if (graph.getPortNodeId(a) !== targetId) {
                ok = false;
                break;
            }
            ports.push(a);
        }
        if (ok && ports.length > 0)
            return {
                meterId: meter.id,
                nodeId: targetId,
                side: side,
                ports: ports
            };
    }
    return null;
}

function dockBarRect(graph, cx, cy, output) {
    return {
        x: output ? cx - graph.theme.nodePadX - graph.dockBarW : cx + graph.theme.nodePadX,
        y: cy + graph.theme.nodePortRow / 2 - graph.dockBarH - 2,
        w: graph.dockBarW,
        h: graph.dockBarH
    };
}

function connectReplacing(graph, outPort, inPort, oldLink) {
    var parts = [];
    if (!graph.findLinkByPorts(outPort, inPort)) {
        var on = graph.getPortNodeId(outPort);
        var inn = graph.getPortNodeId(inPort);
        graph.markHandPlaced(on);
        graph.markHandPlaced(inn);
        graph.controller.connect_ports(outPort, inPort);
        parts.push({
            kind: "connect",
            label: "connect",
            links: [
                {
                    o: outPort,
                    i: inPort,
                    onKey: graph.uniqueKeyById[on] || "",
                    innKey: graph.uniqueKeyById[inn] || ""
                }
            ]
        });
    }
    var rec = graph.linkRecord(oldLink);
    graph.controller.disconnect_link(oldLink.id);
    parts.push({
        kind: "disconnect",
        label: "disconnect",
        links: [rec]
    });
    graph.pushUndo({
        kind: "group",
        label: "route through meter",
        parts: parts
    });
}

function audioPortsOf(graph, nodeId, outputs) {
    var sp = graph.getSortedPorts(nodeId);
    var ports = (outputs ? sp.outputs : sp.inputs).filter(graph.isAudioRow);
    var ids = [];
    for (var i = 0; i < ports.length; i++)
        ids.push(ports[i].id);
    return ids;
}

function meterChunks(graph, n) {
    var out = [];
    while (n > 0) {
        var c = n >= 8 ? 8 : Math.min(n, 6);
        out.push(c);
        n -= c;
    }
    return out;
}

function canMeterPorts(graph, node, outputs) {
    return !!node && !graph.isMeterNode(node) && node.type !== "Subgraph" && node.type !== "SubgraphInput" && node.type !== "SubgraphOutput" && graph.audioPortsOf(node.id, outputs).length > 0;
}

function addMetersTo(graph, node, outputs) {
    if (!graph.canMeterPorts(node, outputs))
        return;
    var ports = graph.audioPortsOf(node.id, outputs);
    var chunks = graph.meterChunks(ports.length);
    var start = 0;
    var jobs = graph.pendingMeterAttach.slice();
    for (var i = 0; i < chunks.length; i++) {
        var name = graph.controller.add_meter(chunks[i]);
        if (name) {
            graph.forgetStaleMeterKeys(name);
            jobs.push({
                name: name,
                hostId: node.id,
                side: outputs ? "source" : "dest",
                hostPorts: ports.slice(start, start + chunks[i]),
                t: Date.now(),
                hidden: false
            });
        }
        start += chunks[i];
    }
    graph.pendingMeterAttach = jobs;
}

function processPendingMeterAttach(graph) {
    if (graph.pendingMeterAttach.length === 0)
        return;
    var keep = [];
    var changed = false;
    for (var i = 0; i < graph.pendingMeterAttach.length; i++) {
        var job = graph.pendingMeterAttach[i];
        var meter = null;
        for (var ni = 0; ni < graph.nodes.length; ni++) {
            if (graph.isMeterNode(graph.nodes[ni]) && graph.nodes[ni].name === job.name) {
                meter = graph.nodes[ni];
                break;
            }
        }
        var host = graph.findNodeData(job.hostId);
        if (!host || (!meter && Date.now() - job.t > 15000)) {
            graph.showToast("Could not meter " + (host ? host.name : "the node") + ": the meter did not start");
            continue;
        }
        if (!meter || !meter.layoutKey) {
            keep.push(job);
            continue;
        }
        if (!job.hidden) {
            job.hidden = true;
            graph.hiddenNodes[meter.layoutKey] = true;
            graph.dockedMeters[meter.layoutKey] = job.side;
            var a = graph.adjacentAnchor(meter, host, job.side === "dest");
            graph.nodePositions[meter.id] = graph.findFreeSpot(meter, a.x, a.y, a.dir, false);
            graph.markHandPlaced(meter.id);
            delete graph.autoPlaced[meter.id];
            graph.renameMeter(meter, graph.snapMeterName(host.id, job.side, job.hostPorts));
            job.name = meter.name;
            changed = true;
        }
        var ins = graph.audioPortsOf(meter.id, false);
        var outs = graph.audioPortsOf(meter.id, true);
        if (ins.length < job.hostPorts.length || outs.length < job.hostPorts.length) {
            if (Date.now() - job.t > 15000)
                graph.showToast("Could not wire the meter on " + host.name + ": its ports did not appear");
            else
                keep.push(job);
            continue;
        }
        var drop = [];
        for (var k = 0; k < job.hostPorts.length; k++) {
            var hp = job.hostPorts[k];
            var olds = [];
            for (var li = 0; li < graph.links.length; li++) {
                var l = graph.links[li];
                if (job.side === "source" ? l.outputPortId === hp : l.inputPortId === hp)
                    olds.push(l);
            }
            if (job.side === "source") {
                graph.controller.connect_ports(hp, ins[k]);
                for (var o = 0; o < olds.length; o++)
                    graph.controller.connect_ports(outs[k], olds[o].inputPortId);
            } else {
                graph.controller.connect_ports(outs[k], hp);
                for (var o2 = 0; o2 < olds.length; o2++)
                    graph.controller.connect_ports(olds[o2].outputPortId, ins[k]);
            }
            drop = drop.concat(olds);
        }
        for (var d = 0; d < drop.length; d++)
            graph.controller.disconnect_link(drop[d].id);
    }
    graph.pendingMeterAttach = keep;
    if (changed) {
        graph.hiddenNodes = graph.hiddenNodes;
        graph.dockedMeters = graph.dockedMeters;
        graph.persistHidden();
        graph.persistDocked();
        graph.persistLayout();
        graph.elements.canvas.requestPaint();
    }
}

function removeMetersOn(graph, nodeId) {
    var keys = graph.metersSnappedOn(nodeId);
    for (var i = 0; i < keys.length; i++) {
        var m = graph.findNodeByLayoutKey(keys[i]);
        if (m) {
            graph.forgetMeterWhenGone(m);
            graph.controller.remove_plugin(m.id);
        }
    }
}

function snapTargetPort(graph, portId) {
    var r = graph.snapRedirect[portId];
    return r !== undefined ? r : portId;
}

function persistDocked(graph) {
    graph.controller.save_docked_meters(JSON.stringify(graph.dockedMeters));
}

function isDockedMeter(graph, n) {
    return graph.isMeterNode(n) && !!n.layoutKey && !!graph.dockedMeters[n.layoutKey];
}

function meterChannelPorts(graph, node) {
    var sp = graph.getSortedPorts(node.id);
    var ins = sp.inputs.filter(graph.isAudioRow);
    var outs = sp.outputs.filter(graph.isAudioRow);
    var chans = [];
    for (var k = 0; k < Math.max(ins.length, outs.length); k++) {
        var c = {
            inPort: ins[k] ? ins[k].id : -1,
            outPort: outs[k] ? outs[k].id : -1,
            src: -1,
            srcs: [],
            dests: []
        };
        for (var i = 0; i < graph.links.length; i++) {
            var l = graph.links[i];
            if (c.inPort >= 0 && l.inputPortId === c.inPort)
                c.srcs.push(l.outputPortId);
            if (c.outPort >= 0 && l.outputPortId === c.outPort)
                c.dests.push(l.inputPortId);
        }
        c.src = c.srcs.length > 0 ? c.srcs[0] : -1;
        chans.push(c);
    }
    return chans;
}

function canSnapMeter(graph, node, side) {
    if (!graph.isMeterNode(node) || !node.layoutKey)
        return false;
    var chans = graph.meterChannelPorts(node);
    var any = false;
    for (var k = 0; k < chans.length; k++) {
        var c = chans[k];
        if (side === "source") {
            if (c.srcs.length > 1 || (c.dests.length > 0 && c.srcs.length === 0))
                return false;
            if (c.srcs.length === 1)
                any = true;
        } else {
            if (c.dests.length > 1 || (c.srcs.length > 0 && c.dests.length === 0))
                return false;
            if (c.dests.length === 1)
                any = true;
        }
    }
    return any;
}

function resolveMeterKey(graph, key) {
    var seen = 0;
    while (graph.meterKeyAlias[key] !== undefined && seen++ < 32)
        key = graph.meterKeyAlias[key];
    return key;
}

function isAutoMeterName(graph, name) {
    return /^Meter \(\d ch\)( #\d+)?$/.test(name) || / (in|out)( \d+(-\d+)?)? meter( #\d+)?$/.test(name);
}

function snapMeterName(graph, hostId, side, anchorPorts) {
    var host = graph.findNodeData(hostId);
    if (!host)
        return "";
    var all = graph.audioPortsOf(hostId, side === "source");
    var idx = [];
    for (var i = 0; i < anchorPorts.length; i++) {
        var at = all.indexOf(anchorPorts[i]);
        if (at >= 0)
            idx.push(at + 1);
    }
    idx.sort(function (a, b) {
        return a - b;
    });
    var range = "";
    if (idx.length > 0 && idx.length < all.length)
        range = " " + (idx.length === 1 ? idx[0] : idx[0] + "-" + idx[idx.length - 1]);
    return host.name + " " + (side === "source" ? "out" : "in") + range + " meter";
}

function snapMeterNameFromLinks(graph, meter, side) {
    var chans = graph.meterChannelPorts(meter);
    var anchors = [];
    var hostId = -1;
    for (var k = 0; k < chans.length; k++) {
        var a = side === "source" ? chans[k].src : (chans[k].dests.length > 0 ? chans[k].dests[0] : -1);
        if (a < 0)
            continue;
        var nid = graph.getPortNodeId(a);
        if (hostId < 0)
            hostId = nid;
        if (nid === hostId)
            anchors.push(a);
    }
    return hostId >= 0 ? graph.snapMeterName(hostId, side, anchors) : "";
}

function renameMeter(graph, meter, base) {
    if (!meter || !graph.isAutoMeterName(meter.name))
        return;
    if (base === "" && meter.layoutKey && graph.dockedMeters[meter.layoutKey])
        return;
    var oldKey = meter.layoutKey;
    var name = graph.controller.rename_meter(meter.id, base);
    if (!name || !oldKey || name === meter.name)
        return;
    var newKey = oldKey.substring(0, oldKey.indexOf(":") + 1) + name;
    var maps = [graph.hiddenNodes, graph.dockedMeters, graph.pinnedNodes, graph.snapSeenAnchored, graph.snapSuspended, graph.snapUnanchoredSince, graph.removedMeterKeys];
    for (var i = 0; i < maps.length; i++) {
        if (maps[i][oldKey] !== undefined) {
            maps[i][newKey] = maps[i][oldKey];
            delete maps[i][oldKey];
        }
    }
    graph.meterKeyAlias[oldKey] = newKey;
    delete graph.meterKeyAlias[newKey];
    meter.name = name;
    meter.layoutKey = newKey;
    graph.hiddenNodes = graph.hiddenNodes;
    graph.dockedMeters = graph.dockedMeters;
    graph.pinnedNodes = graph.pinnedNodes;
    graph.persistDocked();
    graph.persistHidden();
    graph.persistPinned();
}

function setMeterSnapped(graph, key, side) {
    key = graph.resolveMeterKey(key);
    delete graph.snapSuspended[key];
    delete graph.snapUnanchoredSince[key];
    if (side) {
        graph.dockedMeters[key] = side;
        graph.hiddenNodes[key] = true;
    } else {
        delete graph.dockedMeters[key];
        delete graph.hiddenNodes[key];
    }
    graph.dockedMeters = graph.dockedMeters;
    graph.hiddenNodes = graph.hiddenNodes;
    graph.persistDocked();
    graph.persistHidden();
    var meter = graph.findNodeByLayoutKey(key);
    if (meter)
        graph.renameMeter(meter, side ? graph.snapMeterNameFromLinks(meter, side) : "");
    graph.elements.canvas.requestPaint();
}

function snapMeter(graph, node, side) {
    if (!graph.canSnapMeter(node, side))
        return;
    delete graph.selectedNodes[node.id];
    graph.setMeterSnapped(node.layoutKey, side);
    graph.pushUndo({
        kind: "snap",
        label: "snap meter",
        key: node.layoutKey,
        side: side
    });
}

function metersSnappedOn(graph, nodeId) {
    var keys = [];
    for (var id in graph.dockBarFrames) {
        var f = graph.dockBarFrames[id];
        if (f.nodeId === nodeId && keys.indexOf(f.meterKey) < 0)
            keys.push(f.meterKey);
    }
    return keys;
}

function unsnapMetersOn(graph, nodeId) {
    var keys = graph.metersSnappedOn(nodeId);
    var parts = [];
    for (var i = 0; i < keys.length; i++) {
        parts.push({
            kind: "snap",
            label: "unsnap meter",
            key: keys[i],
            side: "",
            prevSide: graph.dockedMeters[keys[i]]
        });
        graph.setMeterSnapped(keys[i], "");
    }
    if (parts.length > 0)
        graph.pushUndo({
            kind: "group",
            label: parts.length > 1 ? "unsnap " + parts.length + " meters" : "unsnap meter",
            parts: parts
        });
}

function addMeter(graph, channels) {
    if (graph.contextCanvasPos)
        graph.pendingPluginPosition = {
            x: graph.contextCanvasPos.x,
            y: graph.contextCanvasPos.y
        };
    var name = graph.controller.add_meter(channels);
    if (name) {
        graph.forgetStaleMeterKeys(name);
        graph.pendingPluginName = name;
    } else {
        graph.pendingPluginPosition = null;
        graph.pendingPluginName = "";
    }
}
