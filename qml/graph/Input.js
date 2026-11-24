function setDragMode(graph, mode) {
    if ((mode !== "grab" && mode !== "select") || mode === graph.dragMode)
        return;
    graph.dragMode = mode;
    graph.controller.set_preference("drag_mode", mode);
}

function toggleDragMode(graph) {
    if (graph.grabPanning || graph.spacePanning || graph.selectDragging)
        return;
    graph.setDragMode(graph.dragMode === "grab" ? "select" : "grab");
}

function updateHover(graph, sx, sy) {
    var port = -1;
    var link = -1;
    var btn = "";
    var empty = false;
    var blockUnder = null;
    var nodeUnder = -1;
    if (sx >= 0 && graph.connectFromPortId < 0 && graph.dragNodeId < 0 && !graph.groupDragging) {
        port = graph.findPortAt(sx, sy);
        nodeUnder = graph.findNodeAt(sx, sy);
        if (nodeUnder < 0 && port >= 0 && graph.portPositions[port])
            nodeUnder = graph.portPositions[port].nodeId;
        if (port < 0) {
            var hit = graph.findButtonAt(sx, sy);
            if (hit) {
                btn = hit.nodeId + ":" + hit.button;
            } else if (nodeUnder < 0) {
                var wire = graph.wireAt(sx, sy);
                link = wire.link;
                blockUnder = wire.block;
                empty = link < 0 && !blockUnder;
            }
        }
    }
    var blockKeyUnder = blockUnder ? graph.blockKey(blockUnder) : "";
    var nodeChanged = nodeUnder !== graph.hoveredNodeId;
    var changed = nodeChanged || blockKeyUnder !== graph.hoveredBlockKey || port !== graph.hoveredPortId || link !== graph.hoveredLinkId || btn !== graph.hoveredButton;
    graph.hoveredBlockKey = blockKeyUnder;
    graph.hoveredPortId = port;
    graph.hoveredLinkId = link;
    graph.hoveredButton = btn;
    graph.hoveredNodeId = nodeUnder;
    graph.hoverEmpty = empty;
    graph.updateCursor();
    if (nodeChanged || port >= 0 || btn !== "") {
        graph.hideNameTip();
        if (nodeUnder >= 0 && port < 0 && btn === "" && graph.nameTruncated[nodeUnder])
            graph.elements.nameTimer.restart();
    }
    if (changed) {
        if (port >= 0 && graph.portPositions[port]) {
            var pp = graph.portPositions[port];
            var sp = graph.toScreen(pp.cx, pp.cy);
            graph.elements.pinTip.text = graph.portName(port);
            graph.elements.pinTip.x = sp.x + 10;
            graph.elements.pinTip.y = sp.y + 10;
            graph.elements.pinTip.open();
        } else if (btn.indexOf(":pin") > 0) {
            var pn = graph.findNodeData(parseInt(btn));
            var pnPos = pn ? graph.nodePositions[pn.id] : null;
            if (pnPos) {
                var pinned = !!(pn.layoutKey && graph.pinnedNodes[pn.layoutKey]);
                var r = graph.pinRect(pn, pnPos.x, pnPos.y);
                var tp = graph.toScreen(r.x + r.w, r.y + r.h);
                graph.elements.pinTip.text = pinned ? "Pinned \u2014 click to unpin" : "Pin (keep this position)";
                graph.elements.pinTip.x = tp.x;
                graph.elements.pinTip.y = tp.y + 4;
                graph.elements.pinTip.open();
            }
        } else if (btn.indexOf(":ports") > 0) {
            var fnode = graph.findNodeData(parseInt(btn));
            var fPos = fnode ? graph.nodePositions[fnode.id] : null;
            if (fPos) {
                var fr = graph.portsRect(fnode, fPos.x, fPos.y);
                var ftp = graph.toScreen(fr.x + fr.w, fr.y + fr.h);
                graph.elements.pinTip.text = graph.isConnectedOnly(fnode) ? "Showing connected ports only — click to show all" : "Show connected ports only";
                graph.elements.pinTip.x = ftp.x;
                graph.elements.pinTip.y = ftp.y + 4;
                graph.elements.pinTip.open();
            }
        } else if (blockUnder) {
            graph.elements.pinTip.text = "Do not connect: " + blockUnder.label + ". Click to select; Delete removes the rule.";
            graph.elements.pinTip.x = sx + 12;
            graph.elements.pinTip.y = sy + 16;
            graph.elements.pinTip.open();
        } else {
            graph.elements.pinTip.close();
        }
        graph.elements.canvas.requestPaint();
    }
}

