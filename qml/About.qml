import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: aboutWindow
    title: "About ZestBay"
    width: 400
    height: 320
    minimumWidth: 360
    minimumHeight: 280
    visible: false

    required property var controller

    function open() {
        visible = true
        raise()
        requestActivate()
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 16

        Label {
            text: "ZestBay"
            font.bold: true
            font.pointSize: 20
            Layout.alignment: Qt.AlignHCenter
        }

        Label {
            text: "A PipeWire patchbay and plugin host"
            font.pointSize: 10
            opacity: 0.7
            Layout.alignment: Qt.AlignHCenter
        }

        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: "#3c3c3c"
        }

        GridLayout {
            Layout.fillWidth: true
            columns: 2
            columnSpacing: 16
            rowSpacing: 8

            Label {
                text: "Version:"
                font.bold: true
                opacity: 0.7
            }
            Label {
                text: controller.get_app_version()
                font.family: "monospace"
            }

            Label {
                text: "Qt Version:"
                font.bold: true
                opacity: 0.7
            }
            Label {
                text: controller.get_qt_version()
                font.family: "monospace"
            }

            Label {
                text: "Author:"
                font.bold: true
                opacity: 0.7
            }
            Label {
                text: "lemonxah"
            }

            Label {
                text: "Source:"
                font.bold: true
                opacity: 0.7
            }
            Label {
                text: "<a href=\"https://github.com/lemonxah/zestbay\">github.com/lemonxah/zestbay</a>"
                textFormat: Text.RichText
                onLinkActivated: function(link) { Qt.openUrlExternally(link) }

                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.NoButton
                    cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
                }
            }

            Label {
                text: "License:"
                font.bold: true
                opacity: 0.7
            }
            Label {
                text: "MIT"
            }
        }

        Item { Layout.fillHeight: true }

        RowLayout {
            Layout.fillWidth: true

            Item { Layout.fillWidth: true }

            Button {
                text: "Close"
                onClicked: aboutWindow.visible = false
            }
        }
    }
}
