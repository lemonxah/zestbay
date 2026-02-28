import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: prefsWindow
    title: "Preferences"
    width: 520
    height: 640
    minimumWidth: 400
    minimumHeight: 400
    visible: false
    color: Theme.windowBg

    required property var controller

    signal pollIntervalChanged(int intervalMs)

    property var prefs: ({})

    function loadPrefs() {
        try {
            prefs = JSON.parse(controller.get_preferences_json());
        } catch (e) {
            prefs = {};
        }
    }

    function open() {
        loadPrefs();
        prefsWindow.visible = true;
        prefsWindow.raise();
        prefsWindow.requestActivate();
    }

    function setPref(key, value) {
        controller.set_preference(key, String(value));
        loadPrefs();
        if (key === "poll_interval_ms") {
            pollIntervalChanged(value);
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        Label {
            text: "Preferences"
            font.bold: true
            font.pointSize: 13
        }

        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: Theme.separator
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Flickable {
                id: prefsFlickable
                anchors.fill: parent
                contentHeight: settingsColumn.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds

                ScrollBar.vertical: ScrollBar {
                    id: prefsScrollBar
                    policy: ScrollBar.AlwaysOn
                    minimumSize: 0.08
                }

            ColumnLayout {
                id: settingsColumn
                width: parent.width
                spacing: 16

                Label {
                    text: "General"
                    font.bold: true
                    font.pointSize: 11
                    opacity: 0.8
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 12

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2

                        Label {
                            text: "Start minimized to tray"
                            font.bold: true
                        }
                        Label {
                            text: "Launch with the window hidden. Use the system tray icon to show it."
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            font.pointSize: 9
                            opacity: 0.5
                        }
                    }

                    Switch {
                        checked: prefs.start_minimized !== undefined ? prefs.start_minimized : false
                        onToggled: setPref("start_minimized", checked)
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    height: 1
                    color: Theme.separatorLight
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 12

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2

                        Label {
                            text: "Close to tray"
                            font.bold: true
                        }
                        Label {
                            text: "Clicking the window close button hides to the system tray instead of quitting."
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            font.pointSize: 9
                            opacity: 0.5
                        }
                    }

                    Switch {
                        checked: prefs.close_to_tray !== undefined ? prefs.close_to_tray : false
                        onToggled: setPref("close_to_tray", checked)
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    height: 1
                    color: Theme.separatorLight
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 12

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2

                        Label {
                            text: "Auto-learn patchbay rules"
                            font.bold: true
                        }
                        Label {
                            text: "Automatically create/update rules when you manually connect ports."
                            wrapMode: Text.WordWrap
                            Layout.fillWidth: true
                            font.pointSize: 9
                            opacity: 0.5
                        }
                    }

                    Switch {
                        checked: prefs.auto_learn_rules !== undefined ? prefs.auto_learn_rules : true
                        onToggled: setPref("auto_learn_rules", checked)
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    height: 1
                    color: Theme.separator
                }

                Label {
                    text: "Timing"
                    font.bold: true
                    font.pointSize: 11
                    opacity: 0.8
                }

                Label {
                    text: "Adjust timing parameters to fine-tune responsiveness vs. reliability."
                    font.italic: true
                    opacity: 0.5
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4

                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            text: "Rule settle time"
                            font.bold: true
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Label {
                            text: ruleSettleSlider.value + " ms"
                            font.family: "monospace"
                            opacity: 0.8
                        }
                    }

                    Label {
                        text: "How long to wait after the graph stops changing before auto-applying patchbay rules. Higher values are more reliable on slow hardware."
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        font.pointSize: 9
                        opacity: 0.5
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        Label {
                            text: "0ms"
                            opacity: 0.4
                            font.pointSize: 8
                        }
                        Slider {
                            id: ruleSettleSlider
                            Layout.fillWidth: true
                            from: 0
                            to: 100
                            stepSize: 2
                            value: prefs.rule_settle_ms !== undefined ? prefs.rule_settle_ms : 50
                            onPressedChanged: {
                                if (!pressed) {
                                    setPref("rule_settle_ms", value);
                                }
                            }
                        }
                        Label {
                            text: "100ms"
                            opacity: 0.4
                            font.pointSize: 8
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    height: 1
                    color: Theme.separatorLight
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4

                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            text: "Poll interval"
                            font.bold: true
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Label {
                            text: pollIntervalSlider.value + " ms"
                            font.family: "monospace"
                            opacity: 0.8
                        }
                    }

                    Label {
                        text: "How often the UI checks for PipeWire events. Lower values give smoother updates but use more CPU."
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        font.pointSize: 9
                        opacity: 0.5
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        Label {
                            text: "16"
                            opacity: 0.4
                            font.pointSize: 8
                        }
                        Slider {
                            id: pollIntervalSlider
                            Layout.fillWidth: true
                            from: 16
                            to: 500
                            stepSize: 1
                            value: prefs.poll_interval_ms || 100
                            onPressedChanged: {
                                if (!pressed) {
                                    setPref("poll_interval_ms", value);
                                }
                            }
                        }
                        Label {
                            text: "500"
                            opacity: 0.4
                            font.pointSize: 8
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    height: 1
                    color: Theme.separatorLight
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4

                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            text: "Parameter save debounce"
                            font.bold: true
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Label {
                            text: paramsPersistSlider.value + " ms"
                            font.family: "monospace"
                            opacity: 0.8
                        }
                    }

                    Label {
                        text: "How long to wait after the last parameter change before saving to disk."
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        font.pointSize: 9
                        opacity: 0.5
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        Label {
                            text: "100"
                            opacity: 0.4
                            font.pointSize: 8
                        }
                        Slider {
                            id: paramsPersistSlider
                            Layout.fillWidth: true
                            from: 100
                            to: 10000
                            stepSize: 100
                            value: prefs.params_persist_ms || 1000
                            onPressedChanged: {
                                if (!pressed) {
                                    setPref("params_persist_ms", value);
                                }
                            }
                        }
                        Label {
                            text: "10000"
                            opacity: 0.4
                            font.pointSize: 8
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    height: 1
                    color: Theme.separatorLight
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4

                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            text: "PipeWire tick interval"
                            font.bold: true
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Label {
                            text: pwTickSlider.value + " ms"
                            font.family: "monospace"
                            opacity: 0.8
                        }
                    }

                    Label {
                        text: "How often the PipeWire thread checks for pending operations. Lower values reduce link latency. Requires restart."
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        font.pointSize: 9
                        opacity: 0.5
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        Label {
                            text: "1"
                            opacity: 0.4
                            font.pointSize: 8
                        }
                        Slider {
                            id: pwTickSlider
                            Layout.fillWidth: true
                            from: 1
                            to: 200
                            stepSize: 1
                            value: prefs.pw_tick_interval_ms || 10
                            onPressedChanged: {
                                if (!pressed) {
                                    setPref("pw_tick_interval_ms", value);
                                }
                            }
                        }
                        Label {
                            text: "200"
                            opacity: 0.4
                            font.pointSize: 8
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    height: 1
                    color: Theme.separatorLight
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4

                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            text: "Plugin operation cooldown"
                            font.bold: true
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Label {
                            text: pwCooldownSlider.value + " ms"
                            font.family: "monospace"
                            opacity: 0.8
                        }
                    }

                    Label {
                        text: "Minimum gap between heavy operations (plugin add/remove). Connect/disconnect are always instant. Requires restart."
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        font.pointSize: 9
                        opacity: 0.5
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        Label {
                            text: "10"
                            opacity: 0.4
                            font.pointSize: 8
                        }
                        Slider {
                            id: pwCooldownSlider
                            Layout.fillWidth: true
                            from: 10
                            to: 1000
                            stepSize: 10
                            value: prefs.pw_operation_cooldown_ms || 50
                            onPressedChanged: {
                                if (!pressed) {
                                    setPref("pw_operation_cooldown_ms", value);
                                }
                            }
                        }
                        Label {
                            text: "1000"
                            opacity: 0.4
                            font.pointSize: 8
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    height: 1
                    color: Theme.separatorLight
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4

                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            text: "Link save debounce"
                            font.bold: true
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Label {
                            text: linksPersistSlider.value + " ms"
                            font.family: "monospace"
                            opacity: 0.8
                        }
                    }

                    Label {
                        text: "How long to wait after the last link change before saving plugin connections to disk."
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                        font.pointSize: 9
                        opacity: 0.5
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        Label {
                            text: "100"
                            opacity: 0.4
                            font.pointSize: 8
                        }
                        Slider {
                            id: linksPersistSlider
                            Layout.fillWidth: true
                            from: 100
                            to: 10000
                            stepSize: 100
                            value: prefs.links_persist_ms || 2000
                            onPressedChanged: {
                                if (!pressed) {
                                    setPref("links_persist_ms", value);
                                }
                            }
                        }
                        Label {
                            text: "10000"
                            opacity: 0.4
                            font.pointSize: 8
                        }
                    }
                }

                Item {
                    Layout.fillHeight: true
                }
            }
        }

            // Bottom fade gradient to hint there's more content below
            Rectangle {
                anchors.left: parent.left
                anchors.right: prefsScrollBar.left
                anchors.bottom: parent.bottom
                height: 32
                visible: !prefsFlickable.atYEnd
                gradient: Gradient {
                    GradientStop { position: 0.0; color: "transparent" }
                    GradientStop { position: 1.0; color: Theme.fadeColor }
                }

                Label {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 4
                    text: "\u25BC  scroll for more  \u25BC"
                    font.pointSize: 8
                    opacity: 0.5
                }
            }

            // Top fade gradient when scrolled down
            Rectangle {
                anchors.left: parent.left
                anchors.right: prefsScrollBar.left
                anchors.top: parent.top
                height: 24
                visible: !prefsFlickable.atYBeginning
                gradient: Gradient {
                    GradientStop { position: 0.0; color: Theme.fadeColor }
                    GradientStop { position: 1.0; color: "transparent" }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: Theme.separator
        }

        RowLayout {
            Layout.fillWidth: true

            Button {
                text: "Reset to Defaults"
                onClicked: {
                    controller.reset_preferences();
                    loadPrefs();
                    pollIntervalChanged(prefs.poll_interval_ms || 100);
                }
            }

            Item {
                Layout.fillWidth: true
            }

            Button {
                text: "Close"
                onClicked: prefsWindow.visible = false
            }
        }
    }
}