function updateCursor(graph) {
    var shape = Qt.ArrowCursor;
    if (graph.grabPanning || graph.spacePanning || graph.middlePanning)
        shape = Qt.ClosedHandCursor;
    else if (graph.spaceHeld)
        shape = Qt.OpenHandCursor;
    else if (graph.hoveredButton !== "" || graph.hoveredPortId >= 0)
        shape = Qt.PointingHandCursor;
    else if (graph.overrideState === "select" && graph.hoverEmpty)
        shape = Qt.CrossCursor;
    else if (graph.dragMode === "grab" && graph.hoverEmpty && graph.placingNodeId < 0)
        shape = Qt.OpenHandCursor;
    if (graph.elements.mouseArea.cursorShape !== shape)
        graph.elements.mouseArea.cursorShape = shape;
}

function portName(graph, portId) {
    for (var nid in graph.portsByNode) {
        var ports = graph.portsByNode[nid];
        for (var i = 0; i < ports.length; i++) {
            if (ports[i].id === portId)
                return ports[i].name;
        }
    }
    return "";
}

function hideNameTip(graph) {
    graph.elements.nameTimer.stop();
    graph.elements.nameTip.close();
}

function showNameTip(graph) {
    var n = graph.hoveredNodeId >= 0 ? graph.findNodeData(graph.hoveredNodeId) : null;
    var pos = n ? graph.nodePositions[n.id] : null;
    if (!n || !pos || !graph.nameTruncated[n.id] || graph.hoveredPortId >= 0 || graph.hoveredButton !== "" || graph.dragNodeId >= 0 || graph.groupDragging || graph.grabPanning || graph.spacePanning || graph.middlePanning)
        return;
    graph.elements.nameTip.text = n.name;
    graph.elements.nameTip.width = Math.min(graph.elements.nameMetrics.advanceWidth + graph.elements.nameTip.leftPadding + graph.elements.nameTip.rightPadding + 2, Math.min(420, graph.width - 16));
    graph.placeNameTip(n, pos);
    graph.elements.nameTip.open();
    Qt.callLater(graph.placeNameTip, n, pos);
}

function placeNameTip(graph, n, pos) {
    var s = graph.toScreen(pos.x, pos.y);
    graph.elements.nameTip.x = Math.max(8, Math.min(graph.width - graph.elements.nameTip.width - 8, s.x));
    var above = s.y - graph.elements.nameTip.height - 6;
    graph.elements.nameTip.y = above >= 8 ? above : s.y + graph.theme.nodeHeader * graph.zoom + 4;
}

function keyShortcutOverride(graph, event) {
    if (event.key === Qt.Key_Escape && (graph.placingNodeId >= 0 || graph.connectFromPortId >= 0 || graph.hasSelection() || graph.currentSubgraph !== ""))
        event.accepted = true;
}

