import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Dialog {
    id: pluginBrowser
    title: "Add LV2 Plugin"
    width: 700
    height: 500
    modal: true
    standardButtons: Dialog.Close

    required property var controller

    property var allPlugins: []
    property var filteredPlugins: []
    property var categories: []
    property string selectedCategory: "All"
    property bool showCompatibleOnly: true

    function loadPlugins() {
        try {
            allPlugins = JSON.parse(controller.get_available_plugins_json())
        } catch(e) {
            allPlugins = []
        }

        // Build category list
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

            // Compatible filter
            if (showCompatibleOnly && !p.compatible)
                continue

            // Category filter
            if (selectedCategory !== "All" && p.category !== selectedCategory)
                continue

            // Text filter — match name, author, or URI
            if (query.length > 0) {
                var name = (p.name || "").toLowerCase()
                var author = (p.author || "").toLowerCase()
                var uri = (p.uri || "").toLowerCase()
                if (name.indexOf(query) < 0 &&
                    author.indexOf(query) < 0 &&
                    uri.indexOf(query) < 0)
                    continue
            }

            result.push(p)
        }

        // Sort by name
        result.sort(function(a, b) {
            return a.name.localeCompare(b.name)
        })

        filteredPlugins = result
    }

    onOpened: loadPlugins()

    contentItem: ColumnLayout {
        spacing: 8

        // Search bar and category filter
        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            TextField {
                id: searchField
                placeholderText: "Search plugins..."
                Layout.fillWidth: true
                selectByMouse: true
                onTextChanged: filterPlugins()
            }

            ComboBox {
                id: categoryCombo
                model: categories
                implicitWidth: 160
                onCurrentTextChanged: {
                    selectedCategory = currentText
                    filterPlugins()
                }
            }
        }

        // Filter bar
        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Label {
                text: filteredPlugins.length + " of " + allPlugins.length + " plugins"
                font.italic: true
                opacity: 0.7
            }

            Item { Layout.fillWidth: true }

            CheckBox {
                id: compatibleCheck
                text: "Compatible only"
                checked: showCompatibleOnly
                onToggled: {
                    showCompatibleOnly = checked
                    filterPlugins()
                }
            }
        }

        // Plugin list
        ListView {
            id: pluginList
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: filteredPlugins.length

            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            delegate: Rectangle {
                id: pluginDelegate
                required property int index
                width: pluginList.width
                height: 64
                color: pluginMouseArea.containsMouse ? "#3a3a3a" : (index % 2 === 0 ? "#2a2a2a" : "#252525")
                opacity: plugin.compatible ? 1.0 : 0.5

                property var plugin: filteredPlugins[index] || {}

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 8
                    spacing: 12

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2

                        RowLayout {
                            spacing: 6
                            Label {
                                text: plugin.name || ""
                                font.bold: true
                                font.pointSize: 10
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }
                            Label {
                                visible: !plugin.compatible
                                text: "incompatible"
                                font.pointSize: 8
                                color: "#e06060"
                            }
                        }

                        Label {
                            text: {
                                var parts = []
                                if (plugin.category) parts.push(plugin.category)
                                if (plugin.author) parts.push("by " + plugin.author)
                                return parts.join("  |  ")
                            }
                            font.pointSize: 8
                            opacity: 0.7
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }

                        Label {
                            text: {
                                var parts = []
                                if (plugin.audioIn > 0 || plugin.audioOut > 0)
                                    parts.push("Audio: " + plugin.audioIn + " in / " + plugin.audioOut + " out")
                                if (plugin.controlIn > 0)
                                    parts.push("Controls: " + plugin.controlIn)
                                return parts.join("  |  ")
                            }
                            font.pointSize: 8
                            opacity: 0.5
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }

                    Button {
                        text: "Add"
                        enabled: plugin.compatible !== false
                        onClicked: {
                            if (plugin.uri) {
                                controller.add_plugin(plugin.uri)
                                pluginBrowser.close()
                            }
                        }
                    }
                }

                MouseArea {
                    id: pluginMouseArea
                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.NoButton
                }
            }
        }
    }
}
