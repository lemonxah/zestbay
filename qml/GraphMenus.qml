import QtQuick
import QtQuick.Controls

Item {
    required property var graph
    anchors.fill: parent

    readonly property var elements: ({
        nodeContextMenu: nodeContextMenu,
        blockContextMenu: blockContextMenu,
        linkContextMenu: linkContextMenu,
        portContextMenu: portContextMenu,
        canvasContextMenu: canvasContextMenu,
        subgraphDialog: subgraphDialog,
        vdevDialog: vdevDialog,
        presetMenu: presetMenu,
        presetSaveDialog: presetSaveDialog,
        deleteSelectionDialog: deleteSelectionDialog
    })

    ZbMenu {
        id: nodeContextMenu

        ZbMenuItem {
            text: "Open subgraph"
            visible: graph.contextIsSubgraph
            height: visible ? implicitHeight : 0
            onTriggered: graph.enterSubgraph(graph.contextNode.subgraphId)
        }
        ZbMenuItem {
            text: "Edit subgraph\u2026"
            visible: graph.contextIsSubgraph
            height: visible ? implicitHeight : 0
            onTriggered: graph.openSubgraphEditor(graph.contextNode.subgraphId)
        }
        ZbMenuItem {
            text: "Duplicate"
            visible: graph.contextIsSubgraph
            height: visible ? implicitHeight : 0
            onTriggered: graph.duplicateSubgraph(graph.contextNode)
        }
        ZbMenuItem {
            text: "Save as template\u2026"
            visible: graph.contextIsSubgraph
            height: visible ? implicitHeight : 0
            onTriggered: {
                graph.templateSubgraphId = graph.contextNode.subgraphId;
                templateNameField.text = graph.contextNode.name;
                templateNameDialog.open();
            }
        }
        ZbMenuSeparator {
            visible: graph.contextIsSubgraph
            height: visible ? implicitHeight : 0
        }

        ZbMenuItem {
            readonly property int snapped: graph.contextNode ? graph.metersSnappedOn(graph.contextNode.id).length : 0
            text: snapped > 1 ? "Unsnap " + snapped + " meters" : "Unsnap meter"
            visible: snapped > 0
            height: visible ? implicitHeight : 0
            onTriggered: graph.unsnapMetersOn(graph.contextNode.id)
        }

        ZbMenuItem {
            readonly property int snapped: graph.contextNode ? graph.metersSnappedOn(graph.contextNode.id).length : 0
            text: snapped > 1 ? "Remove " + snapped + " snapped meters" : "Remove snapped meter"
            destructive: true
            visible: snapped > 0
            height: visible ? implicitHeight : 0
            onTriggered: graph.removeMetersOn(graph.contextNode.id)
        }

        ZbMenuItem {
            readonly property int count: graph.contextNode ? graph.audioPortsOf(graph.contextNode.id, false).length : 0
            text: "Meter inputs" + (count > 0 ? " (" + count + ")" : "")
            visible: graph.canMeterPorts(graph.contextNode, false)
            height: visible ? implicitHeight : 0
            onTriggered: graph.addMetersTo(graph.contextNode, false)
        }

        ZbMenuItem {
            readonly property int count: graph.contextNode ? graph.audioPortsOf(graph.contextNode.id, true).length : 0
            text: "Meter outputs" + (count > 0 ? " (" + count + ")" : "")
            visible: graph.canMeterPorts(graph.contextNode, true)
            height: visible ? implicitHeight : 0
            onTriggered: graph.addMetersTo(graph.contextNode, true)
        }

        ZbMenuItem {
            text: "Hide"
            onTriggered: {
                if (graph.contextNode && graph.contextNode.layoutKey) {
                    graph.hiddenNodes[graph.contextNode.layoutKey] = true;
                    graph.hiddenNodes = graph.hiddenNodes;
                    graph.persistHidden();
                    graph.pushUndo({
                        kind: "hide",
                        label: "hide 1 node",
                        keys: [graph.contextNode.layoutKey]
                    });
                    graph.elements.canvas.requestPaint();
                }
            }
        }

        ZbMenuItem {
            text: graph.contextNode && graph.contextNode.layoutKey && graph.pinnedNodes[graph.contextNode.layoutKey] ? "Unpin" : "Pin"
            onTriggered: graph.togglePinned(graph.contextNode)
        }

        ZbMenuItem {
            text: "Connected ports only"
            checkable: true
            checked: !!graph.contextNode && !!graph.nodeStyles && graph.isConnectedOnly(graph.contextNode)
            visible: !!graph.contextNode && !!graph.contextNode.layoutKey && !graph.contextIsMeter
            height: visible ? implicitHeight : 0
            onToggled: graph.setNodeStyle(graph.contextNode, "connectedOnly", !graph.isConnectedOnly(graph.contextNode))
        }

        ZbMenu {
            id: nodeColorMenu
            title: "Colour"
            enabled: !!graph.contextNode && !!graph.contextNode.layoutKey
            readonly property string current: graph.contextNode && graph.nodeStyles ? ((graph.nodeStyle(graph.contextNode) || {}).color || "") : ""
            ZbMenuItem {
                text: "Default"
                selected: nodeColorMenu.current === ""
                onTriggered: graph.setNodeStyle(graph.contextNode, "color", "")
            }
            ZbMenuSeparator {}
            ZbMenuItem {
                text: "Teal"
                swatch: Theme.catHardware
                selected: nodeColorMenu.current === "catHardware"
                onTriggered: graph.setNodeStyle(graph.contextNode, "color", "catHardware")
            }
            ZbMenuItem {
                text: "Blue"
                swatch: Theme.catVirtual
                selected: nodeColorMenu.current === "catVirtual"
                onTriggered: graph.setNodeStyle(graph.contextNode, "color", "catVirtual")
            }
            ZbMenuItem {
                text: "Green"
                swatch: Theme.catSubgraph
                selected: nodeColorMenu.current === "catSubgraph"
                onTriggered: graph.setNodeStyle(graph.contextNode, "color", "catSubgraph")
            }
            ZbMenuItem {
                text: "Accent"
                swatch: Theme.accent
                selected: nodeColorMenu.current === "accent"
                onTriggered: graph.setNodeStyle(graph.contextNode, "color", "accent")
            }
            ZbMenuItem {
                text: "Orange"
                swatch: Theme.catApp
                selected: nodeColorMenu.current === "catApp"
                onTriggered: graph.setNodeStyle(graph.contextNode, "color", "catApp")
            }
            ZbMenuItem {
                text: "Purple"
                swatch: Theme.catPlugin
                selected: nodeColorMenu.current === "catPlugin"
                onTriggered: graph.setNodeStyle(graph.contextNode, "color", "catPlugin")
            }
            ZbMenuItem {
                text: "Pink"
                swatch: Theme.catMidi
                selected: nodeColorMenu.current === "catMidi"
                onTriggered: graph.setNodeStyle(graph.contextNode, "color", "catMidi")
            }
            ZbMenuItem {
                text: "Red"
                swatch: Theme.textDestructive
                selected: nodeColorMenu.current === "textDestructive"
                onTriggered: graph.setNodeStyle(graph.contextNode, "color", "textDestructive")
            }
        }

        ZbMenuSeparator {
            visible: graph.contextCanBeDefault
            height: visible ? implicitHeight : 0
        }

        ZbMenuItem {
            text: graph.contextNode && graph.contextNode.layoutKey === graph.defaultNodeKey && graph.defaultNodeKey !== "" ? "Clear default" : "Set as default"
            visible: graph.contextCanBeDefault
            height: visible ? implicitHeight : 0
            onTriggered: {
                if (graph.contextNode) {
                    var key = graph.contextNode.layoutKey || "";
                    if (key === graph.defaultNodeKey && graph.defaultNodeKey !== "") {
                        graph.defaultNodeKey = "";
                        graph.controller.set_default_node("");
                    } else if (key !== "") {
                        graph.defaultNodeKey = key;
                        graph.controller.set_default_node(key);
                    }
                    graph.elements.canvas.requestPaint();
                }
            }
        }

        ZbMenuSeparator {
            visible: graph.contextIsPlugin
            height: visible ? implicitHeight : 0
        }

        ZbMenuItem {
            text: "Group into subgraph\u2026"
            visible: graph.contextIsPlugin && graph.currentSubgraph === ""
            height: visible ? implicitHeight : 0
            onTriggered: graph.groupIntoSubgraph(graph.contextNodeId)
        }

        ZbMenuItem {
            text: "Rename\u2026"
            visible: graph.contextIsPlugin
            height: visible ? implicitHeight : 0
            onTriggered: {
                renameField.text = graph.contextNode ? graph.contextNode.name : "";
                renameDialog.open();
            }
        }

        ZbMenuItem {
            text: "Snap to source port"
            visible: graph.contextIsMeter
            enabled: graph.contextIsMeter && graph.canSnapMeter(graph.contextNode, "source")
            height: visible ? implicitHeight : 0
            onTriggered: graph.snapMeter(graph.contextNode, "source")
        }

        ZbMenuItem {
            text: "Snap to destination port"
            visible: graph.contextIsMeter
            enabled: graph.contextIsMeter && graph.canSnapMeter(graph.contextNode, "dest")
            height: visible ? implicitHeight : 0
            onTriggered: graph.snapMeter(graph.contextNode, "dest")
        }

        ZbMenuItem {
            readonly property string snappedSide: graph.contextIsMeter && graph.contextNode.layoutKey ? (graph.dockedMeters[graph.contextNode.layoutKey] || "") : ""
            text: "Unsnap meter"
            visible: snappedSide !== ""
            height: visible ? implicitHeight : 0
            onTriggered: {
                var key = graph.contextNode.layoutKey;
                var prev = snappedSide;
                graph.setMeterSnapped(key, "");
                graph.pushUndo({
                    kind: "snap",
                    label: "unsnap meter",
                    key: key,
                    side: "",
                    prevSide: prev
                });
            }
        }

        ZbMenuItem {
            text: "Open UI\u2026"
            visible: graph.contextIsPlugin && !graph.contextIsMeter
            enabled: graph.contextIsPlugin && graph.contextNode.pluginHasUi !== false
            height: visible ? implicitHeight : 0
            onTriggered: {
                if (graph.contextNodeId >= 0)
                    graph.controller.open_plugin_ui(graph.contextNodeId);
            }
        }

        ZbMenuSeparator {
            visible: graph.contextIsPlugin
            height: visible ? implicitHeight : 0
        }

        ZbMenuItem {
            text: graph.contextIsMeter ? "Remove meter" : "Remove plugin"
            destructive: true
            visible: graph.contextIsPlugin
            height: visible ? implicitHeight : 0
            onTriggered: {
                if (graph.contextNodeId >= 0) {
                    graph.forgetMeterWhenGone(graph.contextNode);
                    graph.controller.remove_plugin(graph.contextNodeId);
                }
            }
        }

        ZbMenuSeparator {
            visible: graph.contextIsSubgraph || graph.contextIsVdev
            height: visible ? implicitHeight : 0
        }
        ZbMenuItem {
            text: "Ungroup"
            visible: graph.contextIsSubgraph
            height: visible ? implicitHeight : 0
            onTriggered: graph.controller.ungroup_subgraph(graph.contextNode.subgraphId)
        }
        ZbMenuItem {
            text: "Delete subgraph"
            destructive: true
            visible: graph.contextIsSubgraph
            height: visible ? implicitHeight : 0
            onTriggered: {
                deleteSubgraphDialog.subgraphId = graph.contextNode.subgraphId;
                deleteSubgraphDialog.subgraphName = graph.contextNode.name;
                deleteSubgraphDialog.open();
            }
        }
        ZbMenuItem {
            text: "Edit virtual device\u2026"
            visible: graph.contextIsVdev
            height: visible ? implicitHeight : 0
            onTriggered: graph.openVdevEditor(graph.contextNode.vdevId)
        }
        ZbMenuItem {
            text: "Remove virtual device"
            destructive: true
            visible: graph.contextIsVdev
            height: visible ? implicitHeight : 0
            onTriggered: graph.controller.remove_virtual_device(graph.contextNode.vdevId)
        }
    }

    ZbMenu {
        id: blockContextMenu
        onClosed: {
            graph.contextBlock = null;
            graph.elements.canvas.requestPaint();
        }

        ZbMenuItem {
            text: graph.contextBlock ? "Blocked: " + graph.contextBlock.label : ""
            enabled: false
        }

        ZbMenuSeparator {}

        ZbMenuItem {
            text: "Remove block"
            onTriggered: {
                if (graph.contextBlock)
                    graph.controller.remove_rule(graph.contextBlock.ruleId);
            }
        }
    }

    ZbMenu {
        id: linkContextMenu

        ZbMenuItem {
            text: graph.contextLinkIds.length > 1 ? "Disconnect " + graph.contextLinkIds.length + " links" : "Disconnect"
            shortcutHint: "Del"
            onTriggered: {
                graph.disconnectLinks(graph.contextLinkIds);
                graph.selectedLinks = {};
                graph.elements.canvas.requestPaint();
            }
        }

        ZbMenuItem {
            text: graph.contextLinkIds.length > 1 ? "Block " + graph.contextLinkIds.length + " connections" : "Block this connection"
            destructive: true
            onTriggered: graph.blockLinks(graph.contextLinkIds)
        }
    }

    ZbMenu {
        id: portContextMenu

        ZbMenuItem {
            text: "Rename port\u2026"
            visible: graph.contextIsPlugin
            height: visible ? implicitHeight : 0
            onTriggered: {
                renamePortField.text = graph.portName(graph.contextPortId);
                renamePortDialog.portId = graph.contextPortId;
                renamePortDialog.open();
            }
        }

        ZbMenuItem {
            readonly property var attached: graph.contextPortId >= 0 ? graph.visibleLinksOfPort(graph.contextPortId) : []
            text: attached.length > 1 ? "Disconnect " + attached.length + " links" : "Disconnect"
            enabled: attached.length > 0
            onTriggered: {
                graph.disconnectLinks(attached);
                graph.elements.canvas.requestPaint();
            }
        }
    }

    ZbMenu {
        id: canvasContextMenu

        ZbMenuItem {
            text: graph.canUndo ? "Undo " + graph.undoStack[graph.undoStack.length - 1].label : "Undo"
            shortcutHint: "Ctrl+Z"
            enabled: graph.canUndo
            onTriggered: graph.undo()
        }

        ZbMenuItem {
            text: graph.canRedo ? "Redo " + graph.redoStack[graph.redoStack.length - 1].label : "Redo"
            shortcutHint: "Ctrl+Shift+Z"
            enabled: graph.canRedo
            onTriggered: graph.redo()
        }

        ZbMenuSeparator {}

        ZbMenuItem {
            text: "Select all"
            shortcutHint: "Ctrl+A"
            onTriggered: graph.selectAll()
        }

        ZbMenuItem {
            text: "Delete selected links"
            enabled: {
                for (var k in graph.selectedLinks)
                    return true;
                return false;
            }
            onTriggered: graph.deleteSelectedLinks()
        }

        ZbMenuItem {
            text: "Block selected links"
            destructive: true
            enabled: {
                for (var k in graph.selectedLinks)
                    return true;
                return false;
            }
            onTriggered: graph.blockSelectedLinks()
        }

        ZbMenuSeparator {}

        ZbMenuItem {
            text: "Add plugin\u2026"
            onTriggered: {
                if (graph.contextCanvasPos) {
                    graph.pendingPluginPosition = {
                        x: graph.contextCanvasPos.x,
                        y: graph.contextCanvasPos.y
                    };
                    graph.pendingPluginName = "";
                }
                graph.openPluginBrowser();
            }
        }

        ZbMenu {
            title: "Add meter"
            ZbMenuItem {
                text: "1 channel (mono)"
                onTriggered: graph.addMeter(1)
            }
            ZbMenuItem {
                text: "2 channels (stereo)"
                onTriggered: graph.addMeter(2)
            }
            ZbMenuItem {
                text: "3 channels"
                onTriggered: graph.addMeter(3)
            }
            ZbMenuItem {
                text: "4 channels"
                onTriggered: graph.addMeter(4)
            }
            ZbMenuItem {
                text: "5 channels"
                onTriggered: graph.addMeter(5)
            }
            ZbMenuItem {
                text: "6 channels"
                onTriggered: graph.addMeter(6)
            }
            ZbMenuItem {
                text: "8 channels (7.1)"
                onTriggered: graph.addMeter(8)
            }
        }

        ZbMenuItem {
            readonly property int snapped: graph.snappedMeterCount()
            text: snapped > 1 ? "Unsnap all " + snapped + " meters" : "Unsnap meter"
            visible: snapped > 0
            height: visible ? implicitHeight : 0
            onTriggered: graph.unsnapAllMeters()
        }

        ZbMenuItem {
            text: "Group into subgraph\u2026"
            visible: graph.currentSubgraph === ""
            height: visible ? implicitHeight : 0
            enabled: graph.selectedPluginIds().length > 0
            onTriggered: graph.groupIntoSubgraph(-1)
        }

        ZbMenuItem {
            text: "Add virtual device\u2026"
            visible: graph.currentSubgraph === ""
            height: visible ? implicitHeight : 0
            onTriggered: graph.openVdevEditor("")
        }

        ZbMenuSeparator {}

        ZbMenuItem {
            text: "Unhide all"
            enabled: {
                for (var k in graph.hiddenNodes)
                    return true;
                return false;
            }
            onTriggered: {
                graph.snapSuspended = {};
                graph.snapUnanchoredSince = {};
                graph.hiddenNodes = {};
                graph.persistHidden();
                graph.elements.canvas.requestPaint();
            }
        }

        ZbMenuItem {
            text: "Fit to view"
            shortcutHint: "Home"
            onTriggered: graph.fitToView()
        }

        ZbMenuItem {
            text: "Reset zoom"
            onTriggered: graph.zoomAt(width / 2, height / 2, 1.0)
        }

        ZbMenuItem {
            text: "Auto layout"
            shortcutHint: "L"
            onTriggered: graph.requestAutoLayout(false)
        }

        ZbMenuItem {
            text: "Auto layout, including pinned"
            shortcutHint: "Shift+L"
            onTriggered: graph.requestAutoLayout(true)
        }
    }

    SubgraphDialog {
        id: subgraphDialog
        onSubmitted: cfg => {
            if (mode === "create") {
                var newId = graph.controller.create_subgraph(JSON.stringify(nodeIds), JSON.stringify(cfg));
                if (newId && graph.groupAnchor) {
                    graph.savedLayout["Subgraph:" + newId] = [graph.groupAnchor.x, graph.groupAnchor.y];
                    graph.controller.save_layout(JSON.stringify(graph.savedLayout));
                }
                graph.clearSelection();
            } else {
                graph.controller.update_subgraph(subgraphId, JSON.stringify(cfg));
            }
        }
    }

    VirtualDeviceDialog {
        id: vdevDialog
        onSubmitted: (name, kind, channels) => {
            if (vdevId === "")
                graph.controller.add_virtual_device(name, kind, channels);
            else
                graph.controller.update_virtual_device(vdevId, name, channels);
        }
    }

    ZbMenu {
        id: presetMenu

        Instantiator {
            model: graph.presetMenuNames
            delegate: ZbMenuItem {
                required property var modelData
                text: (modelData === graph.presetMenuDefault ? "\u2605 " : "") + modelData
                selected: modelData === graph.presetMenuSelected
                onTriggered: {
                    graph.controller.apply_plugin_preset(graph.presetMenuNodeId, modelData);
                    graph.presetChanged();
                }
            }
            onObjectAdded: (index, object) => presetMenu.insertItem(index, object)
            onObjectRemoved: (index, object) => presetMenu.removeItem(object)
        }

        ZbMenuItem {
            text: "No presets saved yet"
            enabled: false
            visible: graph.presetMenuNames.length === 0
            height: visible ? implicitHeight : 0
        }

        ZbMenuSeparator {}

        ZbMenuItem {
            text: "New preset (reset to defaults)"
            onTriggered: {
                graph.controller.start_new_plugin_preset(graph.presetMenuNodeId);
                graph.presetChanged();
            }
        }

        ZbMenuSeparator {}

        ZbMenuItem {
            text: "Changed: " + (graph.presetMenuChanges.length > 0 ? graph.presetMenuChanges[0].split(": ")[0] : "") + (graph.presetMenuChanges.length > 1 ? "  (+" + (graph.presetMenuChanges.length - 1) + " more)" : "")
            enabled: false
            visible: graph.presetMenuChanges.length > 0
            height: visible ? implicitHeight : 0
        }

        ZbMenuItem {
            text: "Set \u201c" + graph.presetMenuSelected + "\u201d as default"
            enabled: graph.presetMenuSelected !== "" && graph.presetMenuSelected !== graph.presetMenuDefault
            onTriggered: {
                graph.controller.set_default_plugin_preset(graph.presetMenuNodeId, graph.presetMenuSelected);
                graph.presetChanged();
            }
        }

        ZbMenuItem {
            text: "Clear default"
            enabled: graph.presetMenuDefault !== ""
            onTriggered: {
                graph.controller.set_default_plugin_preset(graph.presetMenuNodeId, "");
                graph.presetChanged();
            }
        }

        ZbMenuItem {
            text: "Delete \u201c" + graph.presetMenuSelected + "\u201d"
            destructive: true
            enabled: graph.presetMenuSelected !== ""
            onTriggered: {
                graph.controller.delete_plugin_preset(graph.presetMenuNodeId, graph.presetMenuSelected);
                graph.presetChanged();
            }
        }
    }

    ZbDialog {
        id: presetSaveDialog
        property string selectedName: ""
        readonly property string trimmed: presetNameField.text.trim()
        readonly property bool exists: graph.presetSaveNames.indexOf(trimmed) >= 0
        title: "Save preset"
        primaryText: exists ? "Replace preset" : "Save as new preset"
        secondaryText: "Cancel"
        width: 380
        primaryEnabled: trimmed.length > 0

        Column {
            width: parent ? parent.width : 320
            spacing: 8
            ZbTextField {
                id: presetNameField
                width: parent.width
                placeholderText: "New preset name"
                onAccepted: if (presetSaveDialog.primaryEnabled)
                    presetSaveDialog.accept()
            }
            Text {
                width: parent.width
                wrapMode: Text.WordWrap
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeCaption
                color: Theme.textMuted
                text: presetSaveDialog.exists ? "A preset with this name exists; saving replaces its values with the current settings." : "Saves the current parameter values as a new preset for this plugin."
            }
            ZbButton {
                visible: presetSaveDialog.selectedName !== ""
                text: "Update \u201c" + presetSaveDialog.selectedName + "\u201d with the current settings"
                onClicked: {
                    graph.controller.save_plugin_preset(graph.presetSaveNodeId, presetSaveDialog.selectedName);
                    graph.presetChanged();
                    presetSaveDialog.close();
                }
            }
        }
        onAboutToShow: presetNameField.text = ""
        onOpened: presetNameField.forceActiveFocus()
        onAccepted: {
            graph.controller.save_plugin_preset(graph.presetSaveNodeId, trimmed);
            graph.presetChanged();
        }
    }

    ZbDialog {
        id: templateNameDialog
        title: "Save as template"
        primaryText: "Save template"
        secondaryText: "Cancel"
        width: 380
        primaryEnabled: templateNameField.text.trim().length > 0

        Column {
            width: parent ? parent.width : 320
            spacing: 8
            ZbTextField {
                id: templateNameField
                width: parent.width
                placeholderText: "Template name"
                onAccepted: if (templateNameDialog.primaryEnabled)
                    templateNameDialog.accept()
            }
            Text {
                width: parent.width
                wrapMode: Text.WordWrap
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeCaption
                color: Theme.textMuted
                text: "Saves the plugins with their settings and wiring. Insert it again from Add plugin \u203a Subgraphs."
            }
        }
        onOpened: {
            templateNameField.forceActiveFocus();
            templateNameField.selectAll();
        }
        onAccepted: graph.controller.save_subgraph_template(graph.templateSubgraphId, templateNameField.text.trim())
    }

    ZbDialog {
        id: deleteSubgraphDialog
        property string subgraphId: ""
        property string subgraphName: ""
        title: "Delete subgraph?"
        primaryText: "Delete"
        secondaryText: "Cancel"
        width: 400
        Text {
            width: parent ? parent.width : 340
            wrapMode: Text.WordWrap
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            color: Theme.textPrimary
            text: "\u201c" + deleteSubgraphDialog.subgraphName + "\u201d and the plugins inside it will be removed. To keep the plugins, use Ungroup instead."
        }
        onAccepted: graph.controller.delete_subgraph(subgraphId)
    }

    ZbDialog {
        id: deleteSelectionDialog
        title: "Delete selection?"
        primaryText: "Delete"
        secondaryText: "Cancel"
        width: 420
        focus: true
        onOpened: deleteLines.forceActiveFocus()
        Column {
            id: deleteLines
            width: parent ? parent.width : 360
            spacing: 6
            Keys.onReturnPressed: deleteSelectionDialog.accept()
            Keys.onEnterPressed: deleteSelectionDialog.accept()
            Repeater {
                model: deleteSelectionDialog.visible ? graph.deletePlanLines() : []
                Text {
                    required property string modelData
                    width: parent.width
                    wrapMode: Text.WordWrap
                    font.family: Theme.fontSans
                    font.pixelSize: Theme.sizeBody
                    color: Theme.textPrimary
                    text: "\u2022  " + modelData
                }
            }
            Text {
                width: parent.width
                topPadding: 6
                wrapMode: Text.WordWrap
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeBody
                color: Theme.textMuted
                text: "Removing can\u2019t be undone."
            }
        }
        onAccepted: graph.performDelete()
        onRejected: graph.deletePlan = null
    }

    ZbDialog {
        id: renameDialog
        title: "Rename plugin"
        primaryText: "Rename"
        secondaryText: "Cancel"
        width: 360

        ZbTextField {
            id: renameField
            width: parent ? parent.width : 300
            placeholderText: "New name"
            onAccepted: renameDialog.accept()
        }

        onOpened: {
            renameField.forceActiveFocus();
            renameField.selectAll();
        }
        onAccepted: {
            var newName = renameField.text.trim();
            if (newName.length > 0 && graph.contextNodeId >= 0) {
                graph.controller.rename_plugin(graph.contextNodeId, newName);
            }
        }
    }

    ZbDialog {
        id: renamePortDialog
        property int portId: -1
        title: "Rename port"
        primaryText: "Rename"
        secondaryText: "Cancel"
        width: 360

        ZbTextField {
            id: renamePortField
            width: parent ? parent.width : 300
            placeholderText: "Port name (empty restores the original)"
            onAccepted: renamePortDialog.accept()
        }

        onOpened: {
            renamePortField.forceActiveFocus();
            renamePortField.selectAll();
        }
        onAccepted: {
            if (portId >= 0) {
                graph.controller.rename_port(portId, renamePortField.text.trim());
                graph.presetChanged();
            }
        }
    }


}
