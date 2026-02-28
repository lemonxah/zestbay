import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: ruleEditor
    title: "Patchbay Rules"
    width: 720
    height: 520
    minimumWidth: 500
    minimumHeight: 400
    visible: false

    required property var controller

    property var rules: []
    property var nodeNames: []
    property var nodeTypes: ["Any", "Sink", "Source", "App Out", "App In", "Duplex", "Plugin"]

    function loadRules() {
        try {
            rules = JSON.parse(controller.get_rules_json());
        } catch (e) {
            rules = [];
        }
        try {
            nodeNames = JSON.parse(controller.get_node_names_json());
        } catch (e) {
            nodeNames = [];
        }
    }

    function open() {
        loadRules();
        visible = true;
        raise();
        requestActivate();
    }

    Connections {
        target: controller
        function onGraph_changed() {
            if (ruleEditor.visible) {
                loadRules();
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 8

        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            Label {
                text: "Auto-Connect Rules"
                font.bold: true
                font.pointSize: 12
            }

            Item {
                Layout.fillWidth: true
            }

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

            ScrollBar.vertical: ScrollBar {
                policy: ScrollBar.AsNeeded
            }

            // Column header row
            header: Item {
                width: ruleList.width - 12
                height: rules.length > 0 ? 28 : 0
                visible: rules.length > 0
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 10
                    anchors.rightMargin: 8
                    spacing: 8

                    Label {
                        text: "Rule"
                        font.bold: true
                        font.pointSize: 9
                        opacity: 0.5
                        Layout.fillWidth: true
                    }
                    Label {
                        text: "Enabled"
                        font.bold: true
                        font.pointSize: 9
                        opacity: 0.5
                        Layout.preferredWidth: 58
                        horizontalAlignment: Text.AlignRight
                    }
                    Label {
                        text: "Remove"
                        font.bold: true
                        font.pointSize: 9
                        opacity: 0.5
                        Layout.preferredWidth: 58
                        horizontalAlignment: Text.AlignRight
                    }
                }
            }

            delegate: Rectangle {
                id: ruleDelegate
                required property int index
                width: ruleList.width - 12
                height: 56
                color: ruleMouseArea.containsMouse ? "#3a3a3a" : (index % 2 === 0 ? "#2a2a2a" : "#252525")
                radius: 4

                property var rule: rules[index] || {}

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 10
                    anchors.rightMargin: 8
                    anchors.topMargin: 6
                    anchors.bottomMargin: 6
                    spacing: 8

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        spacing: 2

                        RowLayout {
                            spacing: 6
                            Layout.fillWidth: true
                            Label {
                                text: rule.sourceLabel || ""
                                font.bold: true
                                elide: Text.ElideRight
                                Layout.maximumWidth: 220
                            }
                            Label {
                                text: "\u2192"
                                font.pointSize: 12
                                elide: Text.ElideRight
                                opacity: 0.5
                            }
                            Label {
                                text: rule.targetLabel || ""
                                font.bold: true
                                elide: Text.ElideRight
                                Layout.maximumWidth: 220
                            }
                        }

                        Label {
                            text: {
                                if (rule.portMappings && rule.portMappings.length > 0)
                                    return rule.portMappings.length + " port mapping" + (rule.portMappings.length > 1 ? "s" : "");
                                return "heuristic matching";
                            }
                            font.pointSize: 8
                            opacity: 0.4
                        }
                    }

                    Item {
                        Layout.preferredWidth: 58
                        Layout.preferredHeight: 30
                        Layout.alignment: Qt.AlignVCenter | Qt.AlignRight
                        Layout.fillWidth: true

                        Switch {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            checked: rule.enabled || false
                            onToggled: {
                                if (rule.id) {
                                    controller.toggle_rule(rule.id);
                                    loadRules();
                                }
                            }

                            ToolTip.visible: hovered
                            ToolTip.text: rule.enabled ? "Enabled" : "Disabled"
                        }
                    }

                    Rectangle {
                        Layout.preferredWidth: 36
                        Layout.preferredHeight: 30
                        Layout.alignment: Qt.AlignVCenter | Qt.AlignRight
                        radius: 4
                        color: delMouseArea.containsMouse ? "#5c2020" : "transparent"
                        border.color: delMouseArea.containsMouse ? "#cc4444" : "#555555"
                        border.width: 1

                        // Trash can icon drawn with Canvas
                        Canvas {
                            anchors.centerIn: parent
                            width: 16
                            height: 16
                            onPaint: {
                                var ctx = getContext("2d");
                                ctx.reset();
                                var c = delMouseArea.containsMouse ? "#ff6666" : "#999999";
                                ctx.strokeStyle = c;
                                ctx.fillStyle = c;
                                ctx.lineWidth = 1.2;
                                ctx.lineCap = "round";

                                // Lid
                                ctx.beginPath();
                                ctx.moveTo(2, 4);
                                ctx.lineTo(14, 4);
                                ctx.stroke();

                                // Handle
                                ctx.beginPath();
                                ctx.moveTo(6, 4);
                                ctx.lineTo(6, 2.5);
                                ctx.lineTo(10, 2.5);
                                ctx.lineTo(10, 4);
                                ctx.stroke();

                                // Can body
                                ctx.beginPath();
                                ctx.moveTo(3.5, 4);
                                ctx.lineTo(4.5, 14);
                                ctx.lineTo(11.5, 14);
                                ctx.lineTo(12.5, 4);
                                ctx.stroke();

                                // Inner lines
                                ctx.beginPath();
                                ctx.moveTo(6.5, 6.5);
                                ctx.lineTo(6.5, 11.5);
                                ctx.stroke();
                                ctx.beginPath();
                                ctx.moveTo(9.5, 6.5);
                                ctx.lineTo(9.5, 11.5);
                                ctx.stroke();
                            }
                        }

                        MouseArea {
                            id: delMouseArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                if (rule.id) {
                                    controller.remove_rule(rule.id);
                                    loadRules();
                                }
                            }
                        }

                        ToolTip.visible: delMouseArea.containsMouse
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

                Item {
                    Layout.fillWidth: true
                }

                Button {
                    text: "Apply Rules Now"
                    onClicked: {
                        controller.apply_rules();
                    }
                }

                Button {
                    text: "Snapshot Connections"
                    onClicked: {
                        controller.snapshot_rules();
                        loadRules();
                    }

                    ToolTip.visible: hovered
                    ToolTip.text: "Replace all rules with current connections"
                }

                Button {
                    text: "Close"
                    onClicked: ruleEditor.visible = false
                }
            }

            GridLayout {
                visible: addRuleSection.expanded
                Layout.fillWidth: true
                columns: 4
                columnSpacing: 8
                rowSpacing: 6

                Label {
                    text: "Source:"
                }
                TextField {
                    id: sourcePatternField
                    placeholderText: "Pattern (e.g. Firefox*)"
                    Layout.fillWidth: true
                    selectByMouse: true
                }
                Label {
                    text: "Type:"
                }
                ComboBox {
                    id: sourceTypeCombo
                    model: nodeTypes
                    implicitWidth: 120
                }

                Label {
                    text: "Target:"
                }
                TextField {
                    id: targetPatternField
                    placeholderText: "Pattern (e.g. Built-in Audio*)"
                    Layout.fillWidth: true
                    selectByMouse: true
                }
                Label {
                    text: "Type:"
                }
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
                        var srcType = sourceTypeCombo.currentText === "Any" ? "" : sourceTypeCombo.currentText;
                        var tgtType = targetTypeCombo.currentText === "Any" ? "" : targetTypeCombo.currentText;
                        controller.add_rule(sourcePatternField.text, srcType, targetPatternField.text, tgtType);
                        sourcePatternField.text = "";
                        targetPatternField.text = "";
                        sourceTypeCombo.currentIndex = 0;
                        targetTypeCombo.currentIndex = 0;
                        addRuleSection.expanded = false;
                        loadRules();
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
                                    sourcePatternField.text = nodeInfo.name || "";
                                    var srcIdx = nodeTypes.indexOf(nodeInfo.type || "");
                                    if (srcIdx >= 0)
                                        sourceTypeCombo.currentIndex = srcIdx;
                                } else {
                                    targetPatternField.text = nodeInfo.name || "";
                                    var tgtIdx = nodeTypes.indexOf(nodeInfo.type || "");
                                    if (tgtIdx >= 0)
                                        targetTypeCombo.currentIndex = tgtIdx;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
