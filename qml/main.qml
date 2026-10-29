import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ZestBay

ZbWindow {
    id: mainWindow
    visible: false
    width: 1280
    height: 800
    minimumWidth: 760
    minimumHeight: 480
    title: "ZestBay"

    Component.onCompleted: {
        try {
            var geo = JSON.parse(controller.get_window_geometry_json());
            if (geo.width && geo.height) {
                mainWindow.width = geo.width;
                mainWindow.height = geo.height;
            }
            if (geo.x !== undefined && geo.y !== undefined) {
                mainWindow.x = geo.x;
                mainWindow.y = geo.y;
            }
        } catch (e) {}

        try {
            var prefs = JSON.parse(controller.get_preferences_json());
            if (prefs.theme)
                Theme.mode = prefs.theme;
            if (prefs.snap_gap_px !== undefined)
                graphView.snapGap = prefs.snap_gap_px;
            if (prefs.grid_size_px !== undefined)
                graphView.gridSize = prefs.grid_size_px;
            if (prefs.snap_to_grid !== undefined)
                graphView.snapToGrid = prefs.snap_to_grid;
            if (prefs.start_minimized) {
                controller.set_window_visible(false);
                return;
            }
        } catch (e) {}
        mainWindow.visible = true;
    }

    onClosing: function(close) {
        try {
            var prefs = JSON.parse(controller.get_preferences_json());
            if (prefs.close_to_tray) {
                close.accepted = false;
                mainWindow.visible = false;
                controller.set_window_visible(false);
                return;
            }
        } catch (e) {}
        controller.request_quit();
    }

    onWidthChanged: saveGeometryTimer.restart()
    onHeightChanged: saveGeometryTimer.restart()
    onXChanged: saveGeometryTimer.restart()
    onYChanged: saveGeometryTimer.restart()

    Timer {
        id: saveGeometryTimer
        interval: 500
        repeat: false
        onTriggered: {
            var geo = JSON.stringify({
                x: mainWindow.x,
                y: mainWindow.y,
                width: mainWindow.width,
                height: mainWindow.height
            });
            controller.save_window_geometry(geo);
        }
    }

    AppController {
        id: controller
        Component.onCompleted: controller.init()
    }

    // Rule counts for the toolbar Rules button.
    property int ruleCount: 0
    function refreshRuleCount() {
        try {
            ruleCount = JSON.parse(controller.get_rules_json()).length
        } catch (e) {
            ruleCount = 0
        }
    }

    Connections {
        target: controller
        function onGraph_changed() {
            graphView.refreshData();
            mainWindow.refreshRuleCount();
        }
        function onRules_changed() {
            mainWindow.refreshRuleCount();
        }
        function onError_occurred(message) {
            // Errors can arrive in bursts (e.g. while PipeWire reconnects):
            // add to the open dialog instead of replacing it.
            if (errorDialog.visible) {
                if (errorDialogText.text.indexOf(message) < 0) {
                    var combined = errorDialogText.text + "\n\n" + message;
                    if (combined.length > 4000)
                        combined = "\u2026" + combined.substring(combined.length - 4000);
                    errorDialogText.text = combined;
                }
            } else {
                errorDialogText.text = message;
                errorDialog.open();
            }
        }
        function onShow_window_requested() {
            mainWindow.visible = true;
            mainWindow.raise();
            mainWindow.requestActivate();
            controller.set_window_visible(true);
            graphView.refreshData();
            graphView.forceActiveFocus();
        }
        function onTray_action_requested(action) {
            if (action === "add_plugin")
                mainWindow.addPluginAtCenter();
            else if (action === "rules")
                ruleEditor.open();
            else if (action === "preferences")
                preferencesDialog.open();
        }
        function onHide_window_requested() {
            mainWindow.visible = false;
            controller.set_window_visible(false);
        }
        function onMidi_learn_started(instance_id, port_index) {
            pluginParamsDialog.midiLearnInstanceId = instance_id
            pluginParamsDialog.midiLearnPortIndex = port_index
        }
        function onMidi_learn_cancelled() {
            pluginParamsDialog.clearMidiLearnState()
        }
        function onMidi_mapping_added(mapping_json) {
            pluginParamsDialog.clearMidiLearnState()
        }
        function onMidi_mapping_removed(instance_id, port_index) {
        }
        function onMidi_mapping_conflict(source_json, existing_label) {
            pluginParamsDialog.showConflictDialog(source_json, existing_label)
        }
        function onCrash_recovery_available(crashed_uris) {
            crashRecoveryDialogText.text =
                "A crash was detected during plugin restore.\n\n" +
                "Crashed plugins: " + crashed_uris + "\n\n" +
                "A known-good plugin snapshot is available. " +
                "Would you like to restore it?\n\n" +
                "• Restore: reverts plugins.json to the last working state. Restart to load.\n" +
                "• Ignore: keeps the current plugins.json. Restart to try again."
            crashRecoveryDialog.open()
        }
    }

    ZbDialog {
        id: errorDialog
        title: "Error"
        primaryText: "OK"
        width: Math.min(mainWindow.width * 0.6, 500)

        Text {
            id: errorDialogText
            width: parent ? parent.width : 400
            wrapMode: Text.WordWrap
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            color: Theme.textPrimary
        }
    }

    ZbDialog {
        id: crashRecoveryDialog
        title: "Crash recovery"
        primaryText: "Restore"
        secondaryText: "Ignore"
        width: Math.min(mainWindow.width * 0.7, 550)

        onAccepted: {
            if (controller.restore_known_good()) {
                errorDialogText.text = "Plugins restored to last known-good state.\nRestart ZestBay to load them."
                errorDialog.open()
            } else {
                errorDialogText.text = "Failed to restore known-good plugins. Check logs for details."
                errorDialog.open()
            }
        }

        Text {
            id: crashRecoveryDialogText
            width: parent ? parent.width : 450
            wrapMode: Text.WordWrap
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            color: Theme.textPrimary
        }
    }

    Timer {
        id: pollTimer
        interval: controller.get_poll_interval_ms()
        running: true
        repeat: true
        onTriggered: controller.poll_events()
    }

    // Toolbar / menu grid controls; persisted like the Preferences rows.
    function setSnapToGrid(on) {
        controller.set_preference("snap_to_grid", String(on))
        graphView.snapToGrid = on
    }
    function setGridSize(px) {
        controller.set_preference("grid_size_px", String(px))
        graphView.gridSize = px
    }

    function addPluginAtCenter() {
        var center = graphView.toCanvas(graphView.width / 2, graphView.height / 2)
        graphView.pendingPluginPosition = { x: center.x, y: center.y }
        pluginBrowser.allowSubgraphs = graphView.currentSubgraph === ""
        pluginBrowser.open()
    }

    Shortcut {
        sequence: "Ctrl+F"
        onActivated: searchField.forceActiveFocus()
    }

    // ---- Toolbar (replaces the menu bar; design bible §6.4) -------------
    header: Rectangle {
        implicitHeight: Theme.toolbarHeight
        color: Theme.bgPanel

        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 1
            color: Theme.borderPanel
        }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Theme.toolbarPadding
            anchors.rightMargin: Theme.toolbarPadding
            spacing: 8

            ZbLogo {
                size: 26
            }
            Text {
                text: "ZestBay"
                font.family: Theme.fontSans
                font.pixelSize: 14
                font.weight: Font.DemiBold
                color: Theme.textStrong
                Layout.rightMargin: 16
            }

            ZbSearchField {
                id: searchField
                Layout.preferredWidth: 280
                Layout.minimumWidth: 160
                Layout.fillWidth: true
                Layout.maximumWidth: 280
                placeholderText: "Search nodes and ports"
                onTextChanged: graphView.searchText = text
                Keys.onEscapePressed: {
                    text = ""
                    graphView.forceActiveFocus()
                }
            }

            Item { Layout.fillWidth: true }

            ZbButton {
                kind: "primary"
                text: "Add plugin"
                iconName: "plus"
                onClicked: mainWindow.addPluginAtCenter()
            }

            // Rules: accent dot while rules are active, rule count pill.
            ZbButton {
                id: rulesButton
                implicitWidth: rulesRow.implicitWidth + 24
                onClicked: ruleEditor.open()
                contentItem: Item {
                    implicitWidth: rulesRow.implicitWidth
                    Row {
                        id: rulesRow
                        anchors.centerIn: parent
                        spacing: 8
                        Rectangle {
                            visible: controller.patchbay_enabled
                            anchors.verticalCenter: parent.verticalCenter
                            width: 7
                            height: 7
                            radius: 3.5
                            color: Theme.accent
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: "Rules"
                            font.family: Theme.fontSans
                            font.pixelSize: Theme.sizeButton
                            font.weight: Font.Medium
                            color: Theme.textPrimary
                        }
                        ZbCountPill {
                            anchors.verticalCenter: parent.verticalCenter
                            text: mainWindow.ruleCount
                        }
                    }
                }
            }

            ZbDivider { vertical: true; strong: true; Layout.leftMargin: 4; Layout.rightMargin: 4 }

            // Zoom cluster: [−] [100%] [+] [fit], one bordered group.
            Rectangle {
                implicitHeight: Theme.controlHeight
                implicitWidth: zoomRow.implicitWidth + 2
                radius: Theme.radiusControl
                color: Theme.bgRaised
                border.width: 1
                border.color: Theme.borderStrong

                Row {
                    id: zoomRow
                    anchors.centerIn: parent
                    ZoomCell {
                        iconName: "minus"
                        tip: "Zoom out"
                        onClicked: graphView.zoomAt(graphView.width / 2, graphView.height / 2, graphView.zoom / 1.1)
                    }
                    Rectangle { width: 1; height: Theme.controlHeight - 2; color: Theme.borderHairline }
                    ZoomCell {
                        width: 44
                        label: Math.round(graphView.zoom * 100) + "%"
                        tip: "Reset zoom"
                        onClicked: graphView.zoomAt(graphView.width / 2, graphView.height / 2, 1.0)
                    }
                    Rectangle { width: 1; height: Theme.controlHeight - 2; color: Theme.borderHairline }
                    ZoomCell {
                        iconName: "plus"
                        tip: "Zoom in"
                        onClicked: graphView.zoomAt(graphView.width / 2, graphView.height / 2, graphView.zoom * 1.1)
                    }
                    ZoomCell {
                        iconName: "fit"
                        tip: "Fit to view (Home)"
                        onClicked: graphView.fitToView()
                    }
                }
            }

            // Grid cluster: [snap to grid toggle] [grid size ▾].
            Rectangle {
                implicitHeight: Theme.controlHeight
                implicitWidth: gridRow.implicitWidth + 2
                radius: Theme.radiusControl
                color: Theme.bgRaised
                border.width: 1
                border.color: Theme.borderStrong

                Row {
                    id: gridRow
                    anchors.centerIn: parent
                    ZoomCell {
                        iconName: "grid"
                        active: graphView.snapToGrid
                        tip: graphView.snapToGrid ? "Snap to grid: on" : "Snap to grid: off"
                        onClicked: mainWindow.setSnapToGrid(!graphView.snapToGrid)
                    }
                    Rectangle { width: 1; height: Theme.controlHeight - 2; color: Theme.borderHairline }
                    ZoomCell {
                        id: gridSizeCell
                        width: 60
                        label: Math.round(graphView.gridSize) + "px"
                        trailingIcon: "chevronDown"
                        tip: "Grid size"
                        onClicked: gridSizeMenu.popup(gridSizeCell, 0, gridSizeCell.height + 6)

                        ZbMenu {
                            id: gridSizeMenu
                            implicitWidth: 140
                            Repeater {
                                model: [8, 12, 16, 20, 24, 32, 40, 48, 64, 96]
                                delegate: ZbMenuItem {
                                    required property int modelData
                                    implicitWidth: 128
                                    text: modelData + " px"
                                    selected: Math.round(graphView.gridSize) === modelData
                                    onTriggered: mainWindow.setGridSize(modelData)
                                }
                            }
                        }
                    }
                }
            }

            ZbDivider { vertical: true; strong: true; Layout.leftMargin: 4; Layout.rightMargin: 4 }

            ZbButton {
                iconName: "settings"
                iconSize: 16
                textColor: Theme.textIcon
                onClicked: preferencesDialog.open()
                ZbTooltip { visible: parent.hovered; text: "Preferences" }
            }

            ZbButton {
                id: menuButton
                iconName: "menu"
                iconSize: 16
                textColor: Theme.textIcon
                onClicked: appMenu.popup(menuButton, menuButton.width - appMenu.width, menuButton.height + 6)

                // The old File / Patchbay / Help menus.
                ZbMenu {
                    id: appMenu
                    ZbMenuItem {
                        text: "Enable rules"
                        checkable: true
                        checked: controller.patchbay_enabled
                        onToggled: controller.toggle_patchbay(!controller.patchbay_enabled)
                    }
                    ZbMenuItem {
                        // Off: rules only add links; nothing they don't cover is removed.
                        text: "Exclusive mode"
                        checkable: true
                        enabled: controller.patchbay_enabled
                        checked: controller.patchbay_exclusive
                        onToggled: controller.toggle_patchbay_exclusive(!controller.patchbay_exclusive)
                    }
                    ZbMenuItem {
                        text: "Apply rules now"
                        onTriggered: controller.apply_rules()
                    }
                    ZbMenuItem {
                        text: "Snapshot connections"
                        onTriggered: controller.snapshot_rules()
                    }
                    ZbMenuSeparator {}
                    ZbMenuItem {
                        text: "Snap to grid"
                        checkable: true
                        checked: graphView.snapToGrid
                        onToggled: mainWindow.setSnapToGrid(!graphView.snapToGrid)
                    }
                    ZbMenuSeparator {}
                    ZbMenuItem {
                        text: "Manage plugins…"
                        onTriggered: pluginManagerDialog.open()
                    }
                    ZbMenuItem {
                        text: "MIDI mappings…"
                        onTriggered: midiMappingsDialog.open()
                    }
                    ZbMenuItem {
                        text: "CPU usage…"
                        onTriggered: cpuOverlayDialog.open()
                    }
                    ZbMenuSeparator {}
                    ZbMenuItem {
                        text: "About ZestBay"
                        onTriggered: aboutDialog.open()
                    }
                    ZbMenuItem {
                        text: "Quit"
                        shortcutHint: "Ctrl+Q"
                        onTriggered: controller.request_quit()
                    }
                }
            }
        }
    }

    Shortcut {
        sequence: "Ctrl+Q"
        onActivated: controller.request_quit()
    }

    // A cell of the zoom cluster: icon or label, no own border.
    // A cell of a toolbar cluster (zoom, grid): icon and/or label, no own
    // border. `active` = toggle on (accentFaint + accent, like a plugin's On).
    component ZoomCell: AbstractButton {
        id: cell
        property string iconName: ""
        property string label: ""
        property string trailingIcon: ""
        property string tip: ""
        property bool active: false
        readonly property color fg: active ? Theme.accent : Theme.textIcon
        width: Theme.controlHeight
        height: Theme.controlHeight - 2
        hoverEnabled: true
        background: Rectangle {
            radius: Theme.radiusControl - 1
            color: cell.active ? Theme.accentFaint
                 : cell.hovered || cell.down ? Theme.bgHover : "transparent"
        }
        contentItem: Item {
            Row {
                anchors.centerIn: parent
                spacing: 5
                ZbIcon {
                    visible: cell.iconName.length > 0
                    anchors.verticalCenter: parent.verticalCenter
                    name: cell.iconName
                    size: 14
                    color: cell.fg
                }
                Text {
                    visible: cell.label.length > 0
                    anchors.verticalCenter: parent.verticalCenter
                    text: cell.label
                    font.family: Theme.fontMono
                    font.pixelSize: Theme.sizeMono
                    color: cell.active ? Theme.accent : Theme.textPrimary
                }
                ZbIcon {
                    visible: cell.trailingIcon.length > 0
                    anchors.verticalCenter: parent.verticalCenter
                    name: cell.trailingIcon
                    size: 8
                    color: Theme.textMuted
                }
            }
        }
        ZbTooltip { visible: cell.hovered && cell.tip.length > 0; text: cell.tip }
    }

    // ---- Status bar -----------------------------------------------------
    footer: Rectangle {
        implicitHeight: Theme.statusBarHeight
        color: Theme.bgPanel

        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: 1
            color: Theme.borderPanel
        }

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Theme.toolbarPadding
            anchors.rightMargin: Theme.toolbarPadding
            spacing: 16

            StatusText {
                text: controller.node_count + " nodes · " + controller.link_count + " links"
            }

            Row {
                spacing: 14
                Repeater {
                    model: [
                        { label: "Audio", color: Theme.sigAudio, hollow: false },
                        { label: "MIDI", color: Theme.sigMidi, hollow: false },
                        { label: "Video", color: Theme.sigVideo, hollow: false },
                        { label: "Unconnected", color: Theme.textMuted, hollow: true }
                    ]
                    Row {
                        required property var modelData
                        spacing: 6
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 8
                            height: 8
                            radius: 4
                            color: modelData.hollow ? "transparent" : modelData.color
                            border.width: modelData.hollow ? 1.5 : 0
                            border.color: modelData.color
                        }
                        StatusText { text: modelData.label }
                    }
                }
            }

            Item { Layout.fillWidth: true }

            // CPU sparkline + readout; click for the CPU usage window.
            Item {
                implicitWidth: cpuRow.implicitWidth
                implicitHeight: cpuRow.implicitHeight
                Row {
                    id: cpuRow
                    spacing: 8
                    Canvas {
                        id: cpuSparkline
                        anchors.verticalCenter: parent.verticalCenter
                        width: 64
                        height: 14
                        property var cpuData: []

                        // Redraw on every poll, not on cpu_usage changes: the
                        // rounded text often stays the same while the history
                        // moves, which made the line freeze and then jump.
                        Timer {
                            interval: pollTimer.interval
                            running: mainWindow.visible
                            repeat: true
                            onTriggered: {
                                try {
                                    cpuSparkline.cpuData = JSON.parse(controller.get_cpu_history())
                                } catch (e) {
                                    cpuSparkline.cpuData = []
                                }
                                cpuSparkline.requestPaint()
                            }
                        }
                        Connections {
                            target: Theme
                            function onDarkChanged() { cpuSparkline.requestPaint() }
                        }

                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.reset()
                            // All 120 samples: each poll shifts the line by
                            // about half a pixel, so it scrolls smoothly.
                            var d = cpuData
                            if (d.length < 2) return
                            var maxV = 1
                            for (var i = 0; i < d.length; i++) maxV = Math.max(maxV, d[i])
                            ctx.strokeStyle = "" + Theme.catHardware
                            ctx.lineWidth = 1.5
                            ctx.lineJoin = "round"
                            ctx.beginPath()
                            for (var j = 0; j < d.length; j++) {
                                var x = j * (width - 1) / (d.length - 1) + 0.5
                                var y = height - 1.5 - (d[j] / maxV) * (height - 3)
                                if (j === 0) ctx.moveTo(x, y)
                                else ctx.lineTo(x, y)
                            }
                            ctx.stroke()
                        }
                    }
                    StatusText {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "CPU " + controller.cpu_usage
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: cpuOverlayDialog.open()
                }
            }

            // Rules chip: accent while active, neutral when off.
            Rectangle {
                implicitHeight: 20
                implicitWidth: chipRow.implicitWidth + 16
                radius: Theme.radiusSmall
                color: controller.patchbay_enabled ? Theme.accentFaint : "transparent"
                border.width: controller.patchbay_enabled ? 0 : 1
                border.color: Theme.borderStrong
                Row {
                    id: chipRow
                    anchors.centerIn: parent
                    spacing: 6
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 6
                        height: 6
                        radius: 3
                        color: controller.patchbay_enabled ? Theme.accent : Theme.textMuted
                    }
                    StatusText {
                        text: controller.patchbay_enabled
                              ? (controller.patchbay_exclusive ? "Rules active" : "Rules active · non-exclusive")
                              : "Rules off"
                        color: controller.patchbay_enabled ? Theme.accent : Theme.textMuted
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: ruleEditor.open()
                }
            }
        }
    }

    component StatusText: Text {
        font.family: Theme.fontMono
        font.pixelSize: Theme.sizeMono
        color: Theme.textMuted
    }

    GraphView {
        id: graphView
        anchors.fill: parent
        controller: controller
        onOpenPluginBrowser: {
            pluginBrowser.allowSubgraphs = graphView.currentSubgraph === ""
            pluginBrowser.open()
        }
        Component.onCompleted: mainWindow.refreshRuleCount()
        onOpenPluginParams: nodeId => pluginParamsDialog.openForNode(nodeId)
    }

    PluginBrowser {
        id: pluginBrowser
        controller: controller
        onPluginAdded: displayName => {
            graphView.pendingPluginName = displayName
        }
        onCancelled: {
            graphView.pendingPluginPosition = null
            graphView.pendingPluginName = ""
        }
        onTemplateChosen: file => graphView.insertTemplate(file)
    }

    PluginParams {
        id: pluginParamsDialog
        controller: controller
    }

    RuleEditor {
        id: ruleEditor
        controller: controller
        onVisibleChanged: if (!visible) mainWindow.refreshRuleCount()
    }

    PluginManager {
        id: pluginManagerDialog
        controller: controller
    }

    MidiMappings {
        id: midiMappingsDialog
        controller: controller
    }

    Preferences {
        id: preferencesDialog
        controller: controller
        onPollIntervalChanged: intervalMs => {
            pollTimer.interval = intervalMs;
        }
        onSnapGapChanged: px => graphView.snapGap = px
        onSnapToGridChanged: enabled => graphView.snapToGrid = enabled
        onGridSizeChanged: px => graphView.gridSize = px
    }

    CpuOverlay {
        id: cpuOverlayDialog
        controller: controller
    }

    About {
        id: aboutDialog
        controller: controller
    }
}