function keyPressed(graph, event) {
    var ctrl = !!(event.modifiers & Qt.ControlModifier);
    var shift = !!(event.modifiers & Qt.ShiftModifier);
    var plain = !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier));

    if (event.key === Qt.Key_Shift) {
        graph.shiftHeld = true;
        return;
    }
    if (event.key === Qt.Key_Space && plain) {
        if (!event.isAutoRepeat)
            graph.spaceHeld = true;
        event.accepted = true;
        return;
    }

    if (event.key === Qt.Key_Delete || event.key === Qt.Key_Backspace) {
        graph.deleteSelection();
        event.accepted = true;
    } else if (event.key === Qt.Key_Home) {
        graph.fitToView();
        event.accepted = true;
    } else if (event.key === Qt.Key_W && plain && !event.isAutoRepeat) {
        graph.toggleDragMode();
        event.accepted = true;
    } else if (event.key === Qt.Key_F && plain && !event.isAutoRepeat) {
        graph.frameSelection();
        event.accepted = true;
    } else if ((event.key === Qt.Key_Question || (event.key === Qt.Key_Slash && shift)) && !ctrl) {
        graph.showShortcuts();
        event.accepted = true;
    } else if (event.key === Qt.Key_L && !event.isAutoRepeat && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) {
        graph.requestAutoLayout(shift);
        event.accepted = true;
    } else if (ctrl && event.key === Qt.Key_A) {
        graph.selectAll();
        event.accepted = true;
    } else if (ctrl && event.key === Qt.Key_Z) {
        if (shift)
            graph.redo();
        else
            graph.undo();
        event.accepted = true;
    } else if (ctrl && event.key === Qt.Key_Y) {
        graph.redo();
        event.accepted = true;
    } else if (ctrl && (event.key === Qt.Key_Plus || event.key === Qt.Key_Equal)) {
        graph.zoomAt(graph.width / 2, graph.height / 2, graph.zoom * 1.1);
        event.accepted = true;
    } else if (ctrl && event.key === Qt.Key_Minus) {
        graph.zoomAt(graph.width / 2, graph.height / 2, graph.zoom / 1.1);
        event.accepted = true;
    } else if (ctrl && event.key === Qt.Key_0) {
        graph.zoomAt(graph.width / 2, graph.height / 2, 1.0);
        event.accepted = true;
    } else if (!ctrl && (event.key === Qt.Key_Left || event.key === Qt.Key_Right || event.key === Qt.Key_Up || event.key === Qt.Key_Down)) {
        var ndx = event.key === Qt.Key_Left ? -1 : event.key === Qt.Key_Right ? 1 : 0;
        var ndy = event.key === Qt.Key_Up ? -1 : event.key === Qt.Key_Down ? 1 : 0;
        if (graph.nudgeSelection(ndx, ndy, shift))
            event.accepted = true;
    } else if (event.key === Qt.Key_Escape) {
        if (graph.placingNodeId >= 0) {
            graph.finishPlacing();
        } else if (graph.connectFromPortId >= 0) {
            graph.connectFromPortId = -1;
            graph.connectFromDir = "";
            graph.connectBlocking = false;
            graph.connectHoverPortId = -1;
            graph.connectTargetBlocked = false;
            graph.elements.canvas.requestPaint();
        } else if (graph.hasSelection()) {
            graph.clearSelection();
        } else if (graph.currentSubgraph !== "") {
            graph.exitSubgraph();
        }
        event.accepted = true;
    }
}

function keyReleased(graph, event) {
    if (event.key === Qt.Key_Shift) {
        graph.shiftHeld = false;
    } else if (event.key === Qt.Key_Space && !event.isAutoRepeat) {
        graph.spaceHeld = false;
        event.accepted = true;
    }
}

function pointerDoubleClicked(graph, mouse) {
        if (mouse.button !== Qt.LeftButton)
            return;
        var dn = graph.findNodeData(graph.findNodeAt(mouse.x, mouse.y));
        if (dn && dn.type === "Subgraph")
            graph.enterSubgraph(dn.subgraphId);
    }

