import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Patchbay Rules (spec §6.2): dialog shell, filterable card list of rules,
// footer with secondary actions left and "Apply rules now" as the primary.
ZbWindow {
    id: ruleEditor
    title: "Patchbay Rules"
    width: 760
    height: 780
    minimumWidth: 560
    minimumHeight: 420
    visible: false

    required property var controller

    property var rules: []
    property var nodeNames: []
    property var nodeTypes: ["Any", "Sink", "Source", "App Out", "App In", "Duplex", "Plugin"]
    property var backups: []
    property string pendingRestoreFilename: ""

    // Indices into `rules` that match the filter. The list model is the
    // count (not the array) so reloading rules keeps the scroll position.
    readonly property var filteredIdx: computeFilter(rules, filterField.text)
    readonly property int enabledCount: {
        var n = 0;
        for (var i = 0; i < rules.length; i++)
            if (rules[i].enabled)
                n++;
        return n;
    }
    readonly property bool popupOpen: snapshotConfirmDialog.visible || restoreConfirmDialog.visible
                                      || backupNameDialog.visible || addRuleDialog.visible
                                      || backupsDialog.visible

    function computeFilter(list, text) {
        var q = (text || "").trim().toLowerCase();
        var out = [];
        for (var i = 0; i < list.length; i++) {
            var r = list[i] || {};
            if (q.length === 0) {
                out.push(i);
                continue;
            }
            var hay = ((r.sourcePattern || "") + " " + (r.sourceLabel || "") + " "
                       + (r.targetPattern || "") + " " + (r.targetLabel || "")
                       + (r.action === "block" ? " blocked" : "")).toLowerCase();
            if (hay.indexOf(q) >= 0)
                out.push(i);
        }
        return out;
    }

    // Badge tone for a rule endpoint type (bible §2.4).
    function typeTone(t) {
        if (t === "Source" || t === "Duplex") return "teal";
        if (t === "Sink") return "blue";
        if (t === "Plugin") return "violet";
        if (t === "App Out" || t === "App In") return "orange";
        return "neutral";
    }

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

    function loadBackups() {
        try {
            backups = JSON.parse(controller.list_rule_backups_json());
        } catch (e) {
            backups = [];
        }
    }

    function open() {
        loadRules();
        loadBackups();
        visible = true;
        raise();
        requestActivate();
    }

    Connections {
        target: ruleEditor.controller
        function onGraph_changed() {
            if (ruleEditor.visible) {
                ruleEditor.loadRules();
            }
        }
        function onRules_changed() {
            if (ruleEditor.visible) {
                ruleEditor.loadRules();
            }
        }
    }

    // Escape closes the window (open sub-dialogs handle Escape themselves).
    Shortcut {
        sequence: "Escape"
        enabled: ruleEditor.active && !ruleEditor.popupOpen
        onActivated: ruleEditor.close()
    }

    // Sub-dialogs use ZbDialog (in-window modal shell).
    component BodyText: Text {
        font.family: Theme.fontSans
        font.pixelSize: Theme.sizeBody
        color: Theme.textPrimary
        wrapMode: Text.WordWrap
    }

    component CaptionText: Text {
        font.family: Theme.fontSans
        font.pixelSize: Theme.sizeCaption
        color: Theme.textMuted
        wrapMode: Text.WordWrap
    }

    // Footer matching ZbDialog's, for sub-dialogs that need their own
    // buttons (a primary that can be disabled, or extra actions on the left).
    component DialogFooter: Item {
        default property alias buttons: footerRow.data
        implicitHeight: footerRow.implicitHeight + 16 + 20
        RowLayout {
            id: footerRow
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: Theme.dialogPadding
            anchors.rightMargin: Theme.dialogPadding
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 20
            spacing: 8
        }
    }

    ZbDialog {
        id: snapshotConfirmDialog
        z: 2
        title: "Replace all rules?"
        primaryText: "Snapshot connections"
        secondaryText: "Cancel"
        width: Math.min(ruleEditor.width - 2 * Theme.dialogPadding, 440)

        BodyText {
            width: parent.width
            text: "This will delete all existing rules (" + ruleEditor.rules.length + ") and replace them with rules based on the connections currently active on the graph.\n\nThis cannot be undone."
        }

        onAccepted: {
            ruleEditor.controller.snapshot_rules()
            ruleEditor.loadRules()
        }
    }

    ZbDialog {
        id: restoreConfirmDialog
        z: 2
        title: "Restore this backup?"
        primaryText: "Restore"
        secondaryText: "Cancel"
        width: Math.min(ruleEditor.width - 2 * Theme.dialogPadding, 440)

        BodyText {
            width: parent.width
            text: "This will replace all current rules (" + ruleEditor.rules.length + ") with the rules from the selected backup.\n\nThis cannot be undone."
        }

        onAccepted: {
            if (ruleEditor.pendingRestoreFilename !== "") {
                ruleEditor.controller.restore_rule_backup(ruleEditor.pendingRestoreFilename)
                ruleEditor.pendingRestoreFilename = ""
                ruleEditor.loadRules()
                ruleEditor.loadBackups()
            }
        }

        onRejected: {
            ruleEditor.pendingRestoreFilename = ""
        }
    }

    ZbDialog {
        id: backupNameDialog
        z: 2
        title: "Save backup"
        primaryText: "Save backup"
        secondaryText: "Cancel"
        width: Math.min(ruleEditor.width - 2 * Theme.dialogPadding, 400)

        ColumnLayout {
            width: parent.width
            spacing: 12

            CaptionText {
                text: "Enter a name for this backup (optional)."
                Layout.fillWidth: true
            }
            ZbTextField {
                id: backupNameField
                Layout.fillWidth: true
                placeholderText: "e.g. Before mixer changes"
                onAccepted: backupNameDialog.accept()
            }
        }

        onOpened: {
            backupNameField.text = ""
            backupNameField.forceActiveFocus()
        }

        onAccepted: {
            ruleEditor.controller.backup_rules(backupNameField.text)
            backupNameField.text = ""
            ruleEditor.loadBackups()
        }
    }

    // ─── Add rule ───────────────────────────────────────────────────────
    ZbDialog {
        id: addRuleDialog
        title: "Add rule"
        primaryText: "Create rule"
        secondaryText: "Cancel"
        width: Math.min(ruleEditor.width - 2 * Theme.dialogPadding, 560)

        readonly property bool acceptEnabled: sourcePatternField.text.length > 0 && targetPatternField.text.length > 0

        primaryEnabled: acceptEnabled

        function resetFields() {
            actionSeg.currentIndex = 0;
            sourcePatternField.text = "";
            targetPatternField.text = "";
            sourceTypeSeg.currentIndex = 0;
            targetTypeSeg.currentIndex = 0;
        }

        function createRule() {
            if (!acceptEnabled)
                return;
            var srcType = ruleEditor.nodeTypes[sourceTypeSeg.currentIndex];
            var tgtType = ruleEditor.nodeTypes[targetTypeSeg.currentIndex];
            srcType = srcType === "Any" ? "" : srcType;
            tgtType = tgtType === "Any" ? "" : tgtType;
            ruleEditor.controller.add_rule(sourcePatternField.text, srcType, targetPatternField.text, tgtType,
                                           actionSeg.currentIndex === 1 ? "block" : "connect");
            resetFields();
            ruleEditor.loadRules();
        }

        onOpened: sourcePatternField.forceActiveFocus()
        onAccepted: createRule()

        ColumnLayout {
            width: parent.width
            spacing: 8

            ZbSegmented {
                id: actionSeg
                model: ["Connect", "Block"]
            }
            CaptionText {
                text: actionSeg.currentIndex === 1
                      ? "Block: the source can never be connected to the target, by rules, the default node or by hand. Patterns match node names; * is a wildcard."
                      : "Connect: the source is connected to the target whenever both exist. Patterns match node names; * is a wildcard."
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Item { implicitHeight: 4 }

            ZbSectionLabel { text: "Source" }
            ZbTextField {
                id: sourcePatternField
                Layout.fillWidth: true
                placeholderText: "Pattern (e.g. Firefox*)"
                onAccepted: targetPatternField.forceActiveFocus()
            }
            ZbSegmented {
                id: sourceTypeSeg
                model: ruleEditor.nodeTypes
            }

            Item { implicitHeight: 8 }

            ZbSectionLabel { text: "Target" }
            ZbTextField {
                id: targetPatternField
                Layout.fillWidth: true
                placeholderText: "Pattern (e.g. Built-in Audio*)"
                onAccepted: if (addRuleDialog.acceptEnabled) addRuleDialog.accept()
            }
            ZbSegmented {
                id: targetTypeSeg
                model: ruleEditor.nodeTypes
            }

            Item {
                implicitHeight: 8
                visible: ruleEditor.nodeNames.length > 0
            }

            ZbSectionLabel {
                text: "Quick-fill from current nodes"
                visible: ruleEditor.nodeNames.length > 0
            }
            CaptionText {
                visible: ruleEditor.nodeNames.length > 0
                text: "Fills the source first, then the target."
                Layout.fillWidth: true
            }

            Flow {
                visible: ruleEditor.nodeNames.length > 0
                Layout.fillWidth: true
                spacing: 4

                Repeater {
                    model: Math.min(ruleEditor.nodeNames.length, 20)
                    delegate: ZbButton {
                        required property int index
                        property var nodeInfo: ruleEditor.nodeNames[index] || {}
                        kind: "subtle"
                        compact: true
                        text: (nodeInfo.name || "") + (nodeInfo.type ? " · " + nodeInfo.type : "")
                        onClicked: {
                            var idx = ruleEditor.nodeTypes.indexOf(nodeInfo.type || "");
                            if (sourcePatternField.text.length === 0) {
                                sourcePatternField.text = nodeInfo.name || "";
                                if (idx >= 0)
                                    sourceTypeSeg.currentIndex = idx;
                            } else {
                                targetPatternField.text = nodeInfo.name || "";
                                if (idx >= 0)
                                    targetTypeSeg.currentIndex = idx;
                            }
                        }
                    }
                }
            }
        }
    }

    // ─── Backups ────────────────────────────────────────────────────────
    ZbDialog {
        id: backupsDialog
        title: "Backups"
        width: Math.min(ruleEditor.width - 2 * Theme.dialogPadding, 600)

        onAboutToShow: ruleEditor.loadBackups()

        // List dialog: "Save backup…" left, Close right, no primary.
        footer: DialogFooter {
            ZbButton {
                text: "Save backup…"
                onClicked: backupNameDialog.open()
            }
            Item { Layout.fillWidth: true }
            ZbButton {
                text: "Close"
                onClicked: backupsDialog.close()
            }
        }

        ColumnLayout {
            width: parent.width
            spacing: 12

            CaptionText {
                Layout.fillWidth: true
                text: ruleEditor.backups.length > 0
                      ? ruleEditor.backups.length + " backup" + (ruleEditor.backups.length !== 1 ? "s" : "") + " saved"
                      : "No backups yet. Use “Save backup…” to create one."
            }

            ZbCard {
                Layout.fillWidth: true
                visible: ruleEditor.backups.length > 0
                implicitHeight: Math.min(ruleEditor.backups.length * Theme.rowRule,
                                         Math.max(Theme.rowRule * 2, ruleEditor.height - 360)) + 2

                ListView {
                    id: backupList
                    ZbWheelScroll { flickable: backupList }
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.right: backupBar.visible ? backupBar.left : parent.right
                    anchors.margins: 1
                    anchors.rightMargin: backupBar.visible ? Theme.scrollGap : 1
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    model: ruleEditor.backups.length
                    ScrollBar.vertical: backupBar

                    delegate: Item {
                        id: backupDelegate
                        required property int index
                        property var backup: ruleEditor.backups[index] || {}
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
                                    text: backupDelegate.backup.name || backupDelegate.backup.date || ""
                                    font.family: Theme.fontSans
                                    font.pixelSize: Theme.sizeBody
                                    font.weight: Font.Medium
                                    color: Theme.textPrimary
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: (backupDelegate.backup.name ? (backupDelegate.backup.date || "") + " · " : "")
                                          + (backupDelegate.backup.ruleCount || 0) + " rules"
                                    font.family: Theme.fontMono
                                    font.pixelSize: Theme.sizeMono
                                    color: Theme.textMuted
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                            }

                            ZbButton {
                                id: restoreButton
                                kind: "subtle"
                                compact: true
                                text: "Restore"
                                onClicked: {
                                    ruleEditor.pendingRestoreFilename = backupDelegate.backup.filename || ""
                                    restoreConfirmDialog.open()
                                }
                                ZbTooltip {
                                    visible: restoreButton.hovered
                                    text: "Replace current rules with this backup"
                                }
                            }

                            ZbButton {
                                id: backupTrashButton
                                iconName: "trash"
                                compact: true
                                implicitWidth: 28
                                implicitHeight: 28
                                onClicked: {
                                    if (backupDelegate.backup.filename) {
                                        ruleEditor.controller.delete_rule_backup(backupDelegate.backup.filename)
                                        ruleEditor.loadBackups()
                                    }
                                }
                                ZbTooltip {
                                    visible: backupTrashButton.hovered
                                    text: "Delete backup"
                                }
                            }
                        }

                        ZbDivider {
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            visible: backupDelegate.index < ruleEditor.backups.length - 1
                        }
                    }
                }

                ZbScrollBar {
                    id: backupBar
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.right: parent.right
                    anchors.margins: 4
                }
            }
        }
    }

    // ─── Main content ───────────────────────────────────────────────────
    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.dialogPadding
        spacing: 16

        ZbDialogHeader {
            title: "Auto-connect rules"
            subtitle: "Learned automatically when you connect ports by hand. Block a connection by right-clicking a wire, or right-dragging between ports. Disabled rules are kept but not applied."

            ZbSwitch {
                id: patchbaySwitch
                text: "Rules"
                checked: ruleEditor.controller.patchbay_enabled
                onToggled: ruleEditor.controller.toggle_patchbay(checked)
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            ZbSearchField {
                id: filterField
                Layout.fillWidth: true
                placeholderText: "Filter rules"
            }

            Text {
                text: {
                    var total = ruleEditor.rules.length;
                    var shown = ruleEditor.filteredIdx.length;
                    var head = filterField.text.trim().length > 0
                               ? shown + " of " + total + " rule" + (total !== 1 ? "s" : "")
                               : total + " rule" + (total !== 1 ? "s" : "");
                    return head + " · " + ruleEditor.enabledCount + " enabled";
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
                id: ruleList
                ZbWheelScroll { flickable: ruleList }
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.right: ruleBar.visible ? ruleBar.left : parent.right
                anchors.margins: 1
                anchors.rightMargin: ruleBar.visible ? Theme.scrollGap : 1
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: ruleEditor.filteredIdx.length
                ScrollBar.vertical: ruleBar

                delegate: Item {
                    id: ruleDelegate
                    required property int index
                    property var rule: ruleEditor.rules[ruleEditor.filteredIdx[index]] || {}
                    readonly property int mappingCount: rule.portMappings ? rule.portMappings.length : 0
                    readonly property bool isBlock: rule.action === "block"
                    width: ListView.view.width
                    height: Theme.rowRule

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: Theme.cardPadding
                        anchors.rightMargin: Theme.cardPadding
                        spacing: 12

                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.preferredWidth: 0
                            spacing: 3

                            // Line 1: source + badge → target + badge, one line;
                            // the two names share what's left and elide.
                            Item {
                                id: line1
                                Layout.fillWidth: true
                                implicitHeight: lineRow.implicitHeight
                                clip: true

                                readonly property int parts: 3 + (srcBadge.visible ? 1 : 0) + (tgtBadge.visible ? 1 : 0)
                                readonly property real fixedWidth: (srcBadge.visible ? srcBadge.implicitWidth : 0)
                                                                   + arrowIcon.implicitWidth
                                                                   + (tgtBadge.visible ? tgtBadge.implicitWidth : 0)
                                                                   + (parts - 1) * lineRow.spacing
                                readonly property real nameSpace: Math.max(0, width - fixedWidth)

                                Row {
                                    id: lineRow
                                    spacing: 8

                                    Text {
                                        id: srcName
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: Math.min(implicitWidth, Math.max(line1.nameSpace / 2, line1.nameSpace - tgtName.implicitWidth))
                                        text: ruleDelegate.rule.sourcePattern || ruleDelegate.rule.sourceLabel || ""
                                        font.family: Theme.fontSans
                                        font.pixelSize: Theme.sizeBody
                                        font.weight: Font.Medium
                                        color: Theme.textPrimary
                                        elide: Text.ElideRight
                                    }
                                    ZbBadge {
                                        id: srcBadge
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: !!ruleDelegate.rule.sourceType && ruleDelegate.rule.sourceType !== "Any"
                                        text: ruleDelegate.rule.sourceType || ""
                                        tone: ruleEditor.typeTone(ruleDelegate.rule.sourceType)
                                    }
                                    ZbIcon {
                                        id: arrowIcon
                                        anchors.verticalCenter: parent.verticalCenter
                                        name: ruleDelegate.isBlock ? "block" : "arrow"
                                        size: 14
                                        color: ruleDelegate.isBlock ? Theme.textDestructive : Theme.textMuted
                                    }
                                    Text {
                                        id: tgtName
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: Math.min(implicitWidth, Math.max(line1.nameSpace / 2, line1.nameSpace - srcName.implicitWidth))
                                        text: ruleDelegate.rule.targetPattern || ruleDelegate.rule.targetLabel || ""
                                        font.family: Theme.fontSans
                                        font.pixelSize: Theme.sizeBody
                                        font.weight: Font.Medium
                                        color: Theme.textPrimary
                                        elide: Text.ElideRight
                                    }
                                    ZbBadge {
                                        id: tgtBadge
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: !!ruleDelegate.rule.targetType && ruleDelegate.rule.targetType !== "Any"
                                        text: ruleDelegate.rule.targetType || ""
                                        tone: ruleEditor.typeTone(ruleDelegate.rule.targetType)
                                    }
                                }
                            }

                            Row {
                                spacing: 6
                                ZbBadge {
                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: ruleDelegate.isBlock
                                    text: "Blocked"
                                    tone: "danger"
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: ruleDelegate.mappingCount > 0
                                          ? ruleDelegate.mappingCount + " port mapping" + (ruleDelegate.mappingCount !== 1 ? "s" : "")
                                          : ruleDelegate.isBlock ? "never connected" : "heuristic matching"
                                    font.family: Theme.fontMono
                                    font.pixelSize: Theme.sizeMono
                                    color: Theme.textMuted
                                }
                            }
                        }

                        ZbSwitch {
                            id: ruleSwitch
                            compact: true
                            checked: ruleDelegate.rule.enabled === true
                            onToggled: {
                                if (ruleDelegate.rule.id) {
                                    ruleEditor.controller.toggle_rule(ruleDelegate.rule.id);
                                    ruleEditor.loadRules();
                                }
                            }
                            ZbTooltip {
                                visible: ruleSwitch.hovered
                                text: ruleDelegate.rule.enabled ? "Enabled" : "Disabled"
                            }
                        }

                        ZbButton {
                            id: ruleTrashButton
                            iconName: "trash"
                            compact: true
                            implicitWidth: 28
                            implicitHeight: 28
                            onClicked: {
                                if (ruleDelegate.rule.id) {
                                    ruleEditor.controller.remove_rule(ruleDelegate.rule.id);
                                    ruleEditor.loadRules();
                                }
                            }
                            ZbTooltip {
                                visible: ruleTrashButton.hovered
                                text: "Delete rule"
                            }
                        }
                    }

                    ZbDivider {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        visible: ruleDelegate.index < ruleList.count - 1
                    }
                }
            }

            ZbScrollBar {
                id: ruleBar
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.right: parent.right
                anchors.margins: 4
            }

            Text {
                anchors.centerIn: parent
                width: parent.width - 2 * Theme.dialogPadding
                visible: ruleList.count === 0
                text: ruleEditor.rules.length === 0
                      ? "No rules yet.\nConnect some ports to create rules automatically, or use “Add rule” below."
                      : "No rules match “" + filterField.text.trim() + "”."
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeBody
                color: Theme.textMuted
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            ZbButton {
                text: "Add rule"
                iconName: "plus"
                onClicked: addRuleDialog.open()
            }

            ZbButton {
                text: "Backups…"
                onClicked: {
                    ruleEditor.loadBackups()
                    backupsDialog.open()
                }
            }

            ZbButton {
                id: saveBackupButton
                text: "Save backup"
                onClicked: backupNameDialog.open()
                ZbTooltip {
                    visible: saveBackupButton.hovered
                    text: "Save current rules as a backup"
                }
            }

            Item { Layout.fillWidth: true }

            ZbButton {
                id: snapshotButton
                text: "Snapshot connections"
                onClicked: snapshotConfirmDialog.open()
                ZbTooltip {
                    visible: snapshotButton.hovered
                    text: "Replace all rules with current connections"
                }
            }

            ZbButton {
                kind: "primary"
                text: "Apply rules now"
                onClicked: ruleEditor.controller.apply_rules()
            }
        }
    }
}
