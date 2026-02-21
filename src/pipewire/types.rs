use serde::{Deserialize, Serialize};

/// Unique identifier for PipeWire objects
pub type ObjectId = u32;

/// Media type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaType {
    Audio,
    Video,
    Midi,
}

/// Node type based on media class
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeType {
    /// Audio/Video sink (playback device, speaker, headphones)
    Sink,
    /// Audio/Video source (capture device, microphone)
    Source,
    /// Application that produces audio/video
    StreamOutput,
    /// Application that consumes audio/video
    StreamInput,
    /// Duplex device (both input and output)
    Duplex,
    /// LV2 plugin hosted by ZestBay (appears as a filter node)
    Lv2Plugin,
}

impl NodeType {
    /// Returns true if this node type has output ports (produces data)
    pub fn has_outputs(&self) -> bool {
        matches!(
            self,
            NodeType::Source | NodeType::StreamOutput | NodeType::Duplex | NodeType::Lv2Plugin
        )
    }

    /// Returns true if this node type has input ports (consumes data)
    pub fn has_inputs(&self) -> bool {
        matches!(
            self,
            NodeType::Sink | NodeType::StreamInput | NodeType::Duplex | NodeType::Lv2Plugin
        )
    }
}

/// Port direction (Input sorts before Output for display)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PortDirection {
    Input,
    Output,
}

/// A PipeWire node (device or application)
#[derive(Debug, Clone)]
pub struct Node {
    pub id: ObjectId,
    pub name: String,
    pub description: String,
    pub media_type: Option<MediaType>,
    pub node_type: Option<NodeType>,
    /// Application name (for stream nodes)
    /// True when the node has received its first info event and is ready
    pub ready: bool,
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

/// A PipeWire port on a node
#[derive(Debug, Clone)]
pub struct Port {
    pub id: ObjectId,
    pub node_id: ObjectId,
    pub name: String,
    pub direction: PortDirection,
    pub media_type: Option<MediaType>,
    /// Channel name (e.g., "FL", "FR", "mono")
    pub channel: Option<String>,
    /// Physical port index for ordering
    pub physical_index: Option<u32>,
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

/// A PipeWire link between two ports
#[derive(Debug, Clone)]
pub struct Link {
    pub id: ObjectId,
    pub output_node_id: ObjectId,
    pub output_port_id: ObjectId,
    pub input_node_id: ObjectId,
    pub input_port_id: ObjectId,
    pub active: bool,
}

/// Messages from the PipeWire thread to the UI
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PwEvent {
    /// A node was added or updated
    NodeChanged(Node),
    /// A node was removed
    NodeRemoved(ObjectId),
    /// A port was added or updated
    PortChanged(Port),
    /// A port was removed
    PortRemoved {
        port_id: ObjectId,
        node_id: ObjectId,
    },
    /// A link was added or updated
    LinkChanged(Link),
    /// A link was removed
    LinkRemoved(ObjectId),
    /// PipeWire connection error
    Error(String),
    /// Batch update complete (signals UI can refresh)
    BatchComplete,
    /// LV2 plugin event
    Lv2(Lv2Event),
}

/// Commands from the UI to the PipeWire thread
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PwCommand {
    /// Create a link between two ports
    Connect {
        output_port_id: ObjectId,
        input_port_id: ObjectId,
    },
    /// Destroy a link
    Disconnect { link_id: ObjectId },
    /// Add an LV2 plugin instance as a PipeWire filter node
    AddPlugin {
        /// Plugin URI to instantiate
        plugin_uri: String,
        /// Instance ID assigned by the LV2 manager
        instance_id: u64,
        /// Display name for the filter node (may include auto-numbering like " #2")
        display_name: String,
    },
    /// Remove an LV2 plugin instance
    RemovePlugin {
        /// Instance ID to remove
        instance_id: u64,
    },
    /// Set a parameter on an LV2 plugin instance
    SetPluginParameter {
        /// Instance ID
        instance_id: u64,
        /// Port index within the plugin
        port_index: usize,
        /// New value
        value: f32,
    },
    /// Toggle bypass on an LV2 plugin instance
    SetPluginBypass { instance_id: u64, bypassed: bool },
    /// Open the native LV2 plugin UI
    OpenPluginUI { instance_id: u64 },
    /// Close the native LV2 plugin UI (if open)
    ClosePluginUI { instance_id: u64 },
}

/// Events specific to LV2 plugin hosting (PipeWire thread -> UI)
#[derive(Debug, Clone)]
pub enum Lv2Event {
    /// A plugin instance was successfully created
    PluginAdded {
        instance_id: u64,
        pw_node_id: ObjectId,
        display_name: String,
    },
    /// A plugin instance was removed
    PluginRemoved { instance_id: u64 },
    /// A plugin parameter was changed (e.g. by automation)
    ParameterChanged {
        instance_id: u64,
        port_index: usize,
        value: f32,
    },
    /// A plugin's native UI window was opened
    PluginUiOpened { instance_id: u64 },
    /// A plugin's native UI window was closed
    PluginUiClosed { instance_id: u64 },
    /// Error creating/managing a plugin
    PluginError {
        instance_id: Option<u64>,
        message: String,
    },
}
