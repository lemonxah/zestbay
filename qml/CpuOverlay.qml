import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ZestBay

// CPU usage (spec §6.6): process CPU hero + legend, a 60 s log-scale chart of
// process CPU and per-plugin DSP load, and the plugin DSP load table.
ZbWindow {
    id: cpuOverlay
    title: "CPU usage"
    width: 680
    height: 680
    minimumWidth: 560
    minimumHeight: 560
    visible: false

    required property var controller

    // ─── Data ───────────────────────────────────────────────────────────
    readonly property int windowMs: 60000      // chart span (last 60 s)
    readonly property int gapMs: 2500          // break a line across gaps longer than this
    readonly property int maxSlots: Theme.chartSeries.length

    // Latest plugin snapshot, sorted by DSP % descending (table rows).
    property var pluginData: []
    property real totalPluginDsp: 0.0

    // Series state. Plain JS objects mutated in place; `revision` is bumped
    // after every poll so bindings that read them re-evaluate.
    //   series: { key, name, slot (-1 = folded into Other), points: [{t, v}],
    //             latest, live }
    // Slot 0 = process CPU, then plugins in order of first appearance. A
    // plugin keeps its slot for the life of the window; beyond the chart's
    // slots, plugins fold into one "Other" series.
    property var seriesByKey: ({})
    property var seriesOrder: []
    property int nextSlot: 1
    property real lastPollT: 0
    property int revision: 0

    function open() {
        refresh()
        visible = true
        raise()
        requestActivate()
    }

    Shortcut {
        sequence: "Escape"
        onActivated: cpuOverlay.visible = false
    }

    // The only poller: get_plugin_cpu_json() takes the snapshots, so it must
    // be read from exactly one place (refresh()).
    Timer {
        id: overlayRefreshTimer
        interval: 250
        running: cpuOverlay.visible
        repeat: true
        onTriggered: cpuOverlay.refresh()
    }

    // Smooth scrolling: redraw at ~30 fps with the time axis following the
    // clock (one sample interval behind it, so the newest point slides in at
    // the right edge instead of the chart jumping on every sample).
    property real viewT: 0
    property real hoverMouseX: -1
    Timer {
        id: frameTimer
        interval: 33
        running: cpuOverlay.visible && cpuOverlay.lastPollT > 0
        repeat: true
        onTriggered: {
            cpuOverlay.viewT = Date.now() - overlayRefreshTimer.interval
            if (cpuOverlay.hoverActive && cpuOverlay.hoverMouseX >= 0)
                cpuOverlay.updateHover(cpuOverlay.hoverMouseX)
            else
                chart.requestPaint()
        }
    }

    Connections {
        target: Theme
        function onDarkChanged() { chart.requestPaint() }
    }

    function ensureSeries(key, name, slot) {
        var s = seriesByKey[key]
        if (!s) {
            s = { key: key, name: name, slot: slot, points: [], latest: 0, live: false }
            seriesByKey[key] = s
            seriesOrder.push(key)
        }
        return s
    }

    function seriesColor(s) {
        return s && s.slot >= 0 ? Theme.chartSeries[s.slot] : Theme.chartOther
    }

    function pushPoint(s, t, v) {
        s.points.push({ t: t, v: v })
        s.latest = v
        s.live = true
    }

    function refresh() {
        var now = Date.now()
        var hist = []
        try { hist = JSON.parse(controller.get_cpu_history()) || [] } catch (e) { hist = [] }
        var plugins = []
        try { plugins = JSON.parse(controller.get_plugin_cpu_json()) || [] } catch (e) { plugins = [] }

        for (var k = 0; k < seriesOrder.length; k++)
            seriesByKey[seriesOrder[k]].live = false

        // Process CPU: the backend keeps an EMA sample per poll (percent of
        // all cores). Take the newest; after a gap (window was hidden) backfill
        // from the backend history, which is spaced by the poll interval.
        var proc = ensureSeries("cpu", "Process CPU", 0)
        var lastT = proc.points.length ? proc.points[proc.points.length - 1].t : 0
        if (now - lastT > gapMs && hist.length > 1) {
            var step = Math.max(16, controller.get_poll_interval_ms())
            for (var i = 0; i < hist.length - 1; i++) {
                var ht = now - (hist.length - 1 - i) * step
                if (ht > lastT && ht > now - windowMs)
                    proc.points.push({ t: ht, v: hist[i] || 0 })
            }
        }
        pushPoint(proc, now, hist.length ? (hist[hist.length - 1] || 0) : 0)

        // Plugins: no history in the backend, so build it here per poll.
        var total = 0
        var otherSum = 0
        var anyOther = false
        for (var p = 0; p < plugins.length; p++) {
            var pd = plugins[p]
            var dsp = pd.dspPercent || 0
            total += dsp
            var key = "p" + pd.id
            var s = seriesByKey[key]
            if (!s) {
                var slot = nextSlot < maxSlots ? nextSlot++ : -1
                s = ensureSeries(key, pd.name || "", slot)
            }
            s.name = pd.name || s.name
            pd.slot = s.slot
            if (s.slot >= 0) {
                pushPoint(s, now, dsp)
            } else {
                s.live = true
                otherSum += dsp
                anyOther = true
            }
        }
        if (anyOther)
            pushPoint(ensureSeries("other", "Other", -1), now, otherSum)

        // Keep 60 s (plus one older point so lines reach the left edge).
        var cutoff = now - windowMs
        for (var j = 0; j < seriesOrder.length; j++) {
            var pts = seriesByKey[seriesOrder[j]].points
            while (pts.length > 1 && pts[1].t <= cutoff) pts.shift()
            if (pts.length === 1 && pts[0].t < cutoff) pts.shift()
        }

        plugins.sort(function(a, b) { return (b.dspPercent || 0) - (a.dspPercent || 0) })
        pluginData = plugins
        totalPluginDsp = total
        lastPollT = now
        revision++
        if (chartMouse.containsMouse) updateHover(chartMouse.mouseX)
        chart.requestPaint()
    }

    // Series drawn on the chart (process, plugin slots, then Other).
    function chartSeriesList(rev) {
        var list = []
        var other = null
        for (var i = 0; i < seriesOrder.length; i++) {
            var s = seriesByKey[seriesOrder[i]]
            if (s.key === "other") { other = s; continue }
            if (s.slot < 0 || s.points.length === 0) continue
            list.push(s)
        }
        if (other && other.points.length > 0) list.push(other)
        return list
    }

    function formatValue(s, v) {
        return s.key === "cpu" ? v.toFixed(1) + "%" : v.toFixed(2) + "%"
    }

    function formatPct(v) {
        return v.toFixed(2) + "%"
    }

    function formatUs(us) {
        if (us >= 1000) return (us / 1000).toFixed(1) + " ms"
        return us + " µs"
    }

    // ─── Chart geometry (log y: 0.01 % … 100 %) ─────────────────────────
    readonly property real plotLeft: 44
    readonly property real plotRight: chart.width - 8
    readonly property real plotTop: 8
    readonly property real plotBottom: chart.height - 24
    readonly property var gridValues: [100, 10, 1, 0.1, 0.01]

    function xFor(t) {
        var f = 1 - ((viewT > 0 ? viewT : lastPollT) - t) / windowMs
        return plotLeft + f * (plotRight - plotLeft)
    }
    function yFor(v) {
        var lv = Math.log(Math.max(v, 0.01)) / Math.LN10   // -2 … 2
        lv = Math.min(2, lv)
        return plotBottom - (lv + 2) / 4 * (plotBottom - plotTop)
    }

    // ─── Hover ──────────────────────────────────────────────────────────
    property bool hoverActive: false
    property real hoverX: 0
    property string hoverLabel: ""
    property var hoverRows: []   // [{ name, color, value, text, y }]

    function nearestPoint(pts, t, tol) {
        var best = null
        var bestD = tol
        for (var i = 0; i < pts.length; i++) {
            var d = Math.abs(pts[i].t - t)
            if (d <= bestD) { bestD = d; best = pts[i] }
        }
        return best
    }

    function updateHover(mx) {
        if (mx < plotLeft || mx > plotRight || lastPollT === 0) {
            hoverActive = false
            chart.requestPaint()
            return
        }
        hoverMouseX = mx
        var nowT = viewT > 0 ? viewT : lastPollT
        var t = nowT - (1 - (mx - plotLeft) / (plotRight - plotLeft)) * windowMs
        // Snap to the nearest process sample so every series lines up.
        var proc = seriesByKey["cpu"]
        var snap = proc ? nearestPoint(proc.points, t, gapMs) : null
        if (snap) t = snap.t
        var rows = []
        var list = chartSeriesList(revision)
        for (var i = 0; i < list.length; i++) {
            var s = list[i]
            var pt = nearestPoint(s.points, t, overlayRefreshTimer.interval * 1.5)
            if (!pt) continue
            rows.push({ name: s.name, color: "" + seriesColor(s), value: pt.v,
                        text: formatValue(s, pt.v), y: yFor(pt.v) })
        }
        rows.sort(function(a, b) { return b.value - a.value })
        var secs = Math.round((nowT - t) / 1000)
        hoverLabel = secs <= 0 ? "now" : "-" + secs + " s"
        hoverX = xFor(t)
        hoverRows = rows
        hoverActive = rows.length > 0
        chart.requestPaint()
    }

    // ─── Layout ─────────────────────────────────────────────────────────
    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.dialogPadding
        anchors.rightMargin: Theme.dialogPadding
        anchors.topMargin: 20
        anchors.bottomMargin: Theme.dialogPadding
        spacing: 0

        ZbDialogHeader {
            title: "CPU usage"
        }

        // Hero: process CPU + legend chips.
        RowLayout {
            id: heroRow
            Layout.fillWidth: true
            Layout.topMargin: 12
            Layout.bottomMargin: 12
            spacing: 24

            ColumnLayout {
                id: heroCol
                Layout.alignment: Qt.AlignBottom
                spacing: 2
                Text {
                    text: "Process CPU"
                    font.family: Theme.fontSans
                    font.pixelSize: Theme.sizeCaption
                    color: Theme.textMuted
                }
                Text {
                    text: cpuOverlay.controller.cpu_usage
                    font.family: Theme.fontSans
                    font.pixelSize: Theme.sizeHero
                    font.weight: Font.DemiBold
                    font.letterSpacing: -Theme.sizeHero * 0.02
                    color: Theme.textPrimary
                }
                Text {
                    text: "last 60 s · log scale"
                    font.family: Theme.fontMono
                    font.pixelSize: Theme.sizeMono
                    color: Theme.textMuted
                }
            }

            Item { Layout.fillWidth: true }

            // Legend: right-aligned block, wraps to more lines when needed.
            Item {
                id: legend
                readonly property var items: cpuOverlay.chartSeriesList(cpuOverlay.revision)
                Layout.alignment: Qt.AlignBottom
                Layout.preferredWidth: Math.min(legendMeasure.implicitWidth,
                                                Math.max(0, heroRow.width - heroCol.implicitWidth - 2 * heroRow.spacing))
                Layout.preferredHeight: legendFlow.implicitHeight

                // Single-line width, used only to size the block.
                Row {
                    id: legendMeasure
                    visible: false
                    spacing: 14
                    Repeater {
                        model: legend.items
                        delegate: legendChip
                    }
                }

                Flow {
                    id: legendFlow
                    width: parent.width
                    spacing: 14
                    Repeater {
                        model: legend.items
                        delegate: legendChip
                    }
                }
            }
        }

        // Chart card.
        ZbCard {
            Layout.fillWidth: true
            Layout.preferredHeight: 220 + 12 + 8 + 2

            Canvas {
                id: chart
                anchors.fill: parent
                anchors.leftMargin: 16
                anchors.rightMargin: 16
                anchors.topMargin: 12
                anchors.bottomMargin: 8
                antialiasing: true

                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()

                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset()
                    var L = cpuOverlay.plotLeft
                    var R = cpuOverlay.plotRight
                    var T = cpuOverlay.plotTop
                    var B = cpuOverlay.plotBottom
                    if (R <= L || B <= T) return

                    ctx.font = Theme.sizeMono + "px \"" + Theme.fontMono + "\""
                    ctx.lineWidth = 1

                    // Gridlines (log decades); the 0.01 % line is the baseline.
                    ctx.textAlign = "left"
                    ctx.textBaseline = "middle"
                    var gv = cpuOverlay.gridValues
                    for (var g = 0; g < gv.length; g++) {
                        var gy = Math.round(cpuOverlay.yFor(gv[g])) + 0.5
                        ctx.strokeStyle = "" + (g === gv.length - 1 ? Theme.chartBaseline : Theme.chartGrid)
                        ctx.beginPath()
                        ctx.moveTo(L, gy)
                        ctx.lineTo(R, gy)
                        ctx.stroke()
                        ctx.fillStyle = "" + Theme.textMuted
                        ctx.fillText(gv[g] + "%", 0, gy)
                    }

                    // x-axis: -60 s … now, every 15 s.
                    ctx.textBaseline = "bottom"
                    for (var k = 0; k <= 4; k++) {
                        var tx = L + k / 4 * (R - L)
                        ctx.textAlign = k === 0 ? "left" : k === 4 ? "right" : "center"
                        ctx.fillText(k === 4 ? "now" : (-60 + 15 * k) + " s", tx, height)
                    }

                    // Series: 2 px, round joins, clipped to the plot.
                    ctx.save()
                    ctx.beginPath()
                    ctx.rect(L, 0, R - L, height)
                    ctx.clip()
                    ctx.lineWidth = 2
                    ctx.lineJoin = "round"
                    ctx.lineCap = "round"
                    var list = cpuOverlay.chartSeriesList(cpuOverlay.revision)
                    for (var i = 0; i < list.length; i++) {
                        var s = list[i]
                        var pts = s.points
                        ctx.strokeStyle = "" + cpuOverlay.seriesColor(s)
                        ctx.beginPath()
                        var prevT = -1
                        for (var j = 0; j < pts.length; j++) {
                            var px = cpuOverlay.xFor(pts[j].t)
                            var py = cpuOverlay.yFor(pts[j].v)
                            if (prevT < 0 || pts[j].t - prevT > cpuOverlay.gapMs)
                                ctx.moveTo(px, py)
                            else
                                ctx.lineTo(px, py)
                            prevT = pts[j].t
                        }
                        ctx.stroke()
                    }
                    ctx.restore()

                    // Hover: dashed crosshair + markers with a surface ring.
                    if (cpuOverlay.hoverActive) {
                        var hx = Math.round(cpuOverlay.hoverX) + 0.5
                        ctx.lineWidth = 1
                        ctx.strokeStyle = "" + Theme.textMuted
                        ctx.setLineDash([2, 3])
                        ctx.beginPath()
                        ctx.moveTo(hx, T)
                        ctx.lineTo(hx, B)
                        ctx.stroke()
                        ctx.setLineDash([])
                        ctx.lineWidth = 2
                        ctx.strokeStyle = "" + Theme.bgSurface
                        var rows = cpuOverlay.hoverRows
                        for (var r = rows.length - 1; r >= 0; r--) {
                            ctx.fillStyle = rows[r].color
                            ctx.beginPath()
                            ctx.arc(cpuOverlay.hoverX, rows[r].y, 4, 0, Math.PI * 2)
                            ctx.fill()
                            ctx.stroke()
                        }
                    }
                }

                MouseArea {
                    id: chartMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.NoButton
                    onPositionChanged: mouse => cpuOverlay.updateHover(mouse.x)
                    onExited: {
                        cpuOverlay.hoverMouseX = -1
                        cpuOverlay.hoverActive = false
                        chart.requestPaint()
                    }
                }

                // Tooltip (bible tooltip style; rows = series dot + name + value).
                Item {
                    id: tip
                    visible: cpuOverlay.hoverActive
                    width: tipBox.width
                    height: tipBox.height
                    x: {
                        var right = cpuOverlay.hoverX + 12
                        return right + width <= chart.width ? right
                             : Math.max(0, cpuOverlay.hoverX - 12 - width)
                    }
                    y: Math.max(0, Math.min(chartMouse.mouseY - height / 2, chart.height - height))

                    // Soft menu shadow (no effects module on Qt 6.4).
                    Repeater {
                        model: 3
                        Rectangle {
                            required property int index
                            anchors.fill: parent
                            anchors.margins: -(index * 3 + 2)
                            anchors.topMargin: -(index * 3 + 2) + Theme.shadowMenuY * 0.4
                            radius: Theme.radiusControl + index * 3 + 2
                            color: Theme.shadowMenu
                            opacity: 0.16 - index * 0.04
                        }
                    }

                    Rectangle {
                        id: tipBox
                        width: tipCol.implicitWidth + 20
                        height: tipCol.implicitHeight + 16
                        radius: Theme.radiusControl
                        color: Theme.dark ? Theme.bgHover : Theme.bgMenu
                        border.width: 1
                        border.color: Theme.borderStrong

                        Column {
                            id: tipCol
                            x: 10
                            y: 8
                            spacing: 5
                            Text {
                                text: cpuOverlay.hoverLabel
                                font.family: Theme.fontMono
                                font.pixelSize: Theme.sizeMono
                                color: Theme.textMuted
                            }
                            Repeater {
                                model: cpuOverlay.hoverRows
                                delegate: Row {
                                    required property var modelData
                                    spacing: 6
                                    Rectangle {
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 7
                                        height: 7
                                        radius: 3.5
                                        color: modelData.color
                                    }
                                    Text {
                                        width: Math.max(implicitWidth, 120)
                                        text: modelData.name
                                        font.family: Theme.fontSans
                                        font.pixelSize: Theme.sizeMono
                                        color: Theme.textPrimary
                                    }
                                    Text {
                                        width: 48
                                        horizontalAlignment: Text.AlignRight
                                        text: modelData.text
                                        font.family: Theme.fontMono
                                        font.pixelSize: Theme.sizeMono
                                        color: Theme.textPrimary
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Plugin DSP load.
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 20
            Layout.bottomMargin: 8
            spacing: 12
            ZbSectionLabel {
                text: "Plugin DSP load"
                Layout.fillWidth: true
            }
            Text {
                textFormat: Text.StyledText
                text: "Total <font color=\"" + Theme.textPrimary + "\">"
                      + cpuOverlay.formatPct(cpuOverlay.totalPluginDsp) + "</font> DSP"
                font.family: Theme.fontMono
                font.pixelSize: Theme.sizeMono
                color: Theme.textMuted
            }
        }

        ZbCard {
            id: tableCard
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true

            // Column widths shared by header and rows.
            readonly property int colDsp: 72
            readonly property int colTime: 64
            readonly property int colShare: 150
            readonly property int colGap: 12
            readonly property int padX: Theme.cardPadding

            // Header row 30, mono 10 uppercase.
            Item {
                id: tableHeader
                x: 1
                y: 1
                width: parent.width - 2
                height: Theme.rowTableHeader

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: tableCard.padX
                    anchors.rightMargin: tableCard.padX + (tableBar.visible ? Theme.scrollBarWidth + 2 * Theme.scrollGap : 0)
                    spacing: tableCard.colGap
                    HeaderCell { text: "Plugin"; Layout.fillWidth: true; Layout.preferredWidth: 0; horizontalAlignment: Text.AlignLeft }
                    HeaderCell { text: "DSP %"; Layout.preferredWidth: tableCard.colDsp }
                    HeaderCell { text: "Avg"; Layout.preferredWidth: tableCard.colTime }
                    HeaderCell { text: "Last"; Layout.preferredWidth: tableCard.colTime }
                    HeaderCell {
                        text: "Share of total"
                        Layout.preferredWidth: tableCard.colShare
                        horizontalAlignment: Text.AlignLeft
                        leftPadding: 10
                    }
                }
                ZbDivider {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                }
            }

            // Scroll area: the bar sits beside the rows, inside the card.
            Item {
                anchors.top: tableHeader.bottom
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: 1
                anchors.topMargin: 0

                ListView {
                    id: pluginCpuList
                    ZbWheelScroll { flickable: pluginCpuList }
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.right: tableBar.visible ? tableBar.left : parent.right
                    anchors.rightMargin: tableBar.visible ? Theme.scrollGap : 0
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    model: cpuOverlay.pluginData
                    ScrollBar.vertical: tableBar

                    delegate: Item {
                        id: row
                        required property int index
                        required property var modelData
                        readonly property var plugin: modelData || {}
                        readonly property real dsp: plugin.dspPercent || 0
                        readonly property real share: cpuOverlay.totalPluginDsp > 0
                                                      ? dsp / cpuOverlay.totalPluginDsp : 0
                        readonly property color seriesColour: plugin.slot !== undefined && plugin.slot >= 0
                                                              ? Theme.chartSeries[plugin.slot] : Theme.textMuted
                        readonly property bool hasWorker: (plugin.workerPercent || 0) > 0

                        width: ListView.view.width
                        height: Theme.rowTable

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: tableCard.padX
                            anchors.rightMargin: tableCard.padX
                            spacing: tableCard.colGap

                            // Plugin: series dot + name (+ async worker line).
                            RowLayout {
                                Layout.fillWidth: true
                                Layout.preferredWidth: 0
                                spacing: 8
                                Rectangle {
                                    width: 8
                                    height: 8
                                    radius: 4
                                    color: row.seriesColour
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 0
                                    Text {
                                        Layout.fillWidth: true
                                        text: row.plugin.name || ""
                                        elide: Text.ElideRight
                                        font.family: Theme.fontSans
                                        font.pixelSize: Theme.sizeBody
                                        font.weight: Font.Medium
                                        color: Theme.textPrimary
                                    }
                                    Text {
                                        visible: row.hasWorker
                                        Layout.fillWidth: true
                                        text: "worker " + cpuOverlay.formatPct(row.plugin.workerPercent || 0)
                                              + " · avg " + cpuOverlay.formatUs(row.plugin.workerAvgUs || 0)
                                        elide: Text.ElideRight
                                        font.family: Theme.fontMono
                                        font.pixelSize: Theme.sizeBadge
                                        color: Theme.textMuted
                                    }
                                }
                            }

                            NumberCell {
                                Layout.preferredWidth: tableCard.colDsp
                                text: cpuOverlay.formatPct(row.dsp)
                                color: Theme.textPrimary
                            }
                            NumberCell {
                                Layout.preferredWidth: tableCard.colTime
                                text: cpuOverlay.formatUs(row.plugin.avgUs || 0)
                            }
                            NumberCell {
                                Layout.preferredWidth: tableCard.colTime
                                text: cpuOverlay.formatUs(row.plugin.lastUs || 0)
                            }

                            // Share of total: 6 px bar + mono %.
                            RowLayout {
                                Layout.fillWidth: false
                                Layout.preferredWidth: tableCard.colShare - 10
                                Layout.leftMargin: 10
                                spacing: 8
                                Rectangle {
                                    Layout.fillWidth: true
                                    height: 6
                                    radius: 3
                                    color: Theme.bgSubtle
                                    Rectangle {
                                        width: Math.max(row.share > 0 ? height : 0,
                                                        Math.min(1, row.share) * parent.width)
                                        height: parent.height
                                        radius: 3
                                        color: row.seriesColour
                                    }
                                }
                                Text {
                                    Layout.preferredWidth: 32
                                    horizontalAlignment: Text.AlignRight
                                    text: Math.round(row.share * 100) + "%"
                                    font.family: Theme.fontMono
                                    font.pixelSize: Theme.sizeMono
                                    color: Theme.textMuted
                                }
                            }
                        }

                        ZbDivider {
                            visible: row.index < pluginCpuList.count - 1
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                        }
                    }
                }

                ZbScrollBar {
                    id: tableBar
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.right: parent.right
                    anchors.topMargin: Theme.scrollGap
                    anchors.bottomMargin: Theme.scrollGap
                    anchors.rightMargin: Theme.scrollGap
                }

                Text {
                    anchors.centerIn: parent
                    visible: cpuOverlay.pluginData.length === 0
                    text: "No active plugins"
                    font.family: Theme.fontSans
                    font.pixelSize: Theme.sizeCaption
                    color: Theme.textMuted
                }
            }
        }
    }

    // ─── Local building blocks ──────────────────────────────────────────
    Component {
        id: legendChip
        Row {
            required property var modelData
            spacing: 6
            height: 18
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 8
                height: 8
                radius: 4
                color: cpuOverlay.seriesColor(parent.modelData)
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: parent.modelData.name
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeCaption
                color: Theme.textSecondary
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: parent.modelData.live
                      ? cpuOverlay.formatValue(parent.modelData, parent.modelData.latest) : "–"
                font.family: Theme.fontMono
                font.pixelSize: Theme.sizeMono
                color: Theme.textMuted
            }
        }
    }

    component HeaderCell: Text {
        horizontalAlignment: Text.AlignRight
        verticalAlignment: Text.AlignVCenter
        font.family: Theme.fontMono
        font.pixelSize: Theme.sizeTableHeader
        font.capitalization: Font.AllUppercase
        font.letterSpacing: Theme.sizeTableHeader * 0.04
        color: Theme.textMuted
        elide: Text.ElideRight
    }

    // Table numbers: mono 12 (the caption size), right-aligned.
    component NumberCell: Text {
        horizontalAlignment: Text.AlignRight
        font.family: Theme.fontMono
        font.pixelSize: Theme.sizeCaption
        color: Theme.textMuted
    }
}
