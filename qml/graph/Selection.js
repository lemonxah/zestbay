function deleteSelectedLinks(graph) {
    var ids = [];
    for (var linkId in graph.selectedLinks) {
        var lid = parseInt(linkId);
        if (graph.linkMatchesSig(graph.findLinkData(lid), graph.selectedLinks[linkId]))
            ids.push(lid);
    }
    if (graph.disconnectLinks(ids) > 0) {
        graph.selectedLinks = {};
        graph.elements.canvas.requestPaint();
    }
}

function nudgeSelection(graph, dx, dy, big) {
    var ids = graph.selectedNodeIds();
    if (ids.length === 0)
        return false;
    var step = graph.snapToGrid ? graph.gridSize : (big ? 10 : 1);
    var from = {};
    for (var i = 0; i < ids.length; i++) {
        var p = graph.nodePositions[ids[i]];
        if (!p)
            continue;
        from[ids[i]] = {
            x: p.x,
            y: p.y
        };
        graph.nodePositions[ids[i]] = {
            x: p.x + dx * step,
            y: p.y + dy * step
        };
        graph.markHandPlaced(ids[i]);
    }
    var last = graph.undoStack.length > 0 ? graph.undoStack[graph.undoStack.length - 1] : null;
    if (graph.elements.nudgeTimer.running && graph.nudgeEntry !== null && last === graph.nudgeEntry) {
        for (var ni = 0; ni < graph.nudgeEntry.items.length; ni++) {
            var it = graph.nudgeEntry.items[ni];
            var nid = graph.nodeIdForKey(it.key);
            if (nid >= 0 && graph.nodePositions[nid])
                it.to = {
                    x: graph.nodePositions[nid].x,
                    y: graph.nodePositions[nid].y
                };
        }
    } else {
        graph.nudgeEntry = graph.recordMove("move", from);
    }
    graph.elements.nudgeTimer.restart();
    graph.elements.canvas.requestPaint();
    return true;
}

