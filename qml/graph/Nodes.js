function persistHidden(graph) {
    var arr = [];
    for (var k in graph.hiddenNodes) {
        if (graph.hiddenNodes[k])
            arr.push(k);
    }
    for (var sk in graph.snapSuspended) {
        if (graph.dockedMeters[sk] && !graph.hiddenNodes[sk])
            arr.push(sk);
    }
    graph.controller.save_hidden(JSON.stringify(arr));
    var pruned = false;
    for (var dk in graph.dockedMeters) {
        if (!graph.hiddenNodes[dk] && !graph.snapSuspended[dk]) {
            delete graph.dockedMeters[dk];
            delete graph.snapUnanchoredSince[dk];
            pruned = true;
        }
    }
    if (pruned) {
        graph.dockedMeters = graph.dockedMeters;
        graph.persistDocked();
    }
}

function nodeStyle(graph, node) {
    return node && node.layoutKey ? (graph.nodeStyles[node.layoutKey] || null) : null;
}

function setNodeStyle(graph, node, field, value) {
    if (!node || !node.layoutKey)
        return;
    var key = node.layoutKey;
    var entry = graph.nodeStyles[key] || {};
    if (value === undefined || value === null || value === false || value === "")
        delete entry[field];
    else
        entry[field] = value;
    if (Object.keys(entry).length > 0)
        graph.nodeStyles[key] = entry;
    else
        delete graph.nodeStyles[key];
    graph.nodeStyles = graph.nodeStyles;
    graph.controller.save_node_styles(JSON.stringify(graph.nodeStyles));
    var lay = graph.computeNodeLayout(node, graph.getSortedPorts(node.id));
    graph.nodeLayouts[node.id] = lay;
    graph.nodeHeights[node.id] = lay.height;
    graph.elements.canvas.requestPaint();
}

function customNodeColor(graph, node) {
    var st = graph.nodeStyle(node);
    return st && st.color && graph.nodeColorTokens.indexOf(st.color) >= 0 ? graph.theme[st.color] : null;
}

function nodeColor(graph, node) {
    return graph.customNodeColor(node) || graph.nodeCategory(node).color;
}

function isConnectedOnly(graph, node) {
    var st = graph.nodeStyle(node);
    return !!(st && st.connectedOnly);
}

function persistPinned(graph) {
    var arr = [];
    for (var k in graph.pinnedNodes) {
        if (graph.pinnedNodes[k])
            arr.push(k);
    }
    var json = JSON.stringify(arr);
    if (graph.deferSaveDepth > 0)
        graph.pendingPinnedJson = json;
    else
        graph.controller.save_pinned(json);
}

function isNodeBypassed(graph, node) {
    if (node.id in graph.localBypassState)
        return graph.localBypassState[node.id];
    return node.pluginBypassed === true;
}

function isNodeEnabled(graph, node) {
    if (node.id in graph.localEnabledState)
        return graph.localEnabledState[node.id];
    return node.pluginEnabled !== false;
}

function findNodeData(graph, nodeId) {
    for (var i = 0; i < graph.nodes.length; i++) {
        if (graph.nodes[i].id === nodeId)
            return graph.nodes[i];
    }
    return null;
}

function getNodeColumn(graph, type) {
    if (!type)
        return "stream";
    if (type === "Source")
        return "source";
    if (type === "Sink" || type === "Duplex")
        return "sink";
    return "stream";
}

function nodeCategory(graph, node) {
    var type = node.type;
    if (node.mediaType === "Midi" || (node.layoutKey || "").indexOf("MidiBridge:") === 0)
        return {
            color: graph.theme.catMidi,
            glyph: "din"
        };
    if (type === "Plugin")
        return {
            color: graph.theme.catPlugin,
            glyph: "plug"
        };
    if (type === "Subgraph")
        return {
            color: graph.theme.catSubgraph,
            glyph: "subgraph"
        };
    if (type === "SubgraphInput" || type === "SubgraphOutput")
        return {
            color: graph.theme.catSubgraph,
            glyph: "arrow"
        };
    if (type === "StreamOutput" || type === "StreamInput")
        return {
            color: graph.theme.catApp,
            glyph: "wave"
        };
    if (node.mediaType === "Video")
        return {
            color: graph.theme.catHardware,
            glyph: "camera"
        };
    if (node.isVirtual)
        return {
            color: graph.theme.catVirtual,
            glyph: type === "Source" ? "mic" : "speaker"
        };
    if (type === "Source")
        return {
            color: graph.theme.catHardware,
            glyph: "mic"
        };
    if (type === "Sink" || type === "Duplex")
        return {
            color: graph.theme.catHardware,
            glyph: "headphones"
        };
    return {
        color: graph.theme.catHardware,
        glyph: "speaker"
    };
}

function signalColor(graph, mediaType) {
    if (mediaType === "Midi")
        return graph.theme.sigMidi;
    if (mediaType === "Video")
        return graph.theme.sigVideo;
    return graph.theme.sigAudio;
}

