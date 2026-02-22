import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Dialog {
    id: pluginManager
    title: "Manage Plugins"
    width: 750
    height: 600
    modal: true
    standardButtons: Dialog.Close

    required property var controller

    property var plugins: []
    property int selectedIndex: -1
    property var selectedPlugin: selectedIndex >= 0 && selectedIndex < plugins.length ? plugins[selectedIndex] : null

    function loadPlugins() {
        try {
            plugins = JSON.parse(controller.get_active_plugins_json())
        } catch(e) {
            plugins = []
        }
        if (selectedIndex >= plugins.length) {
            selectedIndex = -1
        }
    }

    onOpened: {
        selectedIndex = -1
        loadPlugins()
    }

    Timer {
        id: refreshTimer
        interval: 500
        running: pluginManager.visible
        repeat: true
        onTriggered: loadPlugins()
    }

    contentItem: RowLayout {
        spacing: 8

        ColumnLayout {
            Layout.preferredWidth: 260
            Layout.fillHeight: true
            spacing: 4

            Label {
                text: plugins.length + " active plugin" + (plugins.length !== 1 ? "s" : "")
                font.italic: true
                opacity: 0.7
            }

            ListView {
                id: pluginList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                model: plugins.length
                currentIndex: selectedIndex

                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                delegate: Rectangle {
                    id: pluginDelegate
                    required property int index
                    width: pluginList.width
                    height: 48
                    color: index === selectedIndex ? "#404060" :
                           pluginMouse.containsMouse ? "#3a3a3a" :
                           (index % 2 === 0 ? "#2a2a2a" : "#252525")
                    radius: 3

                    property var plugin: plugins[index] || {}

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 6
                        spacing: 2

                        Label {
                            text: plugin.displayName || ""
                            font.bold: true
                            font.pointSize: 9
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }

                        RowLayout {
                            spacing: 6
                            Label {
                                text: plugin.bypassed ? "Bypassed" : "Active"
                                font.pointSize: 8
                                color: plugin.bypassed ? "#e0a040" : "#60c060"
                                opacity: 0.8
                            }
                            Label {
                                text: (plugin.parameters ? plugin.parameters.length : 0) + " params"
                                font.pointSize: 8
                                opacity: 0.5
                            }
                        }
                    }

                    MouseArea {
                        id: pluginMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: selectedIndex = index
                    }
                }
            }
        }

        Rectangle {
            Layout.fillHeight: true
            width: 1
            color: "#3c3c3c"
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 8

            Label {
                visible: !selectedPlugin
                text: "Select a plugin from the list"
                opacity: 0.5
                Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter
                Layout.fillWidth: true
                Layout.fillHeight: true
            }

            ColumnLayout {
                visible: !!selectedPlugin
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 6

                Label {
                    text: selectedPlugin ? selectedPlugin.displayName : ""
                    font.bold: true
                    font.pointSize: 12
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }

                Label {
                    text: selectedPlugin ? selectedPlugin.pluginUri : ""
                    font.pointSize: 8
                    opacity: 0.5
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    Button {
                        text: "Reset Params"
                        enabled: selectedPlugin && selectedPlugin.parameters && selectedPlugin.parameters.length > 0
                        onClicked: {
                            if (selectedPlugin) {
                                controller.reset_plugin_params_by_stable_id(selectedPlugin.stableId)
                            }
                        }
                    }

                    Button {
                        text: "Remove Plugin"
                        onClicked: {
                            if (selectedPlugin) {
                                controller.remove_plugin_by_stable_id(selectedPlugin.stableId)
                                selectedIndex = -1
                                loadPlugins()
                            }
                        }
                    }

                    Item { Layout.fillWidth: true }
                }

                Rectangle {
                    Layout.fillWidth: true
                    height: 1
                    color: "#3c3c3c"
                }

                Label {
                    text: {
                        if (!selectedPlugin || !selectedPlugin.parameters) return ""
                        return selectedPlugin.parameters.length + " parameter" +
                               (selectedPlugin.parameters.length !== 1 ? "s" : "")
                    }
                    opacity: 0.6
                    visible: selectedPlugin && selectedPlugin.parameters && selectedPlugin.parameters.length > 0
                }

                Label {
                    text: "No control parameters"
                    opacity: 0.5
                    visible: !selectedPlugin || !selectedPlugin.parameters || selectedPlugin.parameters.length === 0
                    Layout.alignment: Qt.AlignHCenter
                }

                ListView {
                    id: paramList
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: selectedPlugin && selectedPlugin.parameters ? selectedPlugin.parameters.length : 0
                    spacing: 2

                    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                    delegate: Rectangle {
                        id: paramDelegate
                        required property int index
                        width: paramList.width - 12
                        height: 56
                        color: index % 2 === 0 ? "#2a2a2a" : "#252525"
                        radius: 3

                        property var param: selectedPlugin && selectedPlugin.parameters ? selectedPlugin.parameters[index] : {}

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: 6
                            spacing: 2

                            RowLayout {
                                Layout.fillWidth: true
                                Label {
                                    text: param.name || param.symbol || ""
                                    font.pointSize: 9
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                                Label {
                                    text: param.value !== undefined ? param.value.toFixed(3) : ""
                                    font.pointSize: 9
                                    font.family: "monospace"
                                    opacity: 0.8
                                }
                                Button {
                                    text: "R"
                                    flat: true
                                    implicitWidth: 24
                                    implicitHeight: 20
                                    font.pointSize: 8
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Reset to default (" + (param.default !== undefined ? param.default.toFixed(3) : "") + ")"
                                    onClicked: {
                                        if (selectedPlugin && param.portIndex !== undefined) {
                                            controller.set_plugin_param_by_stable_id(
                                                selectedPlugin.stableId, param.portIndex, param.default)
                                        }
                                    }
                                }
                            }

                            Slider {
                                Layout.fillWidth: true
                                from: param.min !== undefined ? param.min : 0
                                to: param.max !== undefined ? param.max : 1
                                value: param.value !== undefined ? param.value : 0
                                onMoved: {
                                    if (selectedPlugin && param.portIndex !== undefined) {
                                        controller.set_plugin_param_by_stable_id(
                                            selectedPlugin.stableId, param.portIndex, value)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