function deleteSelection(graph) {
    var plan = {
        links: [],
        plugins: [],
        subgraphs: [],
        vdevs: [],
        hide: [],
        blocks: []
    };
    var removed = {};
    var ids = graph.selectedNodeIds();
    for (var i = 0; i < ids.length; i++) {
        var n = graph.findNodeData(ids[i]);
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
    for (var linkId in graph.selectedLinks) {
        var l = graph.findLinkData(parseInt(linkId));
        if (graph.linkMatchesSig(l, graph.selectedLinks[linkId]) && !removed[l.outputNodeId] && !removed[l.inputNodeId])
            plan.links.push(l.id);
    }
    for (var bk in graph.selectedBlocks) {
        var sb = graph.selectedBlocks[bk];
        for (var bi2 = 0; bi2 < graph.blockedPairs.length; bi2++) {
            var live = graph.blockedPairs[bi2];
            if (live.ruleId === sb.ruleId && live.source === sb.source && live.target === sb.target) {
                var seen = false;
                for (var ri = 0; ri < plan.blocks.length; ri++) {
                    if (plan.blocks[ri].ruleId === live.ruleId)
                        seen = true;
                }
                if (!seen)
                    plan.blocks.push({
                        ruleId: live.ruleId,
                        label: live.label
                    });
            }
        }
    }
    var destructive = plan.plugins.length + plan.subgraphs.length + plan.vdevs.length;
    if (destructive + plan.hide.length + plan.links.length + plan.blocks.length === 0)
        return;
    graph.deletePlan = plan;
    if (destructive > 0)
        graph.elements.deleteSelectionDialog.open();
    else
        graph.performDelete();
}

function deletePlanLines(graph) {
    var plan = graph.deletePlan;
    if (!plan)
        return [];
    var lines = [];
    if (plan.plugins.length > 0)
        lines.push("Remove " + graph.counted(plan.plugins.length, "plugin", "plugins"));
    if (plan.subgraphs.length > 0)
        lines.push("Delete " + graph.counted(plan.subgraphs.length, "subgraph", "subgraphs") + " and the plugins inside");
    if (plan.vdevs.length > 0)
        lines.push("Remove " + graph.counted(plan.vdevs.length, "virtual device", "virtual devices"));
    if (plan.hide.length > 0)
        lines.push("Hide " + graph.counted(plan.hide.length, "node", "nodes") + " (devices and app streams can\u2019t be removed)");
    if (plan.links.length > 0)
        lines.push("Disconnect " + graph.counted(plan.links.length, "wire", "wires"));
    if (plan.blocks.length > 0)
        lines.push("Remove " + graph.counted(plan.blocks.length, "do-not-connect rule", "do-not-connect rules"));
    return lines;
}

function performDelete(graph) {
    var plan = graph.deletePlan;
    graph.deletePlan = null;
    if (!plan)
        return;
    graph.disconnectLinks(plan.links);
    if (plan.hide.length > 0) {
        var keys = [];
        for (var hi = 0; hi < plan.hide.length; hi++) {
            graph.hiddenNodes[plan.hide[hi].layoutKey] = true;
            keys.push(plan.hide[hi].layoutKey);
        }
        graph.hiddenNodes = graph.hiddenNodes;
        graph.persistHidden();
        graph.pushUndo({
            kind: "hide",
            label: "hide " + graph.plural(keys.length, "node", "nodes"),
            keys: keys
        });
    }
    for (var pi = 0; pi < plan.plugins.length; pi++) {
        graph.forgetMeterWhenGone(graph.findNodeData(plan.plugins[pi].id));
        graph.controller.remove_plugin(plan.plugins[pi].id);
    }
    for (var si = 0; si < plan.subgraphs.length; si++)
        graph.controller.delete_subgraph(plan.subgraphs[si].subgraphId);
    for (var vi = 0; vi < plan.vdevs.length; vi++)
        graph.controller.remove_virtual_device(plan.vdevs[vi].vdevId);
    if (plan.blocks.length > 0) {
        var others = 0;
        for (var oi = 0; oi < graph.blockedPairs.length; oi++) {
            for (var rj = 0; rj < plan.blocks.length; rj++) {
                if (graph.blockedPairs[oi].ruleId === plan.blocks[rj].ruleId && !graph.selectedBlocks[graph.blockKey(graph.blockedPairs[oi])])
                    others++;
            }
        }
        for (var bj = 0; bj < plan.blocks.length; bj++)
            graph.controller.remove_rule(plan.blocks[bj].ruleId);
        graph.showToast("Removed " + (plan.blocks.length === 1 ? "the rule \u201c" + plan.blocks[0].label + "\u201d" : graph.counted(plan.blocks.length, "do-not-connect rule", "do-not-connect rules")) + (others > 0 ? " (it also unblocks " + graph.counted(others, "other pair", "other pairs") + ")" : "") + ".");
    }
    graph.selectedNodes = {};
    graph.selectedLinks = {};
    graph.selectedBlocks = {};
    if (plan.hide.length > 0)
        graph.showToast("Hid " + graph.counted(plan.hide.length, "node", "nodes") + ". Ctrl+Z or right-click \u203a Unhide all brings them back.");
    graph.elements.canvas.requestPaint();
}

function blockedReasonFor(graph, fromPort, fromDir, toPort) {
    return fromDir === "Output" ? graph.controller.blocked_reason(fromPort, toPort) : graph.controller.blocked_reason(toPort, fromPort);
}

function blockLinks(graph, linkIds) {
    var pairs = {};
    for (var i = 0; i < linkIds.length; i++) {
        var l = graph.findLinkData(linkIds[i]);
        if (!l)
            continue;
        var key = l.outputNodeId + ">" + l.inputNodeId;
        if (pairs[key])
            continue;
        pairs[key] = true;
        graph.controller.block_link(l.id);
    }
    graph.selectedLinks = {};
    graph.elements.canvas.requestPaint();
}

function blockSelectedLinks(graph) {
    var ids = [];
    for (var linkId in graph.selectedLinks) {
        var lid = parseInt(linkId);
        if (graph.linkMatchesSig(graph.findLinkData(lid), graph.selectedLinks[linkId]))
            ids.push(lid);
    }
    if (ids.length > 0)
        graph.blockLinks(ids);
}

function clearSelection(graph) {
    if (graph.hasSelection()) {
        graph.selectedLinks = {};
        graph.selectedNodes = {};
        graph.selectedBlocks = {};
        graph.elements.canvas.requestPaint();
    }
}
