import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// About (spec §6.5): centred mark, wordmark and tagline over a key/value card.
ZbWindow {
    id: aboutWindow
    title: "About ZestBay"
    width: 420
    height: 380
    minimumWidth: 360
    minimumHeight: 360
    visible: false

    required property var controller

    readonly property string sourceUrl: "https://github.com/lemonxah/zestbay"
    // No Theme tokens for these yet (spec §3 / §6.5): About wordmark 24/600,
    // info-grid label column 96 px.
    readonly property int wordmarkSize: 24
    readonly property int labelColumnWidth: 96

    function open() {
        visible = true
        raise()
        requestActivate()
    }

    Shortcut {
        sequence: "Escape"
        onActivated: aboutWindow.visible = false
    }

    // One key/value row; hairlines under each column (not under the last row).
    component InfoRow: ColumnLayout {
        id: infoRow
        property string label
        property string value
        property bool mono: false
        property bool link: false
        property bool last: false
        default property alias valueItem: valueSlot.data
        Layout.fillWidth: true
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Theme.grid * 2
            Layout.bottomMargin: Theme.grid * 2
            spacing: Theme.grid * 3

            Text {
                Layout.preferredWidth: aboutWindow.labelColumnWidth
                text: infoRow.label
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeCaption
                color: Theme.textMuted
                elide: Text.ElideRight
            }
            Text {
                Layout.fillWidth: true
                visible: !infoRow.link
                text: infoRow.value
                font.family: infoRow.mono ? Theme.fontMono : Theme.fontSans
                font.pixelSize: Theme.sizeCaption
                color: Theme.textPrimary
                elide: Text.ElideRight
            }
            Item {
                id: valueSlot
                Layout.fillWidth: true
                visible: infoRow.link
                implicitHeight: childrenRect.height
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: !infoRow.last
            spacing: Theme.grid * 3
            ZbDivider { Layout.preferredWidth: aboutWindow.labelColumnWidth }
            ZbDivider { Layout.fillWidth: true }
        }
    }

    ColumnLayout {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: Theme.dialogPadding
        anchors.rightMargin: Theme.dialogPadding
        spacing: 0

        ZbLogo {
            Layout.alignment: Qt.AlignHCenter
            size: 48
        }

        Text {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Theme.grid * 4
            text: "ZestBay"
            font.family: Theme.fontSans
            font.pixelSize: aboutWindow.wordmarkSize
            font.weight: Font.DemiBold
            font.letterSpacing: -aboutWindow.wordmarkSize * 0.02
            color: Theme.textStrong
        }

        Text {
            Layout.fillWidth: true
            Layout.topMargin: Theme.grid * 2
            text: "A PipeWire patchbay and plugin host"
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            color: Theme.textMuted
        }

        ZbCard {
            Layout.fillWidth: true
            Layout.topMargin: Theme.grid * 5
            implicitHeight: infoColumn.implicitHeight + Theme.grid * 2 + 2

            ColumnLayout {
                id: infoColumn
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.leftMargin: Theme.grid * 4
                anchors.rightMargin: Theme.grid * 4
                anchors.topMargin: Theme.grid + 1
                spacing: 0

                InfoRow {
                    label: "Version"
                    value: aboutWindow.controller.get_app_version()
                    mono: true
                }
                InfoRow {
                    label: "Qt"
                    value: aboutWindow.controller.get_qt_version()
                    mono: true
                }
                InfoRow {
                    label: "Author"
                    value: "lemonxah"
                }
                InfoRow {
                    label: "Source"
                    link: true

                    Text {
                        width: parent.width
                        text: "github.com/lemonxah/zestbay"
                        font.family: Theme.fontSans
                        font.pixelSize: Theme.sizeCaption
                        font.underline: linkMouse.containsMouse
                        color: linkMouse.containsMouse ? Theme.accentHover : Theme.accent
                        elide: Text.ElideRight

                        MouseArea {
                            id: linkMouse
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: Qt.openUrlExternally(aboutWindow.sourceUrl)
                        }
                    }
                }
                InfoRow {
                    label: "License"
                    value: "MIT"
                    last: true
                }
            }
        }
    }
}
