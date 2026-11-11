import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Preferences (spec §6.4): General toggles, Appearance (theme), Timing and
// Advanced slider rows. Every change is saved immediately.
ZbWindow {
    id: prefsWindow
    title: "Preferences"
    width: 720
    height: 760
    minimumWidth: 480
    minimumHeight: 400
    visible: false

    required property var controller

    signal pollIntervalChanged(int intervalMs)
    signal snapGapChanged(int px)
    signal snapToGridChanged(bool enabled)
    // The canvas drag mode has no row here (it is switched on the canvas);
    // this only reports its value after a reset to defaults.
    signal dragModeChanged(string mode)
    signal gridSizeChanged(int px)

    property var prefs: ({})
    property var sessionInfo: ({ current: "", path: "", sessions: [] })
    property string sessionError: ""
    property string sessionToDelete: ""

    ZbDialog {
        id: deleteSessionConfirm
        title: "Delete session?"
        primaryText: "Delete"
        secondaryText: "Cancel"
        width: Math.min(prefsWindow.width * 0.8, 420)

        Text {
            width: parent ? parent.width : 380
            wrapMode: Text.WordWrap
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            color: Theme.textPrimary
            text: "The session \u201c" + prefsWindow.sessionToDelete + "\u201d and everything saved in it (plugins, links, rules, layout) is deleted. This can't be undone. Presets and settings are kept."
        }
        onAccepted: {
            prefsWindow.sessionError = controller.delete_session(prefsWindow.sessionToDelete);
            prefsWindow.sessionToDelete = "";
            prefsWindow.loadSessions();
        }
    }

    Connections {
        target: controller
        function onSession_changed() {
            prefsWindow.loadSessions();
        }
    }


    readonly property var themeModes: ["system", "dark", "light"]

    function loadPrefs() {
        try {
            prefs = JSON.parse(controller.get_preferences_json());
        } catch (e) {
            prefs = {};
        }
    }

    function loadSessions() {
        try {
            sessionInfo = JSON.parse(controller.get_sessions_json());
        } catch (e) {
            sessionInfo = { current: "", path: "", sessions: [] };
        }
    }

    function open() {
        loadPrefs();
        loadSessions();
        sessionError = "";
        prefsWindow.visible = true;
        prefsWindow.raise();
        prefsWindow.requestActivate();
    }

    // Current value of a preference, or `fallback` when it isn't set.
    function pref(key, fallback) {
        const v = prefs[key];
        return v !== undefined ? v : fallback;
    }

    function setPref(key, value) {
        controller.set_preference(key, String(value));
        loadPrefs();
        if (key === "poll_interval_ms") {
            pollIntervalChanged(value);
        }
        if (key === "snap_gap_px") {
            snapGapChanged(value);
        }
        if (key === "snap_to_grid") {
            snapToGridChanged(value);
        }
        if (key === "grid_size_px") {
            gridSizeChanged(value);
        }
    }

    function themeIndex() {
        return Math.max(0, themeModes.indexOf(Theme.mode));
    }

    function setTheme(mode) {
        Theme.mode = mode;
        setPref("theme", mode);
        themeSegmented.currentIndex = Qt.binding(() => prefsWindow.themeIndex());
    }

    function resetToDefaults() {
        controller.reset_preferences();
        loadPrefs();
        pollIntervalChanged(pref("poll_interval_ms", 100));
        snapGapChanged(pref("snap_gap_px", 10));
        snapToGridChanged(pref("snap_to_grid", true));
        gridSizeChanged(pref("grid_size_px", 24));
        dragModeChanged(pref("drag_mode", "grab"));
        Theme.mode = "system";
        themeSegmented.currentIndex = Qt.binding(() => prefsWindow.themeIndex());
    }

    // "10 000" style grouping for five-digit-plus range labels.
    function formatNumber(n) {
        const s = String(n);
        return s.length > 4 ? s.replace(/\B(?=(\d{3})+(?!\d))/g, " ") : s;
    }

    Shortcut {
        sequence: "Escape"
        enabled: prefsWindow.visible
        onActivated: prefsWindow.visible = false
    }

    // ─── Building blocks ────────────────────────────────────────────────

    // Row title 13/500 + description 12 muted.
    component RowText: ColumnLayout {
        id: rowText
        property string title
        property string description
        spacing: Theme.grid
        Text {
            Layout.fillWidth: true
            text: rowText.title
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            font.weight: Font.Medium
            color: Theme.textPrimary
            wrapMode: Text.WordWrap
        }
        Text {
            Layout.fillWidth: true
            visible: text.length > 0
            text: rowText.description
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeCaption
            color: Theme.textMuted
            wrapMode: Text.WordWrap
        }
    }

    // Uppercase label (+ optional inline hint) above a card of rows.
    // Separate rows with ZbDivider { Layout.fillWidth: true }.
    component SettingsSection: ColumnLayout {
        id: section
        property string label
        property string hint: ""
        default property alias rows: rowsColumn.data
        Layout.fillWidth: true
        spacing: Theme.grid * 2

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.grid * 2
            ZbSectionLabel {
                Layout.alignment: Qt.AlignBaseline
                text: section.label
            }
            Text {
                Layout.alignment: Qt.AlignBaseline
                Layout.fillWidth: true
                visible: section.hint.length > 0
                text: section.hint
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeCaption
                color: Theme.textMuted
                elide: Text.ElideRight
            }
        }

        ZbCard {
            Layout.fillWidth: true
            implicitHeight: rowsColumn.implicitHeight + 2
            ColumnLayout {
                id: rowsColumn
                x: 1
                y: 1
                width: parent.width - 2
                spacing: 0
            }
        }
    }

    // Toggle row: text left, standard switch right.
    component ToggleRow: RowLayout {
        id: toggleRow
        property string title
        property string description
        property string key
        property bool fallback: false
        Layout.fillWidth: true
        Layout.leftMargin: Theme.grid * 4
        Layout.rightMargin: Theme.grid * 4
        Layout.topMargin: Theme.grid * 3
        Layout.bottomMargin: Theme.grid * 3
        spacing: Theme.grid * 4

        RowText {
            Layout.fillWidth: true
            title: toggleRow.title
            description: toggleRow.description
        }
        ZbSwitch {
            id: toggle
            Layout.alignment: Qt.AlignVCenter
            checked: prefsWindow.pref(toggleRow.key, toggleRow.fallback)
            onToggled: {
                prefsWindow.setPref(toggleRow.key, checked);
                checked = Qt.binding(() => prefsWindow.pref(toggleRow.key, toggleRow.fallback));
            }
        }
    }

    // Slider row: text left, value read-out right, slider and range labels
    // beneath. Saved on release (or on each keyboard step).
    // A number typed into a field, committed on Enter or when focus leaves.
    component NumberRow: RowLayout {
        id: numberRow
        property string title
        property string description
        property string key
        property int fallback: 0
        Layout.fillWidth: true
        Layout.leftMargin: Theme.grid * 4
        Layout.rightMargin: Theme.grid * 4
        Layout.topMargin: Theme.grid * 3
        Layout.bottomMargin: Theme.grid * 3
        spacing: Theme.grid * 4

        RowText {
            Layout.fillWidth: true
            title: numberRow.title
            description: numberRow.description
        }
        ZbTextField {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: 90
            horizontalAlignment: TextInput.AlignRight
            inputMethodHints: Qt.ImhDigitsOnly
            validator: IntValidator { bottom: 1024; top: 65535 }
            text: String(prefsWindow.pref(numberRow.key, numberRow.fallback))
            onEditingFinished: {
                if (acceptableInput)
                    prefsWindow.setPref(numberRow.key, parseInt(text));
                text = Qt.binding(() => String(prefsWindow.pref(numberRow.key, numberRow.fallback)));
            }
        }
    }

    component SliderRow: ColumnLayout {
        id: sliderRow
        property string title
        property string description
        property string key
        property real from: 0
        property real to: 100
        property real stepSize: 1
        property real fallback: 0
        property string unit: "ms"
        Layout.fillWidth: true
        Layout.leftMargin: Theme.grid * 4
        Layout.rightMargin: Theme.grid * 4
        Layout.topMargin: Theme.cardPadding
        Layout.bottomMargin: Theme.grid * 3
        spacing: Theme.grid * 2

        function commit() {
            prefsWindow.setPref(key, Math.round(slider.value));
            slider.value = Qt.binding(() => prefsWindow.pref(sliderRow.key, sliderRow.fallback));
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.grid * 4
            RowText {
                Layout.fillWidth: true
                title: sliderRow.title
                description: sliderRow.description
            }
            Rectangle {
                Layout.alignment: Qt.AlignVCenter
                implicitWidth: readout.implicitWidth + Theme.grid * 5
                implicitHeight: Theme.compactButtonHeight
                radius: Theme.radiusSmall
                color: Theme.bgInset
                border.width: 1
                border.color: Theme.borderStrong
                Row {
                    id: readout
                    anchors.centerIn: parent
                    spacing: Theme.grid
                    Text {
                        text: Math.round(slider.value)
                        font.family: Theme.fontMono
                        font.pixelSize: Theme.sizeCaption
                        color: Theme.textPrimary
                    }
                    Text {
                        text: sliderRow.unit
                        font.family: Theme.fontMono
                        font.pixelSize: Theme.sizeCaption
                        color: Theme.textMuted
                    }
                }
            }
        }

        ZbSlider {
            id: slider
            Layout.fillWidth: true
            leftPadding: 0
            rightPadding: 0
            from: sliderRow.from
            to: sliderRow.to
            stepSize: sliderRow.stepSize
            snapMode: Slider.SnapAlways
            value: prefsWindow.pref(sliderRow.key, sliderRow.fallback)
            onPressedChanged: {
                if (!pressed)
                    sliderRow.commit();
            }
            onMoved: {
                if (!pressed)
                    sliderRow.commit();
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.grid * 2
            Text {
                text: prefsWindow.formatNumber(sliderRow.from) + " " + sliderRow.unit
                font.family: Theme.fontMono
                font.pixelSize: Theme.sizeBadge
                color: Theme.textMuted
            }
            Item { Layout.fillWidth: true }
            Text {
                text: prefsWindow.formatNumber(sliderRow.to) + " " + sliderRow.unit
                font.family: Theme.fontMono
                font.pixelSize: Theme.sizeBadge
                color: Theme.textMuted
            }
        }
    }

    // ─── Layout ─────────────────────────────────────────────────────────

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        ZbDialogHeader {
            Layout.leftMargin: Theme.dialogPadding
            Layout.rightMargin: Theme.dialogPadding
            Layout.topMargin: Theme.dialogPadding - Theme.grid
            Layout.bottomMargin: Theme.grid * 4
            title: "Preferences"
        }

        // Scroll area: the bar sits in the right padding gutter, beside the
        // content (bible §5.3).
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Flickable {
                id: prefsFlickable
                ZbWheelScroll { flickable: prefsFlickable }
                anchors.left: parent.left
                anchors.leftMargin: Theme.dialogPadding
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.right: prefsScrollBar.visible ? prefsScrollBar.left : parent.right
                anchors.rightMargin: prefsScrollBar.visible ? Theme.scrollGap : Theme.dialogPadding
                contentWidth: width
                contentHeight: settingsColumn.implicitHeight + Theme.grid * 4
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: prefsScrollBar

                ColumnLayout {
                    id: settingsColumn
                    width: prefsFlickable.width
                    spacing: Theme.dialogPadding

                    SettingsSection {
                        label: "Session"
                        hint: "Switching saves this session and loads the other."

                        RowText {
                            Layout.fillWidth: true
                            Layout.leftMargin: Theme.grid * 4
                            Layout.rightMargin: Theme.grid * 4
                            Layout.topMargin: Theme.grid * 3
                            Layout.bottomMargin: Theme.grid * 3
                            title: prefsWindow.sessionInfo.current !== "" ? "Current: " + prefsWindow.sessionInfo.current : "Current: opened from a folder"
                            description: prefsWindow.sessionInfo.path
                        }
                        Repeater {
                            model: prefsWindow.sessionInfo.sessions
                            delegate: ColumnLayout {
                                required property string modelData
                                Layout.fillWidth: true
                                spacing: 0
                                ZbDivider { Layout.fillWidth: true }
                                RowLayout {
                                    Layout.fillWidth: true
                                    Layout.leftMargin: Theme.grid * 4
                                    Layout.rightMargin: Theme.grid * 4
                                    Layout.topMargin: Theme.grid * 2
                                    Layout.bottomMargin: Theme.grid * 2
                                    spacing: Theme.grid * 4
                                    Text {
                                        Layout.fillWidth: true
                                        text: modelData
                                        elide: Text.ElideRight
                                        font.family: Theme.fontSans
                                        font.pixelSize: Theme.sizeBody
                                        color: Theme.textPrimary
                                    }
                                    // The open session is marked; the others get icon buttons.
                                    ZbIcon {
                                        visible: modelData === prefsWindow.sessionInfo.current
                                        name: "check"
                                        size: 14
                                        color: Theme.accent
                                    }
                                    ZbButton {
                                        id: switchButton
                                        visible: modelData !== prefsWindow.sessionInfo.current
                                        iconName: "switch"
                                        iconSize: 14
                                        textColor: Theme.textIcon
                                        onClicked: {
                                            prefsWindow.sessionError = controller.switch_session(modelData);
                                            prefsWindow.loadSessions();
                                        }
                                        ZbTooltip {
                                            visible: switchButton.hovered
                                            text: "Switch to session"
                                        }
                                    }
                                    ZbButton {
                                        id: deleteButton
                                        visible: modelData !== prefsWindow.sessionInfo.current
                                        iconName: "trash"
                                        iconSize: 14
                                        destructive: true
                                        onClicked: {
                                            prefsWindow.sessionToDelete = modelData;
                                            deleteSessionConfirm.open();
                                        }
                                        ZbTooltip {
                                            visible: deleteButton.hovered
                                            text: "Delete session"
                                        }
                                    }
                                }
                            }
                        }
                        ZbDivider { Layout.fillWidth: true }
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.leftMargin: Theme.grid * 4
                            Layout.rightMargin: Theme.grid * 4
                            Layout.topMargin: Theme.grid * 3
                            Layout.bottomMargin: Theme.grid * 3
                            spacing: Theme.grid * 3
                            ZbTextField {
                                id: newSessionField
                                Layout.fillWidth: true
                                placeholderText: "New session name"
                                onAccepted: createSessionButton.clicked()
                            }
                            ZbButton {
                                id: createSessionButton
                                text: "Create"
                                enabled: newSessionField.text.trim().length > 0
                                onClicked: {
                                    const err = controller.create_session(newSessionField.text.trim());
                                    prefsWindow.sessionError = err;
                                    if (err === "") {
                                        newSessionField.text = "";
                                        prefsWindow.loadSessions();
                                    }
                                }
                            }
                        }
                        Text {
                            visible: prefsWindow.sessionError !== ""
                            Layout.fillWidth: true
                            Layout.leftMargin: Theme.grid * 4
                            Layout.rightMargin: Theme.grid * 4
                            Layout.bottomMargin: Theme.grid * 3
                            wrapMode: Text.Wrap
                            text: prefsWindow.sessionError
                            font.family: Theme.fontSans
                            font.pixelSize: Theme.sizeCaption
                            color: Theme.textDestructive
                        }
                    }

                    SettingsSection {
                        label: "General"

                        ToggleRow {
                            title: "Start minimized to tray"
                            description: "Launch with the window hidden; show it from the tray icon."
                            key: "start_minimized"
                            fallback: false
                        }
                        ZbDivider { Layout.fillWidth: true }
                        ToggleRow {
                            title: "Close to tray"
                            description: "The window close button hides ZestBay instead of quitting."
                            key: "close_to_tray"
                            fallback: false
                        }
                        ZbDivider { Layout.fillWidth: true }
                        ToggleRow {
                            title: "Auto-learn patchbay rules"
                            description: "Create or update a rule whenever you connect ports by hand."
                            key: "auto_learn_rules"
                            fallback: true
                        }
                    }

                    SettingsSection {
                        label: "Remote control (OSC)"
                        hint: "Control plugins over UDP. Links can't be controlled."

                        ToggleRow {
                            title: "Enable OSC"
                            description: "Listen for OSC messages. Changes apply immediately."
                            key: "osc_enabled"
                            fallback: false
                        }
                        ZbDivider { Layout.fillWidth: true }
                        NumberRow {
                            title: "UDP port"
                            description: "1024\u201365535."
                            key: "osc_port"
                            fallback: 9000
                        }
                        ZbDivider { Layout.fillWidth: true }
                        ToggleRow {
                            title: "Accept from other devices"
                            description: "Listen on the network instead of only on this computer. There is no password, so anyone who can reach the port can control your plugins."
                            key: "osc_listen_all"
                            fallback: false
                        }
                        ZbDivider { Layout.fillWidth: true }
                        Text {
                            Layout.fillWidth: true
                            Layout.leftMargin: Theme.grid * 4
                            Layout.rightMargin: Theme.grid * 4
                            Layout.topMargin: Theme.grid * 3
                            Layout.bottomMargin: Theme.grid * 3
                            wrapMode: Text.Wrap
                            font.family: Theme.fontMono
                            font.pixelSize: Theme.sizeCaption
                            color: Theme.textMuted
                            text: "/zestbay/plugins\n"
                                + "/zestbay/plugin/<name>/enable <1|0>\n"
                                + "/zestbay/plugin/<name>/bypass <1|0>\n"
                                + "/zestbay/plugin/<name>/toggle\n"
                                + "/zestbay/plugin/<name>/param/<param> <value>\n"
                                + "/zestbay/plugin/<name>/param/<param>/normalized <0..1>\n"
                                + "/zestbay/plugin/<name>/preset <preset name>\n"
                                + "/zestbay/plugin/<name>/params\n\n"
                                + "<name>: the plugin's name in lowercase, _ for spaces.\n"
                                + "<param>: parameter symbol, name or port index."
                        }
                    }

                    SettingsSection {
                        label: "Appearance"

                        RowLayout {
                            Layout.fillWidth: true
                            Layout.leftMargin: Theme.grid * 4
                            Layout.rightMargin: Theme.grid * 4
                            Layout.topMargin: Theme.grid * 3
                            Layout.bottomMargin: Theme.grid * 3
                            spacing: Theme.grid * 4

                            RowText {
                                Layout.fillWidth: true
                                title: "Theme"
                                description: "System follows the desktop's colour scheme."
                            }
                            ZbSegmented {
                                id: themeSegmented
                                Layout.alignment: Qt.AlignVCenter
                                model: ["System", "Dark", "Light"]
                                currentIndex: prefsWindow.themeIndex()
                                onActivated: index => prefsWindow.setTheme(prefsWindow.themeModes[index])
                            }
                        }
                    }

                    SettingsSection {
                        label: "Graph"

                        ToggleRow {
                            title: "Snap to grid"
                            description: "Keep every node on the grid when dragging, placing and running Auto layout."
                            key: "snap_to_grid"
                            fallback: true
                        }
                        ZbDivider { Layout.fillWidth: true }
                        SliderRow {
                            title: "Grid size"
                            description: "Spacing of the grid and its dots (at 100 % zoom)."
                            key: "grid_size_px"
                            from: 8
                            to: 96
                            stepSize: 4
                            fallback: 24
                            unit: "px"
                        }
                        ZbDivider { Layout.fillWidth: true }
                        SliderRow {
                            title: "Snap gap"
                            description: "Space left between nodes when Shift-dragging snaps one above, below or beside another (at 100 % zoom). Not used while snap to grid is on."
                            key: "snap_gap_px"
                            from: 0
                            to: 48
                            stepSize: 1
                            fallback: 10
                            unit: "px"
                        }
                    }

                    SettingsSection {
                        label: "Timing"
                        hint: "Trade responsiveness against reliability."

                        SliderRow {
                            title: "Rule settle time"
                            description: "Wait this long after the graph stops changing before applying rules. Higher is safer on slow hardware."
                            key: "rule_settle_ms"
                            from: 0
                            to: 2000
                            stepSize: 50
                            fallback: 500
                        }
                        ZbDivider { Layout.fillWidth: true }
                        SliderRow {
                            title: "Poll interval"
                            description: "How often the UI checks for PipeWire events. Lower is smoother but uses more CPU."
                            key: "poll_interval_ms"
                            from: 16
                            to: 500
                            stepSize: 1
                            fallback: 100
                        }
                        ZbDivider { Layout.fillWidth: true }
                        SliderRow {
                            title: "Parameter save debounce"
                            description: "Wait this long after the last parameter change before writing to disk."
                            key: "params_persist_ms"
                            from: 100
                            to: 10000
                            stepSize: 100
                            fallback: 1000
                        }
                    }

                    SettingsSection {
                        label: "Advanced"
                        hint: "Rarely needed; some take effect after a restart."

                        ToggleRow {
                            title: "Hide unnamed nodes"
                            description: "Leave out nodes that have no name of their own, such as the level-meter streams pavucontrol creates."
                            key: "hide_unnamed_nodes"
                            fallback: true
                        }
                        ZbDivider { Layout.fillWidth: true }
                        ToggleRow {
                            title: "Let idle devices sleep"
                            description: "Mark effect plugins as passive, so a mic or output linked only through plugins can suspend when nothing else uses it. Off keeps chains like mic \u2192 effect \u2192 headset running. Applies to plugins added or reloaded afterwards."
                            key: "passive_plugins"
                            fallback: false
                        }
                        ZbDivider { Layout.fillWidth: true }
                        SliderRow {
                            title: "Link save debounce"
                            description: "Wait this long after the last link change before saving plugin connections to disk."
                            key: "links_persist_ms"
                            from: 100
                            to: 10000
                            stepSize: 100
                            fallback: 2000
                        }
                        ZbDivider { Layout.fillWidth: true }
                        SliderRow {
                            title: "PipeWire tick interval"
                            description: "How often the PipeWire thread checks for pending operations. Lower reduces link latency. Requires restart."
                            key: "pw_tick_interval_ms"
                            from: 1
                            to: 200
                            stepSize: 1
                            fallback: 10
                        }
                        ZbDivider { Layout.fillWidth: true }
                        SliderRow {
                            title: "Plugin operation cooldown"
                            description: "Minimum gap between heavy operations (plugin add/remove). Connecting and disconnecting are always instant. Requires restart."
                            key: "pw_operation_cooldown_ms"
                            from: 10
                            to: 1000
                            stepSize: 10
                            fallback: 50
                        }
                    }
                }
            }

            ZbScrollBar {
                id: prefsScrollBar
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.right: parent.right
                // Keeps the content's right edge at dialogPadding either way.
                anchors.rightMargin: Theme.dialogPadding - Theme.scrollBarWidth - Theme.scrollGap
            }
        }

        ZbDivider { Layout.fillWidth: true }

        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.dialogPadding
            Layout.rightMargin: Theme.dialogPadding
            Layout.topMargin: Theme.grid * 4
            Layout.bottomMargin: Theme.grid * 5
            spacing: Theme.grid * 2

            ZbButton {
                text: "Reset to defaults"
                onClicked: prefsWindow.resetToDefaults()
            }
            Item { Layout.fillWidth: true }
            Text {
                text: "Changes apply immediately."
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeCaption
                color: Theme.textMuted
            }
        }
    }
}
