function reloadSessionData(graph) {
    graph.elements.viewportSaveTimer.stop();
    graph.viewportLoaded = false;
    graph.layoutLoaded = false;
    graph.hiddenLoaded = false;
    graph.pinnedLoaded = false;
    graph.nodeStylesLoaded = false;
    graph.savedLayout = {};
    graph.hiddenNodes = {};
    graph.snapSuspended = {};
    graph.snapUnanchoredSince = {};
    graph.snapSeenAnchored = {};
    graph.removedMeterKeys = {};
    graph.pendingMeterAttach = [];
    graph.pinnedNodes = {};
    graph.nodePositions = {};
    graph.placedKeyById = {};
    graph.placedBaseById = {};
    graph.appearedAt = {};
    graph.autoPlaced = {};
    graph.awaitingLinkPlacement = {};
    graph.selectedNodes = {};
    graph.selectedLinks = {};
    graph.selectedBlocks = {};
    graph.clearUndo();
    graph.quietCues();
    graph.defaultNodeKey = "";
    graph.currentSubgraph = "";
    graph.currentSubgraphName = "";
    graph.mainViewport = null;
    graph.zoom = 1.0;
    graph.panX = 0;
    graph.panY = 0;
    graph.refreshData();
}

function showToast(graph, message) {
    graph.elements.toastText.text = message;
    graph.elements.toast.opacity = 1;
    graph.elements.toastTimer.restart();
}

function portSortCmp(graph, a, b) {
    var am = a.mediaType === "Midi" ? 0 : 1;
    var bm = b.mediaType === "Midi" ? 0 : 1;
    return am !== bm ? am - bm : graph.naturalCmp(a.sortName || a.name, b.sortName || b.name);
}

function getSortedPorts(graph, nodeId) {
    return graph.sortedPortsByNode[nodeId] || {
        inputs: [],
        outputs: []
    };
}

function linkSig(graph, link) {
    return {
        o: link.outputPortId,
        i: link.inputPortId,
        on: link.outputNodeId,
        inn: link.inputNodeId
    };
}

function linkMatchesSig(graph, link, sig) {
    return !!link && !!sig && link.outputPortId === sig.o && link.inputPortId === sig.i && link.outputNodeId === sig.on && link.inputNodeId === sig.inn;
}

function nodeSig(graph, node) {
    return {
        key: (node && node.layoutKey) || ""
    };
}

function findLinkData(graph, linkId) {
    for (var i = 0; i < graph.links.length; i++) {
        if (graph.links[i].id === linkId)
            return graph.links[i];
    }
    return null;
}

function naturalCmp(graph, a, b) {
    var ax = a.split(/(\d+)/), bx = b.split(/(\d+)/);
    for (var i = 0; i < Math.min(ax.length, bx.length); i++) {
        var an = parseInt(ax[i]), bn = parseInt(bx[i]);
        if (!isNaN(an) && !isNaN(bn)) {
            if (an !== bn)
                return an - bn;
        } else {
            if (ax[i] < bx[i])
                return -1;
            if (ax[i] > bx[i])
                return 1;
        }
    }
    return ax.length - bx.length;
}

function refreshData(graph) {
    graph.deferSaveDepth++;
    try {
        graph.refreshDataNow();
    } finally {
        graph.deferSaveDepth--;
        if (graph.deferSaveDepth === 0) {
            if (graph.pendingLayoutJson !== null) {
                var layoutJson = graph.pendingLayoutJson;
                graph.pendingLayoutJson = null;
                graph.controller.save_layout(layoutJson);
            }
            if (graph.pendingPinnedJson !== null) {
                var pinnedJson = graph.pendingPinnedJson;
                graph.pendingPinnedJson = null;
                graph.controller.save_pinned(pinnedJson);
            }
        }
    }
}