function pointerPressed(graph, mouse) {
        graph.forceActiveFocus();
        graph.elements.mouseArea.lastX = mouse.x;
        graph.elements.mouseArea.lastY = mouse.y;
        graph.elements.mouseArea.pressX = mouse.x;
        graph.elements.mouseArea.pressY = mouse.y;
        graph.hideNameTip();
        graph.grabPanning = false;
        graph.spacePanning = false;
        graph.shiftHeld = !!(mouse.modifiers & Qt.ShiftModifier);

        if (graph.placingNodeId >= 0) {
            graph.finishPlacing();
            return;
        }

        if (mouse.button === Qt.MiddleButton) {
            graph.middlePanning = true;
            graph.updateCursor();
            return;
        }

        if (mouse.button === Qt.RightButton) {
            var blockPort = graph.findPortAt(mouse.x, mouse.y);
            if (blockPort >= 0) {
                graph.connectFromPortId = blockPort;
                graph.connectFromDir = graph.getPortDirection(blockPort);
                graph.connectBlocking = true;
                graph.connectPressX = mouse.x;
                graph.connectPressY = mouse.y;
                graph.connectMouseX = mouse.x;
                graph.connectMouseY = mouse.y;
                return;
            }
            var labelPort = graph.findPortLabelAt(mouse.x, mouse.y);
            if (labelPort >= 0) {
                graph.openPortMenu(labelPort);
                return;
            }
            var nodeId = graph.findNodeAt(mouse.x, mouse.y);
            var menuLinkId = nodeId < 0 ? graph.findLinkAt(mouse.x, mouse.y) : -1;
            if (menuLinkId >= 0) {
                var menuLink = graph.findLinkData(menuLinkId);
                if (graph.selectedLinks[menuLinkId]) {
                    var ids = [];
                    for (var sk in graph.selectedLinks) {
                        var sid = parseInt(sk);
                        if (graph.linkMatchesSig(graph.findLinkData(sid), graph.selectedLinks[sk]))
                            ids.push(sid);
                    }
                    graph.contextLinkIds = ids;
                } else {
                    var one = {};
                    if (menuLink)
                        one[menuLinkId] = graph.linkSig(menuLink);
                    graph.selectedLinks = one;
                    graph.selectedNodes = {};
                    graph.selectedBlocks = {};
                    graph.contextLinkIds = [menuLinkId];
                    graph.elements.canvas.requestPaint();
                }
                graph.elements.linkContextMenu.popup();
            } else if (nodeId < 0 && graph.findBlockAt(mouse.x, mouse.y)) {
                graph.contextBlock = graph.findBlockAt(mouse.x, mouse.y);
                graph.elements.canvas.requestPaint();
                graph.elements.blockContextMenu.popup();
            } else if (nodeId >= 0) {
                graph.contextNodeId = nodeId;
                graph.contextNode = graph.findNodeData(nodeId);
                graph.contextPortId = -1;
                graph.elements.nodeContextMenu.popup();
            } else {
                graph.contextNodeId = -1;
                graph.contextNode = null;
                var cPos = graph.toCanvas(mouse.x, mouse.y);
                graph.contextCanvasPos = {
                    x: cPos.x,
                    y: cPos.y
                };
                graph.elements.canvasContextMenu.popup();
            }
            return;
        }

        if (mouse.button === Qt.LeftButton) {
            if (graph.spaceHeld) {
                graph.spacePanning = true;
                graph.updateCursor();
                return;
            }

            var btnHit = graph.findButtonAt(mouse.x, mouse.y);
            if (btnHit) {
                if (btnHit.button === "toggle") {
                    var tn = graph.findNodeData(btnHit.nodeId);
                    if (tn)
                        graph.toggleExpanded(tn);
                } else if (btnHit.button === "pin") {
                    graph.togglePinned(graph.findNodeData(btnHit.nodeId));
                } else if (btnHit.button === "ports") {
                    var fn = graph.findNodeData(btnHit.nodeId);
                    graph.setNodeStyle(fn, "connectedOnly", !graph.isConnectedOnly(fn));
                } else if (btnHit.button === "onoff") {
                    var newEnabled = !btnHit.enabled;
                    graph.localEnabledState[btnHit.nodeId] = newEnabled;
                    graph.localEnabledState = graph.localEnabledState;
                    graph.controller.set_plugin_enabled(btnHit.nodeId, newEnabled);
                    graph.elements.canvas.requestPaint();
                } else if (btnHit.button === "bypass") {
                    var newBypassed = !btnHit.bypassed;
                    graph.localBypassState[btnHit.nodeId] = newBypassed;
                    graph.localBypassState = graph.localBypassState;
                    graph.controller.set_plugin_bypass(btnHit.nodeId, newBypassed);
                    graph.elements.canvas.requestPaint();
                } else if (btnHit.button === "ui" && btnHit.hasUi) {
                    graph.controller.open_plugin_ui(btnHit.nodeId);
                } else if (btnHit.button === "params") {
                    graph.openPluginParams(btnHit.nodeId);
                } else if (btnHit.button === "preset") {
                    graph.openPresetMenu(btnHit.nodeId);
                } else if (btnHit.button === "presetsave") {
                    graph.openPresetSave(btnHit.nodeId);
                } else if (btnHit.button === "open") {
                    graph.enterSubgraph(btnHit.subgraphId);
                }
                return;
            }

            var portId = graph.findPortAt(mouse.x, mouse.y);
            if (portId >= 0) {
                graph.connectFromPortId = portId;
                graph.connectFromDir = graph.getPortDirection(portId);
                graph.connectMouseX = mouse.x;
                graph.connectMouseY = mouse.y;
                return;
            }

            var nodeIdDrag = graph.findNodeAt(mouse.x, mouse.y);
            if (nodeIdDrag >= 0) {
                var ctrlHeld = (mouse.modifiers & Qt.ControlModifier);
                var nodeIsSelected = !!graph.selectedNodes[nodeIdDrag];

                if (ctrlHeld) {
                    var newSel = Object.assign({}, graph.selectedNodes);
                    if (nodeIsSelected) {
                        delete newSel[nodeIdDrag];
                    } else {
                        newSel[nodeIdDrag] = graph.nodeSig(graph.findNodeData(nodeIdDrag));
                    }
                    graph.selectedNodes = newSel;
                    graph.selectedLinks = {};
                    graph.selectedBlocks = {};
                    graph.elements.canvas.requestPaint();
                    return;
                }

                var loneMeter = nodeIsSelected && graph.isMeterNode(graph.findNodeData(nodeIdDrag)) && graph.selectedNodeIds().length === 1;
                if (nodeIsSelected && !loneMeter) {
                    graph.groupDragging = true;
                    for (var agid in graph.selectedNodes) {
                        if (graph.selectedNodes[agid])
                            graph.markHandPlaced(agid);
                    }
                    var cg = graph.toCanvas(mouse.x, mouse.y);
                    graph.groupDragStartX = cg.x;
                    graph.groupDragStartY = cg.y;
                    var starts = {};
                    for (var gsid in graph.selectedNodes) {
                        if (graph.selectedNodes[gsid] && graph.nodePositions[gsid])
                            starts[gsid] = {
                                x: graph.nodePositions[gsid].x,
                                y: graph.nodePositions[gsid].y
                            };
                    }
                    graph.groupDragStart = starts;
                } else {
                    graph.selectedNodes = {};
                    graph.selectedLinks = {};
                    graph.selectedBlocks = {};
                    graph.dragNodeId = nodeIdDrag;
                    graph.markHandPlaced(nodeIdDrag);
                    var c = graph.toCanvas(mouse.x, mouse.y);
                    var pos = graph.nodePositions[nodeIdDrag];
                    graph.dragOffsetX = c.x - pos.x;
                    graph.dragOffsetY = c.y - pos.y;
                    graph.dragStartPos = {
                        x: pos.x,
                        y: pos.y
                    };
                }
                graph.elements.canvas.requestPaint();
                return;
            }

            var clickedWire = graph.wireAt(mouse.x, mouse.y);
            var clickedLinkId = clickedWire.link;
            var ctrlHeldWire = !!(mouse.modifiers & Qt.ControlModifier);
            if (clickedWire.block) {
                var cb = clickedWire.block;
                var cbKey = graph.blockKey(cb);
                var cbSig = {
                    ruleId: cb.ruleId,
                    source: cb.source,
                    target: cb.target
                };
                if (ctrlHeldWire) {
                    var newSelBlocks = Object.assign({}, graph.selectedBlocks);
                    if (newSelBlocks[cbKey])
                        delete newSelBlocks[cbKey];
                    else
                        newSelBlocks[cbKey] = cbSig;
                    graph.selectedBlocks = newSelBlocks;
                    graph.selectedNodes = {};
                } else {
                    var freshBlocks = {};
                    freshBlocks[cbKey] = cbSig;
                    graph.selectedBlocks = freshBlocks;
                    graph.selectedLinks = {};
                    graph.selectedNodes = {};
                }
                graph.elements.canvas.requestPaint();
                return;
            }
            if (clickedLinkId >= 0) {
                var clickedLink = graph.findLinkData(clickedLinkId);
                if (ctrlHeldWire) {
                    var newSelLinks = Object.assign({}, graph.selectedLinks);
                    if (newSelLinks[clickedLinkId]) {
                        delete newSelLinks[clickedLinkId];
                    } else if (clickedLink) {
                        newSelLinks[clickedLinkId] = graph.linkSig(clickedLink);
                    }
                    graph.selectedLinks = newSelLinks;
                } else {
                    var freshSel = {};
                    if (clickedLink)
                        freshSel[clickedLinkId] = graph.linkSig(clickedLink);
                    graph.selectedLinks = freshSel;
                    graph.selectedNodes = {};
                    graph.selectedBlocks = {};
                }
                graph.elements.canvas.requestPaint();
                return;
            }

            var boxShift = !!(mouse.modifiers & Qt.ShiftModifier);
            if (graph.dragMode === "grab" && !boxShift) {
                graph.grabPanning = true;
                graph.grabMoved = false;
                graph.updateCursor();
                return;
            }
            graph.selectAdditive = boxShift || !!(mouse.modifiers & Qt.ControlModifier);
            if (graph.selectAdditive) {
                graph.selectBaseNodes = Object.assign({}, graph.selectedNodes);
                graph.selectBaseLinks = Object.assign({}, graph.selectedLinks);
            } else {
                graph.clearSelection();
            }
            graph.selectDragging = true;
            graph.selectStartX = mouse.x;
            graph.selectStartY = mouse.y;
            graph.selectEndX = mouse.x;
            graph.selectEndY = mouse.y;
        }
    }

