use serde::{Deserialize, Serialize};

use crate::midi::types::{MappingMode, MidiCcMapping, MidiCcSource};

pub type ObjectId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaType {
    Audio,
    Video,
    Midi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeType {
    Sink,
    Source,
    StreamOutput,
    StreamInput,
    Duplex,
    Plugin,
}

impl NodeType {
    pub fn has_outputs(&self) -> bool {
        matches!(
            self,
            NodeType::Source | NodeType::StreamOutput | NodeType::Duplex | NodeType::Plugin
        )
    }

    pub fn has_inputs(&self) -> bool {
        matches!(
            self,
            NodeType::Sink | NodeType::StreamInput | NodeType::Duplex | NodeType::Plugin
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PortDirection {
    Input,
    Output,
}

/// Which edge of a subgraph a ZestBay boundary node belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubgraphRole {
    /// Feeds the subgraph: external inputs, internal outputs.
    In,
    /// Leaves the subgraph: internal inputs, external outputs.
    Out,
}

/// Which side of the boundary a node carries. A plain pass-through node has
/// both; a subgraph shown as a device splits into a device node (external)
/// and a loopback stream (internal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubgraphSide {
    Both,
    External,
    Internal,
}

/// Marks a node ZestBay created for a subgraph boundary
/// (`zestbay.subgraph.*` properties).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubgraphTag {
    pub id: String,
    pub role: SubgraphRole,
    pub side: SubgraphSide,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: ObjectId,
    pub name: String,
    pub description: String,
    pub media_type: Option<MediaType>,
    pub node_type: Option<NodeType>,
    pub is_virtual: bool,
    pub is_jack: bool,
    pub is_bridge: bool,
    pub ready: bool,
    /// Set on subgraph boundary nodes.
    pub subgraph: Option<SubgraphTag>,
    /// Set on both halves of a virtual device ZestBay created
    /// (`zestbay.vdev.id`).
    pub vdev_id: Option<String>,
}

impl Node {
    pub fn display_name(&self) -> &str {
        if !self.description.is_empty() {
            &self.description
        } else if !self.name.is_empty() {
            &self.name
        } else {
            "Unknown"
        }
    }
}

#[derive(Debug, Clone)]
pub struct Port {
    pub id: ObjectId,
    pub node_id: ObjectId,
    pub name: String,
    pub direction: PortDirection,
    pub media_type: Option<MediaType>,
    pub channel: Option<String>,
    pub physical_index: Option<u32>,
    pub port_group: Option<String>,
    pub port_alias: Option<String>,
}

impl Port {
    pub fn display_name(&self) -> &str {
        if let Some(ref channel) = self.channel {
            channel
        } else if !self.name.is_empty() {
            &self.name
        } else {
            "port"
        }
    }
}

#[derive(Debug, Clone)]
pub struct Link {
    pub id: ObjectId,
    pub output_node_id: ObjectId,
    pub output_port_id: ObjectId,
    pub input_node_id: ObjectId,
    pub input_port_id: ObjectId,
    pub active: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PwEvent {
    NodeChanged(Node),
    NodeRemoved(ObjectId),
    PortChanged(Port),
    PortRemoved {
        port_id: ObjectId,
        node_id: ObjectId,
    },
    LinkChanged(Link),
    LinkRemoved(ObjectId),
    Error(String),
    BatchComplete,
    Plugin(PluginEvent),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PwCommand {
    Connect {
        output_port_id: ObjectId,
        input_port_id: ObjectId,
    },
    Disconnect {
        link_id: ObjectId,
    },
    AddPlugin {
        plugin_uri: String,
        instance_id: u64,
        display_name: String,
        /// "LV2", "CLAP", or "VST3"
        format: String,
        lv2_state: Vec<crate::lv2::state::StateEntry>,
        /// Opaque CLAP/VST3 state blob to restore after instantiation.
        plugin_state: Option<Vec<u8>>,
        /// Persistent instance identity (survives restarts, unlike instance_id).
        stable_id: String,
    },
    RemovePlugin {
        instance_id: u64,
    },
    SetPluginParameter {
        instance_id: u64,
        port_index: usize,
        value: f32,
    },
    SetPluginBypass {
        instance_id: u64,
        bypassed: bool,
    },
    OpenPluginUI {
        instance_id: u64,
    },
    ClosePluginUI {
        instance_id: u64,
    },
    StartMidiLearn {
        instance_id: u64,
        port_index: usize,
        label: String,
        mode: MappingMode,
    },
    CancelMidiLearn,
    AddMidiMapping(MidiCcMapping),
    RemoveMidiMapping(MidiCcSource),
    RemoveMidiMappingsForPlugin {
        instance_id: u64,
    },
    /// Not sent by anything at present (unplugged devices keep their
    /// mappings dormant in the UI instead); kept for API completeness.
    RemoveMidiMappingsForDevice {
        device_name: String,
    },
    /// Ask the PipeWire thread to snapshot the state of every live plugin.
    /// Replies with one `PluginStateSaved` per plugin, then `PluginStatesDone`.
    RequestPluginStates,
    /// Create (or re-create, when the spec changed) a node ZestBay owns —
    /// a subgraph boundary or a virtual device — under `key`; `None`
    /// removes it.
    SetOwnedNode {
        key: String,
        spec: Option<OwnedNodeSpec>,
    },
}

/// A node ZestBay creates in PipeWire itself (not a plugin).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnedNodeSpec {
    /// A `pw_filter` that copies each input channel (and MIDI) to the
    /// matching output: a subgraph boundary that isn't a device.
    Pass(PassSpec),
    /// An in-process `libpipewire-module-loopback`: a capture stream and a
    /// playback stream joined inside PipeWire. One side is a device
    /// (`media.class` Audio/Sink or Audio/Source) when its props say so.
    Loopback(LoopbackSpec),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassSpec {
    pub node_name: String,
    pub description: String,
    /// Channel positions (FL, FR, MONO, AUX0…); one input and one output
    /// port per channel.
    pub channels: Vec<String>,
    pub midi: bool,
    /// Extra node properties (tags).
    pub props: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackSpec {
    pub description: String,
    pub channels: Vec<String>,
    pub capture_props: Vec<(String, String)>,
    pub playback_props: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub enum PluginEvent {
    PluginAdded {
        instance_id: u64,
        pw_node_id: ObjectId,
        display_name: String,
    },
    PluginRemoved {
        instance_id: u64,
    },
    ParameterChanged {
        instance_id: u64,
        port_index: usize,
        value: f32,
    },
    PluginUiOpened {
        instance_id: u64,
    },
    PluginUiClosed {
        instance_id: u64,
    },
    PluginError {
        instance_id: Option<u64>,
        message: String,
        fatal: bool,
    },
    /// The plugin could not be loaded (probe timeout/failure, failed
    /// instantiation or reload) and is skipped for this session.  It is not
    /// running, but its saved configuration, state and mappings are kept.
    /// Followed by a non-fatal `PluginError` explaining why.
    PluginSkipped {
        instance_id: u64,
    },
    MidiLearnStarted {
        instance_id: u64,
        port_index: usize,
    },
    MidiLearnCancelled,
    MidiMappingAdded(MidiCcMapping),
    MidiMappingRemoved(MidiCcSource),
    MidiMappingConflict {
        source: MidiCcSource,
        existing_label: String,
    },
    MidiCcReceived {
        device_name: String,
        channel: u8,
        cc: u8,
        message_type: crate::midi::MidiMessageType,
    },
    PluginStateSaved {
        instance_id: u64,
        /// LV2 state entries (empty for CLAP/VST3).
        lv2_state: Vec<crate::lv2::state::StateEntry>,
        /// CLAP/VST3 opaque state blob (None for LV2).
        blob: Option<Vec<u8>>,
    },
    /// Sent after all `PluginStateSaved` replies to a `RequestPluginStates`.
    PluginStatesDone,
}

/// Backward-compatible alias for `PluginEvent`.
pub type Lv2Event = PluginEvent;
