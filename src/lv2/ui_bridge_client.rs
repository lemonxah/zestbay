//! Host-side client for the UI bridge process.
//!
//! Spawns `zestbay-ui-bridge` as a child process and communicates
//! via stdin/stdout JSON messages.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::lv2::urid::UridMapper;
use crate::pipewire::{PluginEvent, PwCommand, PwEvent};
use crate::ui_bridge::protocol::{BridgeMessage, HostMessage};

pub struct UiBridgeClient {
    child: Child,
    /// Sender to write messages to the bridge's stdin.
    stdin_tx: Sender<String>,
    /// Opened instance IDs.
    open_instances: Arc<Mutex<std::collections::HashSet<u64>>>,
}

impl UiBridgeClient {
    pub fn spawn(
        event_tx: Sender<PwEvent>,
        cmd_tx: Sender<PwCommand>,
    ) -> Result<Self, String> {
        // Find the bridge binary: check next to our executable first,
        // then the system install location, then PATH.
        let candidates = [
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("zestbay-ui-bridge"))),
            Some(std::path::PathBuf::from("/usr/lib/zestbay/zestbay-ui-bridge")),
            Some(std::path::PathBuf::from("/usr/local/lib/zestbay/zestbay-ui-bridge")),
        ];

        let bridge_path = candidates
            .into_iter()
            .flatten()
            .find(|p| p.exists())
            .unwrap_or_else(|| std::path::PathBuf::from("zestbay-ui-bridge"));

        if !bridge_path.exists() {
            return Err(format!("UI bridge binary not found (searched next to exe and /usr/lib/zestbay/)"));
        }

        log::info!("UI bridge binary: {:?}", bridge_path);

        let mut child = Command::new(&bridge_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // bridge errors go to our stderr
            // Force Mesa GLX client library — avoids NVIDIA/Mesa vendor
            // mismatch on Wayland/XWayland where the server is Mesa but
            // the host process loaded NVIDIA's EGL.
            .env("__GLX_VENDOR_LIBRARY_NAME", "mesa")
            .spawn()
            .map_err(|e| format!("Failed to spawn UI bridge: {}", e))?;

        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take().unwrap();

        let open_instances = Arc::new(Mutex::new(std::collections::HashSet::<u64>::new()));
        let open_instances_clone = open_instances.clone();

        // Stdin writer thread
        let (stdin_tx, stdin_rx) = std::sync::mpsc::channel::<String>();
        std::thread::Builder::new()
            .name("ui-bridge-stdin".into())
            .spawn(move || {
                let mut stdin = stdin;
                for msg in stdin_rx {
                    if writeln!(stdin, "{}", msg).is_err() {
                        break;
                    }
                    if stdin.flush().is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn stdin thread: {}", e))?;

        // Stdout reader thread
        std::thread::Builder::new()
            .name("ui-bridge-stdout".into())
            .spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let line = match line {
                        Ok(l) => l,
                        Err(_) => break,
                    };
                    if line.is_empty() {
                        continue;
                    }

                    let msg: BridgeMessage = match serde_json::from_str(&line) {
                        Ok(m) => m,
                        Err(e) => {
                            log::warn!("UI bridge: invalid message: {} — {:?}", line, e);
                            continue;
                        }
                    };

                    match msg {
                        BridgeMessage::Ready => {
                            log::info!("UI bridge process ready");
                        }
                        BridgeMessage::Opened { instance_id } => {
                            log::info!("UI bridge: UI opened for instance {}", instance_id);
                            open_instances_clone.lock().unwrap().insert(instance_id);
                        }
                        BridgeMessage::OpenFailed { instance_id, error } => {
                            log::error!("UI bridge: UI open failed for instance {}: {}", instance_id, error);
                            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                                instance_id: Some(instance_id),
                                message: format!("Plugin UI failed: {}", error),
                                fatal: false,
                            }));
                        }
                        BridgeMessage::Closed { instance_id } => {
                            log::info!("UI bridge: UI closed for instance {}", instance_id);
                            open_instances_clone.lock().unwrap().remove(&instance_id);
                        }
                        BridgeMessage::PortWrite { instance_id, port_index, value } => {
                            let _ = cmd_tx.send(PwCommand::SetPluginParameter {
                                instance_id,
                                port_index,
                                value,
                            });
                        }
                        BridgeMessage::AtomWrite { .. } => {
                            // TODO: handle atom events from UI
                        }
                    }
                }
                log::info!("UI bridge stdout reader exited");
            })
            .map_err(|e| format!("Failed to spawn stdout thread: {}", e))?;

        Ok(Self {
            child,
            stdin_tx,
            open_instances,
        })
    }

    pub fn open_ui(
        &self,
        instance_id: u64,
        plugin_uri: &str,
        ui_uri: &str,
        ui_type_uri: &str,
        bundle_path: &str,
        binary_path: &str,
        display_name: &str,
        control_values: Vec<(usize, f32)>,
        urid_mapper: &Arc<UridMapper>,
        sample_rate: f32,
    ) {
        let urid_map: Vec<(String, u32)> = urid_mapper.snapshot();

        let msg = HostMessage::Open {
            instance_id,
            plugin_uri: plugin_uri.to_string(),
            ui_uri: ui_uri.to_string(),
            ui_type_uri: ui_type_uri.to_string(),
            bundle_path: bundle_path.to_string(),
            binary_path: binary_path.to_string(),
            title: format!("ZestBay — {}", display_name),
            control_values,
            urid_map,
            lv2_handle: 0, // Not usable across processes
            sample_rate,
        };

        if let Ok(json) = serde_json::to_string(&msg) {
            log::info!("UI bridge: sending Open for instance {} (plugin={})", instance_id, plugin_uri);
            let _ = self.stdin_tx.send(json);
        }
    }

    pub fn send_port_event(&self, instance_id: u64, port_index: usize, value: f32) {
        let msg = HostMessage::PortEvent { instance_id, port_index, value };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.stdin_tx.send(json);
        }
    }

    pub fn close_ui(&self, instance_id: u64) {
        let msg = HostMessage::Close { instance_id };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.stdin_tx.send(json);
        }
    }

    pub fn is_open(&self, instance_id: u64) -> bool {
        self.open_instances.lock().unwrap().contains(&instance_id)
    }

    pub fn shutdown(&mut self) {
        let msg = HostMessage::Quit;
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.stdin_tx.send(json);
        }
        let _ = self.child.wait();
    }
}

impl Drop for UiBridgeClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}
