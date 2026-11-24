import QtQuick
import QtQuick.Controls

Item {
    required property var graph
    anchors.fill: parent

    readonly property var elements: ({
        toast: toast,
        toastText: toastText,
        toastTimer: toastTimer,
        hScrollBar: hScrollBar,
        vScrollBar: vScrollBar
    })

    component ModeCell: AbstractButton {
        id: cell
        property string iconName
        property string tip
        property bool active: false
        width: 28
        height: 28
        hoverEnabled: true
        focusPolicy: Qt.NoFocus
        Accessible.name: tip
        background: Rectangle {
            radius: Theme.radiusSmall
            color: cell.active ? Theme.accentFaint : cell.hovered || cell.down ? Theme.bgHover : "transparent"
            border.width: cell.active ? 1 : 0
            border.color: Theme.accentRing
        }
        contentItem: Item {
            ZbIcon {
                anchors.centerIn: parent
                name: cell.iconName
                size: 18
                color: cell.active ? Theme.accent : cell.hovered ? Theme.textIcon : Theme.textMuted
            }
        }
        ZbTooltip {
            visible: cell.hovered
            text: cell.tip
        }
    }

    Rectangle {
        id: modeSwitch
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.margins: 12
        height: 34
        width: modeRow.implicitWidth + 6
        radius: Theme.radiusControl
        color: Theme.bgPanel
        border.width: 1
        border.color: Theme.borderStrong

        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            hoverEnabled: true
            cursorShape: Qt.ArrowCursor
            onEntered: graph.updateHover(-1, -1)
        }

        Row {
            id: modeRow
            anchors.centerIn: parent
            spacing: 2
            ModeCell {
                iconName: "grab"
                tip: "Grab: drag the canvas to move around (W)"
                active: graph.dragMode === "grab"
                onClicked: graph.setDragMode("grab")
            }
            ModeCell {
                iconName: "select"
                tip: "Select: drag a box around nodes and wires (W)"
                active: graph.dragMode === "select"
                onClicked: graph.setDragMode("select")
            }
        }
    }

    Rectangle {
        id: autoLayoutButton
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.rightMargin: 12 + (vScrollBar.visible ? vScrollBar.width : 0)
        anchors.topMargin: 12
        height: 34
        width: 34
        radius: Theme.radiusControl
        color: Theme.bgPanel
        border.width: 1
        border.color: Theme.borderStrong

        AbstractButton {
            id: autoLayoutCell
            anchors.centerIn: parent
            width: 28
            height: 28
            hoverEnabled: true
            focusPolicy: Qt.NoFocus
            Accessible.name: "Auto layout"
            onClicked: graph.requestAutoLayout(graph.shiftHeld)
            background: Rectangle {
                radius: Theme.radiusSmall
                color: autoLayoutCell.down ? Theme.borderStrong : autoLayoutCell.hovered ? Theme.bgHover : "transparent"
            }
            contentItem: Item {
                ZbIcon {
                    anchors.centerIn: parent
                    name: "autoLayout"
                    size: 18
                    color: autoLayoutCell.hovered ? Theme.textPrimary : Theme.textIcon
                }
            }
            ZbTooltip {
                visible: autoLayoutCell.hovered
                text: "Auto layout (L). Pinned nodes stay put; Shift+click (Shift+L) lays them out too."
            }
        }
    }

    Rectangle {
        id: helpButton
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.rightMargin: 12 + (vScrollBar.visible ? vScrollBar.width : 0)
        anchors.bottomMargin: 12 + (hScrollBar.visible ? hScrollBar.height : 0)
        width: 28
        height: 28
        radius: width / 2
        color: Theme.bgPanel
        border.width: 1
        border.color: Theme.borderStrong

        AbstractButton {
            id: helpCell
            anchors.fill: parent
            hoverEnabled: true
            focusPolicy: Qt.NoFocus
            Accessible.name: "Keyboard shortcuts"
            onClicked: graph.showShortcuts()
            background: Rectangle {
                radius: width / 2
                color: helpCell.down ? Theme.borderStrong : helpCell.hovered ? Theme.bgHover : "transparent"
            }
            contentItem: Text {
                text: "?"
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                font.family: Theme.fontMono
                font.pixelSize: 13
                font.weight: Font.Medium
                color: helpCell.hovered ? Theme.textPrimary : Theme.textMuted
            }
            ZbTooltip {
                visible: helpCell.hovered
                text: "Keyboard shortcuts (? or F1)"
            }
        }
    }

    Rectangle {
        id: breadcrumb
        visible: graph.currentSubgraph !== ""
        anchors.left: modeSwitch.right
        anchors.leftMargin: 8
        anchors.top: parent.top
        anchors.topMargin: 12
        height: 34
        width: crumbRow.implicitWidth + 12
        radius: Theme.radiusControl
        color: Theme.bgPanel
        border.width: 1
        border.color: Theme.borderStrong

        Row {
            id: crumbRow
            anchors.verticalCenter: parent.verticalCenter
            anchors.left: parent.left
            anchors.leftMargin: 4
            spacing: 4
            ZbButton {
                id: crumbHome
                kind: "subtle"
                compact: true
                text: "Graph"
                anchors.verticalCenter: parent.verticalCenter
                onClicked: graph.exitSubgraph()
                ZbTooltip {
                    visible: crumbHome.hovered
                    text: "Back to the main graph (Esc)"
                }
            }
            ZbIcon {
                name: "chevronRight"
                size: 10
                color: Theme.textMuted
                anchors.verticalCenter: parent.verticalCenter
            }
            ZbIcon {
                name: "subgraph"
                size: 14
                color: Theme.catSubgraph
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                text: graph.currentSubgraphName
                anchors.verticalCenter: parent.verticalCenter
                rightPadding: 6
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeBody
                font.weight: Font.DemiBold
                color: Theme.textPrimary
            }
        }
    }

    Repeater {
        model: graph.edgeMarkers
        delegate: Rectangle {
            id: marker
            required property var modelData
            x: Math.max(8, Math.min(graph.width - width - 8, modelData.x - width / 2))
            y: Math.max(8, Math.min(graph.height - height - 8, modelData.y - height / 2))
            width: markerRow.implicitWidth + 20
            height: 26
            radius: height / 2
            color: Theme.bgPanel
            border.width: 1
            border.color: Theme.accentRing

            Rectangle {
                anchors.fill: parent
                radius: parent.radius
                color: markerMouse.containsMouse ? Theme.accentRing : Theme.accentFaint
            }
            Row {
                id: markerRow
                anchors.centerIn: parent
                spacing: 6
                ZbIcon {
                    name: "arrow"
                    size: 12
                    color: Theme.accent
                    rotation: marker.modelData.angle
                    anchors.verticalCenter: parent.verticalCenter
                }
                Text {
                    text: "New: " + marker.modelData.name
                    width: Math.min(implicitWidth, 180)
                    elide: Text.ElideRight
                    anchors.verticalCenter: parent.verticalCenter
                    font.family: Theme.fontSans
                    font.pixelSize: 11
                    font.weight: Font.Medium
                    color: Theme.textPrimary
                }
            }
            MouseArea {
                id: markerMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: graph.revealNode(marker.modelData.id)
            }
        }
    }

    Rectangle {
        id: overrideHint
        readonly property bool active: graph.overrideState !== ""
        readonly property string action: graph.overrideState !== "" ? graph.overrideState : (graph.dragMode === "grab" ? "select" : "pan")
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 18
        height: 28
        width: hintRow.implicitWidth + 22
        radius: Theme.radiusControl
        color: Theme.bgPanel
        border.width: 1
        border.color: active ? Theme.accentRing : Theme.borderStrong

        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: Theme.accentFaint
            visible: overrideHint.active
        }

        component HintText: Text {
            property bool on: false
            anchors.verticalCenter: parent.verticalCenter
            font.family: Theme.fontSans
            font.pixelSize: 11
            font.weight: on ? Font.DemiBold : Font.Normal
            color: on ? Theme.accent : Theme.textMuted
        }

        Row {
            id: hintRow
            anchors.centerIn: parent
            spacing: 6
            ZbIcon {
                name: overrideHint.action === "pan" ? "grab" : "select"
                size: 14
                color: overrideHint.active ? Theme.accent : Theme.textMuted
                anchors.verticalCenter: parent.verticalCenter
            }
            HintText {
                on: overrideHint.active
                text: overrideHint.active ? (overrideHint.action === "pan" ? "Panning \u2014 release" : "Selecting \u2014 release") : "Hold"
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                height: 18
                width: keyText.implicitWidth + 12
                radius: Theme.radiusTag
                color: "transparent"
                border.width: 1
                border.color: overrideHint.active ? Theme.accent : Theme.borderStrong
                Text {
                    id: keyText
                    anchors.centerIn: parent
                    text: overrideHint.action === "pan" ? "Space" : "Shift"
                    font.family: Theme.fontMono
                    font.pixelSize: 10
                    color: overrideHint.active ? Theme.accent : Theme.textIcon
                }
            }
            HintText {
                on: overrideHint.active
                text: overrideHint.active ? (graph.dragMode === "grab" ? "to go back to Grab" : "to go back to Select") : (overrideHint.action === "pan" ? "to pan" : "and drag to select")
            }
        }
    }

    Rectangle {
        id: toast
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: overrideHint.top
        anchors.bottomMargin: 10
        width: Math.min(toastText.implicitWidth + 32, parent.width - 32)
        height: toastText.implicitHeight + 16
        radius: Theme.radiusControl
        color: Theme.bgTooltip
        border.width: 1
        border.color: Theme.borderStrong
        opacity: 0
        visible: opacity > 0
        Behavior on opacity {
            NumberAnimation {
                duration: 150
            }
        }

        Text {
            id: toastText
            anchors.centerIn: parent
            width: Math.min(implicitWidth, toast.parent.width - 64)
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            color: Theme.textPrimary
            wrapMode: Text.WordWrap
        }

        Timer {
            id: toastTimer
            interval: 3500
            onTriggered: toast.opacity = 0
        }
    }

    ZbScrollBar {
        id: hScrollBar
        orientation: Qt.Horizontal
        anchors.left: parent.left
        anchors.right: vScrollBar.visible ? vScrollBar.left : parent.right
        anchors.bottom: parent.bottom
        visible: size < 1
        policy: ScrollBar.AlwaysOn
        property real rangeLo: 0
        property real rangeLen: 1
        onPositionChanged: {
            if (!pressed)
                return;
            graph.panX = -(rangeLo + position * rangeLen) * graph.zoom;
            graph.elements.canvas.requestPaint();
        }
        onPressedChanged: if (!pressed)
            graph.elements.canvas.requestPaint()
    }

    ZbScrollBar {
        id: vScrollBar
        orientation: Qt.Vertical
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.bottom: hScrollBar.visible ? hScrollBar.top : parent.bottom
        visible: size < 1
        policy: ScrollBar.AlwaysOn
        property real rangeLo: 0
        property real rangeLen: 1
        onPositionChanged: {
            if (!pressed)
                return;
            graph.panY = -(rangeLo + position * rangeLen) * graph.zoom;
            graph.elements.canvas.requestPaint();
        }
        onPressedChanged: if (!pressed)
            graph.elements.canvas.requestPaint()
    }

}