function pointerPositionChanged(graph, mouse) {
        graph.shiftHeld = !!(mouse.modifiers & Qt.ShiftModifier);
        if (graph.placingNodeId >= 0) {
            var pc = graph.toCanvas(mouse.x, mouse.y);
            var pn = graph.findNodeData(graph.placingNodeId);
            var pw = (graph.getNodeWidth(graph.placingNodeId) || graph.minNodeWidth);
            var ph = pn ? graph.calculateNodeHeight(pn) : 80;
            graph.nodePositions[graph.placingNodeId] = {
                x: pc.x - pw / 2,
                y: pc.y - ph / 2
            };
            graph.elements.canvas.requestPaint();
            return;
        }

        if ((mouse.buttons & Qt.MiddleButton) || ((graph.grabPanning || graph.spacePanning) && (mouse.buttons & Qt.LeftButton))) {
            if (graph.grabPanning && !graph.grabMoved && (Math.abs(mouse.x - graph.elements.mouseArea.pressX) > 4 || Math.abs(mouse.y - graph.elements.mouseArea.pressY) > 4))
                graph.grabMoved = true;
            graph.panX += mouse.x - graph.elements.mouseArea.lastX;
            graph.panY += mouse.y - graph.elements.mouseArea.lastY;
            graph.clampPan();
            graph.elements.mouseArea.lastX = mouse.x;
            graph.elements.mouseArea.lastY = mouse.y;
            graph.elements.canvas.requestPaint();
            return;
        }

        if (graph.groupDragging && (mouse.buttons & Qt.LeftButton)) {
            var cg = graph.toCanvas(mouse.x, mouse.y);
            var deltaX = cg.x - graph.groupDragStartX;
            var deltaY = cg.y - graph.groupDragStartY;
            var refId = -1;
            for (var rid in graph.groupDragStart) {
                refId = parseInt(rid);
                break;
            }
            var adjX = 0;
            var adjY = 0;
            graph.activeSnapLines = [];
            if (refId >= 0) {
                var rawGX = graph.groupDragStart[refId].x + deltaX;
                var rawGY = graph.groupDragStart[refId].y + deltaY;
                if (graph.snapToGrid) {
                    var gp = graph.gridPos({
                        x: rawGX,
                        y: rawGY
                    });
                    adjX = gp.x - rawGX;
                    adjY = gp.y - rawGY;
                } else if (mouse.modifiers & Qt.ShiftModifier) {
                    var gsnap = graph.computeSnap(refId, rawGX, rawGY, graph.selectedNodes);
                    adjX = gsnap.x - rawGX;
                    adjY = gsnap.y - rawGY;
                    graph.activeSnapLines = gsnap.lines;
                }
            }
            for (var nid in graph.groupDragStart) {
                graph.nodePositions[nid] = {
                    x: graph.groupDragStart[nid].x + deltaX + adjX,
                    y: graph.groupDragStart[nid].y + deltaY + adjY
                };
            }
            graph.elements.canvas.requestPaint();
            return;
        }

        if (graph.dragNodeId >= 0 && (mouse.buttons & Qt.LeftButton)) {
            var c = graph.toCanvas(mouse.x, mouse.y);
            var rawX = c.x - graph.dragOffsetX;
            var rawY = c.y - graph.dragOffsetY;
            if (graph.snapToGrid) {
                graph.nodePositions[graph.dragNodeId] = graph.gridPos({
                    x: rawX,
                    y: rawY
                });
                graph.activeSnapLines = [];
            } else if (mouse.modifiers & Qt.ShiftModifier) {
                var snap = graph.computeSnap(graph.dragNodeId, rawX, rawY, null);
                graph.nodePositions[graph.dragNodeId] = {
                    x: snap.x,
                    y: snap.y
                };
                graph.activeSnapLines = snap.lines;
            } else {
                graph.nodePositions[graph.dragNodeId] = {
                    x: rawX,
                    y: rawY
                };
                graph.activeSnapLines = [];
            }
            graph.updateInsertPreview(graph.dragNodeId);
            var draggedMeter = graph.findNodeData(graph.dragNodeId);
            graph.snapPreview = graph.isMeterNode(draggedMeter) ? graph.meterSnapCandidate(draggedMeter, mouse.x, mouse.y) : null;
            if (graph.snapPreview)
                graph.insertPreview = null;
            graph.elements.canvas.requestPaint();
            return;
        }

        if (graph.connectFromPortId >= 0) {
            graph.connectMouseX = mouse.x;
            graph.connectMouseY = mouse.y;
            if (!graph.connectBlocking) {
                var hoverPort = graph.findPortAt(mouse.x, mouse.y);
                if (hoverPort !== graph.connectHoverPortId) {
                    graph.connectHoverPortId = hoverPort;
                    graph.connectTargetBlocked = hoverPort >= 0 && graph.getPortDirection(hoverPort) !== graph.connectFromDir && graph.blockedReasonFor(graph.connectFromPortId, graph.connectFromDir, hoverPort) !== "";
                }
            }
            graph.elements.canvas.requestPaint();
            return;
        }

        if (graph.selectDragging) {
            graph.selectEndX = mouse.x;
            graph.selectEndY = mouse.y;
            graph.elements.canvas.requestPaint();
            return;
        }

        if (mouse.buttons === Qt.NoButton)
            graph.updateHover(mouse.x, mouse.y);
    }

