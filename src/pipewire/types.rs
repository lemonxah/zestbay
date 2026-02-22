use serde::{Deserialize, Serialize};

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
    Lv2Plugin,
}

impl NodeType {
    pub fn has_outputs(&self) -> bool {
        matches!(
            self,
            NodeType::Source | NodeType::StreamOutput | NodeType::Duplex | NodeType::Lv2Plugin
        )
    }

    pub fn has_inputs(&self) -> bool {
        matches!(
            self,
            NodeType::Sink | NodeType::StreamInput | NodeType::Duplex | NodeType::Lv2Plugin
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PortDirection {
    Input,
    Output,
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

#[derive(Debug, Clone)]
pub struct Port {
    pub id: ObjectId,
    pub node_id: ObjectId,
    pub name: String,
    pub direction: PortDirection,
    pub media_type: Option<MediaType>,
    pub channel: Option<String>,
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
    Lv2(Lv2Event),
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
}

#[derive(Debug, Clone)]
pub enum Lv2Event {
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
    },
}
