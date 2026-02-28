import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: cpuOverlay
    title: "CPU Usage"
    width: 680
    height: 520
    minimumWidth: 500
    minimumHeight: 400
    visible: false

    required property var controller

    property var cpuData: []
    property var pluginData: []
    property real totalPluginDsp: 0.0

    function open() {
        refresh()
        visible = true
        raise()
        requestActivate()
    }

    function refresh() {
        try {
            cpuData = JSON.parse(controller.get_cpu_history())
        } catch(e) {
            cpuData = []
        }
        try {
            pluginData = JSON.parse(controller.get_plugin_cpu_json())
        } catch(e) {
            pluginData = []
        }
        var total = 0
        for (var i = 0; i < pluginData.length; i++) {
            total += pluginData[i].dspPercent || 0
        }
        totalPluginDsp = Math.round(total * 100) / 100
        bigGraph.requestPaint()
    }

    Timer {
        id: overlayRefreshTimer
        interval: 500
        running: cpuOverlay.visible
        repeat: true
        onTriggered: refresh()
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 10

        // Header with current CPU value
        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            Label {
                text: "Process CPU"
                font.bold: true
                font.pointSize: 12
            }

            Item { Layout.fillWidth: true }

            Label {
                text: controller.cpu_usage
                font.family: "monospace"
                font.pointSize: 14
                font.bold: true
            }
        }

        // Big CPU graph
        Canvas {
            id: bigGraph
            Layout.fillWidth: true
            Layout.preferredHeight: 180
            clip: true

            onPaint: {
                var ctx = getContext("2d")
                ctx.reset()
                var d = cpuData
                var w = width
                var h = height

                // Background
                ctx.fillStyle = "#1a1a1a"
                ctx.fillRect(0, 0, w, h)

                // Grid lines with labels
                ctx.setLineDash([3, 3])
                ctx.font = "9px monospace"
                ctx.textBaseline = "middle"

                var gridLines = [
                    { pct: 5,  color: "#334455" },
                    { pct: 10, color: "#445566" },
                    { pct: 25, color: "#4466AA" },
                    { pct: 50, color: "#AA4444" },
                    { pct: 75, color: "#4466AA" },
                ]

                for (var gi = 0; gi < gridLines.length; gi++) {
                    var gl = gridLines[gi]
                    var gy = h - (gl.pct / 100.0) * (h - 4) - 2
                    ctx.strokeStyle = gl.color
                    ctx.lineWidth = 0.5
                    ctx.beginPath()
                    ctx.moveTo(30, gy)
                    ctx.lineTo(w, gy)
                    ctx.stroke()
                    // Label
                    ctx.fillStyle = gl.color
                    ctx.textAlign = "right"
                    ctx.fillText(gl.pct + "%", 27, gy)
                }

                ctx.setLineDash([])

                // Data area
                var graphX = 32
                var graphW = w - graphX - 4

                if (d.length >= 2) {
                    // Filled area
                    ctx.beginPath()
                    var step = graphW / (d.length - 1)
                    for (var j = 0; j < d.length; j++) {
                        var x = graphX + j * step
                        var y = h - (d[j] / 100.0) * (h - 4) - 2
                        if (j === 0) ctx.moveTo(x, y)
                        else ctx.lineTo(x, y)
                    }
                    ctx.lineTo(graphX + (d.length - 1) * step, h)
                    ctx.lineTo(graphX, h)
                    ctx.closePath()
                    ctx.fillStyle = "rgba(76, 175, 80, 0.15)"
                    ctx.fill()

                    // Line
                    ctx.strokeStyle = "#4CAF50"
                    ctx.lineWidth = 1.5
                    ctx.beginPath()
                    for (var k = 0; k < d.length; k++) {
                        var lx = graphX + k * step
                        var ly = h - (d[k] / 100.0) * (h - 4) - 2
                        if (k === 0) ctx.moveTo(lx, ly)
                        else ctx.lineTo(lx, ly)
                    }
                    ctx.stroke()
                }

                // Border
                ctx.strokeStyle = "#333333"
                ctx.lineWidth = 1
                ctx.strokeRect(0, 0, w, h)
            }
        }

        // Separator
        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: "#3c3c3c"
        }

        // Plugin CPU section header
        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Label {
                text: "Plugin DSP Load"
                font.bold: true
                font.pointSize: 11
            }

            Item { Layout.fillWidth: true }

            Label {
                text: "Total: " + totalPluginDsp + "% DSP"
                font.family: "monospace"
                opacity: 0.7
            }
        }

        // Column headers
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 8
            Layout.rightMargin: 8
            spacing: 8

            Label {
                text: "Plugin"
                font.bold: true
                font.pointSize: 9
                opacity: 0.5
                Layout.fillWidth: true
            }
            Label {
                text: "DSP %"
                font.bold: true
                font.pointSize: 9
                opacity: 0.5
                Layout.preferredWidth: 70
                horizontalAlignment: Text.AlignRight
            }
            Label {
                text: "Avg"
                font.bold: true
                font.pointSize: 9
                opacity: 0.5
                Layout.preferredWidth: 70
                horizontalAlignment: Text.AlignRight
            }
            Label {
                text: "Last"
                font.bold: true
                font.pointSize: 9
                opacity: 0.5
                Layout.preferredWidth: 70
                horizontalAlignment: Text.AlignRight
            }
            // Bar column
            Item {
                Layout.preferredWidth: 100
            }
        }

        // Plugin list
        ListView {
            id: pluginCpuList
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: pluginData.length
            spacing: 2

            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            delegate: Rectangle {
                required property int index
                width: pluginCpuList.width - 12
                height: 32
                color: index % 2 === 0 ? "#2a2a2a" : "#252525"
                radius: 3

                property var plugin: pluginData[index] || {}

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 8

                    Label {
                        text: plugin.name || ""
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                        font.pointSize: 10
                    }

                    Label {
                        text: (plugin.dspPercent || 0).toFixed(2) + "%"
                        font.family: "monospace"
                        font.pointSize: 10
                        Layout.preferredWidth: 70
                        horizontalAlignment: Text.AlignRight
                        color: (plugin.dspPercent || 0) > 50 ? "#FF4444"
                             : (plugin.dspPercent || 0) > 20 ? "#FFAA44"
                             : "#88CC88"
                    }

                    Label {
                        text: formatUs(plugin.avgUs || 0)
                        font.family: "monospace"
                        font.pointSize: 9
                        opacity: 0.6
                        Layout.preferredWidth: 70
                        horizontalAlignment: Text.AlignRight
                    }

                    Label {
                        text: formatUs(plugin.lastUs || 0)
                        font.family: "monospace"
                        font.pointSize: 9
                        opacity: 0.6
                        Layout.preferredWidth: 70
                        horizontalAlignment: Text.AlignRight
                    }

                    // DSP bar
                    Rectangle {
                        Layout.preferredWidth: 100
                        Layout.preferredHeight: 12
                        Layout.alignment: Qt.AlignVCenter
                        color: "#1a1a1a"
                        radius: 2
                        border.color: "#333333"
                        border.width: 1

                        Rectangle {
                            anchors.left: parent.left
                            anchors.top: parent.top
                            anchors.bottom: parent.bottom
                            anchors.margins: 1
                            width: Math.min(1.0, (plugin.dspPercent || 0) / 100.0) * (parent.width - 2)
                            radius: 1
                            color: (plugin.dspPercent || 0) > 50 ? "#CC3333"
                                 : (plugin.dspPercent || 0) > 20 ? "#CC8833"
                                 : "#4CAF50"
                        }
                    }
                }
            }

            Label {
                anchors.centerIn: parent
                text: "No active plugins"
                opacity: 0.4
                visible: pluginData.length === 0
            }
        }
    }

    function formatUs(us) {
        if (us >= 1000) {
            return (us / 1000).toFixed(1) + " ms"
        }
        return us + " us"
    }
}
