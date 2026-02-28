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
    color: Theme.windowBg

    required property var controller

    property var cpuData: []
    property var pluginData: []
    property real totalPluginDsp: 0.0

    // Per-plugin DSP history: { pluginId: { name, color, history: [dsp%...] } }
    property var pluginHistory: ({})
    property int historyLength: 120

    // Distinct colors for plugin lines
    readonly property var pluginColors: [
        "#FF6B6B", "#4ECDC4", "#FFE66D", "#A06CD5", "#FF8C42",
        "#6BCB77", "#4D96FF", "#FF6B9D", "#C9B1FF", "#00D2FF",
        "#FF4081", "#69F0AE", "#FFD740", "#B388FF", "#FF9E80"
    ]

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

        // Accumulate per-plugin DSP history
        var ph = pluginHistory
        var activeIds = {}
        var colorIdx = 0

        // Assign stable colors: count existing entries first
        for (var existingId in ph) colorIdx++

        for (var pi = 0; pi < pluginData.length; pi++) {
            var p = pluginData[pi]
            var pid = p.id
            activeIds[pid] = true

            if (!ph[pid]) {
                // New plugin — assign a color and fill history with zeros
                ph[pid] = {
                    name: p.name,
                    color: pluginColors[Object.keys(ph).length % pluginColors.length],
                    history: []
                }
                for (var z = 0; z < historyLength - 1; z++)
                    ph[pid].history.push(0)
            }
            ph[pid].name = p.name
            ph[pid].history.push(p.dspPercent || 0)
            if (ph[pid].history.length > historyLength)
                ph[pid].history.shift()
        }

        // Remove plugins that are no longer active
        for (var oldId in ph) {
            if (!activeIds[oldId])
                delete ph[oldId]
        }

        pluginHistory = ph
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
                ctx.fillStyle = "" + Theme.chartBg
                ctx.fillRect(0, 0, w, h)

                // Grid lines with labels
                ctx.setLineDash([3, 3])
                ctx.font = "9px monospace"
                ctx.textBaseline = "middle"

                var gridLines = [
                    { pct: 5,  color: "" + Theme.chartGridLight },
                    { pct: 10, color: "" + Theme.chartGrid25 },
                    { pct: 25, color: "" + Theme.chartGrid25 },
                    { pct: 50, color: "" + Theme.chartGrid50 },
                    { pct: 75, color: "" + Theme.chartGrid25 },
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

                // Data area (leave right margin for DSP% axis labels)
                var graphX = 32
                var graphW = w - graphX - 48

                if (d.length >= 2) {
                    // Filled area for process CPU
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
                    ctx.fillStyle = "" + Theme.chartFill
                    ctx.fill()

                    // Process CPU line
                    ctx.strokeStyle = "" + Theme.chartLine
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

                // Draw per-plugin DSP% lines with auto-scaled Y axis
                var ph = pluginHistory
                var legendItems = []

                // Find peak DSP% across all plugin histories for auto-scaling
                var peakDsp = 0
                for (var pid in ph) {
                    var hist0 = ph[pid].history
                    for (var si = 0; si < hist0.length; si++) {
                        if (hist0[si] > peakDsp) peakDsp = hist0[si]
                    }
                }

                // Ceiling is just 10% above the peak so lines fill the graph
                var dspCeil = peakDsp * 1.1
                if (dspCeil < 0.001) dspCeil = 0.001

                for (var pid2 in ph) {
                    var entry = ph[pid2]
                    var hist = entry.history
                    if (hist.length < 2) continue

                    legendItems.push({ name: entry.name, color: entry.color })

                    var pStep = graphW / (hist.length - 1)
                    ctx.strokeStyle = entry.color
                    ctx.lineWidth = 1.2
                    ctx.beginPath()
                    for (var hi = 0; hi < hist.length; hi++) {
                        var hx = graphX + hi * pStep
                        // Scale to dspCeil instead of 100%
                        var hy = h - (hist[hi] / dspCeil) * (h - 4) - 2
                        if (hi === 0) ctx.moveTo(hx, hy)
                        else ctx.lineTo(hx, hy)
                    }
                    ctx.stroke()
                }

                // Draw right-side axis labels for plugin DSP scale
                if (legendItems.length > 0 && dspCeil > 0) {
                    ctx.setLineDash([2, 4])
                    ctx.font = "8px monospace"
                    ctx.textAlign = "left"
                    ctx.textBaseline = "middle"
                    var dspTicks = [0.25, 0.5, 0.75, 1.0]
                    for (var ti = 0; ti < dspTicks.length; ti++) {
                        var frac = dspTicks[ti]
                        var tickVal = dspCeil * frac
                        var ty = h - frac * (h - 4) - 2
                        ctx.strokeStyle = "rgba(255,255,255,0.12)"
                        ctx.lineWidth = 0.5
                        ctx.beginPath()
                        ctx.moveTo(graphX, ty)
                        ctx.lineTo(graphX + graphW, ty)
                        ctx.stroke()
                        // Label on right side
                        var label = tickVal >= 1 ? tickVal.toFixed(1) + "%" : tickVal.toFixed(2) + "%"
                        ctx.fillStyle = "rgba(255,180,100,0.6)"
                        ctx.fillText(label, graphX + graphW + 2 - ctx.measureText(label).width - 2, ty)
                    }
                    ctx.setLineDash([])
                }

                // Draw legend in top-left corner
                if (legendItems.length > 0) {
                    ctx.font = "9px sans-serif"
                    ctx.textAlign = "left"
                    ctx.textBaseline = "middle"
                    var legendY = 8
                    var lineH = 14

                    // Measure widest label to size the background
                    var maxLabelW = 0
                    for (var mi = 0; mi < legendItems.length; mi++) {
                        var tw = ctx.measureText(legendItems[mi].name).width
                        if (tw > maxLabelW) maxLabelW = tw
                    }
                    // +1 for the "Process CPU" entry
                    var totalH = (legendItems.length + 1) * lineH + 6
                    var boxW = maxLabelW + 24
                    var boxX = graphX + 4
                    var boxY = legendY - 3

                    ctx.fillStyle = "" + Theme.chartBg
                    ctx.globalAlpha = 0.85
                    ctx.fillRect(boxX, boxY, boxW, totalH)
                    ctx.globalAlpha = 1.0
                    ctx.strokeStyle = "" + Theme.chartBorder
                    ctx.lineWidth = 0.5
                    ctx.strokeRect(boxX, boxY, boxW, totalH)

                    // Process CPU entry
                    var ey = legendY + lineH / 2
                    ctx.strokeStyle = "" + Theme.chartLine
                    ctx.lineWidth = 2
                    ctx.beginPath()
                    ctx.moveTo(boxX + 4, ey)
                    ctx.lineTo(boxX + 16, ey)
                    ctx.stroke()
                    ctx.fillStyle = "" + Theme.textSecondary
                    ctx.fillText("Process CPU", boxX + 20, ey)

                    for (var li = 0; li < legendItems.length; li++) {
                        ey = legendY + (li + 1) * lineH + lineH / 2
                        ctx.strokeStyle = legendItems[li].color
                        ctx.lineWidth = 2
                        ctx.beginPath()
                        ctx.moveTo(boxX + 4, ey)
                        ctx.lineTo(boxX + 16, ey)
                        ctx.stroke()
                        ctx.fillStyle = "" + Theme.textSecondary
                        ctx.fillText(legendItems[li].name, boxX + 20, ey)
                    }
                }

                // Border
                ctx.strokeStyle = "" + Theme.chartBorder
                ctx.lineWidth = 1
                ctx.strokeRect(0, 0, w, h)
            }
        }

        // Separator
        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: Theme.separator
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
                property var plugin: pluginData[index] || {}
                property bool hasWorker: (plugin.workerPercent || 0) > 0
                height: hasWorker ? 52 : 32
                color: index % 2 === 0 ? Theme.rowEven : Theme.rowOdd
                radius: 3

                ColumnLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 8
                    spacing: 0

                    // Main row: RT thread stats
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: !hasWorker
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
                            color: (plugin.dspPercent || 0) > 50 ? Theme.dspHigh
                                 : (plugin.dspPercent || 0) > 20 ? Theme.dspMedium
                                 : Theme.dspLow
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
                            color: Theme.chartBg
                            radius: 2
                            border.color: Theme.chartBorder
                            border.width: 1

                            Rectangle {
                                anchors.left: parent.left
                                anchors.top: parent.top
                                anchors.bottom: parent.bottom
                                anchors.margins: 1
                                width: Math.min(1.0, (plugin.dspPercent || 0) / 100.0) * (parent.width - 2)
                                radius: 1
                                color: (plugin.dspPercent || 0) > 50 ? Theme.dspBarHigh
                                     : (plugin.dspPercent || 0) > 20 ? Theme.dspBarMedium
                                     : Theme.dspBarLow
                            }
                        }
                    }

                    // Worker row: shown only for plugins with worker activity
                    RowLayout {
                        visible: hasWorker
                        Layout.fillWidth: true
                        spacing: 8

                        Label {
                            text: "  worker (async)"
                            font.italic: true
                            font.pointSize: 8
                            opacity: 0.5
                            Layout.fillWidth: true
                        }

                        Label {
                            text: (plugin.workerPercent || 0).toFixed(2) + "%"
                            font.family: "monospace"
                            font.pointSize: 9
                            Layout.preferredWidth: 70
                            horizontalAlignment: Text.AlignRight
                            color: Theme.statusBypassed
                        }

                        Label {
                            text: formatUs(plugin.workerAvgUs || 0)
                            font.family: "monospace"
                            font.pointSize: 8
                            opacity: 0.5
                            Layout.preferredWidth: 70
                            horizontalAlignment: Text.AlignRight
                        }

                        // Spacers to align with columns above
                        Item { Layout.preferredWidth: 70 }
                        Item { Layout.preferredWidth: 100 }
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
