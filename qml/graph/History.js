function pushUndo(graph, entry) {
    var st = graph.undoStack.slice();
    st.push(entry);
    if (st.length > graph.undoLimit)
        st.shift();
    graph.undoStack = st;
    graph.redoStack = [];
}

function clearUndo(graph) {
    graph.undoStack = [];
    graph.redoStack = [];
}

function plural(graph, n, one, many) {
    return n === 1 ? one : n + " " + many;
}

function counted(graph, n, one, many) {
    return n + " " + (n === 1 ? one : many);
}

function nodeIdForKey(graph, key) {
    for (var id in graph.uniqueKeyById) {
        if (graph.uniqueKeyById[id] === key)
            return parseInt(id);
    }
    return -1;
}

function markHandPlaced(graph, nodeId) {
    delete graph.autoPlaced[nodeId];
    delete graph.placedKeyById[nodeId];
    delete graph.awaitingLinkPlacement[nodeId];
}

function recordMove(graph, verb, fromById) {
    var entry = graph.moveEntry(verb, fromById);
    if (entry)
        graph.pushUndo(entry);
    return entry;
}

function moveEntry(graph, verb, fromById) {
    var items = [];
    for (var id in fromById) {
        var nid = parseInt(id);
        var from = fromById[id];
        var to = graph.nodePositions[nid];
        var key = graph.uniqueKeyById[nid];
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
    return {
        kind: "move",
        label: verb + " " + graph.plural(items.length, "node", "nodes"),
        items: items
    };
}

function portLinkRecord(graph, outPort, inPort) {
    return {
        o: outPort,
        i: inPort,
        onKey: graph.uniqueKeyById[graph.getPortNodeId(outPort)] || "",
        innKey: graph.uniqueKeyById[graph.getPortNodeId(inPort)] || ""
    };
}

function insertRewireEntry(graph, linkId, nodeId) {
    var plan = null;
    try {
        var json = graph.controller.insert_on_link_preview_json(linkId, nodeId);
        plan = json ? JSON.parse(json) : null;
    } catch (e) {
        plan = null;
    }
    if (!plan)
        return null;
    var removed = [];
    for (var ri = 0; ri < plan.remove.length; ri++) {
        var l = graph.findLinkData(plan.remove[ri]);
        if (l)
            removed.push(graph.linkRecord(l));
    }
    var added = [];
    for (var ai = 0; ai < plan.add.length; ai++)
        added.push(graph.portLinkRecord(plan.add[ai][0], plan.add[ai][1]));
    if (removed.length === 0 && added.length === 0)
        return null;
    return {
        kind: "rewire",
        label: "insert node",
        removed: removed,
        added: added
    };
}

function applyLinkRecords(graph, recs, make) {
    var done = 0;
    for (var li = 0; li < recs.length; li++) {
        var rec = recs[li];
        if (!graph.linkRecordValid(rec))
            continue;
        var live = graph.findLinkByPorts(rec.o, rec.i);
        if (make && !live) {
            graph.controller.connect_ports(rec.o, rec.i);
            done++;
        } else if (!make && live) {
            graph.controller.disconnect_link(live.id);
            done++;
        }
    }
    return done;
}

function linkRecord(graph, link) {
    return {
        o: link.outputPortId,
        i: link.inputPortId,
        onKey: graph.uniqueKeyById[link.outputNodeId] || "",
        innKey: graph.uniqueKeyById[link.inputNodeId] || ""
    };
}

function linkRecordValid(graph, rec) {
    var on = graph.getPortNodeId(rec.o);
    var inn = graph.getPortNodeId(rec.i);
    return on >= 0 && inn >= 0 && (graph.uniqueKeyById[on] || "") === rec.onKey && (graph.uniqueKeyById[inn] || "") === rec.innKey;
}

function findLinkByPorts(graph, outPort, inPort) {
    for (var i = 0; i < graph.links.length; i++) {
        if (graph.links[i].outputPortId === outPort && graph.links[i].inputPortId === inPort)
            return graph.links[i];
    }
    return null;
}

function connectPorts(graph, outPort, inPort) {
    var on = graph.getPortNodeId(outPort);
    var inn = graph.getPortNodeId(inPort);
    graph.markHandPlaced(on);
    graph.markHandPlaced(inn);
    graph.controller.connect_ports(outPort, inPort);
    graph.pushUndo({
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

function disconnectLinks(graph, ids) {
    var recs = [];
    for (var i = 0; i < ids.length; i++) {
        var l = graph.findLinkData(ids[i]);
        if (!l)
            continue;
        recs.push(graph.linkRecord(l));
        graph.controller.disconnect_link(l.id);
    }
    if (recs.length > 0)
        graph.pushUndo({
            kind: "disconnect",
            label: "disconnect " + graph.plural(recs.length, "wire", "wires"),
            links: recs
        });
    return recs.length;
}

function applyHistory(graph, entry, undoing) {
    var done = 0;
    if (entry.kind === "move") {
        for (var mi = 0; mi < entry.items.length; mi++) {
            var it = entry.items[mi];
            var nid = graph.nodeIdForKey(it.key);
            if (nid < 0)
                continue;
            var target = undoing ? it.from : it.to;
            graph.nodePositions[nid] = {
                x: target.x,
                y: target.y
            };
            graph.markHandPlaced(nid);
            done++;
        }
        if (done > 0)
            graph.persistLayout();
    } else if (entry.kind === "hide") {
        for (var hi = 0; hi < entry.keys.length; hi++) {
            if (undoing)
                delete graph.hiddenNodes[entry.keys[hi]];
            else
                graph.hiddenNodes[entry.keys[hi]] = true;
            done++;
        }
        graph.hiddenNodes = graph.hiddenNodes;
        graph.persistHidden();
    } else if (entry.kind === "snap") {
        var snapTo = undoing ? (entry.side ? "" : entry.prevSide) : entry.side;
        graph.setMeterSnapped(entry.key, snapTo || "");
        done++;
    } else if (entry.kind === "group") {
        for (var pi = 0; pi < entry.parts.length; pi++) {
            var part = entry.parts[undoing ? entry.parts.length - 1 - pi : pi];
            done += graph.applyHistory(part, undoing);
        }
    } else if (entry.kind === "rewire") {
        if (undoing) {
            done += graph.applyLinkRecords(entry.added, false);
            done += graph.applyLinkRecords(entry.removed, true);
        } else {
            done += graph.applyLinkRecords(entry.removed, false);
            done += graph.applyLinkRecords(entry.added, true);
        }
    } else {
        var make = (entry.kind === "disconnect") === undoing;
        done += graph.applyLinkRecords(entry.links, make);
    }
    graph.elements.canvas.requestPaint();
    return done;
}

function undo(graph) {
    if (graph.undoStack.length === 0) {
        graph.showToast("Nothing to undo");
        return;
    }
    var st = graph.undoStack.slice();
    var entry = st.pop();
    graph.undoStack = st;
    if (graph.applyHistory(entry, true) > 0) {
        var rs = graph.redoStack.slice();
        rs.push(entry);
        graph.redoStack = rs;
        graph.showToast("Undo: " + entry.label);
    } else {
        graph.showToast("Can\u2019t undo \u201c" + entry.label + "\u201d: it is no longer there");
    }
}

function redo(graph) {
    if (graph.redoStack.length === 0) {
        graph.showToast("Nothing to redo");
        return;
    }
    var rs = graph.redoStack.slice();
    var entry = rs.pop();
    graph.redoStack = rs;
    if (graph.applyHistory(entry, false) > 0) {
        var st = graph.undoStack.slice();
        st.push(entry);
        graph.undoStack = st;
        graph.showToast("Redo: " + entry.label);
    } else {
        graph.showToast("Can\u2019t redo \u201c" + entry.label + "\u201d: it is no longer there");
    }
}

function selectedNodeIds(graph) {
    var ids = [];
    for (var k in graph.selectedNodes) {
        var n = graph.findNodeData(parseInt(k));
        if (n && graph.selectedNodes[k] && graph.selectedNodes[k].key === (n.layoutKey || ""))
            ids.push(n.id);
    }
    return ids;
}

function selectAll(graph) {
    var allNodes = {};
    for (var i = 0; i < graph.nodes.length; i++) {
        var n = graph.nodes[i];
        if ((n.layoutKey && graph.hiddenNodes[n.layoutKey]) || !graph.nodePositions[n.id])
            continue;
        allNodes[n.id] = graph.nodeSig(n);
    }
    var allLinks = {};
    for (var li = 0; li < graph.links.length; li++) {
        var l = graph.links[li];
        if (graph.portPositions[l.outputPortId] && graph.portPositions[l.inputPortId] && !graph.isSnapInternal(graph.portPositions[l.outputPortId], graph.portPositions[l.inputPortId]))
            allLinks[l.id] = graph.linkSig(l);
    }
    graph.selectedNodes = allNodes;
    graph.selectedLinks = allLinks;
    graph.elements.canvas.requestPaint();
}
