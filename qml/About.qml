import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ZbWindow {
    id: aboutWindow
    title: "About ZestBay"
    width: 420
    height: 570
    minimumWidth: 360
    minimumHeight: 550
    visible: false

    required property var controller

    readonly property string sourceUrl: "https://github.com/lemonxah/zestbay"
    readonly property string websiteUrl: "https://zestworx.com/zestbay"
    readonly property string issuesUrl: sourceUrl + "/issues"

    function bugReportUrl() {
        var body = "ZestBay " + controller.get_app_version() + ", Qt " + controller.get_qt_version()
            + "\n\n**What happened?**\n\n\n**What did you expect?**\n\n\n**How can it be reproduced?**\n"
        return issuesUrl + "/new?labels=bug&body=" + encodeURIComponent(body)
    }
    function featureRequestUrl() {
        var body = "**What would you like ZestBay to do?**\n\n\n**Why would it help?**\n"
        return issuesUrl + "/new?labels=enhancement&title=" + encodeURIComponent("Feature request: ") + "&body=" + encodeURIComponent(body)
    }
    readonly property int labelColumnWidth: 96

    function open() {
        visible = true
        raise()
        requestActivate()
    }

    Shortcut {
        sequence: "Escape"
        enabled: aboutWindow.visible
        onActivated: aboutWindow.visible = false
    }

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

    component LinkText: Text {
        id: linkText
        property string url
        font.family: Theme.fontSans
        font.pixelSize: Theme.sizeCaption
        font.underline: linkArea.containsMouse
        color: linkArea.containsMouse ? Theme.accentHover : Theme.accent
        elide: Text.ElideRight
        MouseArea {
            id: linkArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: Qt.openUrlExternally(linkText.url)
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
            size: 132
            withName: true
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
                    label: "Website"
                    link: true

                    LinkText {
                        width: parent.width
                        text: "zestworx.com/zestbay"
                        url: aboutWindow.websiteUrl
                    }
                }
                InfoRow {
                    label: "Source"
                    link: true

                    LinkText {
                        width: parent.width
                        text: "github.com/lemonxah/zestbay"
                        url: aboutWindow.sourceUrl
                    }
                }
                InfoRow {
                    label: "License"
                    value: "MIT"
                    last: true
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Theme.grid * 4
            spacing: Theme.grid * 2

            ZbButton {
                Layout.fillWidth: true
                text: "Report an issue"
                onClicked: Qt.openUrlExternally(aboutWindow.bugReportUrl())
                ZbTooltip {
                    visible: parent.hovered
                    text: "Open a new issue on GitHub, with your version filled in"
                }
            }
            ZbButton {
                Layout.fillWidth: true
                text: "Request a feature"
                onClicked: Qt.openUrlExternally(aboutWindow.featureRequestUrl())
                ZbTooltip {
                    visible: parent.hovered
                    text: "Suggest a feature on GitHub"
                }
            }
        }

        LinkText {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Theme.grid * 2
            text: "Browse open issues"
            url: aboutWindow.issuesUrl
        }
    }
}
