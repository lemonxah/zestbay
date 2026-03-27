import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ZestBay

ApplicationWindow {
    id: midiMappings
    title: "MIDI Mappings"
    color: Theme.windowBg
    width: 650
    height: 450
    minimumWidth: 450
    minimumHeight: 300
    visible: false

    required property var controller

    property var mappings: []

    function loadMappings() {
        try {
            mappings = JSON.parse(controller.get_midi_mappings_json())
        } catch(e) {
            mappings = []
        }
    }

    function open() {
        loadMappings()
        visible = true
        raise()
        requestActivate()
    }

    Connections {
        target: controller
        function onMidi_mapping_added(mapping_json) {
            midiMappings.loadMappings()
        }
        function onMidi_mapping_removed(instance_id, port_index) {
            midiMappings.loadMappings()
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 8

        Label {
            text: mappings.length + " MIDI mapping" + (mappings.length !== 1 ? "s" : "")
            font.bold: true
            font.pointSize: 11
        }

        Label {
            text: "Connect your MIDI controller to \"ZestBay MIDI In\" in the graph view,\nthen use the \"M\" button in Plugin Parameters to learn a mapping."
            opacity: 0.5
            visible: mappings.length === 0
            Layout.alignment: Qt.AlignHCenter
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }

        ListView {
            id: mappingList
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: mappings.length
            spacing: 2
            visible: mappings.length > 0

            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            delegate: Rectangle {
                id: mappingDelegate
                required property int index
                width: mappingList.width - 12
                height: 52
                color: index % 2 === 0 ? Theme.rowEven : Theme.rowOdd
                radius: 3

                property var mapping: mappings[index] || {}
                property var source: mapping.source || {}
                property var target: mapping.target || {}

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 8
                    spacing: 8

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2

                        Label {
                            text: mapping.label || ("Instance " + (target.instance_id || "?") + " port " + (target.port_index || "?"))
                            font.pointSize: 9
                            font.bold: true
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }

                        RowLayout {
                            spacing: 8

                            Label {
                                text: {
                                    var ch = source.channel !== null && source.channel !== undefined ? (source.channel + 1) : "*"
                                    return "CC " + (source.cc !== undefined ? source.cc : "?") + "  ch" + ch
                                }
                                font.pointSize: 8
                                font.family: "monospace"
                                color: Theme.colMidi
                            }

                            Label {
                                text: source.device_name || "Unknown device"
                                font.pointSize: 8
                                opacity: 0.5
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }

                            Label {
                                text: {
                                    var m = mapping.mode
                                    if (m === "Toggle") return "Toggle"
                                    if (m === "Momentary") return "Momentary"
                                    return "Continuous"
                                }
                                font.pointSize: 7
                                font.italic: true
                                opacity: 0.6
                            }
                        }
                    }

                    Button {
                        text: "\u00d7"
                        flat: true
                        implicitWidth: 28
                        implicitHeight: 28
                        font.pointSize: 12
                        ToolTip.visible: hovered
                        ToolTip.text: "Remove this mapping"
                        onClicked: {
                            if (target.instance_id !== undefined && target.port_index !== undefined) {
                                controller.remove_midi_mapping_for_param(target.instance_id, target.port_index)
                            }
                        }
                    }
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

            Item { Layout.fillWidth: true }

            Button {
                text: "Close"
                onClicked: midiMappings.visible = false
            }
        }
    }
}
