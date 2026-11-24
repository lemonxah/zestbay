import QtQuick
import QtQuick.Controls

Item {
    required property var graph
    anchors.fill: parent

    Item {
        id: meterLayer
        anchors.fill: parent
        clip: true

        Repeater {
            model: graph.meterNodeIds
            delegate: Item {
                id: meterCard
                required property var modelData
                readonly property var frame: graph.meterFrames[modelData] || null
                readonly property var levels: graph.meterLevels[modelData] || []
                visible: frame !== null
                x: frame ? frame.sx : 0
                y: frame ? frame.sy : 0
                width: frame ? frame.w : 0
                height: frame ? frame.h : 0
                scale: frame ? frame.zoom : 1
                transformOrigin: Item.TopLeft
                opacity: frame ? frame.alpha : 1

                Repeater {
                    model: meterCard.frame ? meterCard.frame.channels : 0
                    delegate: Item {
                        id: bar
                        required property int index
                        readonly property var rect: graph.meterBarRect(meterCard.width, meterCard.frame.rows[index] !== undefined ? meterCard.frame.rows[index] : index)
                        readonly property real frac: graph.levelFraction(meterCard.levels[index] || 0)
                        x: rect.x
                        y: rect.y
                        width: rect.w
                        height: rect.h
                        clip: true

                        Rectangle {
                            width: bar.width * Math.min(bar.frac, graph.meterHotFrac)
                            height: bar.height
                            radius: bar.height / 2
                            color: Theme.meterLow
                        }
                        Rectangle {
                            x: bar.width * graph.meterHotFrac
                            width: bar.width * Math.max(0, Math.min(bar.frac, graph.meterClipFrac) - graph.meterHotFrac)
                            height: bar.height
                            color: Theme.meterHot
                        }
                        Rectangle {
                            x: bar.width * graph.meterClipFrac
                            width: bar.width * Math.max(0, bar.frac - graph.meterClipFrac)
                            height: bar.height
                            color: Theme.meterClip
                        }
                    }
                }
            }
        }
    }

    Item {
        id: dockBarLayer
        anchors.fill: parent
        clip: true

        Repeater {
            model: graph.dockBarIds
            delegate: Item {
                id: dockBar
                required property var modelData
                readonly property var frame: graph.dockBarFrames[modelData] || null
                readonly property real level: {
                    var lv = frame ? graph.meterLevels[frame.meterId] : null;
                    return lv && frame ? (lv[frame.channel] || 0) : 0;
                }
                readonly property real frac: graph.levelFraction(level)
                readonly property real barW: graph.dockBarW
                readonly property var rect: frame ? graph.dockBarRect(frame.cx, frame.cy, frame.output) : null
                visible: frame !== null
                x: rect ? graph.panX + rect.x * graph.zoom : 0
                y: rect ? graph.panY + rect.y * graph.zoom : 0
                width: graph.dockBarW
                height: graph.dockBarH
                scale: graph.zoom
                transformOrigin: Item.TopLeft
                clip: true

                Rectangle {
                    anchors.fill: parent
                    radius: height / 2
                    color: Theme.meterTrack
                }
                Item {
                    width: parent.width
                    height: parent.height
                    Rectangle {
                        width: dockBar.barW * Math.min(dockBar.frac, graph.meterHotFrac)
                        height: parent.height
                        radius: height / 2
                        color: Theme.meterLow
                    }
                    Rectangle {
                        x: dockBar.barW * graph.meterHotFrac
                        width: dockBar.barW * Math.max(0, Math.min(dockBar.frac, graph.meterClipFrac) - graph.meterHotFrac)
                        height: parent.height
                        color: Theme.meterHot
                    }
                    Rectangle {
                        x: dockBar.barW * graph.meterClipFrac
                        width: dockBar.barW * Math.max(0, dockBar.frac - graph.meterClipFrac)
                        height: parent.height
                        color: Theme.meterClip
                    }
                }
            }
        }
    }}
