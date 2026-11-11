import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Keyboard and mouse shortcuts, grouped. Opened with ? or F1, from the app
// menu and from the help button on the canvas. Each row is a list of
// alternatives (key combinations, each a list of caps) and what they do.
// Keep in step with GraphView's key handling and the README's shortcut table.
ZbDialog {
    id: dialog
    title: "Keyboard and mouse shortcuts"
    primaryText: "Close"
    secondaryText: ""
    width: 700

    // [{ title, rows: [{ keys: [[cap, ...], ...], text }] }]
    readonly property var sections: [
        {
            title: "Move around",
            rows: [
                { keys: [["Wheel"]], text: "Zoom around the cursor" },
                { keys: [["Middle-drag"]], text: "Pan the canvas" },
                { keys: [["Left-drag"]], text: "On empty canvas: pan in Grab mode, draw a selection box in Select mode" },
                { keys: [["W"]], text: "Switch between Grab and Select mode" },
                { keys: [["Space"]], text: "Hold and drag: pan from anywhere, even over a node" },
                { keys: [["Shift"]], text: "Hold and drag on empty canvas in Grab mode: draw a selection box" },
                { keys: [["Home"]], text: "Fit the whole graph to the view" },
                { keys: [["F"]], text: "Frame the selected nodes (everything when none are selected)" },
                { keys: [["Ctrl", "="]], text: "Zoom in" },
                { keys: [["Ctrl", "-"]], text: "Zoom out" },
                { keys: [["Ctrl", "0"]], text: "Reset zoom to 100 %" },
                { keys: [["Pinch"]], text: "Zoom on a touchpad or touchscreen" }
            ]
        },
        {
            title: "Select",
            rows: [
                { keys: [["Click"]], text: "Select a node, a wire or a do-not-connect line" },
                { keys: [["Ctrl", "Click"]], text: "Add to or remove from the selection" },
                { keys: [["Shift"], ["Ctrl"]], text: "Hold while drawing a selection box to add to the selection" },
                { keys: [["Ctrl", "A"]], text: "Select all nodes and wires (not do-not-connect rules)" },
                { keys: [["Esc"]], text: "Cancel placing or wiring, then clear the selection, then leave a subgraph" }
            ]
        },
        {
            title: "Connect and edit",
            rows: [
                { keys: [["Drag"]], text: "From a pin to a pin: connect. Drag a node to move it" },
                { keys: [["Right-drag"]], text: "From a pin to a pin: never connect these (a red line)" },
                { keys: [["Delete"], ["Backspace"]], text: "Disconnect the selected wires, remove the selected plugins, subgraphs and virtual devices (it asks first), hide the other selected nodes, and remove the selected do-not-connect rules" },
                { keys: [["←", "↑", "→", "↓"]], text: "Nudge the selected nodes one grid step (1 px, or 10 px with Shift, when snap to grid is off)" },
                { keys: [["Shift", "Drag"]], text: "Snap a node to nearby nodes (when snap to grid is off)" },
                { keys: [["Ctrl", "Z"]], text: "Undo a move, a wire, an Auto layout or a hidden node" },
                { keys: [["Ctrl", "Shift", "Z"], ["Ctrl", "Y"]], text: "Redo" },
                { keys: [["Double-click"]], text: "Open a subgraph" },
                { keys: [["Right-click"]], text: "Menus for nodes, wires, pins, do-not-connect lines and the canvas" }
            ]
        },
        {
            title: "Layout",
            rows: [
                { keys: [["L"]], text: "Auto layout. Pinned nodes stay where they are" },
                { keys: [["Shift", "L"]], text: "Auto layout including pinned nodes" }
            ]
        },
        {
            title: "Application",
            rows: [
                { keys: [["Ctrl", "F"]], text: "Search nodes and ports" },
                { keys: [["Ctrl", "S"]], text: "Save every modified plugin's settings into its selected preset" },
                { keys: [["F1"], ["?"]], text: "Show this list" },
                { keys: [["Ctrl", "Q"]], text: "Quit" }
            ]
        }
    ]

    // One key cap: mono 10, outline tag (same look as the canvas hint).
    component Cap: Rectangle {
        id: cap
        property string label
        implicitHeight: 20
        implicitWidth: Math.max(20, capText.implicitWidth + 12)
        radius: Theme.radiusTag
        color: Theme.bgSubtle
        border.width: 1
        border.color: Theme.borderStrong
        Text {
            id: capText
            anchors.centerIn: parent
            text: cap.label
            font.family: Theme.fontMono
            font.pixelSize: Theme.sizeBadge
            color: Theme.textIcon
        }
    }

    // One row: the alternatives (separated by "or") and the description.
    component ShortcutRow: RowLayout {
        id: row
        property var keys: []
        property string text
        Layout.fillWidth: true
        spacing: 16

        Flow {
            Layout.preferredWidth: 220
            Layout.maximumWidth: 220
            Layout.alignment: Qt.AlignTop
            spacing: 4
            Repeater {
                model: row.keys
                delegate: Row {
                    id: alt
                    required property var modelData
                    required property int index
                    spacing: 4
                    Text {
                        visible: alt.index > 0
                        anchors.verticalCenter: parent.verticalCenter
                        text: "or"
                        font.family: Theme.fontSans
                        font.pixelSize: Theme.sizeCaption
                        color: Theme.textMuted
                        rightPadding: 2
                    }
                    Repeater {
                        model: alt.modelData
                        delegate: Row {
                            id: capItem
                            required property string modelData
                            required property int index
                            spacing: 4
                            Text {
                                visible: capItem.index > 0
                                anchors.verticalCenter: parent.verticalCenter
                                text: "+"
                                font.family: Theme.fontMono
                                font.pixelSize: Theme.sizeBadge
                                color: Theme.textMuted
                            }
                            Cap {
                                label: capItem.modelData
                            }
                        }
                    }
                }
            }
        }
        Text {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            text: row.text
            wrapMode: Text.WordWrap
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            color: Theme.textPrimary
            topPadding: 1
        }
    }

    Item {
        id: viewport
        // A Dialog sizes itself from its content's implicit size; keep the
        // list within the window and let it scroll.
        readonly property real maxHeight: Math.max(240, (Overlay.overlay ? Overlay.overlay.height : 760) - 230)
        implicitWidth: 640
        implicitHeight: Math.min(content.implicitHeight, maxHeight)
        width: parent ? parent.width : implicitWidth
        height: implicitHeight

        Flickable {
            id: flick
            anchors.fill: parent
            anchors.rightMargin: Theme.scrollBarWidth + Theme.scrollGap
            contentWidth: width
            contentHeight: content.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: bar
            ZbWheelScroll { flickable: flick }

            Column {
                id: content
                width: flick.width
                spacing: 4

                Repeater {
                    model: dialog.sections
                    delegate: Column {
                        id: section
                        required property var modelData
                        required property int index
                        width: content.width
                        spacing: 8
                        topPadding: section.index > 0 ? 14 : 0
                        bottomPadding: 4

                        ZbSectionLabel {
                            text: section.modelData.title
                        }
                        Repeater {
                            model: section.modelData.rows
                            delegate: ShortcutRow {
                                required property var modelData
                                width: section.width
                                keys: modelData.keys
                                text: modelData.text
                            }
                        }
                    }
                }
            }
        }

        ZbScrollBar {
            id: bar
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.right: parent.right
        }
    }
}
