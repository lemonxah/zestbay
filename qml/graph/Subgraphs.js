function hasSelection(graph) {
    for (var k in graph.selectedLinks)
        return true;
    for (var k2 in graph.selectedNodes)
        return true;
    for (var k3 in graph.selectedBlocks)
        return true;
    return false;
}

function selectedPluginIds(graph) {
    var ids = [];
    for (var k in graph.selectedNodes) {
        var n = graph.findNodeData(parseInt(k));
        if (n && n.type === "Plugin")
            ids.push(n.id);
    }
    return ids;
}

function subgraphInfo(graph, id) {
    try {
        var list = JSON.parse(graph.controller.get_subgraphs_json()).subgraphs;
        for (var i = 0; i < list.length; i++)
            if (list[i].id === id)
                return list[i];
    } catch (e) {}
    return null;
}

function groupIntoSubgraph(graph, nodeId) {
    var ids = graph.selectedPluginIds();
    if (nodeId >= 0 && ids.indexOf(nodeId) < 0)
        ids = [nodeId];
    if (ids.length === 0)
        return;
    var minX = Infinity, minY = Infinity;
    for (var i = 0; i < ids.length; i++) {
        var p = graph.nodePositions[ids[i]];
        if (p) {
            minX = Math.min(minX, p.x);
            minY = Math.min(minY, p.y);
        }
    }
    graph.groupAnchor = isFinite(minX) ? graph.gridPos({
        x: minX,
        y: minY
    }) : null;
    var io = {};
    try {
        io = JSON.parse(graph.controller.infer_subgraph_io(JSON.stringify(ids)));
    } catch (e) {}
    graph.elements.subgraphDialog.mode = "create";
    graph.elements.subgraphDialog.nodeIds = ids;
    graph.elements.subgraphDialog.pluginCount = io.plugins || ids.length;
    graph.elements.subgraphDialog.load(io);
    graph.elements.subgraphDialog.open();
}

function openSubgraphEditor(graph, id) {
    var info = graph.subgraphInfo(id);
    if (!info)
        return;
    graph.elements.subgraphDialog.mode = "edit";
    graph.elements.subgraphDialog.subgraphId = id;
    graph.elements.subgraphDialog.load(info);
    graph.elements.subgraphDialog.open();
}

function openVdevEditor(graph, id) {
    graph.elements.vdevDialog.vdevId = id;
    graph.elements.vdevDialog.deviceName = "Virtual Sink";
    graph.elements.vdevDialog.channels = 2;
    graph.elements.vdevDialog.kindIndex = 0;
    if (id !== "") {
        try {
            var list = JSON.parse(graph.controller.get_virtual_devices_json());
            for (var i = 0; i < list.length; i++) {
                if (list[i].id === id) {
                    graph.elements.vdevDialog.deviceName = list[i].name;
                    graph.elements.vdevDialog.channels = list[i].channels;
                    graph.elements.vdevDialog.kindIndex = list[i].kind === "source" ? 1 : 0;
                }
            }
        } catch (e) {}
    }
    graph.elements.vdevDialog.open();
}

function applyInsertedLayout(graph, resultJson) {
    try {
        var r = JSON.parse(resultJson);
        for (var key in r.layout)
            graph.savedLayout[key] = r.layout[key];
        graph.controller.save_layout(JSON.stringify(graph.savedLayout));
        return r.subgraphId;
    } catch (e) {
        return "";
    }
}

function duplicateSubgraph(graph, node) {
    var p = graph.nodePositions[node.id] || {
        x: 0,
        y: 0
    };
    var at = graph.gridPos({
        x: p.x + graph.getNodeWidth(node.id) + 40,
        y: p.y
    });
    graph.applyInsertedLayout(graph.controller.duplicate_subgraph(node.subgraphId, at.x, at.y));
}

function insertTemplate(graph, file) {
    var at = graph.pendingPluginPosition || graph.toCanvas(graph.width / 2, graph.height / 2);
    graph.pendingPluginPosition = null;
    graph.pendingPluginName = "";
    at = graph.gridPos({
        x: at.x,
        y: at.y
    });
    graph.applyInsertedLayout(graph.controller.insert_subgraph_template(file, at.x, at.y));
}

function enterSubgraph(graph, id) {
    if (!id || id === graph.currentSubgraph)
        return;
    var info = graph.subgraphInfo(id);
    if (!info)
        return;
    if (graph.currentSubgraph === "")
        graph.mainViewport = {
            panX: graph.panX,
            panY: graph.panY,
            zoom: graph.zoom
        };
    graph.clearSelection();
    graph.clearUndo();
    graph.quietCues();
    graph.currentSubgraph = id;
    graph.currentSubgraphName = info.name;
    graph.fitAfterRefresh = !(info.view && info.view.zoom);
    if (!graph.fitAfterRefresh) {
        graph.zoom = info.view.zoom;
        graph.panX = info.view.panX;
        graph.panY = info.view.panY;
    }
    graph.controller.set_subgraph_view(id);
}

function exitSubgraph(graph) {
    if (graph.currentSubgraph === "")
        return;
    graph.controller.save_subgraph_view_state(graph.currentSubgraph, JSON.stringify({
        panX: graph.panX,
        panY: graph.panY,
        zoom: graph.zoom
    }));
    graph.clearSelection();
    graph.clearUndo();
    graph.quietCues();
    graph.currentSubgraph = "";
    graph.currentSubgraphName = "";
    graph.fitAfterRefresh = false;
    if (graph.mainViewport) {
        graph.zoom = graph.mainViewport.zoom;
        graph.panX = graph.mainViewport.panX;
        graph.panY = graph.mainViewport.panY;
    }
    graph.controller.set_subgraph_view("");
}
