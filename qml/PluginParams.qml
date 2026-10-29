import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ZestBay

// Plugin Parameters (spec §6.7): header with category chip, name, URI and the
// Bypass switch; filter row; scrolling card list of parameter rows.
ZbWindow {
    id: pluginParams
    title: pluginName ? pluginName + " - Parameters" : "Plugin Parameters"
    width: 600
    height: 720
    minimumWidth: 460
    minimumHeight: 420
    visible: false

    required property var controller

    property int pluginNodeId: -1
    property string pluginName: ""
    property string pluginUri: ""
    property bool pluginBypassed: false
    property int instanceId: -1
    // Port indices currently in paramModel (to detect a changed parameter set).
    property var paramPorts: []
    // Bumped whenever MIDI mappings change so the mapping labels re-evaluate.
    property int mappingsRevision: 0

    property int midiLearnInstanceId: -1
    property int midiLearnPortIndex: -1

    property string conflictSourceJson: ""
    property string conflictExistingLabel: ""

    // Filter field text (matches name or symbol, case-insensitive).
    property string filterText: ""

    ListModel {
        id: paramModel
    }

    // Parameter values arrive through pluginParameterChanged; this slow poll
    // only keeps name/bypass (and the parameter set) in sync.
    Timer {
        id: refreshTimer
        interval: 1000
        running: pluginParams.visible
        repeat: true
        onTriggered: pluginParams.loadParams()
    }

    Connections {
        target: pluginParams.controller
        function onPlugin_parameter_changed(instance_id, port_index, value) {
            if (!pluginParams.visible || instance_id !== pluginParams.instanceId) return
            var row = pluginParams.rowForPort(port_index)
            if (row >= 0) paramModel.setProperty(row, "value", value)
        }
        function onMidi_mapping_added(mapping_json) {
            pluginParams.mappingsRevision++
        }
        function onMidi_mapping_removed(instance_id, port_index) {
            pluginParams.mappingsRevision++
        }
    }

    // Escape: cancel a value edit, then clear the filter, then close.
    Shortcut {
        sequence: "Escape"
        enabled: pluginParams.visible
        onActivated: {
            if (midiConflictDialog.opened) {
                midiConflictDialog.reject()
                return
            }
            var item = pluginParams.activeFocusItem
            if (item && item.paramEditor === true) {
                item.cancelEdit()
                return
            }
            if (filterField.activeFocus && filterField.text.length > 0) {
                filterField.text = ""
                return
            }
            pluginParams.visible = false
        }
    }

    function rowForPort(portIndex) {
        for (var i = 0; i < paramModel.count; i++) {
            if (paramModel.get(i).portIndex === portIndex) return i
        }
        return -1
    }

    // Parameter info by LV2 port index (not by list position).
    function findParamByPort(portIndex) {
        var row = rowForPort(portIndex)
        return row >= 0 ? paramModel.get(row) : null
    }

    function openForNode(nodeId) {
        if (nodeId !== pluginNodeId) {
            paramModel.clear()
            paramPorts = []
            filterField.text = ""
        }
        pluginNodeId = nodeId
        loadParams()
        visible = true
        raise()
        requestActivate()
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
            var params = data.parameters || []
            var ports = params.map(function(p) { return p.portIndex })
            var sameSet = ports.length === paramPorts.length
            for (var k = 0; sameSet && k < ports.length; k++) {
                if (ports[k] !== paramPorts[k]) sameSet = false
            }
            if (!sameSet) {
                paramModel.clear()
                for (var i = 0; i < params.length; i++) {
                    var p = params[i]
                    paramModel.append({
                        portIndex: p.portIndex,
                        symbol: p.symbol || "",
                        name: p.name || "",
                        value: p.value,
                        min: p.min,
                        max: p.max,
                        defaultValue: p["default"],
                        isToggle: !!p.isToggle
                    })
                }
                paramPorts = ports
            } else {
                // Same parameters: only touch values that actually differ.
                for (var j = 0; j < params.length; j++) {
                    if (paramModel.get(j).value !== params[j].value)
                        paramModel.setProperty(j, "value", params[j].value)
                }
            }
        } catch(e) {
            paramModel.clear()
            paramPorts = []
        }
    }

    // Send a value to the plugin and reflect it in the list immediately.
    function setParam(portIndex, value) {
        if (pluginNodeId < 0 || portIndex === undefined || value === undefined) return
        controller.set_plugin_parameter(pluginNodeId, portIndex, value)
        var row = rowForPort(portIndex)
        if (row >= 0) paramModel.setProperty(row, "value", value)
    }

    function resetAllParams() {
        for (var i = 0; i < paramModel.count; i++) {
            var p = paramModel.get(i)
            if (p.defaultValue !== undefined && p.value !== p.defaultValue)
                setParam(p.portIndex, p.defaultValue)
        }
    }

    function matchesFilter(name, symbol, filter) {
        if (!filter) return true
        var f = filter.toLowerCase()
        return (name || "").toLowerCase().indexOf(f) >= 0
            || (symbol || "").toLowerCase().indexOf(f) >= 0
    }

    readonly property int matchCount: {
        var f = filterText
        var c = 0
        for (var i = 0; i < paramModel.count; i++) {
            var p = paramModel.get(i)
            if (matchesFilter(p.name, p.symbol, f)) c++
        }
        return c
    }

    function formatNumber(v) {
        return v !== undefined && v !== null ? Number(v).toFixed(3) : ""
    }

    function clearMidiLearnState() {
        midiLearnInstanceId = -1
        midiLearnPortIndex = -1
    }

    function showConflictDialog(sourceJson, existingLabel) {
        conflictSourceJson = sourceJson
        conflictExistingLabel = existingLabel
        midiConflictDialog.open()
    }

    // `revision` is only there so bindings re-run when mappings change.
    function getMidiMappingText(portIndex, revision) {
        if (instanceId < 0 || portIndex === undefined) return ""
        try {
            var json = controller.get_midi_mapping_for_param_json(instanceId, portIndex)
            if (!json || json === "") return ""
            var m = JSON.parse(json)
            if (!m || !m.source) return ""
            var ch = m.source.channel !== null && m.source.channel !== undefined ? (m.source.channel + 1) : "*"
            var prefix = m.source.message_type === "Note" ? "Note " : "CC "
            return prefix + m.source.cc + " ch" + ch
        } catch(e) {
            return ""
        }
    }

    // ─── MIDI mapping conflict ──────────────────────────────────────────
    Dialog {
        id: midiConflictDialog
        title: "MIDI mapping conflict"
        anchors.centerIn: parent
        modal: true
        width: Math.min(pluginParams.width * 0.8, 400)
        padding: 20
        topPadding: 12

        // Escape / closing without choosing keeps the existing mapping.
        onRejected: {
            pluginParams.controller.dismiss_midi_conflict()
            pluginParams.clearMidiLearnState()
        }

        background: Rectangle {
            color: Theme.bgPanel
            border.width: 1
            border.color: Theme.borderStrong
            radius: Theme.radiusCard
        }

        header: Item {
            implicitHeight: conflictTitle.implicitHeight + 20
            Text {
                id: conflictTitle
                x: 20
                y: 20
                width: parent.width - 40
                text: midiConflictDialog.title
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeListPrimary
                font.weight: Font.DemiBold
                color: Theme.textPrimary
                elide: Text.ElideRight
            }
        }

        ColumnLayout {
            width: parent.width
            spacing: 16

            Text {
                text: {
                    try {
                        var src = JSON.parse(pluginParams.conflictSourceJson)
                        var ch = src.channel !== null && src.channel !== undefined ? (src.channel + 1) : "*"
                        var prefix = src.message_type === "Note" ? "Note " : "CC "
                        return prefix + src.cc + " on channel " + ch + " from \"" + src.device_name + "\" is already assigned to:\n\n\"" + pluginParams.conflictExistingLabel + "\"\n\nReassign to the current parameter?"
                    } catch(e) {
                        return "This control is already assigned to \"" + pluginParams.conflictExistingLabel + "\". Reassign?"
                    }
                }
                wrapMode: Text.WordWrap
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeBody
                color: Theme.textPrimary
                Layout.fillWidth: true
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Item { Layout.fillWidth: true }

                ZbButton {
                    text: "Cancel"
                    onClicked: midiConflictDialog.reject()
                }

                ZbButton {
                    kind: "primary"
                    text: "Reassign"
                    onClicked: {
                        // The backend remembers the learned mapping that hit the
                        // conflict: it removes the existing mapping for that
                        // control and adds the new one for this parameter.
                        pluginParams.controller.reassign_midi_mapping()
                        pluginParams.clearMidiLearnState()
                        midiConflictDialog.close()
                    }
                }
            }
        }
    }

    // ─── Layout ─────────────────────────────────────────────────────────
    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.dialogPadding
        anchors.rightMargin: Theme.dialogPadding
        anchors.topMargin: 20
        anchors.bottomMargin: Theme.dialogPadding
        spacing: 12

        // Header: category chip · name + URI · Bypass switch.
        RowLayout {
            Layout.fillWidth: true
            spacing: 16

            RowLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignTop
                spacing: 10

                Rectangle {
                    Layout.alignment: Qt.AlignVCenter
                    implicitWidth: 28
                    implicitHeight: 28
                    radius: Theme.radiusSmall
                    color: Theme.tint(Theme.catPlugin, Theme.chipAlpha)
                    ZbIcon {
                        anchors.centerIn: parent
                        name: "plug"
                        size: 16
                        color: Theme.catPlugin
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Text {
                        Layout.fillWidth: true
                        text: pluginParams.pluginName || "Plugin"
                        font.family: Theme.fontSans
                        font.pixelSize: Theme.sizeTitle
                        font.weight: Font.DemiBold
                        font.letterSpacing: -Theme.sizeTitle * 0.01
                        color: Theme.textPrimary
                        elide: Text.ElideRight
                    }
                    Text {
                        id: uriText
                        readonly property bool isLink: /^https?:\/\//.test(pluginParams.pluginUri)
                        visible: pluginParams.pluginUri.length > 0
                        Layout.fillWidth: true
                        text: pluginParams.pluginUri.replace(/^https?:\/\//, "")
                        font.family: Theme.fontMono
                        font.pixelSize: Theme.sizeMono
                        font.underline: isLink && uriMouse.containsMouse
                        color: Theme.textMuted
                        elide: Text.ElideRight

                        MouseArea {
                            id: uriMouse
                            anchors.fill: parent
                            enabled: uriText.isLink
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: Qt.openUrlExternally(pluginParams.pluginUri)
                        }
                        ZbTooltip {
                            visible: uriMouse.containsMouse
                            text: pluginParams.pluginUri
                        }
                    }
                }
            }

            ZbSwitch {
                id: bypassSwitch
                Layout.alignment: Qt.AlignTop
                Layout.topMargin: 5
                text: "Bypass"
                checked: pluginParams.pluginBypassed
                onToggled: {
                    if (pluginParams.pluginNodeId >= 0) {
                        pluginParams.controller.set_plugin_bypass(pluginParams.pluginNodeId, checked)
                        pluginParams.pluginBypassed = checked
                    }
                }
            }
        }

        // Filter row: search · count · Reset all.
        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            ZbSearchField {
                id: filterField
                Layout.fillWidth: true
                placeholderText: "Filter parameters"
                onTextChanged: pluginParams.filterText = text
            }

            Text {
                text: pluginParams.filterText.length > 0
                      ? pluginParams.matchCount + " of " + paramModel.count + " parameters"
                      : paramModel.count + " parameter" + (paramModel.count !== 1 ? "s" : "")
                font.family: Theme.fontMono
                font.pixelSize: Theme.sizeMono
                color: Theme.textMuted
            }

            ZbButton {
                text: "Reset all"
                enabled: paramModel.count > 0
                onClicked: pluginParams.resetAllParams()
                ZbTooltip {
                    visible: parent.hovered
                    text: "Reset every parameter to its default"
                }
            }
        }

        // Card list of parameter rows; the bar sits beside the rows.
        ZbCard {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Item {
                anchors.fill: parent
                anchors.margins: 1

                ListView {
                    id: paramList
                    ZbWheelScroll { flickable: paramList }
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.right: paramBar.visible ? paramBar.left : parent.right
                    anchors.rightMargin: paramBar.visible ? Theme.scrollGap : 0
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    model: paramModel
                    ScrollBar.vertical: paramBar

                    delegate: Item {
                        id: paramDelegate
                        required property int index
                        required property var model

                        // Model row object: its role properties (value, ...) notify.
                        property var param: model
                        readonly property bool matches: pluginParams.matchesFilter(param.name, param.symbol,
                                                                                   pluginParams.filterText)
                        property bool isLearning: pluginParams.midiLearnInstanceId === pluginParams.instanceId
                                                  && pluginParams.midiLearnPortIndex === param.portIndex
                        property string midiMapping: pluginParams.getMidiMappingText(param.portIndex,
                                                                                     pluginParams.mappingsRevision)
                        readonly property real minValue: param.min !== undefined ? param.min : 0
                        readonly property real maxValue: param.max !== undefined ? param.max : 1
                        // Balance / pan style ranges label their ends L … R.
                        readonly property bool bipolar: minValue < 0 && maxValue > 0
                                                        && /(^|[^a-z])(bal|balance|pan|panning)([^a-z]|$)/i.test((param.name || "") + " " + (param.symbol || ""))

                        visible: matches
                        width: ListView.view.width
                        height: matches ? rowContent.implicitHeight + 24 : 0

                        ColumnLayout {
                            id: rowContent
                            x: Theme.cardPadding
                            y: 12
                            width: parent.width - 2 * Theme.cardPadding
                            spacing: 8

                            // Line 1: name · mapping · value · MIDI · reset.
                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 8

                                Text {
                                    Layout.fillWidth: true
                                    text: paramDelegate.param.name || ""
                                    elide: Text.ElideRight
                                    font.family: Theme.fontSans
                                    font.pixelSize: Theme.sizeBody
                                    font.weight: Font.Medium
                                    color: Theme.textPrimary
                                }

                                // Current MIDI mapping, with a remove control.
                                Rectangle {
                                    visible: paramDelegate.midiMapping !== "" && !paramDelegate.isLearning
                                    implicitHeight: 22
                                    implicitWidth: mappingRow.implicitWidth + 14
                                    radius: Theme.radiusSmall
                                    color: removeMouse.containsMouse ? Theme.bgHover : "transparent"
                                    border.width: 1
                                    border.color: Theme.borderStrong

                                    Row {
                                        id: mappingRow
                                        anchors.centerIn: parent
                                        spacing: 6
                                        Rectangle {
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: 6
                                            height: 6
                                            radius: 3
                                            color: Theme.sigMidi
                                        }
                                        Text {
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: paramDelegate.midiMapping
                                            font.family: Theme.fontMono
                                            font.pixelSize: Theme.sizeBadge
                                            font.weight: Font.Medium
                                            color: Theme.textSecondary
                                        }
                                        ZbIcon {
                                            anchors.verticalCenter: parent.verticalCenter
                                            name: "close"
                                            size: 10
                                            color: removeMouse.containsMouse ? Theme.textPrimary : Theme.textMuted
                                        }
                                    }

                                    MouseArea {
                                        id: removeMouse
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: {
                                            if (pluginParams.instanceId >= 0 && paramDelegate.param.portIndex !== undefined)
                                                pluginParams.controller.remove_midi_mapping_for_param(
                                                    pluginParams.instanceId, paramDelegate.param.portIndex)
                                        }
                                    }
                                    ZbTooltip {
                                        visible: removeMouse.containsMouse
                                        text: "Remove MIDI mapping"
                                    }
                                }

                                Text {
                                    visible: paramDelegate.isLearning
                                    text: "Listening…"
                                    font.family: Theme.fontMono
                                    font.pixelSize: Theme.sizeBadge
                                    color: Theme.sigMidi

                                    SequentialAnimation on opacity {
                                        running: paramDelegate.isLearning
                                        loops: Animation.Infinite
                                        NumberAnimation { to: 0.3; duration: 600 }
                                        NumberAnimation { to: 1.0; duration: 600 }
                                    }
                                }

                                ZbValueField {
                                    id: valueField
                                    // Marks this as a value editor for the Escape shortcut.
                                    readonly property bool paramEditor: true
                                    readonly property real current: paramDelegate.param.value !== undefined
                                                                    ? paramDelegate.param.value : 0
                                    inputMethodHints: Qt.ImhFormattedNumbersOnly

                                    Component.onCompleted: text = pluginParams.formatNumber(current)
                                    onCurrentChanged: if (!activeFocus) text = pluginParams.formatNumber(current)
                                    onAccepted: paramList.forceActiveFocus()
                                    onActiveFocusChanged: {
                                        if (activeFocus) selectAll()
                                        else commit()
                                    }

                                    function commit() {
                                        var num = parseFloat(text)
                                        if (!isNaN(num) && paramDelegate.param.portIndex !== undefined) {
                                            num = Math.max(paramDelegate.minValue, Math.min(paramDelegate.maxValue, num))
                                            if (Math.abs(num - current) > 1e-9)
                                                pluginParams.setParam(paramDelegate.param.portIndex, num)
                                        }
                                        text = pluginParams.formatNumber(current)
                                    }

                                    function cancelEdit() {
                                        text = pluginParams.formatNumber(current)
                                        paramList.forceActiveFocus()
                                    }
                                }

                                // MIDI learn pill (mono 10, 26 tall).
                                ZbButton {
                                    compact: true
                                    text: "MIDI"
                                    font.family: Theme.fontMono
                                    font.pixelSize: Theme.sizeBadge
                                    textColor: paramDelegate.isLearning ? Theme.sigMidi : Theme.textSecondary
                                    onClicked: {
                                        if (paramDelegate.isLearning) {
                                            pluginParams.controller.cancel_midi_learn()
                                        } else if (pluginParams.instanceId >= 0 && paramDelegate.param.portIndex !== undefined) {
                                            pluginParams.controller.start_midi_learn(
                                                pluginParams.instanceId,
                                                paramDelegate.param.portIndex,
                                                pluginParams.pluginName + " > " + (paramDelegate.param.name || ""),
                                                paramDelegate.param.isToggle ? "toggle" : "continuous"
                                            )
                                        }
                                    }
                                    ZbTooltip {
                                        visible: parent.hovered
                                        text: paramDelegate.isLearning ? "Cancel MIDI learn"
                                            : paramDelegate.midiMapping !== "" ? "Re-learn MIDI control"
                                            : "MIDI learn: move a control to map it"
                                    }
                                }

                                // Reset to default.
                                ZbButton {
                                    compact: true
                                    iconName: "reset"
                                    iconSize: 13
                                    onClicked: pluginParams.setParam(paramDelegate.param.portIndex,
                                                                     paramDelegate.param.defaultValue)
                                    ZbTooltip {
                                        visible: parent.hovered
                                        text: "Reset to default (" + pluginParams.formatNumber(paramDelegate.param.defaultValue) + ")"
                                    }
                                }
                            }

                            // Line 2: switch for toggles, otherwise slider + range.
                            Loader {
                                Layout.fillWidth: true
                                sourceComponent: paramDelegate.param.isToggle ? toggleComponent : sliderComponent

                                Component {
                                    id: toggleComponent
                                    RowLayout {
                                        spacing: 10
                                        ZbSwitch {
                                            id: toggleSwitch
                                            compact: true
                                            checked: paramDelegate.param.value !== undefined
                                                     ? paramDelegate.param.value > 0.5 : false
                                            onToggled: pluginParams.setParam(paramDelegate.param.portIndex,
                                                                             checked ? 1.0 : 0.0)
                                        }
                                        Text {
                                            text: toggleSwitch.checked ? "On" : "Off"
                                            font.family: Theme.fontSans
                                            font.pixelSize: Theme.sizeCaption
                                            color: Theme.textSecondary
                                        }
                                        Item { Layout.fillWidth: true }
                                    }
                                }

                                Component {
                                    id: sliderComponent
                                    ColumnLayout {
                                        spacing: 4
                                        ZbSlider {
                                            Layout.fillWidth: true
                                            // Groove spans the row, aligned with the range labels.
                                            leftPadding: 0
                                            rightPadding: 0
                                            from: paramDelegate.minValue
                                            to: paramDelegate.maxValue
                                            value: paramDelegate.param.value !== undefined ? paramDelegate.param.value : 0
                                            onMoved: pluginParams.setParam(paramDelegate.param.portIndex, value)
                                        }
                                        RowLayout {
                                            Layout.fillWidth: true
                                            Text {
                                                text: (paramDelegate.bipolar ? "L " : "")
                                                      + pluginParams.formatNumber(paramDelegate.minValue)
                                                font.family: Theme.fontMono
                                                font.pixelSize: Theme.sizeBadge
                                                color: Theme.textMuted
                                            }
                                            Item { Layout.fillWidth: true }
                                            Text {
                                                text: (paramDelegate.bipolar ? "R " : "")
                                                      + pluginParams.formatNumber(paramDelegate.maxValue)
                                                font.family: Theme.fontMono
                                                font.pixelSize: Theme.sizeBadge
                                                color: Theme.textMuted
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        ZbDivider {
                            visible: paramDelegate.index < paramModel.count - 1
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                        }
                    }
                }

                ZbScrollBar {
                    id: paramBar
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.right: parent.right
                    anchors.topMargin: Theme.scrollGap
                    anchors.bottomMargin: Theme.scrollGap
                    anchors.rightMargin: Theme.scrollGap
                }

                Text {
                    anchors.centerIn: parent
                    visible: paramModel.count === 0 || pluginParams.matchCount === 0
                    text: paramModel.count === 0 ? "No control parameters" : "No matching parameters"
                    font.family: Theme.fontSans
                    font.pixelSize: Theme.sizeCaption
                    color: Theme.textMuted
                }
            }
        }
    }
}
