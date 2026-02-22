import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ZestBay

ApplicationWindow {
    id: mainWindow
    visible: false
    width: 800
    height: 600
    title: "ZestBay - Qt6"

    // System tray icon is handled on the Rust side via ksni (D-Bus StatusNotifier).
    // The tray communicates with QML through the AppController poll loop,
    // which checks tray flags (show_requested / quit_requested) each tick.

    // Restore saved window geometry, then show unless start_minimized is on.
    // Window starts hidden (visible: false) to avoid a flash.
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

        // Show the window unless "start minimized to tray" is enabled
        try {
            var prefs = JSON.parse(controller.get_preferences_json());
            if (prefs.start_minimized) {
                controller.set_window_visible(false);
                return;
            }
        } catch (e) {}
        mainWindow.visible = true;
    }

    // Intercept window close: hide to tray instead of quitting (if pref enabled)
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
        // close_to_tray is off — let Qt close normally, then request_quit
        // persists state and shuts down cleanly.
        controller.request_quit();
    }

    // Save window geometry on resize/move (debounced)
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
                onTriggered: pluginBrowser.open()
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
        anchors.centerIn: parent
    }

    PluginParams {
        id: pluginParamsDialog
        controller: controller
        anchors.centerIn: parent
    }

    RuleEditor {
        id: ruleEditor
        controller: controller
        anchors.centerIn: parent
    }

    Preferences {
        id: preferencesDialog
        controller: controller
        onPollIntervalChanged: intervalMs => {
            pollTimer.interval = intervalMs;
        }
    }
}