function pointerReleased(graph, mouse) {
        graph.activeSnapLines = [];
        if (mouse.button === Qt.MiddleButton) {
            graph.middlePanning = false;
            graph.updateCursor();
        }
        if (mouse.button === Qt.RightButton && graph.connectBlocking) {
            var moved = Math.abs(mouse.x - graph.connectPressX) > 4 || Math.abs(mouse.y - graph.connectPressY) > 4;
            var blockFrom = graph.connectFromPortId;
            var blockDir = graph.connectFromDir;
            graph.connectFromPortId = -1;
            graph.connectFromDir = "";
            graph.connectBlocking = false;
            graph.elements.canvas.requestPaint();
            if (!moved) {
                graph.openPortMenu(blockFrom);
                return;
            }
            var blockTo = graph.findPortAt(mouse.x, mouse.y);
            if (blockTo >= 0 && blockTo !== blockFrom && graph.getPortDirection(blockTo) !== blockDir && graph.getPortNodeId(blockTo) !== graph.getPortNodeId(blockFrom)) {
                if (blockDir === "Output")
                    graph.controller.block_ports(blockFrom, blockTo);
                else
                    graph.controller.block_ports(blockTo, blockFrom);
            }
            return;
        }
        if (mouse.button === Qt.LeftButton) {
            if (graph.grabPanning) {
                graph.grabPanning = false;
                if (!graph.grabMoved)
                    graph.clearSelection();
                graph.updateCursor();
            }
            if (graph.spacePanning) {
                graph.spacePanning = false;
                graph.updateCursor();
            }

            if (graph.connectFromPortId >= 0) {
                var targetId = graph.findPortAt(mouse.x, mouse.y);
                if (targetId >= 0 && targetId !== graph.connectFromPortId) {
                    var targetDir = graph.getPortDirection(targetId);
                    var fromPort = graph.snapTargetPort(graph.connectFromPortId);
                    var toPort = graph.snapTargetPort(targetId);
                    var fromNodeId = graph.getPortNodeId(fromPort);
                    var toNodeId = graph.getPortNodeId(toPort);
                    if (targetDir !== graph.connectFromDir && fromNodeId !== toNodeId && fromPort !== toPort) {
                        var why = graph.blockedReasonFor(fromPort, graph.connectFromDir, toPort);
                        var outP = graph.connectFromDir === "Output" ? fromPort : toPort;
                        var inP = graph.connectFromDir === "Output" ? toPort : fromPort;
                        var origOut = graph.connectFromDir === "Output" ? graph.connectFromPortId : targetId;
                        var origIn = graph.connectFromDir === "Output" ? targetId : graph.connectFromPortId;
                        var skipping = (outP !== origOut || inP !== origIn) ? graph.findLinkByPorts(origOut, origIn) : null;
                        if (why !== "") {
                            graph.showToast("Blocked by rule " + why + ". Remove the block to connect.");
                        } else if (skipping) {
                            graph.connectReplacing(outP, inP, skipping);
                        } else {
                            graph.connectPorts(outP, inP);
                        }
                    }
                }
                graph.connectFromPortId = -1;
                graph.connectFromDir = "";
                graph.connectHoverPortId = -1;
                graph.connectTargetBlocked = false;
                graph.elements.canvas.requestPaint();
            }

            if (graph.groupDragging) {
                graph.groupDragging = false;
                for (var gid in graph.selectedNodes) {
                    if (graph.selectedNodes[gid]) {
                        var gNode = graph.findNodeData(parseInt(gid));
                        if (gNode) {
                            graph.resolveNodeOverlap(gNode, graph.selectedNodes);
                        }
                    }
                }
                graph.recordMove("move", graph.groupDragStart);
                graph.persistLayout();
                graph.elements.canvas.requestPaint();
            }

            if (graph.dragNodeId >= 0 && graph.snapPreview && graph.snapPreview.meterId === graph.dragNodeId) {
                var sp = graph.snapPreview;
                graph.snapPreview = null;
                graph.insertPreview = null;
                if (graph.dragStartPos)
                    graph.nodePositions[graph.dragNodeId] = {
                        x: graph.dragStartPos.x,
                        y: graph.dragStartPos.y
                    };
                graph.snapMeter(graph.findNodeData(graph.dragNodeId), sp.side);
                graph.persistLayout();
                graph.elements.canvas.requestPaint();
            } else if (graph.dragNodeId >= 0) {
                var rewire = null;
                var linkUnder = graph.findLinkUnderNode(graph.dragNodeId);
                if (linkUnder >= 0) {
                    var draggedNode = graph.findNodeData(graph.dragNodeId);
                    if (draggedNode && draggedNode.type === "Plugin") {
                        rewire = graph.insertRewireEntry(linkUnder, graph.dragNodeId);
                        graph.controller.insert_node_on_link(linkUnder, graph.dragNodeId);
                    }
                }
                graph.insertPreview = null;
                var droppedNode = graph.findNodeData(graph.dragNodeId);
                if (droppedNode) {
                    graph.resolveNodeOverlap(droppedNode);
                }
                var moved = null;
                if (graph.dragStartPos) {
                    var movedFrom = {};
                    movedFrom[graph.dragNodeId] = graph.dragStartPos;
                    moved = graph.moveEntry("move", movedFrom);
                }
                if (rewire && moved)
                    graph.pushUndo({
                        kind: "group",
                        label: "insert node",
                        parts: [moved, rewire]
                    });
                else if (rewire)
                    graph.pushUndo(rewire);
                else if (moved)
                    graph.pushUndo(moved);
                graph.persistLayout();
                graph.elements.canvas.requestPaint();
            }
            graph.dragNodeId = -1;
            graph.dragStartPos = null;
            graph.snapPreview = null;

            if (graph.selectDragging) {
                graph.selectDragging = false;
                var dx = Math.abs(graph.selectEndX - graph.selectStartX);
                var dy = Math.abs(graph.selectEndY - graph.selectStartY);
                if (dx > 5 || dy > 5) {
                    var boxLinks = graph.findLinksInRect(graph.selectStartX, graph.selectStartY, graph.selectEndX, graph.selectEndY);
                    var boxNodes = graph.findNodesInRect(graph.selectStartX, graph.selectStartY, graph.selectEndX, graph.selectEndY);
                    if (graph.selectAdditive) {
                        boxLinks = Object.assign({}, graph.selectBaseLinks, boxLinks);
                        boxNodes = Object.assign({}, graph.selectBaseNodes, boxNodes);
                    }
                    graph.selectedLinks = boxLinks;
                    graph.selectedNodes = boxNodes;
                }
                graph.selectAdditive = false;
                graph.elements.canvas.requestPaint();
            }
        }
    }

function pointerWheel(graph, wheel) {
        if (wheel.angleDelta.y === 0)
            return;
        var factor = wheel.angleDelta.y > 0 ? 1.1 : 0.9;
        graph.zoomAt(wheel.x, wheel.y, graph.zoom * factor);
    }
