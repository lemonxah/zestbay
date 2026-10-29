import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Create or edit a virtual device: an output device (sink) apps play into,
// whose stream ZestBay routes onward, or an input device (source) apps
// record from. Both halves appear on the graph, like loopback devices from
// PipeWire config.
ZbDialog {
    id: dialog

    property string vdevId: ""          // "" = create
    property alias deviceName: nameField.text
    property alias channels: channelStepper.value
    property int kindIndex: 0           // 0 = output (sink), 1 = input (source)

    signal submitted(string name, string kind, int channels)

    title: vdevId === "" ? "Add virtual device" : "Edit virtual device"
    primaryText: vdevId === "" ? "Add device" : "Save"
    secondaryText: "Cancel"
    width: Math.min(parent ? parent.width - 2 * Theme.dialogPadding : 440, 440)
    primaryEnabled: nameField.text.trim().length > 0

    onOpened: {
        kindSeg.currentIndex = dialog.kindIndex
        nameField.forceActiveFocus()
        nameField.selectAll()
    }
    onAccepted: submitted(nameField.text.trim(), kindSeg.currentIndex === 1 ? "source" : "sink", channelStepper.value)

    ColumnLayout {
        width: parent ? parent.width : 380
        spacing: 8

        ZbSectionLabel { text: "Name" }
        ZbTextField {
            id: nameField
            Layout.fillWidth: true
            placeholderText: "e.g. Comms Sink"
            onAccepted: if (dialog.primaryEnabled) dialog.accept()
        }

        Item { implicitHeight: 4 }
        ZbSectionLabel { text: "Type" }
        ZbSegmented {
            id: kindSeg
            model: ["Output device", "Input device"]
            currentIndex: dialog.kindIndex
            enabled: dialog.vdevId === ""
            onActivated: (i) => { currentIndex = i; dialog.kindIndex = i }
        }
        Text {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeCaption
            color: Theme.textMuted
            text: kindSeg.currentIndex === 0
                  ? "A sink apps can play into. Its output goes wherever your rules, the default node or your wires send it."
                  : "A source apps can record from. Wire anything into it on the graph."
        }

        Item { implicitHeight: 4 }
        RowLayout {
            id: channelRow
            Layout.fillWidth: true
            spacing: 6
            Text {
                text: "Channels"
                Layout.fillWidth: true
                font.family: Theme.fontSans
                font.pixelSize: Theme.sizeBody
                color: Theme.textPrimary
            }
            ZbButton {
                iconName: "minus"
                compact: true
                implicitWidth: 26
                enabled: channelStepper.value > 1
                onClicked: channelStepper.value -= 1
            }
            Text {
                id: channelStepper
                property int value: 2
                Layout.preferredWidth: 28
                horizontalAlignment: Text.AlignHCenter
                text: value
                font.family: Theme.fontMono
                font.pixelSize: Theme.sizeValue
                color: Theme.textPrimary
            }
            ZbButton {
                iconName: "plus"
                compact: true
                implicitWidth: 26
                enabled: channelStepper.value < 16
                onClicked: channelStepper.value += 1
            }
        }

        Text {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeCaption
            color: Theme.textMuted
            text: "Virtual devices exist while ZestBay runs (keep it in the tray)."
        }
    }
}
