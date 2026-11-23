use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum HostMessage {
    Open {
        instance_id: u64,
        plugin_uri: String,
        ui_uri: String,
        ui_type_uri: String,
        bundle_path: String,
        binary_path: String,
        title: String,
        control_values: Vec<(usize, f32)>,
        urid_map: Vec<(String, u32)>,
        lv2_handle: u64,
        sample_rate: f32,
    },
    PortEvent {
        instance_id: u64,
        port_index: usize,
        value: f32,
    },
    Close {
        instance_id: u64,
    },
    Quit,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "evt")]
pub enum BridgeMessage {
    Opened {
        instance_id: u64,
    },
    OpenFailed {
        instance_id: u64,
        error: String,
    },
    Closed {
        instance_id: u64,
    },
    PortWrite {
        instance_id: u64,
        port_index: usize,
        value: f32,
    },
    AtomWrite {
        instance_id: u64,
        port_index: usize,
        data_b64: String,
    },
    Ready,
}