function visiblePortCount(graph, ports) {
    var last = -1;
    for (var i = 0; i < ports.length; i++) {
        if (graph.connectedPorts[ports[i].id])
            last = i;
    }
    return Math.min(ports.length, Math.max(last + 1, 3));
}

function isMeterNode(graph, n) {
    return !!n && n.type === "Plugin" && (n.meterChannels || 0) > 0;
}

function computeNodeLayout(graph, node, sorted) {
    var ins = sorted.inputs;
    var outs = sorted.outputs;
    var hiddenIn = 0;
    var hiddenOut = 0;
    var meter = graph.isMeterNode(node);
    if (!meter && graph.isConnectedOnly(node)) {
        ins = ins.filter(function (p) {
            return !!graph.connectedPorts[p.id];
        });
        outs = outs.filter(function (p) {
            return !!graph.connectedPorts[p.id];
        });
    } else if (node.type === "Plugin" && !meter) {
        hiddenIn = ins.length - graph.visiblePortCount(ins);
        hiddenOut = outs.length - graph.visiblePortCount(outs);
        if (hiddenIn < graph.collapseMinHidden)
            hiddenIn = 0;
        if (hiddenOut < graph.collapseMinHidden)
            hiddenOut = 0;
    }
    var expandable = hiddenIn + hiddenOut > 0;
    var expanded = expandable && !!(node.layoutKey && graph.expandedNodes[node.layoutKey]);
    var visIn = expandable && !expanded ? ins.slice(0, ins.length - hiddenIn) : ins;
    var visOut = expandable && !expanded ? outs.slice(0, outs.length - hiddenOut) : outs;
    var rows = Math.max(visIn.length, visOut.length, 1);
    var h = graph.theme.nodeHeader + graph.theme.nodePortsTop + graph.theme.nodePortRow * (rows + (expandable ? 1 : 0)) + graph.theme.nodePortsBottom + 2;
    if ((node.type === "Plugin" && !meter) || node.type === "Subgraph")
        h += graph.theme.nodeFooter;
    if (node.type === "Plugin" && !meter)
        h += graph.theme.nodePresetRow;
    return {
        inputs: visIn,
        outputs: visOut,
        rows: rows,
        expandable: expandable,
        expanded: expanded,
        hiddenIn: hiddenIn,
        hiddenOut: hiddenOut,
        height: h
    };
}

function nodeLayout(graph, node) {
    return graph.nodeLayouts[node.id] || graph.computeNodeLayout(node, graph.getSortedPorts(node.id));
}

function calculateNodeHeight(graph, node) {
    var cached = graph.nodeHeights[node.id];
    if (cached !== undefined)
        return cached;
    return graph.nodeLayout(node).height;
}

function calculateNodeWidths(graph) {
    var newWidths = {};
    for (var ni = 0; ni < graph.nodes.length; ni++) {
        var node = graph.nodes[ni];
        newWidths[node.id] = node.type === "Plugin" || node.type === "Subgraph" ? graph.theme.nodeWidthPlugin : graph.theme.nodeWidth;
    }
    graph.nodeWidths = newWidths;
}

function getNodeWidth(graph, nodeId) {
    return graph.nodeWidths[nodeId] || graph.minNodeWidth;
}

function toggleExpanded(graph, node) {
    if (!node.layoutKey)
        return;
    if (graph.expandedNodes[node.layoutKey])
        delete graph.expandedNodes[node.layoutKey];
    else
        graph.expandedNodes[node.layoutKey] = true;
    graph.expandedNodes = graph.expandedNodes;
    var lay = graph.computeNodeLayout(node, graph.getSortedPorts(node.id));
    graph.nodeLayouts[node.id] = lay;
    graph.nodeHeights[node.id] = lay.height;
    graph.elements.canvas.requestPaint();
}

function nodeMatchesSearch(graph, node) {
    if (graph.searchText.length === 0)
        return true;
    var q = graph.searchText.toLowerCase();
    if ((node.name || "").toLowerCase().indexOf(q) >= 0)
        return true;
    var ports = graph.portsByNode[node.id] || [];
    for (var i = 0; i < ports.length; i++) {
        if ((ports[i].name || "").toLowerCase().indexOf(q) >= 0)
            return true;
    }
    return false;
}

function portMatchesSearch(graph, port) {
    return graph.searchText.length > 0 && (port.name || "").toLowerCase().indexOf(graph.searchText.toLowerCase()) >= 0;
}

function connectFromNodeId(graph) {
    if (graph.dragSourcePortId !== graph.connectFromPortId) {
        graph.dragSourceNodeId = graph.getPortNodeId(graph.connectFromPortId);
        graph.dragSourcePortId = graph.connectFromPortId;
    }
    return graph.dragSourceNodeId;
}

function isValidDropTarget(graph, port, nodeId) {
    if (graph.connectFromPortId < 0 || port.id === graph.connectFromPortId)
        return false;
    if (port.direction === graph.connectFromDir)
        return false;
    if (graph.connectFromNodeId() === nodeId)
        return false;
    var fromMt = graph.portMediaTypes[graph.connectFromPortId];
    return !fromMt || fromMt === "Unknown" || !port.mediaType || port.mediaType === fromMt;
}
