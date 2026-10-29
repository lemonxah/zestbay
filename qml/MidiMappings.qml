import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ZestBay

// MIDI mappings: the dialog shell (bible §6.1) with a filterable card list
// of CC/note → parameter mappings. Mono for CC numbers, channels and modes.
ZbWindow {
    id: midiMappings
    title: "MIDI Mappings"
    width: 680
    height: 560
    minimumWidth: 480
    minimumHeight: 340
    visible: false

    required property var controller

    property var mappings: []

    // Indices into `mappings` matching the filter; the list model is the
    // count so reloads keep the scroll position.
    readonly property var filteredIdx: computeFilter(mappings, filterField.text)

    function computeFilter(list, text) {
        var q = (text || "").trim().toLowerCase()
        var out = []
        for (var i = 0; i < list.length; i++) {
            if (q.length === 0) {
                out.push(i)
                continue
            }
            var m = list[i] || {}
            var src = m.source || {}
            var hay = ((m.label || "") + " " + (src.device_name || "") + " " + sourceText(src)).toLowerCase()
            if (hay.indexOf(q) >= 0)
                out.push(i)
        }
        return out
    }

    // "CC 7 · ch 1" / "Note 60 · any ch"
    function sourceText(src) {
        var kind = src.message_type === "Note" ? "Note " : "CC "
        var num = src.cc !== undefined && src.cc !== null ? src.cc : "?"
        var ch = src.channel !== null && src.channel !== undefined ? "ch " + (src.channel + 1) : "any ch"
        return kind + num + " · " + ch
    }

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
        target: midiMappings.controller
        function onMidi_mapping_added(mapping_json) {
            midiMappings.loadMappings()
        }
        function onMidi_mapping_removed(instance_id, port_index) {
            midiMappings.loadMappings()
        }
    }

    Shortcut {
        sequence: "Escape"
        enabled: midiMappings.active
        onActivated: midiMappings.close()
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.dialogPadding
        spacing: 16

        ZbDialogHeader {
            title: "MIDI mappings"
            subtitle: "Connect your MIDI controller to “ZestBay MIDI In” in the graph, then use the MIDI button in Plugin Parameters to learn a mapping."
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            ZbSearchField {
                id: filterField
                Layout.fillWidth: true
                placeholderText: "Filter mappings"
            }

            Text {
                text: {
                    var total = midiMappings.mappings.length
                    var noun = total + " mapping" + (total !== 1 ? "s" : "")
                    return filterField.text.trim().length > 0
                           ? midiMappings.filteredIdx.length + " of " + noun
                           : noun
                }
                font.family: Theme.fontMono
                font.pixelSize: Theme.sizeMono
                color: Theme.textMuted
            }
        }

        ZbCard {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ListView {
                id: mappingList
                ZbWheelScroll { flickable: mappingList }
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.right: mappingBar.visible ? mappingBar.left : parent.right
                anchors.margins: 1
                anchors.rightMargin: mappingBar.visible ? Theme.scrollGap : 1
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: midiMappings.filteredIdx.length
                ScrollBar.vertical: mappingBar

                delegate: Item {
                    id: mappingDelegate
                    required property int index
                    property var mapping: midiMappings.mappings[midiMappings.filteredIdx[index]] || {}
                    property var source: mapping.source || {}
                    property var target: mapping.target || {}
                    width: ListView.view.width
                    height: Theme.rowRule

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: Theme.cardPadding
                        anchors.rightMargin: Theme.cardPadding
                        spacing: 12

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 3

                            Text {
                                text: mappingDelegate.mapping.label
                                      || ("Instance " + (mappingDelegate.target.instance_id !== undefined ? mappingDelegate.target.instance_id : "?")
                                          + " port " + (mappingDelegate.target.port_index !== undefined ? mappingDelegate.target.port_index : "?"))
                                font.family: Theme.fontSans
                                font.pixelSize: Theme.sizeBody
                                font.weight: Font.Medium
                                color: Theme.textPrimary
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 8

                                Text {
                                    text: midiMappings.sourceText(mappingDelegate.source)
                                    font.family: Theme.fontMono
                                    font.pixelSize: Theme.sizeMono
                                    color: Theme.textSecondary
                                }
                                Text {
                                    text: "·"
                                    font.family: Theme.fontMono
                                    font.pixelSize: Theme.sizeMono
                                    color: Theme.textMuted
                                }
                                Text {
                                    text: mappingDelegate.source.device_name || "Unknown device"
                                    font.family: Theme.fontMono
                                    font.pixelSize: Theme.sizeMono
                                    color: Theme.textMuted
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                            }
                        }

                        ZbBadge {
                            text: {
                                var m = mappingDelegate.mapping.mode
                                if (m === "Toggle") return "Toggle"
                                if (m === "Momentary") return "Momentary"
                                return "Continuous"
                            }
                        }

                        ZbButton {
                            id: mappingTrashButton
                            iconName: "trash"
                            compact: true
                            implicitWidth: 28
                            implicitHeight: 28
                            onClicked: {
                                if (mappingDelegate.target.instance_id !== undefined && mappingDelegate.target.port_index !== undefined) {
                                    midiMappings.controller.remove_midi_mapping_for_param(mappingDelegate.target.instance_id, mappingDelegate.target.port_index)
                                }
                            }
                            ZbTooltip {
                                visible: mappingTrashButton.hovered
                                text: "Remove this mapping"
                            }
                        }
                    }

                    ZbDivider {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        visible: mappingDelegate.index < mappingList.count - 1
                    }
                }
            }

            ZbScrollBar {
                id: mappingBar
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.right: parent.right
                anchors.margins: 4
            }

            Text {
                anchors.centerIn: parent
                width: parent.width - 2 * Theme.dialogPadding
                visible: mappingList.count === 0
                text: midiMappings.mappings.length === 0
                      ? "No MIDI mappings yet."
                      : "No mappings match “" + filterField.text.trim() + "”."
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeBody
                color: Theme.textMuted
            }
        }
    }
}