function refreshDataNow(graph) {
    var sgView = graph.controller.get_subgraph_view ? graph.controller.get_subgraph_view() : "";
    if (sgView !== graph.currentSubgraph) {
        if (sgView === "" && graph.mainViewport) {
            graph.zoom = graph.mainViewport.zoom;
            graph.panX = graph.mainViewport.panX;
            graph.panY = graph.mainViewport.panY;
        }
        graph.currentSubgraph = sgView;
        var sgInfo = sgView !== "" ? graph.subgraphInfo(sgView) : null;
        graph.currentSubgraphName = sgInfo ? sgInfo.name : "";
    }
    if (!graph.viewportLoaded) {
        try {
            var vp = JSON.parse(graph.controller.get_viewport_json());
            if (vp.panX !== undefined)
                graph.panX = vp.panX;
            if (vp.panY !== undefined)
                graph.panY = vp.panY;
            if (vp.zoom !== undefined)
                graph.zoom = vp.zoom;
        } catch (e) {}
        graph.viewportLoaded = true;
    }

    if (!graph.layoutLoaded) {
        try {
            graph.savedLayout = JSON.parse(graph.controller.get_layout_json());
        } catch (e) {
            graph.savedLayout = {};
        }
        graph.layoutLoaded = true;
        try {
            var defNode = graph.controller.get_default_node();
            if (defNode)
                graph.defaultNodeKey = defNode;
        } catch (e) {}
    }
    if (!graph.hiddenLoaded) {
        try {
            var hiddenArr = JSON.parse(graph.controller.get_hidden_json());
            var hSet = {};
            for (var hi = 0; hi < hiddenArr.length; hi++) {
                hSet[hiddenArr[hi]] = true;
            }
            graph.hiddenNodes = hSet;
        } catch (e) {
            graph.hiddenNodes = {};
        }
        try {
            var docked = JSON.parse(graph.controller.get_docked_meters_json());
            graph.dockedMeters = docked && typeof docked === "object" && !Array.isArray(docked) ? docked : {};
        } catch (e) {
            graph.dockedMeters = {};
        }
        for (var dockedKey in graph.dockedMeters)
            graph.hiddenNodes[dockedKey] = true;
        graph.hiddenLoaded = true;
    }
    if (!graph.pinnedLoaded) {
        try {
            var pinnedArr = JSON.parse(graph.controller.get_pinned_json());
            var pSet = {};
            for (var pi = 0; pi < pinnedArr.length; pi++) {
                pSet[pinnedArr[pi]] = true;
            }
            graph.pinnedNodes = pSet;
        } catch (e) {
            graph.pinnedNodes = {};
        }
        graph.pinnedLoaded = true;
    }
    if (!graph.nodeStylesLoaded) {
        try {
            var st = JSON.parse(graph.controller.get_node_styles_json());
            graph.nodeStyles = st && typeof st === "object" && !Array.isArray(st) ? st : {};
        } catch (e) {
            graph.nodeStyles = {};
        }
        graph.nodeStylesLoaded = true;
    }

    try {
        var nodesJson = graph.controller.get_nodes_json();
        var linksJson = graph.controller.get_links_json();
        graph.nodes = JSON.parse(nodesJson);
        graph.links = JSON.parse(linksJson);
    } catch (e) {
        graph.nodes = [];
        graph.links = [];
    }
    graph.updateMeterNodeIds();
    graph.refreshPresetInfo();
    try {
        graph.blockedPairs = JSON.parse(graph.controller.get_blocked_pairs_json());
    } catch (e) {
        graph.blockedPairs = [];
    }
    graph.pruneSelectedBlocks();

    var newPorts = {};
    for (var i = 0; i < graph.nodes.length; i++) {
        try {
            newPorts[graph.nodes[i].id] = JSON.parse(graph.controller.get_ports_json(graph.nodes[i].id));
        } catch (e) {
            newPorts[graph.nodes[i].id] = [];
        }
    }
    graph.portsByNode = newPorts;
    if (graph.insertPreview && graph.dragNodeId >= 0) {
        graph.insertPreview = null;
        graph.updateInsertPreview(graph.dragNodeId);
    }

    var newConnected = {};
    for (var ci = 0; ci < graph.links.length; ci++) {
        newConnected[graph.links[ci].outputPortId] = true;
        newConnected[graph.links[ci].inputPortId] = true;
    }
    graph.connectedPorts = newConnected;

    var newSorted = {};
    var newHeights = {};
    var newLayouts = {};
    for (var si = 0; si < graph.nodes.length; si++) {
        var sn = graph.nodes[si];
        var sp = newPorts[sn.id] || [];
        var sorted = {
            inputs: sp.filter(function (p) {
                return p.direction === "Input";
            }).sort(function (a, b) {
                return graph.portSortCmp(a, b);
            }),
            outputs: sp.filter(function (p) {
                return p.direction === "Output";
            }).sort(function (a, b) {
                return graph.portSortCmp(a, b);
            })
        };
        newSorted[sn.id] = sorted;
        var lay = graph.computeNodeLayout(sn, sorted);
        newLayouts[sn.id] = lay;
        newHeights[sn.id] = lay.height;
    }
    graph.sortedPortsByNode = newSorted;
    graph.nodeLayouts = newLayouts;
    graph.nodeHeights = newHeights;
    graph.updateSnapStates();
    graph.uniqueKeyById = graph.buildUniqueLayoutKeys();
    graph.calculateNodeWidths();
    graph.widthsDirty = true;

    var newPortMedia = {};
    for (var nid in newPorts) {
        var pp = newPorts[nid];
        for (var pi2 = 0; pi2 < pp.length; pi2++) {
            newPortMedia[pp[pi2].id] = pp[pi2].mediaType || "Unknown";
        }
    }
    graph.portMediaTypes = newPortMedia;
    graph.dragSourcePortId = -1;

    var liveIds = {};
    for (var lv = 0; lv < graph.nodes.length; lv++)
        liveIds[graph.nodes[lv].id] = true;
    for (var stale in graph.nodePositions) {
        if (!liveIds[stale])
            delete graph.nodePositions[stale];
    }
    for (var staleA in graph.awaitingLinkPlacement) {
        if (!liveIds[staleA])
            delete graph.awaitingLinkPlacement[staleA];
    }
    for (var staleP in graph.autoPlaced) {
        if (!liveIds[staleP])
            delete graph.autoPlaced[staleP];
    }
    for (var staleK in graph.placedKeyById) {
        if (!liveIds[staleK])
            delete graph.placedKeyById[staleK];
    }
    for (var staleB in graph.placedBaseById) {
        if (!liveIds[staleB])
            delete graph.placedBaseById[staleB];
    }
    for (var staleT in graph.appearedAt) {
        if (!liveIds[staleT])
            delete graph.appearedAt[staleT];
    }

    var liveNodeById = {};
    for (var ln = 0; ln < graph.nodes.length; ln++)
        liveNodeById[graph.nodes[ln].id] = graph.nodes[ln];
    var keptNodes = {};
    var nodesPruned = false;
    for (var selN in graph.selectedNodes) {
        var selNode = liveNodeById[selN];
        if (selNode && graph.selectedNodes[selN] && graph.selectedNodes[selN].key === (selNode.layoutKey || "")) {
            keptNodes[selN] = graph.selectedNodes[selN];
        } else {
            nodesPruned = true;
        }
    }
    if (nodesPruned)
        graph.selectedNodes = keptNodes;

    var liveLinkById = {};
    for (var lk = 0; lk < graph.links.length; lk++)
        liveLinkById[graph.links[lk].id] = graph.links[lk];
    var keptLinks = {};
    var linksPruned = false;
    for (var selL in graph.selectedLinks) {
        if (graph.linkMatchesSig(liveLinkById[selL], graph.selectedLinks[selL])) {
            keptLinks[selL] = graph.selectedLinks[selL];
        } else {
            linksPruned = true;
        }
    }
    if (linksPruned)
        graph.selectedLinks = keptLinks;

    var bypassPruned = false;
    for (var bpId in graph.localBypassState) {
        var bpNode = liveNodeById[bpId];
        if (!bpNode || bpNode.pluginBypassed !== undefined) {
            delete graph.localBypassState[bpId];
            bypassPruned = true;
        }
    }
    if (bypassPruned)
        graph.localBypassState = graph.localBypassState;
    var enabledPruned = false;
    for (var enId in graph.localEnabledState) {
        var enNode = liveNodeById[enId];
        if (!enNode || enNode.pluginEnabled !== undefined) {
            delete graph.localEnabledState[enId];
            enabledPruned = true;
        }
    }
    if (enabledPruned)
        graph.localEnabledState = graph.localEnabledState;

    graph.refreshSeq++;
    var nowMs = Date.now();
    var appeared = [];
    for (var ni = 0; ni < graph.nodes.length; ni++) {
        var n = graph.nodes[ni];
        var key = graph.uniqueKeyById[n.id] || "";
        var baseKey = n.layoutKey || "";
        if (n.id in graph.nodePositions) {
            var placedKey = graph.placedKeyById[n.id];
            if (placedKey !== undefined && placedKey !== key) {
                if (graph.placedBaseById[n.id] !== baseKey && key && graph.savedLayout[key]) {
                    var resaved = graph.savedLayout[key];
                    graph.nodePositions[n.id] = {
                        x: resaved[0],
                        y: resaved[1]
                    };
                    delete graph.autoPlaced[n.id];
                    delete graph.awaitingLinkPlacement[n.id];
                } else {
                    graph.persistLayout();
                }
                graph.placedKeyById[n.id] = key;
                graph.placedBaseById[n.id] = baseKey;
            }
            continue;
        }
        graph.placedKeyById[n.id] = key;
        graph.placedBaseById[n.id] = baseKey;
        graph.appearedAt[n.id] = {
            t: nowMs,
            seq: graph.refreshSeq
        };
        appeared.push(n.id);

        if (key && graph.savedLayout[key]) {
            var saved = graph.savedLayout[key];
            var restored = {
                x: saved[0],
                y: saved[1]
            };
            graph.nodePositions[n.id] = restored;
            if (!graph.nodeIsPinned(n) && !(n.layoutKey && graph.hiddenNodes[n.layoutKey]) && graph.spotIsTaken(n, restored.x, restored.y)) {
                graph.nodePositions[n.id] = graph.findFreeSpot(n, restored.x, restored.y, 0, false);
                graph.persistLayout();
            }
        } else if (n.type === "Plugin" && graph.pendingPluginPosition && (graph.pendingPluginName === "" || n.name === graph.pendingPluginName)) {
            var anchor = graph.pendingPluginPosition;
            graph.pendingPluginPosition = null;
            graph.pendingPluginName = "";
            graph.nodePositions[n.id] = {
                x: anchor.x,
                y: anchor.y
            };
            var spot = graph.findFreeSpot(n, anchor.x, anchor.y, 0, true);
            if (spot) {
                graph.nodePositions[n.id] = spot;
                graph.persistLayout();
            } else {
                graph.startPlacing(n.id);
            }
        } else {
            graph.autoPlaced[n.id] = true;
            if (!graph.placeNearPeer(n)) {
                var pa = graph.provisionalAnchor(n);
                graph.nodePositions[n.id] = graph.findFreeSpot(n, pa.x, pa.y, pa.dir, false);
                graph.persistLayout();
                if (!graph.nodeIsPinned(n)) {
                    graph.awaitingLinkPlacement[n.id] = true;
                    graph.awaitingLinkPlacement = graph.awaitingLinkPlacement;
                }
            }
        }

        if (n.type === "Plugin" && n.layoutKey && !graph.pinnedNodes[n.layoutKey]) {
            graph.pinnedNodes[n.layoutKey] = true;
            graph.pinnedNodes = graph.pinnedNodes;
            graph.persistPinned();
        }
    }

    var stillAwaiting = {};
    for (var aid in graph.awaitingLinkPlacement) {
        var anode = graph.findNodeData(parseInt(aid));
        if (!anode)
            continue;
        if (graph.nodeIsPinned(anode) || !graph.isFresh(anode.id) || graph.nodeIdIsHidden(anode.id))
            continue;
        if (graph.placeNearPeer(anode))
            continue;
        if (!graph.nodeHasLinks(anode.id))
            stillAwaiting[aid] = true;
    }
    graph.awaitingLinkPlacement = stillAwaiting;

    for (var bid in graph.autoPlaced) {
        if (graph.awaitingLinkPlacement[bid])
            continue;
        var bnode = graph.findNodeData(parseInt(bid));
        if (!bnode || graph.nodeIsPinned(bnode) || graph.nodeIdIsHidden(bnode.id))
            continue;
        if (!graph.isFresh(bnode.id)) {
            delete graph.autoPlaced[bid];
            continue;
        }
        if (graph.nodeHasBackwardLink(parseInt(bid))) {
            delete graph.autoPlaced[bid];
            graph.placeNearPeer(bnode);
        }
    }

    graph.noteAppeared(appeared);
    graph.processPendingMeterAttach();

    if (!graph.initialViewChecked && graph.nodes.length > 0 && graph.width > 0 && graph.height > 0) {
        graph.initialViewChecked = true;
        var b = graph.contentBounds();
        if (b && (b.right * graph.zoom + graph.panX < 0 || b.left * graph.zoom + graph.panX > graph.width || b.bottom * graph.zoom + graph.panY < 0 || b.top * graph.zoom + graph.panY > graph.height))
            graph.fitToView();
    }

    if (graph.fitAfterRefresh && graph.nodes.length > 0 && graph.width > 0 && graph.height > 0) {
        graph.fitAfterRefresh = false;
        Qt.callLater(graph.fitToView);
    }

    graph.elements.canvas.requestPaint();
    graph.elements.repaintTimer.restart();
}
