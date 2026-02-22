import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Dialog {
    id: ruleEditor
    title: "Patchbay Rules"
    width: 720
    height: 520
    modal: true
    standardButtons: Dialog.Close

    required property var controller

    property var rules: []
    property var nodeNames: []
    property var nodeTypes: ["Any", "Sink", "Source", "App Out", "App In", "Duplex", "Plugin"]

    function loadRules() {
        try {
            rules = JSON.parse(controller.get_rules_json())
        } catch(e) {
            rules = []
        }
        try {
            nodeNames = JSON.parse(controller.get_node_names_json())
        } catch(e) {
            nodeNames = []
        }
    }

    onOpened: loadRules()

    Connections {
        target: controller
        function onGraph_changed() {
            if (ruleEditor.visible) {
                loadRules()
            }
        }
    }

    contentItem: ColumnLayout {
        spacing: 8

        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            Label {
                text: "Auto-Connect Rules"
                font.bold: true
                font.pointSize: 12
            }

            Item { Layout.fillWidth: true }

            Label {
                text: "Rules:"
            }

            Switch {
                id: patchbaySwitch
                checked: controller.patchbay_enabled
                onToggled: controller.toggle_patchbay(checked)
            }
        }

        Label {
            text: "Rules are auto-learned when you connect ports manually."
            font.italic: true
            opacity: 0.7
            Layout.fillWidth: true
        }

        Label {
            text: rules.length + " rule" + (rules.length !== 1 ? "s" : "")
            opacity: 0.6
        }

        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: "#3c3c3c"
        }

        ListView {
            id: ruleList
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: rules.length
            spacing: 2

            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            delegate: Rectangle {
                id: ruleDelegate
                required property int index
                width: ruleList.width - 12
                height: 60
                color: ruleMouseArea.containsMouse ? "#3a3a3a" : (index % 2 === 0 ? "#2a2a2a" : "#252525")
                radius: 4

                property var rule: rules[index] || {}

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 8
                    spacing: 8

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2

                        RowLayout {
                            spacing: 6
                            Label {
                                text: rule.sourceLabel || ""
                                font.bold: true
                                elide: Text.ElideRight
                                Layout.maximumWidth: 220
                            }
                            Label {
                                text: "\u2192"
                                font.pointSize: 12
                                opacity: 0.6
                            }
                            Label {
                                text: rule.targetLabel || ""
                                font.bold: true
                                elide: Text.ElideRight
                                Layout.maximumWidth: 220
                            }
                        }

                        RowLayout {
                            spacing: 8
                            Label {
                                text: {
                                    var parts = []
                                    if (rule.portMappings && rule.portMappings.length > 0)
                                        parts.push(rule.portMappings.length + " port mapping" + (rule.portMappings.length > 1 ? "s" : ""))
                                    else
                                        parts.push("heuristic matching")
                                    return parts.join("  |  ")
                                }
                                font.pointSize: 8
                                opacity: 0.5
                            }
                        }
                    }

                    Switch {
                        checked: rule.enabled || false
                        onToggled: {
                            if (rule.id) {
                                controller.toggle_rule(rule.id)
                                loadRules()
                            }
                        }

                        ToolTip.visible: hovered
                        ToolTip.text: rule.enabled ? "Enabled" : "Disabled"
                    }

                    Button {
                        text: "X"
                        flat: true
                        implicitWidth: 32
                        implicitHeight: 32
                        onClicked: {
                            if (rule.id) {
                                controller.remove_rule(rule.id)
                                loadRules()
                            }
                        }

                        ToolTip.visible: hovered
                        ToolTip.text: "Delete rule"
                    }
                }

                MouseArea {
                    id: ruleMouseArea
                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.NoButton
                }
            }

            Label {
                anchors.centerIn: parent
                text: "No rules yet.\nConnect some ports to create rules automatically,\nor use 'Add Rule' below."
                horizontalAlignment: Text.AlignHCenter
                opacity: 0.5
                visible: rules.length === 0
            }
        }

        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: "#3c3c3c"
        }

        ColumnLayout {
            id: addRuleSection
            Layout.fillWidth: true
            spacing: 6

            property bool expanded: false

            RowLayout {
                Layout.fillWidth: true
                spacing: 8

                Button {
                    text: addRuleSection.expanded ? "Cancel" : "Add Rule..."
                    onClicked: addRuleSection.expanded = !addRuleSection.expanded
                }

                Item { Layout.fillWidth: true }

                Button {
                    text: "Apply Rules Now"
                    onClicked: {
                        controller.apply_rules()
                    }
                }

                Button {
                    text: "Snapshot Connections"
                    onClicked: {
                        controller.snapshot_rules()
                        loadRules()
                    }

                    ToolTip.visible: hovered
                    ToolTip.text: "Replace all rules with current connections"
                }
            }

            GridLayout {
                visible: addRuleSection.expanded
                Layout.fillWidth: true
                columns: 4
                columnSpacing: 8
                rowSpacing: 6

                Label { text: "Source:" }
                TextField {
                    id: sourcePatternField
                    placeholderText: "Pattern (e.g. Firefox*)"
                    Layout.fillWidth: true
                    selectByMouse: true
                }
                Label { text: "Type:" }
                ComboBox {
                    id: sourceTypeCombo
                    model: nodeTypes
                    implicitWidth: 120
                }

                Label { text: "Target:" }
                TextField {
                    id: targetPatternField
                    placeholderText: "Pattern (e.g. Built-in Audio*)"
                    Layout.fillWidth: true
                    selectByMouse: true
                }
                Label { text: "Type:" }
                ComboBox {
                    id: targetTypeCombo
                    model: nodeTypes
                    implicitWidth: 120
                }

                Item {}
                Button {
                    text: "Create Rule"
                    enabled: sourcePatternField.text.length > 0 && targetPatternField.text.length > 0
                    onClicked: {
                        var srcType = sourceTypeCombo.currentText === "Any" ? "" : sourceTypeCombo.currentText
                        var tgtType = targetTypeCombo.currentText === "Any" ? "" : targetTypeCombo.currentText
                        controller.add_rule(sourcePatternField.text, srcType, targetPatternField.text, tgtType)
                        sourcePatternField.text = ""
                        targetPatternField.text = ""
                        sourceTypeCombo.currentIndex = 0
                        targetTypeCombo.currentIndex = 0
                        addRuleSection.expanded = false
                        loadRules()
                    }
                }
                Item {}
                Item {}
            }

            GridLayout {
                visible: addRuleSection.expanded && nodeNames.length > 0
                Layout.fillWidth: true
                columns: 1
                columnSpacing: 8

                Label {
                    text: "Quick-fill from existing nodes:"
                    font.italic: true
                    opacity: 0.6
                }

                Flow {
                    Layout.fillWidth: true
                    spacing: 4

                    Repeater {
                        model: Math.min(nodeNames.length, 20)
                        delegate: Button {
                            required property int index
                            property var nodeInfo: nodeNames[index] || {}
                            text: (nodeInfo.name || "") + " [" + (nodeInfo.type || "") + "]"
                            flat: true
                            font.pointSize: 8
                            onClicked: {
                                if (sourcePatternField.text.length === 0) {
                                    sourcePatternField.text = nodeInfo.name || ""
                                    var srcIdx = nodeTypes.indexOf(nodeInfo.type || "")
                                    if (srcIdx >= 0) sourceTypeCombo.currentIndex = srcIdx
                                } else {
                                    targetPatternField.text = nodeInfo.name || ""
                                    var tgtIdx = nodeTypes.indexOf(nodeInfo.type || "")
                                    if (tgtIdx >= 0) targetTypeCombo.currentIndex = tgtIdx
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
