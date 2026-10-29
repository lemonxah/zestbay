import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQml.Models

// Add plugin dialog (design spec §6.3, 880x700).
ZbWindow {
    id: pluginBrowser
    title: "Add plugin"
    width: 880
    height: 700
    minimumWidth: 560
    minimumHeight: 400
    visible: false

    required property var controller

    // Emitted with the new plugin's display name when one is added.
    signal pluginAdded(string displayName)
    // Emitted when the browser closes without adding a plugin.
    signal cancelled()

    property bool addedPlugin: false

    onVisibleChanged: {
        if (!visible && !addedPlugin) cancelled()
    }

    readonly property var formats: ["All", "LV2", "VST3", "CLAP"]

    property var allPlugins: []
    property var filteredPlugins: []
    property var categories: []
    property string selectedCategory: "All"
    property string selectedFormat: "All"
    property bool showCompatibleOnly: true

    function loadPlugins() {
        try {
            allPlugins = JSON.parse(controller.get_available_plugins_json())
        } catch(e) {
            allPlugins = []
        }

        var catSet = {}
        for (var i = 0; i < allPlugins.length; i++) {
            var cat = allPlugins[i].category || "Other"
            catSet[cat] = true
        }
        var cats = ["All"]
        var sorted = Object.keys(catSet).sort()
        for (var ci = 0; ci < sorted.length; ci++) {
            cats.push(sorted[ci])
        }
        categories = cats
        selectedCategory = "All"

        filterPlugins()
    }

    function filterPlugins() {
        var query = searchField.text.toLowerCase()
        var result = []
        for (var i = 0; i < allPlugins.length; i++) {
            var p = allPlugins[i]

            if (showCompatibleOnly && !p.compatible)
                continue

            if (selectedFormat !== "All" && (p.format || "LV2") !== selectedFormat)
                continue

            if (selectedCategory !== "All" && (p.category || "Other") !== selectedCategory)
                continue

            if (query.length > 0) {
                var name = (p.name || "").toLowerCase()
                var author = (p.author || "").toLowerCase()
                var category = (p.category || "").toLowerCase()
                var uri = (p.uri || "").toLowerCase()
                if (name.indexOf(query) < 0 &&
                    author.indexOf(query) < 0 &&
                    category.indexOf(query) < 0 &&
                    uri.indexOf(query) < 0)
                    continue
            }

            result.push(p)
        }

        result.sort(function(a, b) {
            return (a.name || "").localeCompare(b.name || "")
        })

        filteredPlugins = result
        pluginList.currentIndex = result.length > 0 ? 0 : -1
        pluginList.positionViewAtBeginning()
    }

    function addPlugin(plugin) {
        if (!plugin || !plugin.uri || plugin.compatible === false)
            return
        var addedName = controller.add_plugin(plugin.uri)
        pluginBrowser.addedPlugin = true
        if (addedName) pluginBrowser.pluginAdded(addedName)
        pluginBrowser.visible = false
    }

    function moveHighlight(delta) {
        var n = filteredPlugins.length
        if (n === 0) return
        var i = Math.max(0, Math.min(n - 1, pluginList.currentIndex + delta))
        pluginList.currentIndex = i
        pluginList.positionViewAtIndex(i, ListView.Contain)
    }

    function open() {
        addedPlugin = false
        searchField.text = ""
        loadPlugins()
        visible = true
        raise()
        requestActivate()
        searchField.forceActiveFocus()
    }

    Shortcut {
        sequence: "Escape"
        onActivated: pluginBrowser.visible = false
    }

    Shortcut {
        sequences: ["Return", "Enter"]
        onActivated: {
            var i = pluginList.currentIndex
            if (i >= 0 && i < pluginBrowser.filteredPlugins.length)
                pluginBrowser.addPlugin(pluginBrowser.filteredPlugins[i])
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.dialogPadding
        spacing: 16

        ZbDialogHeader {
            title: "Add plugin"
        }

        ZbSearchField {
            id: searchField
            large: true
            Layout.fillWidth: true
            placeholderText: "Search by name, author or category"
            onTextChanged: pluginBrowser.filterPlugins()
            Keys.onUpPressed: pluginBrowser.moveHighlight(-1)
            Keys.onDownPressed: pluginBrowser.moveHighlight(1)
            Keys.onPressed: event => {
                if (event.key === Qt.Key_PageUp) {
                    pluginBrowser.moveHighlight(-8)
                    event.accepted = true
                } else if (event.key === Qt.Key_PageDown) {
                    pluginBrowser.moveHighlight(8)
                    event.accepted = true
                }
            }
        }

        // Filter row: format · count · spacer · category · compatible only
        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            ZbSegmented {
                model: pluginBrowser.formats
                currentIndex: Math.max(0, pluginBrowser.formats.indexOf(pluginBrowser.selectedFormat))
                onActivated: index => {
                    pluginBrowser.selectedFormat = pluginBrowser.formats[index]
                    pluginBrowser.filterPlugins()
                }
            }

            Text {
                text: pluginBrowser.filteredPlugins.length + " of " + pluginBrowser.allPlugins.length + " plugins"
                font.family: Theme.fontMono
                font.pixelSize: Theme.sizeMono
                color: Theme.textMuted
            }

            Item { Layout.fillWidth: true }

            ZbButton {
                id: categoryButton
                text: pluginBrowser.selectedCategory === "All" ? "All categories" : pluginBrowser.selectedCategory
                trailingIconName: "chevronDown"
                Layout.maximumWidth: 220
                onClicked: categoryMenu.opened ? categoryMenu.close() : categoryMenu.open()

                ZbMenu {
                    id: categoryMenu
                    y: categoryButton.height + 4

                    Instantiator {
                        model: pluginBrowser.categories
                        delegate: ZbMenuItem {
                            required property var modelData
                            text: modelData === "All" ? "All categories" : modelData
                            selected: pluginBrowser.selectedCategory === modelData
                            onTriggered: {
                                pluginBrowser.selectedCategory = modelData
                                pluginBrowser.filterPlugins()
                            }
                        }
                        onObjectAdded: (index, object) => categoryMenu.insertItem(index, object)
                        onObjectRemoved: (index, object) => categoryMenu.removeItem(object)
                    }
                }
            }

            ZbSwitch {
                compact: true
                text: "Compatible only"
                checked: pluginBrowser.showCompatibleOnly
                onToggled: {
                    pluginBrowser.showCompatibleOnly = checked
                    pluginBrowser.filterPlugins()
                }
            }
        }

        // Plugin list card
        ZbCard {
            Layout.fillWidth: true
            Layout.fillHeight: true

            ListView {
                id: pluginList
                ZbWheelScroll { flickable: pluginList }
                anchors {
                    left: parent.left
                    top: parent.top
                    bottom: parent.bottom
                    right: listBar.visible ? listBar.left : parent.right
                    leftMargin: 1
                    topMargin: Theme.scrollGap
                    bottomMargin: Theme.scrollGap
                    rightMargin: listBar.visible ? Theme.scrollGap : 1
                }
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                highlightFollowsCurrentItem: false
                model: pluginBrowser.filteredPlugins
                ScrollBar.vertical: listBar

                delegate: Rectangle {
                    id: row
                    required property int index
                    required property var modelData
                    readonly property var plugin: modelData || {}
                    readonly property bool active: ListView.isCurrentItem
                    readonly property bool isCompatible: plugin.compatible !== false

                    readonly property string metaText: {
                        var parts = []
                        if (plugin.category) parts.push(plugin.category)
                        if (plugin.author) parts.push(plugin.author)
                        return parts.join(" · ")
                    }
                    readonly property string portsText: {
                        var parts = [(plugin.audioIn || 0) + " in", (plugin.audioOut || 0) + " out"]
                        if (plugin.controlIn > 0) parts.push(plugin.controlIn + " controls")
                        return parts.join(" · ")
                    }

                    width: ListView.view.width
                    height: Theme.rowPlugin
                    color: active ? Theme.bgHover : "transparent"

                    MouseArea {
                        anchors.fill: parent
                        hoverEnabled: true
                        onContainsMouseChanged: if (containsMouse) pluginList.currentIndex = row.index
                        onClicked: pluginList.currentIndex = row.index
                        onDoubleClicked: pluginBrowser.addPlugin(row.plugin)
                    }

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 16
                        anchors.rightMargin: 16
                        spacing: 14

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 4
                            opacity: row.isCompatible ? 1 : 0.5

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 8
                                Text {
                                    text: row.plugin.name || ""
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
                                    id: formatBadge
                                    text: row.plugin.format || "LV2"
                                    tone: formatBadge.formatTone(text)
                                }
                                Item { Layout.fillWidth: true }
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 8
                                Text {
                                    visible: row.metaText.length > 0
                                    text: row.metaText
                                    font.family: Theme.fontSans
                                    font.pixelSize: Theme.sizeCaption
                                    color: Theme.textMuted
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                    Layout.preferredWidth: Math.ceil(implicitWidth)
                                    Layout.maximumWidth: Math.ceil(implicitWidth)
                                }
                                ZbDivider {
                                    visible: row.metaText.length > 0
                                    vertical: true
                                    strong: true
                                    Layout.preferredHeight: 10
                                }
                                Text {
                                    text: row.portsText
                                    font.family: Theme.fontMono
                                    font.pixelSize: Theme.sizeMono
                                    color: Theme.textMuted
                                }
                                Item { Layout.fillWidth: true }
                            }
                        }

                        ZbBadge {
                            visible: !row.isCompatible
                            text: "incompatible"
                            tone: "danger"
                        }

                        ZbBadge {
                            visible: row.plugin.hasUi === false
                            text: "no UI"
                        }

                        ZbButton {
                            text: "Add"
                            kind: row.active && row.isCompatible ? "primary" : "ghost"
                            enabled: row.isCompatible
                            onClicked: pluginBrowser.addPlugin(row.plugin)
                        }
                    }

                    ZbDivider {
                        visible: row.index < pluginList.count - 1
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                    }
                }

                Text {
                    visible: pluginList.count === 0
                    anchors.centerIn: parent
                    text: pluginBrowser.allPlugins.length === 0 ? "No plugins found" : "No plugins match the current filters"
                    font.family: Theme.fontSans
                    font.pixelSize: Theme.sizeBody
                    color: Theme.textMuted
                }
            }

            ZbScrollBar {
                id: listBar
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
