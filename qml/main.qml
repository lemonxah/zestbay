import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ZestBay

ApplicationWindow {
    id: mainWindow
    visible: false
    width: 800
    height: 600
    color: Theme.windowBg
    title: "ZestBay - Qt6"

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

    Connections {
        target: controller
        function onGraph_changed() {
            graphView.refreshData();
        }
        function onError_occurred(message) {
            errorDialogText.text = message;
            errorDialog.open();
        }
        function onShow_window_requested() {
            mainWindow.visible = true;
            mainWindow.raise();
            mainWindow.requestActivate();
            controller.set_window_visible(true);
            graphView.refreshData();
            graphView.forceActiveFocus();
        }
        function onHide_window_requested() {
            mainWindow.visible = false;
            controller.set_window_visible(false);
        }
    }

    Dialog {
        id: errorDialog
        title: "Error"
        anchors.centerIn: parent
        modal: true
        standardButtons: Dialog.Ok
        width: Math.min(mainWindow.width * 0.6, 500)

        Label {
            id: errorDialogText
            width: parent.width
            wrapMode: Text.WordWrap
        }
    }

    Timer {
        id: pollTimer
        interval: controller.get_poll_interval_ms()
        running: true
        repeat: true
        onTriggered: controller.poll_events()
    }

    menuBar: MenuBar {
        Menu {
            title: "&File"
            Action {
                text: "Add &Plugin..."
                onTriggered: {
                    var center = graphView.toCanvas(graphView.width / 2, graphView.height / 2)
                    graphView.pendingPluginPosition = { x: center.x, y: center.y }
                    pluginBrowser.open()
                }
            }
            Action {
                text: "&Manage Plugins..."
                onTriggered: pluginManagerDialog.open()
            }
            MenuSeparator {}
            Action {
                text: "&Preferences..."
                onTriggered: preferencesDialog.open()
            }
            MenuSeparator {}
            Action {
                text: "&Quit"
                onTriggered: controller.request_quit()
            }
        }
        Menu {
            title: "&Patchbay"
            Action {
                text: "Enable Rules"
                checkable: true
                checked: controller.patchbay_enabled
                onToggled: controller.toggle_patchbay(checked)
            }
            MenuSeparator {}
            Action {
                text: "Edit Rules..."
                onTriggered: ruleEditor.open()
            }
            Action {
                text: "Apply Rules Now"
                onTriggered: controller.apply_rules()
            }
            MenuSeparator {}
            Action {
                text: "Snapshot Connections"
                onTriggered: controller.snapshot_rules()
            }
        }
        Menu {
            title: "&Help"
            Action {
                text: "&About ZestBay..."
                onTriggered: aboutDialog.open()
            }
        }
    }

    footer: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8

            Label {
                text: "Nodes: " + controller.node_count + "  Links: " + controller.link_count
            }

            Item {
                Layout.fillWidth: true
            }

            Canvas {
                id: cpuSparkline
                width: 80
                height: 40
                Layout.alignment: Qt.AlignVCenter

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: cpuOverlayDialog.open()
                }

                ToolTip.visible: cpuSparklineHover.hovered
                ToolTip.text: "Click for detailed CPU view"

                HoverHandler {
                    id: cpuSparklineHover
                }

                property var cpuData: []

                Connections {
                    target: controller
                    function onCpu_usageChanged() {
                        try {
                            cpuSparkline.cpuData = JSON.parse(controller.get_cpu_history())
                        } catch(e) {
                            cpuSparkline.cpuData = []
                        }
                        cpuSparkline.requestPaint()
                    }
                }

                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset()
                    var d = cpuData
                    var w = width
                    var h = height

                    ctx.fillStyle = "" + Theme.chartBg
                    ctx.fillRect(0, 0, w, h)

                    ctx.setLineDash([2, 2])

                    ctx.strokeStyle = "" + Theme.chartGrid25
                    ctx.lineWidth = 0.5
                    ctx.beginPath()
                    ctx.moveTo(0, h * 0.75)
                    ctx.lineTo(w, h * 0.75)
                    ctx.stroke()
                    ctx.beginPath()
                    ctx.moveTo(0, h * 0.25)
                    ctx.lineTo(w, h * 0.25)
                    ctx.stroke()

                    ctx.strokeStyle = "" + Theme.chartGrid50
                    ctx.lineWidth = 0.5
                    ctx.beginPath()
                    ctx.moveTo(0, h * 0.5)
                    ctx.lineTo(w, h * 0.5)
                    ctx.stroke()

                    ctx.setLineDash([])

                    if (d.length >= 2) {
                        ctx.strokeStyle = "" + Theme.chartLine
                        ctx.lineWidth = 1
                        ctx.beginPath()
                        var step = w / (d.length - 1)
                        for (var j = 0; j < d.length; j++) {
                            var x = j * step
                            var y = h - (d[j] / 100.0) * (h - 2) - 1
                            if (j === 0) ctx.moveTo(x, y)
                            else ctx.lineTo(x, y)
                        }
                        ctx.stroke()
                    }

                    ctx.strokeStyle = "" + Theme.chartBorder
                    ctx.lineWidth = 1
                    ctx.strokeRect(0, 0, w, h)
                }
            }

            Label {
                text: "CPU: " + controller.cpu_usage
                font.family: "monospace"
                opacity: 0.7

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: cpuOverlayDialog.open()
                }
            }

            Rectangle {
                width: 1
                height: parent.height * 0.6
                color: Theme.separator
                Layout.alignment: Qt.AlignVCenter
            }

            Label {
                text: controller.patchbay_enabled ? "Rules Active" : "Rules Disabled"
                opacity: controller.patchbay_enabled ? 1.0 : 0.5
            }
        }
    }

    GraphView {
        id: graphView
        anchors.fill: parent
        controller: controller
        onOpenPluginBrowser: pluginBrowser.open()
        onOpenPluginParams: nodeId => pluginParamsDialog.openForNode(nodeId)
    }

    PluginBrowser {
        id: pluginBrowser
        controller: controller
    }

    PluginParams {
        id: pluginParamsDialog
        controller: controller
    }

    RuleEditor {
        id: ruleEditor
        controller: controller
    }

    PluginManager {
        id: pluginManagerDialog
        controller: controller
    }

    Preferences {
        id: preferencesDialog
        controller: controller
        onPollIntervalChanged: intervalMs => {
            pollTimer.interval = intervalMs;
        }
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
