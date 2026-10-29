import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Manage plugins: the active plugin instances (list left, details right).
// Follows the dialog shell (bible §6.1) with card lists and parameter rows.
ZbWindow {
    id: pluginManager
    title: "Manage plugins"
    width: 820
    height: 640
    minimumWidth: 600
    minimumHeight: 420
    visible: false

    required property var controller

    property var plugins: []
    // Selection is tracked by stable id: the list is reloaded (and may be
    // reordered) on every refresh.
    property string selectedStableId: ""
    property int selectedIndex: indexOfPlugin(plugins, selectedStableId)
    property var selectedPlugin: selectedIndex >= 0 ? plugins[selectedIndex] : null
    readonly property var selectedParams: selectedPlugin && selectedPlugin.parameters ? selectedPlugin.parameters : []

    // Two-click confirmation for "Remove plugin" (cleared after a few seconds).
    property bool confirmRemove: false
    onSelectedStableIdChanged: confirmRemove = false

    function indexOfPlugin(list, stableId) {
        if (!stableId) return -1
        for (var i = 0; i < list.length; i++) {
            if (list[i].stableId === stableId) return i
        }
        return -1
    }

    function loadPlugins() {
        try {
            plugins = JSON.parse(controller.get_active_plugins_json())
        } catch(e) {
            plugins = []
        }
        if (selectedStableId && indexOfPlugin(plugins, selectedStableId) < 0) {
            selectedStableId = ""
        }
    }

    function open() {
        selectedStableId = ""
        loadPlugins()
        visible = true
        raise()
        requestActivate()
    }

    function formatValue(v) {
        return v !== undefined && v !== null ? Number(v).toFixed(3) : ""
    }

    Timer {
        id: refreshTimer
        interval: 500
        running: pluginManager.visible
        repeat: true
        onTriggered: pluginManager.loadPlugins()
    }

    Timer {
        id: confirmTimer
        interval: 3000
        running: pluginManager.confirmRemove
        onTriggered: pluginManager.confirmRemove = false
    }

    Shortcut {
        sequence: "Escape"
        enabled: pluginManager.visible
        onActivated: pluginManager.visible = false
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.dialogPadding
        spacing: 16

        ZbDialogHeader {
            title: "Manage plugins"
            subtitle: pluginManager.plugins.length + " active plugin" + (pluginManager.plugins.length !== 1 ? "s" : "")
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 16

            // ── Active plugin list ──────────────────────────────────────
            ZbCard {
                Layout.preferredWidth: 280
                Layout.fillHeight: true

                ListView {
                    id: pluginList
                    ZbWheelScroll { flickable: pluginList }
                    anchors {
                        left: parent.left
                        top: parent.top
                        bottom: parent.bottom
                        right: pluginBar.visible ? pluginBar.left : parent.right
                        leftMargin: 1
                        topMargin: Theme.scrollGap
                        bottomMargin: Theme.scrollGap
                        rightMargin: pluginBar.visible ? Theme.scrollGap : 1
                    }
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    // Int model: the list is reloaded every 500 ms; an array
                    // model would rebuild every delegate (and reset scroll).
                    model: pluginManager.plugins.length
                    currentIndex: pluginManager.selectedIndex
                    highlightFollowsCurrentItem: false
                    ScrollBar.vertical: pluginBar

                    delegate: Rectangle {
                        id: pluginRow
                        required property int index
                        readonly property var plugin: pluginManager.plugins[index] || {}
                        readonly property bool selected: index === pluginManager.selectedIndex
                        readonly property int paramCount: plugin.parameters ? plugin.parameters.length : 0

                        width: ListView.view.width
                        height: Theme.rowRule
                        color: selected || pluginMouse.containsMouse ? Theme.bgHover : "transparent"

                        // Selection marker (accent: selection).
                        Rectangle {
                            visible: pluginRow.selected
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            width: 2
                            height: parent.height - 16
                            radius: 1
                            color: Theme.accent
                        }

                        ColumnLayout {
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.leftMargin: Theme.cardPadding
                            anchors.rightMargin: Theme.cardPadding
                            spacing: 4

                            Text {
                                text: pluginRow.plugin.displayName || ""
                                font.family: Theme.fontSans
                                font.pixelSize: Theme.sizeBody
                                font.weight: Font.Medium
                                color: Theme.textPrimary
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }

                            Text {
                                text: (pluginRow.plugin.bypassed ? "Bypassed" : "Active")
                                      + " · " + pluginRow.paramCount + (pluginRow.paramCount === 1 ? " param" : " params")
                                font.family: Theme.fontMono
                                font.pixelSize: Theme.sizeMono
                                color: Theme.textMuted
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }
                        }

                        MouseArea {
                            id: pluginMouse
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: pluginManager.selectedStableId = pluginRow.plugin.stableId || ""
                        }

                        ZbDivider {
                            visible: pluginRow.index < pluginList.count - 1
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                        }
                    }

                    Text {
                        visible: pluginList.count === 0
                        anchors.centerIn: parent
                        text: "No active plugins"
                        font.family: Theme.fontSans
                        font.pixelSize: Theme.sizeBody
                        color: Theme.textMuted
                    }
                }

                ZbScrollBar {
                    id: pluginBar
                    anchors {
                        top: parent.top
                        bottom: parent.bottom
                        right: parent.right
                        topMargin: Theme.scrollGap
                        bottomMargin: Theme.scrollGap
                        rightMargin: Theme.scrollGap
                    }
                }
            }

            // ── Details of the selected plugin ──────────────────────────
            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true

                Text {
                    visible: !pluginManager.selectedPlugin
                    anchors.centerIn: parent
                    text: "Select a plugin from the list"
                    font.family: Theme.fontSans
                    font.pixelSize: Theme.sizeBody
                    color: Theme.textMuted
                }

                ColumnLayout {
                    visible: !!pluginManager.selectedPlugin
                    anchors.fill: parent
                    spacing: 16

                    // Name, URI and actions
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 4

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 8
                                Text {
                                    text: pluginManager.selectedPlugin ? pluginManager.selectedPlugin.displayName : ""
                                    font.family: Theme.fontSans
                                    font.pixelSize: Theme.sizeListPrimary
                                    font.weight: Font.DemiBold
                                    color: Theme.textPrimary
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                    Layout.preferredWidth: Math.ceil(implicitWidth)
                                    Layout.maximumWidth: Math.ceil(implicitWidth)
                                }
                                ZbBadge {
                                    id: detailFormatBadge
                                    visible: !!pluginManager.selectedPlugin && !!pluginManager.selectedPlugin.format
                                    text: pluginManager.selectedPlugin && pluginManager.selectedPlugin.format
                                          ? pluginManager.selectedPlugin.format : ""
                                    tone: detailFormatBadge.formatTone(text)
                                }
                                ZbBadge {
                                    visible: !!pluginManager.selectedPlugin && !!pluginManager.selectedPlugin.bypassed
                                    text: "bypassed"
                                }
                                Item { Layout.fillWidth: true }
                            }

                            Text {
                                text: pluginManager.selectedPlugin ? pluginManager.selectedPlugin.pluginUri : ""
                                font.family: Theme.fontMono
                                font.pixelSize: Theme.sizeMono
                                color: Theme.textMuted
                                elide: Text.ElideMiddle
                                Layout.fillWidth: true
                            }
                        }

                        ZbButton {
                            text: "Reset params"
                            iconName: "reset"
                            enabled: pluginManager.selectedParams.length > 0
                            onClicked: {
                                if (pluginManager.selectedPlugin) {
                                    pluginManager.controller.reset_plugin_params_by_stable_id(pluginManager.selectedPlugin.stableId)
                                }
                            }
                        }

                        ZbButton {
                            text: pluginManager.confirmRemove ? "Confirm remove" : "Remove plugin"
                            iconName: "trash"
                            destructive: true
                            onClicked: {
                                if (!pluginManager.selectedPlugin) return
                                if (!pluginManager.confirmRemove) {
                                    pluginManager.confirmRemove = true
                                    return
                                }
                                pluginManager.confirmRemove = false
                                pluginManager.controller.remove_plugin_by_stable_id(pluginManager.selectedPlugin.stableId)
                                pluginManager.selectedStableId = ""
                                pluginManager.loadPlugins()
                            }
                        }
                    }

                    // Parameters section
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 8
                        ZbSectionLabel { text: "Parameters" }
                        Item { Layout.fillWidth: true }
                        Text {
                            text: pluginManager.selectedParams.length + " parameter"
                                  + (pluginManager.selectedParams.length !== 1 ? "s" : "")
                            font.family: Theme.fontMono
                            font.pixelSize: Theme.sizeMono
                            color: Theme.textMuted
                        }
                    }

                    ZbCard {
                        Layout.fillWidth: true
                        Layout.fillHeight: true

                        Text {
                            visible: pluginManager.selectedParams.length === 0
                            anchors.centerIn: parent
                            text: "No control parameters"
                            font.family: Theme.fontSans
                            font.pixelSize: Theme.sizeBody
                            color: Theme.textMuted
                        }

                        ListView {
                            id: paramList
                            ZbWheelScroll { flickable: paramList }
                            anchors {
                                left: parent.left
                                top: parent.top
                                bottom: parent.bottom
                                right: paramBar.visible ? paramBar.left : parent.right
                                leftMargin: 1
                                topMargin: Theme.scrollGap
                                bottomMargin: Theme.scrollGap
                                rightMargin: paramBar.visible ? Theme.scrollGap : 1
                            }
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            model: pluginManager.selectedParams.length
                            ScrollBar.vertical: paramBar

                            delegate: Item {
                                id: paramRow
                                required property int index
                                readonly property var param: pluginManager.selectedParams[index] || {}
                                readonly property real pMin: param.min !== undefined ? param.min : 0
                                readonly property real pMax: param.max !== undefined ? param.max : 1

                                width: ListView.view.width
                                height: paramCol.implicitHeight + 24

                                ColumnLayout {
                                    id: paramCol
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    anchors.leftMargin: Theme.cardPadding
                                    anchors.rightMargin: Theme.cardPadding
                                    spacing: 8

                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: 8

                                        Text {
                                            text: paramRow.param.name || paramRow.param.symbol || ""
                                            font.family: Theme.fontSans
                                            font.pixelSize: Theme.sizeBody
                                            font.weight: Font.Medium
                                            color: Theme.textPrimary
                                            elide: Text.ElideRight
                                            Layout.fillWidth: true
                                        }

                                        Text {
                                            text: pluginManager.formatValue(paramRow.param.value)
                                            font.family: Theme.fontMono
                                            font.pixelSize: Theme.sizeMono
                                            color: Theme.textPrimary
                                            horizontalAlignment: Text.AlignRight
                                            Layout.preferredWidth: 64
                                        }

                                        ZbButton {
                                            id: resetButton
                                            compact: true
                                            iconName: "reset"
                                            onClicked: {
                                                if (pluginManager.selectedPlugin && paramRow.param.portIndex !== undefined) {
                                                    pluginManager.controller.set_plugin_param_by_stable_id(
                                                        pluginManager.selectedPlugin.stableId, paramRow.param.portIndex, paramRow.param.default)
                                                }
                                            }
                                            ZbTooltip {
                                                visible: resetButton.hovered
                                                text: "Reset to default (" + pluginManager.formatValue(paramRow.param.default) + ")"
                                            }
                                        }
                                    }

                                    ZbSlider {
                                        flush: true
                                        Layout.fillWidth: true
                                        from: paramRow.pMin
                                        to: paramRow.pMax
                                        value: paramRow.param.value !== undefined ? paramRow.param.value : 0
                                        onMoved: {
                                            if (pluginManager.selectedPlugin && paramRow.param.portIndex !== undefined) {
                                                pluginManager.controller.set_plugin_param_by_stable_id(
                                                    pluginManager.selectedPlugin.stableId, paramRow.param.portIndex, value)
                                            }
                                        }
                                    }

                                    RowLayout {
                                        Layout.fillWidth: true
                                        Text {
                                            text: pluginManager.formatValue(paramRow.pMin)
                                            font.family: Theme.fontMono
                                            font.pixelSize: Theme.sizeBadge
                                            color: Theme.textMuted
                                        }
                                        Item { Layout.fillWidth: true }
                                        Text {
                                            text: pluginManager.formatValue(paramRow.pMax)
                                            font.family: Theme.fontMono
                                            font.pixelSize: Theme.sizeBadge
                                            color: Theme.textMuted
                                        }
                                    }
                                }

                                ZbDivider {
                                    visible: paramRow.index < paramList.count - 1
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.bottom: parent.bottom
                                }
                            }
                        }

                        ZbScrollBar {
                            id: paramBar
                            anchors {
                                top: parent.top
                                bottom: parent.bottom
                                right: parent.right
                                topMargin: Theme.scrollGap
                                bottomMargin: Theme.scrollGap
                                rightMargin: Theme.scrollGap
                            }
                        }
                    }
                }
            }
        }
    }
}
