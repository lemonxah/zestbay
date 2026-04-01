//! IPC protocol for the UI bridge process.
//!
//! Communication is via stdin/stdout using newline-delimited JSON messages.

use serde::{Deserialize, Serialize};

/// Messages from the host to the bridge.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum HostMessage {
    /// Open a plugin UI.
    Open {
        instance_id: u64,
        plugin_uri: String,
        ui_uri: String,
        ui_type_uri: String,
        bundle_path: String,
        binary_path: String,
        title: String,
        /// Initial control port values: [(port_index, value), ...]
        control_values: Vec<(usize, f32)>,
        /// URID mappings the UI needs: [(uri_string, urid), ...]
        urid_map: Vec<(String, u32)>,
        /// Raw pointer to the LV2 instance handle (for instance-access feature).
        /// Passed as 0 when running out-of-process (not usable).
        lv2_handle: u64,
        sample_rate: f32,
    },
    /// Send a port value update to the UI.
    PortEvent {
        instance_id: u64,
        port_index: usize,
        value: f32,
    },
    /// Close a plugin UI.
    Close {
        instance_id: u64,
    },
    /// Shut down the bridge process.
    Quit,
}

/// Messages from the bridge to the host.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "evt")]
pub enum BridgeMessage {
    /// UI was opened successfully.
    Opened {
        instance_id: u64,
    },
    /// UI failed to open.
    OpenFailed {
        instance_id: u64,
        error: String,
    },
    /// UI was closed (user clicked close button).
    Closed {
        instance_id: u64,
    },
    /// Plugin UI wrote a port value.
    PortWrite {
        instance_id: u64,
        port_index: usize,
        value: f32,
    },
    /// Plugin UI wrote an atom event (for atom ports).
    AtomWrite {
        instance_id: u64,
        port_index: usize,
        /// Base64-encoded atom data.
        data_b64: String,
    },
    /// Bridge is ready.
    Ready,
}
