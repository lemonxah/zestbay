import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Create or edit a subgraph (design/GROUPS.md, bible §6.6): name, inputs and
// outputs, MIDI pass-through, and whether each edge is offered to apps as a
// device. MIDI only passes through an edge that isn't a device.
ZbDialog {
    id: dialog

    // "create" (from selected plugins) or "edit" (an existing subgraph).
    property string mode: "create"
    property var nodeIds: []
    property string subgraphId: ""
    property int pluginCount: 0

    signal submitted(var config)

    title: mode === "create" ? "Group into subgraph" : "Edit subgraph"
    primaryText: mode === "create" ? "Create subgraph" : "Save"
    secondaryText: "Cancel"
    width: Math.min(parent ? parent.width - 2 * Theme.dialogPadding : 480, 480)
    primaryEnabled: nameField.text.trim().length > 0
                    && (inputs.value > 0 || outputs.value > 0 || midiIn.checked || midiOut.checked)

    // Fill the fields: `cfg` = {name, inputs, outputs, midiIn, midiOut, asSink, asSource}.
    function load(cfg) {
        nameField.text = cfg.name || ""
        inputs.value = cfg.inputs !== undefined ? cfg.inputs : 2
        outputs.value = cfg.outputs !== undefined ? cfg.outputs : 2
        midiIn.checked = !!cfg.midiIn
        midiOut.checked = !!cfg.midiOut
        asSink.checked = !!cfg.asSink
        asSource.checked = !!cfg.asSource
    }

    onOpened: {
        nameField.forceActiveFocus()
        nameField.selectAll()
    }
    onAccepted: submitted({
        name: nameField.text.trim(),
        inputs: inputs.value,
        outputs: outputs.value,
        midiIn: midiIn.checked && !asSink.checked,
        midiOut: midiOut.checked && !asSource.checked,
        asSink: asSink.checked,
        asSource: asSource.checked
    })

    component Caption: Text {
        font.family: Theme.fontSans
        font.pixelSize: Theme.sizeCaption
        color: Theme.textMuted
        wrapMode: Text.WordWrap
    }

    // A count with − / + buttons.
    component Stepper: RowLayout {
        id: stepper
        property string label: ""
        property int value: 0
        property int from: 0
        property int to: 16
        spacing: 6
        Text {
            text: stepper.label
            Layout.fillWidth: true
            font.family: Theme.fontSans
            font.pixelSize: Theme.sizeBody
            color: Theme.textPrimary
        }
        ZbButton {
            iconName: "minus"
            compact: true
            implicitWidth: 26
            enabled: stepper.value > stepper.from
            onClicked: stepper.value = Math.max(stepper.from, stepper.value - 1)
        }
        Text {
            Layout.preferredWidth: 28
            horizontalAlignment: Text.AlignHCenter
            text: stepper.value
            font.family: Theme.fontMono
            font.pixelSize: Theme.sizeValue
            color: Theme.textPrimary
        }
        ZbButton {
            iconName: "plus"
            compact: true
            implicitWidth: 26
            enabled: stepper.value < stepper.to
            onClicked: stepper.value = Math.min(stepper.to, stepper.value + 1)
        }
    }

    ColumnLayout {
        width: parent ? parent.width : 400
        spacing: 8

        Caption {
            Layout.fillWidth: true
            visible: dialog.mode === "create"
            text: dialog.pluginCount + (dialog.pluginCount === 1 ? " plugin" : " plugins")
                  + " move into the subgraph. Wires from outside are rerouted through its inputs and outputs."
        }

        ZbSectionLabel { text: "Name" }
        ZbTextField {
            id: nameField
            Layout.fillWidth: true
            placeholderText: "e.g. Mic Chain"
            onAccepted: if (dialog.primaryEnabled) dialog.accept()
        }

        Item { implicitHeight: 4 }
        ZbSectionLabel { text: "Channels" }
        Stepper { id: inputs; Layout.fillWidth: true; label: "Inputs" }
        Stepper { id: outputs; Layout.fillWidth: true; label: "Outputs" }
        ZbSwitch {
            id: midiIn
            text: "MIDI in"
            enabled: !asSink.checked
        }
        ZbSwitch {
            id: midiOut
            text: "MIDI out"
            enabled: !asSource.checked
        }

        Item { implicitHeight: 4 }
        ZbSectionLabel { text: "Show to apps as a device" }
        ZbSwitch {
            id: asSink
            text: "Output device"
            enabled: inputs.value > 0
        }
        Caption {
            Layout.fillWidth: true
            text: "Apps can play into “" + (nameField.text.trim() || "the subgraph") + " In”, like any speaker."
        }
        ZbSwitch {
            id: asSource
            text: "Input device"
            enabled: outputs.value > 0
        }
        Caption {
            Layout.fillWidth: true
            text: "Apps can record from “" + (nameField.text.trim() || "the subgraph") + " Out”, like a microphone (pick it in Discord)."
        }
    }
}
