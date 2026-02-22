import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Dialog {
    id: pluginParams
    title: pluginName ? pluginName + " - Parameters" : "Plugin Parameters"
    width: 420
    height: 500
    modal: false
    standardButtons: Dialog.Close

    required property var controller

    property int pluginNodeId: -1
    property string pluginName: ""
    property string pluginUri: ""
    property bool pluginBypassed: false
    property var parameters: []
    property int instanceId: -1

    Timer {
        id: refreshTimer
        interval: 200
        running: pluginParams.visible
        repeat: true
        onTriggered: loadParams()
    }

    function openForNode(nodeId) {
        pluginNodeId = nodeId
        loadParams()
        open()
    }

    function loadParams() {
        if (pluginNodeId < 0) return
        try {
            var data = JSON.parse(controller.get_plugin_params_json(pluginNodeId))
            if (!data || !data.parameters) return
            pluginName = data.displayName || ""
            pluginUri = data.pluginUri || ""
            pluginBypassed = data.bypassed || false
            instanceId = data.instanceId || -1
            parameters = data.parameters || []
        } catch(e) {
            parameters = []
        }
    }

    contentItem: ColumnLayout {
        spacing: 8

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Label {
                text: pluginName
                font.bold: true
                font.pointSize: 11
                elide: Text.ElideRight
                Layout.fillWidth: true
            }

            Switch {
                id: bypassSwitch
                text: "Bypass"
                checked: pluginBypassed
                onToggled: {
                    if (pluginNodeId >= 0) {
                        controller.set_plugin_bypass(pluginNodeId, checked)
                    }
                }
            }
        }

        Label {
            text: pluginUri
            font.pointSize: 8
            opacity: 0.5
            elide: Text.ElideRight
            Layout.fillWidth: true
        }

        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: "#3c3c3c"
        }

        Label {
            text: parameters.length + " parameter" + (parameters.length !== 1 ? "s" : "")
            opacity: 0.6
            visible: parameters.length > 0
        }

        Label {
            text: "No control parameters"
            opacity: 0.5
            visible: parameters.length === 0
            Layout.alignment: Qt.AlignHCenter
        }

        ListView {
            id: paramList
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: parameters.length
            spacing: 2

            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            delegate: Rectangle {
                id: paramDelegate
                required property int index
                width: paramList.width - 12
                height: 56
                color: index % 2 === 0 ? "#2a2a2a" : "#252525"
                radius: 3

                property var param: parameters[index] || {}

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 6
                    spacing: 2

                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            text: param.name || ""
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
                                if (pluginNodeId >= 0 && param.portIndex !== undefined) {
                                    controller.set_plugin_parameter(pluginNodeId, param.portIndex, param.default)
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
                            if (pluginNodeId >= 0 && param.portIndex !== undefined) {
                                controller.set_plugin_parameter(pluginNodeId, param.portIndex, value)
                            }
                        }
                    }
                }
            }
        }
    }
}
